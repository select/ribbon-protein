#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["gemmi", "mdtraj", "numpy", "scipy", "cairosvg", "shapely>=2.0"]
# ///
"""Render mmCIF/PDB as pen-plotter SVG: smooth joins, dark loops, bold borders.

The default output contains only visible line paths, laid out on A4 paper.
Use --illustration for the older filled, captioned illustration instead.
Occlusion uses depth-sorted surfaces (an approximation, not exact ray tracing).
"""
from __future__ import annotations

import argparse
from collections import Counter
from html import escape
from pathlib import Path
import re
import tempfile

import gemmi
import mdtraj as md
import numpy as np
from scipy.interpolate import CubicSpline, PchipInterpolator
from scipy.ndimage import gaussian_filter1d
from scipy.spatial.transform import Rotation

import plotter


def unit(v):
    return v / np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-9)


def load_structure(path, chains):
    structure = gemmi.read_structure(str(path))
    if not len(structure):
        raise ValueError("Structure has no models")
    # PDB is an interchange format only; do not truncate long mmCIF chain IDs.
    model = structure[0].clone()
    if chains:
        for chain in list(model):
            if chain.name not in chains:
                model.remove_chain(chain.name)
    if not len(model):
        raise ValueError("No matching chains")
    selected = gemmi.Structure()
    selected.add_model(model)
    original_chain_names = [chain.name for chain in selected[0]]
    selected.shorten_chain_names()
    chain_names = {chain.name: original for chain, original
                   in zip(selected[0], original_chain_names)}
    # Use exactly the same atom/residue ordering for coordinates and DSSP.
    with tempfile.TemporaryDirectory() as directory:
        pdb = Path(directory) / "input.pdb"
        selected.write_pdb(str(pdb))
        trajectory = md.load_pdb(str(pdb))
    sse = md.compute_dssp(trajectory, simplified=True)[0]
    xyz = trajectory.xyz[0].astype(float) * 10  # nm -> Angstrom
    segments = []
    counts = Counter()
    # MDTraj can split one PDB chain into separate polymer/solvent chains at
    # TER records. Resolve by chain ID, never zip chain lists by position:
    # zipping silently drops later proteins and mislabels earlier ones.
    for chain in trajectory.topology.chains:
        if chain.chain_id not in chain_names:
            raise ValueError(f'Unmapped interchange chain ID: {chain.chain_id!r}')
        chain_name = chain_names[chain.chain_id]
        segment = []
        for residue in chain.residues:
            atoms = {atom.name: xyz[atom.index] for atom in residue.atoms}
            valid = residue.is_protein and all(k in atoms for k in ("CA", "C", "O", "N"))
            if valid:
                if segment and np.linalg.norm(atoms["CA"] - segment[-1][0]) > 4.5:
                    if len(segment) >= 4:
                        segments.append(segment)
                    segment = []
                kind = str(sse[residue.index])
                if kind not in ("H", "E"):
                    kind = "C"
                segment.append((atoms["CA"], atoms["O"] - atoms["C"], kind, chain_name))
                counts[kind] += 1
            else:
                if len(segment) >= 4:
                    segments.append(segment)
                segment = []
        if len(segment) >= 4:
            segments.append(segment)
    if not segments:
        raise ValueError("No continuous protein backbone with at least four complete residues")
    return segments, counts


def make_ribbon(segment, samples):
    ca = np.array([r[0] for r in segment])
    carbonyl = np.array([r[1] for r in segment])
    kinds = [r[2] for r in segment]
    n = len(ca)
    # Smooth atom-scale kinks, retaining the winding of alpha helices.
    centers = gaussian_filter1d(ca, 0.55, axis=0, mode="nearest")
    tangent = unit(np.gradient(centers, axis=0))
    sides = unit(carbonyl - np.sum(carbonyl * tangent, axis=1)[:, None] * tangent)
    for i in range(1, n):
        if np.dot(sides[i - 1], sides[i]) < 0:
            sides[i] *= -1
    sides = gaussian_filter1d(sides, 0.8, axis=0, mode="nearest")
    widths = np.array([1.15 if k == "H" else 1.35 if k == "E" else 0.20 for k in kinds])
    # Beta strands terminate in a recognizable arrowhead.
    for i in range(n):
        if kinds[i] == "E" and (i == n - 1 or kinds[i + 1] != "E"):
            widths[i] = 0.06
            if i > 0 and kinds[i - 1] == "E":
                widths[i - 1] = 2.0
    widths = gaussian_filter1d(widths, 0.75, mode="nearest")
    widths[0] = widths[-1] = 0.08
    # A continuous material coordinate replaces the old hard tube/ribbon switch.
    loop = gaussian_filter1d(np.array([float(k == "C") for k in kinds]),
                            1.15, mode="nearest")
    t = np.linspace(0, n - 1, (n - 1) * samples + 1)
    center_spline = CubicSpline(np.arange(n), centers, axis=0)
    points = center_spline(t)
    directions = unit(center_spline(t, 1))
    side = CubicSpline(np.arange(n), sides, axis=0)(t)
    side = unit(side - np.sum(side * directions, axis=1)[:, None] * directions)
    width = np.maximum(PchipInterpolator(np.arange(n), widths)(t), 0.04)
    loop = np.clip(PchipInterpolator(np.arange(n), loop)(t), 0, 1)
    loop = loop * loop * (3 - 2 * loop)  # ease into stable tube/ribbon regions
    return points, side, directions, width, loop


def camera(all_points, angles):
    center = all_points.mean(axis=0)
    _, _, axes = np.linalg.svd(all_points - center, full_matrices=False)
    # Major principal axis goes vertically, next largest horizontally.
    basis = np.array([axes[1], axes[0], np.cross(axes[1], axes[0])])
    rotation = Rotation.from_euler("xyz", angles, degrees=True).as_matrix()
    return center, rotation @ basis


def render(segments, args, counts):
    ribbons = [make_ribbon(segment, args.samples) for segment in segments]
    all_points = np.concatenate([r[0] for r in ribbons])
    if getattr(args, 'fixed_camera', False):
        # Coordinates have already been superposed into a shared camera frame.
        center = np.zeros(3)
        matrix = Rotation.from_euler("xyz", args.rotate, degrees=True).as_matrix()
    else:
        center, matrix = camera(all_points, args.rotate)
    transformed = []
    bounds = []
    for points, sides, tangent, width, loop in ribbons:
        p = (points - center) @ matrix.T
        s = sides @ matrix.T
        normal = unit(np.cross(tangent, sides)) @ matrix.T
        transformed.append((p, s, normal, width, loop))
        bounds.extend([p - s * width[:, None], p + s * width[:, None]])
    bounds = np.concatenate(bounds)
    low, high = bounds[:, :2].min(axis=0), bounds[:, :2].max(axis=0)
    if getattr(args, 'view_bounds', None) is not None:
        low = np.array(args.view_bounds[:2], dtype=float)
        high = np.array(args.view_bounds[2:], dtype=float)
    page_w, page_h = args.size, round(args.size * 1.23)
    margin, top = page_w * 0.085, page_w * 0.07
    drawing_w, drawing_h = page_w - 2 * margin, page_h - top - page_w * 0.24
    scale = min(drawing_w / (high[0] - low[0]), drawing_h / (high[1] - low[1]))
    middle = (low + high) / 2

    def project(p):
        return np.column_stack((
            (p[:, 0] - middle[0]) * scale + page_w / 2,
            -(p[:, 1] - middle[1]) * scale + top + drawing_h / 2,
        ))

    def pts(p):
        return " ".join(f"{x:.2f},{y:.2f}" for x, y in p)

    def line(a, b, color, width):
        return (f'<path d="M{a[0]:.2f},{a[1]:.2f}L{b[0]:.2f},{b[1]:.2f}" '
                f'stroke="{color}" stroke-width="{width:.3f}" fill="none"/>')

    panels = []
    ink = "#292b29"
    stroke = args.size / 1100
    light = unit(np.array([-0.4, 0.5, 1.0]))
    for chain_index, (p, side, normal, width, loop) in enumerate(transformed):
        chain_name = segments[chain_index][0][3]
        chain_ink = getattr(args, 'chain_colors', {}).get(chain_name, 'black')
        center2 = project(p)
        screen_tangent = unit(np.gradient(center2, axis=0))
        screen_side = np.column_stack((-screen_tangent[:, 1], screen_tangent[:, 0]))
        ribbon_offset = np.column_stack((side[:, 0], -side[:, 1])) * (width * scale)[:, None]
        # Cross-sections are symmetric, so choose the nearest left/right labeling
        # at every sample. There is no threshold where the tube frame switches.
        # This avoids a 180-degree label flip producing a bow-tie panel at joins.
        offset = np.empty_like(ribbon_offset)
        for i in range(len(p)):
            sign = 1.0 if np.dot(ribbon_offset[i], screen_side[i]) >= 0 else -1.0
            tube = screen_side[i] * sign * width[i] * scale
            value = ribbon_offset[i] * (1 - loop[i]) + tube * loop[i]
            if i and np.dot(value, offset[i - 1]) < 0:
                value = -value
            offset[i] = value
        left2, right2 = center2 - offset, center2 + offset
        for i in range(len(p) - 1):
            z = float((p[i, 2] + p[i + 1, 2]) / 2)
            corners = np.array([left2[i], left2[i + 1], right2[i + 1], right2[i]])
            brightness = abs(float(np.dot(unit(normal[i] + normal[i + 1]), light)))
            blend = (loop[i] + loop[i + 1]) / 2
            gray = int((220 + 33 * brightness) * (1 - blend) + 140 * blend)
            fill = f"#{gray:02x}{gray:02x}{gray:02x}"
            fragment = (f'<polygon points="{pts(corners)}" fill="{fill}" '
                        f'data-loop-a="{loop[i]:.6f}" data-loop-b="{loop[i + 1]:.6f}" '
                        f'data-contours="{args.lines}" data-chain="{chain_index}" data-step="{i}" '
                        f'data-ink="{chain_ink}"/>')
            # Longitudinal contour lines give the ribbon its engraved appearance.
            for t in np.linspace(0.12, 0.88, args.lines):
                a = left2[i] * (1 - t) + right2[i] * t
                b = left2[i + 1] * (1 - t) + right2[i + 1] * t
                fragment += line(a, b, getattr(args, 'chain_colors', {}).get(chain_name, "#575a55"), 0.37 * stroke)
            border_ink = getattr(args, 'chain_colors', {}).get(chain_name, ink)
            fragment += line(left2[i], left2[i + 1], border_ink, 0.88 * stroke)
            fragment += line(right2[i], right2[i + 1], border_ink, 0.88 * stroke)
            panels.append((z, fragment))
    panels.sort(key=lambda x: x[0])
    title = args.title or args.input.stem.replace("-", " ").replace("_", " ")
    subtitle = f"{sum(counts.values())} residues  /  DSSP-assigned secondary structure"
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{page_w}" height="{page_h}" viewBox="0 0 {page_w} {page_h}">',
           f'<title>{escape(title)}</title>',
           f'<desc>Protein ribbon illustration from {escape(args.input.name)}. '
           'Coordinates preserved in 3D; orthographic projection; depth-sorted SVG surfaces. '
           'Secondary structure assigned using DSSP via MDTraj. '
           'No generative image model used.</desc>',
           '<rect width="100%" height="100%" fill="#fffefa"/>',
           '<g stroke-linecap="round" stroke-linejoin="round">']
    svg.extend(fragment for _, fragment in panels)
    svg += ['</g>',
            f'<g fill="{ink}" text-anchor="middle" font-family="monospace">',
            f'<text x="{page_w / 2}" y="{page_h - page_w * 0.125}" font-size="{page_w * 0.021}" letter-spacing="1.8">{escape(title)}</text>',
            f'<text x="{page_w / 2}" y="{page_h - page_w * 0.088}" font-size="{page_w * 0.011}" letter-spacing="1.1">{escape(subtitle)}</text>',
            '</g>', '</svg>']
    source_svg = "\n".join(svg) + "\n"
    if args.source:
        args.source.parent.mkdir(parents=True, exist_ok=True)
        args.source.write_text(source_svg)
        print(f"Source illustration: {args.source.resolve()}")
    if args.illustration:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(source_svg)
        print(f"Illustration SVG: {args.output.resolve()} ({len(panels):,} panels)")
    else:
        # The intermediate polygons exist only for hidden-line removal. They
        # never enter the final SVG and need not be kept on disk.
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / (args.input.stem + ".svg")
            source.write_text(source_svg)
            options = argparse.Namespace(**vars(args))
            options.input = source
            if getattr(args, 'view_bounds', None) is not None:
                frame = project(np.array([low, high]))
                options.frame_bounds = (*frame.min(axis=0), *frame.max(axis=0))
            plotter.convert(options)
    if args.png:
        import cairosvg
        args.png.parent.mkdir(parents=True, exist_ok=True)
        cairosvg.svg2png(url=str(args.output), write_to=str(args.png),
                        output_width=args.size, background_color="white")
        print(f"PNG: {args.png.resolve()}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="Local mmCIF or PDB structure")
    parser.add_argument("-o", "--output", type=Path, default=Path("output/ribbon.svg"))
    parser.add_argument("--png", type=Path, help="Also rasterize a PNG preview")
    parser.add_argument("--illustration", action="store_true", help="Export the older filled, captioned illustration instead of pen paths")
    parser.add_argument("--source", type=Path, help="Optionally keep the intermediate illustration SVG for later restyling")
    parser.add_argument("--title", help="Caption (default: input filename)")
    parser.add_argument("--chains", nargs="+", help="Original chain IDs to include")
    parser.add_argument("--chain-colors", nargs="+", default=[], metavar="CHAIN=COLOR",
                        help="Pen colors by original chain ID, e.g. A=black B=red (black, red, or #RRGGBB)")
    parser.add_argument("--rotate", nargs=3, type=float, default=[0, 0, 0], metavar=("X", "Y", "Z"), help="Rotation in degrees relative to principal-axis view")
    parser.add_argument("--fixed-camera", action="store_true", help="Use input XYZ axes instead of independently computing a principal-axis camera")
    parser.add_argument("--view-bounds", nargs=4, type=float, metavar=('XMIN', 'YMIN', 'XMAX', 'YMAX'),
                        help="Shared projected coordinate bounds, for identical framing and molecular scale")
    parser.add_argument("--size", type=int, default=1400, help="PNG preview width / intermediate illustration width in pixels")
    parser.add_argument("--samples", type=int, default=16, help="Spline samples per residue")
    parser.add_argument("--lines", type=int, default=7, help="Interior contour lines per ribbon")
    plotter.add_plot_options(parser)
    args = parser.parse_args()
    plotter.validate_options(args, parser)
    colors = {}
    for item in args.chain_colors:
        chain, separator, color = item.partition('=')
        if not chain or not separator or not re.fullmatch(r'black|red|#[0-9a-fA-F]{6}', color):
            parser.error("Chain colors must be CHAIN=black, CHAIN=red, or CHAIN=#RRGGBB")
        colors[chain] = color
    args.chain_colors = colors
    if args.view_bounds is not None:
        xmin, ymin, xmax, ymax = args.view_bounds
        if not args.fixed_camera or not np.isfinite(args.view_bounds).all() or xmax <= xmin or ymax <= ymin:
            parser.error('--view-bounds requires --fixed-camera and finite increasing bounds')
    files = [args.input, args.output] + [p for p in (args.source, args.png) if p is not None]
    if len({p.resolve() for p in files}) != len(files):
        parser.error("Input, output, source, and PNG paths must all be different")
    if args.size < 100 or args.samples < 2 or args.lines < 0:
        parser.error("size must be >= 100, samples >= 2, and lines >= 0")
    segments, counts = load_structure(args.input, args.chains)
    print(f"DSSP: {counts['H']} helix, {counts['E']} sheet, {counts['C']} loop residues; {len(segments)} backbone segments")
    render(segments, args, counts)


if __name__ == "__main__":
    main()
