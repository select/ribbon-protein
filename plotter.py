#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["shapely>=2.0", "numpy"]
# ///
"""Convert this project's depth-sorted illustration SVG to actual visible pen paths.

This is deliberately NOT a general SVG converter: it accepts the polygon/line
panels written by render.py. Filled panels are used for clipping, never exported.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
from dataclasses import dataclass, field
from html import escape
import math
from pathlib import Path
import re
import xml.etree.ElementTree as ET

import numpy as np
import shapely
from shapely import affinity
from shapely.geometry import LineString, MultiLineString, Polygon
from shapely.strtree import STRtree

SVG = "{http://www.w3.org/2000/svg}"
NUMBER = r"[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?"
SEGMENT = re.compile(rf"M\s*({NUMBER})[, ]+({NUMBER})\s*L\s*({NUMBER})[, ]+({NUMBER})\s*")


@dataclass
class Panel:
    surface: object
    lines: list = field(default_factory=list)
    loop_corners: tuple | None = None
    corners: tuple | None = None
    loop_blend: tuple | None = None
    contour_count: int = 7
    ink: str = 'black'
    chain_id: str | None = None


def read_panels(path, keep_highlights=False):
    root = ET.parse(path).getroot()
    if root.tag != SVG + "svg":
        raise ValueError("Expected SVG document")
    group = root.find(SVG + "g")
    if group is None:
        raise ValueError("Missing render.py geometry group")
    panels = []
    for element in group.iter():
        if "transform" in element.attrib:
            raise ValueError("Transformed input geometry is not supported")
        if element.tag == SVG + "g":
            continue
        if element.tag == SVG + "polygon":
            numbers = [float(n) for n in re.findall(NUMBER, element.attrib["points"])]
            if len(numbers) != 8:
                raise ValueError("Expected quadrilateral surface panels")
            corners = tuple(zip(numbers[::2], numbers[1::2]))
            surface = shapely.make_valid(Polygon(corners))
            loop_corners = corners if element.attrib.get("fill") == "#8b8e87" else None
            blend = None
            if "data-loop-a" in element.attrib:
                blend = (float(element.attrib["data-loop-a"]), float(element.attrib["data-loop-b"]))
            panels.append(Panel(surface, loop_corners=loop_corners, corners=corners,
                                loop_blend=blend,
                                contour_count=int(element.attrib.get("data-contours", 7)),
                                ink=element.attrib.get("data-ink", "black"),
                                chain_id=element.attrib.get("data-chain")))
        elif element.tag == SVG + "path":
            if not panels:
                raise ValueError("Line without a surface panel")
            match = SEGMENT.fullmatch(element.attrib.get("d", ""))
            if match is None:
                raise ValueError("Expected simple M/L segments from render.py")
            if element.attrib.get("stroke") == "#c5c7c0" and not keep_highlights:
                continue  # tube highlight is a lighting effect, not an ink contour
            x1, y1, x2, y2 = map(float, match.groups())
            if (x1, y1) != (x2, y2):
                panels[-1].lines.append(LineString([(x1, y1), (x2, y2)]))
        else:
            raise ValueError(f"Unsupported geometry element: {element.tag}")
    if not panels:
        raise ValueError("No render.py panels found")
    return panels


def continuous_tracks(panels, units_to_mm, fill_loops, loop_spacing, border_passes, border_spacing):
    """One shared streamline family across all ribbon/loop junctions.

    Instead of ending hatching and restarting tube strokes at a seam, morph
    their fractional positions across each cross-section. Extra loop-fill
    tracks merge into existing ribbon tracks; borders fan smoothly into the
    tube interior. Exactly matching endpoint profiles keep adjacent panels
    joined, including varying-width transition regions.
    """
    contour_count = max(panel.contour_count for panel in panels)
    base_count = contour_count + 2 * border_passes
    loop_widths = [math.dist(panel.corners[a], panel.corners[b]) * units_to_mm
                   for panel in panels for a, b, alpha in
                   ((0, 3, panel.loop_blend[0]), (1, 2, panel.loop_blend[1]))
                   if alpha > 0.95]
    intervals = math.ceil(max(loop_widths, default=0) / loop_spacing) if fill_loops else 1
    count = max(base_count, intervals + 1)

    def profile(width, alpha):
        interior = ([0.12 + j * 0.76 / (contour_count - 1) for j in range(contour_count)]
                    if contour_count > 1 else [0.5] if contour_count else [])
        border = [min(j * border_spacing / max(width, 1e-12),
                      0.35 * j / max(border_passes - 1, 1)) for j in range(border_passes)]
        ribbon = sorted(border + interior + [1 - t for t in border])
        # Duplicate tracks collapse onto the same line in stable ribbon regions.
        ribbon = [ribbon[round(j * (len(ribbon) - 1) / (count - 1))] for j in range(count)]
        tube = [j / (count - 1) if fill_loops else float(j >= count / 2) for j in range(count)]
        return [(1 - alpha) * r + alpha * t for r, t in zip(ribbon, tube)]

    total = 0
    for panel in panels:
        left_start, left_end, right_end, right_start = panel.corners
        start = profile(math.dist(left_start, right_start) * units_to_mm, panel.loop_blend[0])
        end = profile(math.dist(left_end, right_end) * units_to_mm, panel.loop_blend[1])
        panel.lines = []
        for a, b in zip(start, end):
            p = tuple((1 - a) * x + a * y for x, y in zip(left_start, right_start))
            q = tuple((1 - b) * x + b * y for x, y in zip(left_end, right_end))
            if p != q:
                panel.lines.append(LineString([p, q]))
                total += 1
    return count, total


def hatch_loops(panels, units_to_mm, spacing_mm):
    """Add close, longitudinal pen paths inside the loop-tube surfaces.

    All loop panels use the same number of fractional-width tracks so adjacent
    spline samples join continuously. Spacing is at most spacing_mm at the
    widest panel; it becomes tighter where a loop tapers. Existing outlines
    form the two outermost tracks. Occlusion is applied after adding hatching.
    """
    loops = [panel for panel in panels if panel.loop_corners is not None]
    if not loops:
        return 0
    max_width = max(math.dist(c[a], c[b]) * units_to_mm
                    for panel in loops for c in [panel.loop_corners]
                    for a, b in ((0, 3), (1, 2)))
    intervals = max(1, math.ceil(max_width / spacing_mm))
    added = 0
    for panel in loops:
        left_start, left_end, right_end, right_start = panel.loop_corners
        for j in range(1, intervals):
            t = j / intervals
            start = tuple((1 - t) * a + t * b for a, b in zip(left_start, right_start))
            end = tuple((1 - t) * a + t * b for a, b in zip(left_end, right_end))
            if start != end:
                panel.lines.append(LineString([start, end]))
                added += 1
    return added


def reinforce_borders(panels, units_to_mm, passes, spacing_mm):
    """Add parallel inset border tracks to helices/sheets, never loop tubes.

    Distances are measured across the projected ribbon. Offsets taper at narrow
    arrow tips and edge-on sections instead of crossing the opposite border.
    The original contour is pass one. Every added track is occlusion-clipped.
    """
    added = 0
    for panel in panels:
        if panel.loop_corners is not None or panel.corners is None:
            continue
        left_start, left_end, right_end, right_start = panel.corners
        start_width = math.dist(left_start, right_start) * units_to_mm
        end_width = math.dist(left_end, right_end) * units_to_mm
        for j in range(1, passes):
            # At a narrow tip, spread the whole border band over <=35% width.
            start_offset = min(j * spacing_mm, start_width * 0.35 * j / (passes - 1))
            end_offset = min(j * spacing_mm, end_width * 0.35 * j / (passes - 1))
            start_t = start_offset / max(start_width, 1e-12)
            end_t = end_offset / max(end_width, 1e-12)
            for flip in (False, True):
                a = 1 - start_t if flip else start_t
                b = 1 - end_t if flip else end_t
                start = tuple((1 - a) * x + a * y for x, y in zip(left_start, right_start))
                end = tuple((1 - b) * x + b * y for x, y in zip(left_end, right_end))
                if start != end:
                    panel.lines.append(LineString([start, end]))
                    added += 1
    return added


def lines_only(geometry):
    if geometry.is_empty:
        return []
    if geometry.geom_type == "LineString":
        return [geometry]
    if hasattr(geometry, "geoms"):
        return [line for child in geometry.geoms for line in lines_only(child)]
    return []


def visible_lines(panels, by_color=False, by_chain=False, preserve_direction=False):
    """Clip against all nearer surfaces; optionally retain each source line's direction."""
    surfaces = [panel.surface for panel in panels]
    tree = STRtree(surfaces)
    result = defaultdict(list) if by_color or by_chain else []
    for index, panel in enumerate(panels):
        if not panel.lines:
            continue
        ink = MultiLineString(panel.lines)
        candidates = tree.query(ink, predicate="intersects")
        nearer = [surfaces[int(j)] for j in candidates if j > index]
        mask = shapely.union_all(nearer) if nearer else None
        key = (panel.ink, panel.chain_id) if by_chain else panel.ink
        target = result[key] if by_color or by_chain else result
        if preserve_direction:
            # Vectorized clipping keeps a reference to every input segment,
            # unlike a MultiLineString difference, which may reorder its parts.
            clipped = shapely.difference(panel.lines, mask) if mask is not None else panel.lines
            for source, geometry in zip(panel.lines, clipped):
                a, b = source.coords[0], source.coords[-1]
                direction = (b[0] - a[0], b[1] - a[1])
                for piece in lines_only(geometry):
                    p, q = piece.coords[0], piece.coords[-1]
                    if (q[0] - p[0]) * direction[0] + (q[1] - p[1]) * direction[1] < 0:
                        piece = LineString(list(piece.coords)[::-1])
                    target.append(piece)
        else:
            if mask is not None:
                ink = ink.difference(mask)
            target.extend(lines_only(ink))
    return result


def welded_segments(lines, tolerance=0.01):
    """Deduplicate welded edges, retaining their first source orientation."""
    buckets = defaultdict(list)

    def weld(point):
        x, y = point
        bx, by = int(x // tolerance), int(y // tolerance)
        nearby = [p for dx in (-1, 0, 1) for dy in (-1, 0, 1)
                  for p in buckets[(bx + dx, by + dy)]
                  if (p[0] - x) ** 2 + (p[1] - y) ** 2 <= tolerance ** 2]
        if nearby:
            return min(nearby, key=lambda p: (p[0] - x) ** 2 + (p[1] - y) ** 2)
        buckets[(bx, by)].append((x, y))
        return x, y

    unique = {}
    for line in lines:
        coords = [weld(point) for point in line.coords]
        for a, b in zip(coords, coords[1:]):
            if a != b:
                unique.setdefault(tuple(sorted((a, b))), (a, b))
    return unique


def merge_lines(lines, tolerance=0.01):
    """Weld microscopic gaps, deduplicate edges, join degree-two endpoints."""
    unique = welded_segments(lines, tolerance)
    if not unique:
        return []
    return lines_only(shapely.line_merge(MultiLineString(sorted(unique))))


def merge_directed_lines(lines, tolerance=0.01):
    """Join forward edges through branches using the smoothest continuation.

    Every unique edge is traversed exactly once, in its source direction.
    Only already-touching (or microscopically welded) endpoints join. No
    connecting chord, retracing, crosswise hatch, or long-gap bridge is added.
    Call separately for every backbone segment and ink color.
    """
    edges = list(welded_segments(lines, tolerance).values())
    incoming, outgoing = defaultdict(list), defaultdict(list)
    for index, (a, b) in enumerate(edges):
        outgoing[a].append(index)
        incoming[b].append(index)
    following, preceding = {}, {}
    for vertex in incoming.keys() & outgoing.keys():
        ins, outs = incoming[vertex], outgoing[vertex]
        if len(ins) == len(outs) == 1:
            pairs = [(0, ins[0], outs[0])]
        else:
            pairs = []
            for i in ins:
                a, b = edges[i]
                dx, dy = b[0] - a[0], b[1] - a[1]
                for j in outs:
                    p, q = edges[j]
                    ex, ey = q[0] - p[0], q[1] - p[1]
                    cosine = (dx * ex + dy * ey) / (math.hypot(dx, dy) * math.hypot(ex, ey))
                    pairs.append((-cosine, i, j))
            pairs.sort()
        for _, i, j in pairs:
            if i not in following and j not in preceding:
                following[i], preceding[j] = j, i
    used, result = set(), []

    def walk(start):
        coords = [edges[start][0]]
        index = start
        while index not in used:
            used.add(index)
            coords.append(edges[index][1])
            if index not in following:
                break
            index = following[index]
        result.append(LineString(coords))

    # Open trails first; remaining edges form directed cycles.
    for index in range(len(edges)):
        if index not in preceding and index not in used:
            walk(index)
    for index in range(len(edges)):
        if index not in used:
            walk(index)
    return result


def order_paths(lines, allow_reverse=True):
    """Greedy travel ordering; disabling reversal preserves source direction."""
    if not lines:
        return []
    coordinates = [list(line.coords) for line in lines]
    endpoints = np.array([[coords[0], coords[-1]] for coords in coordinates])
    used = np.zeros(len(lines), dtype=bool)
    cursor = np.zeros(2)
    ordered = []
    for _ in lines:
        distances = ((endpoints - cursor) ** 2).sum(axis=2)
        distances[used] = np.inf
        if not allow_reverse:
            distances[:, 1] = np.inf
        best_id, reverse = divmod(int(distances.argmin()), 2)
        used[best_id] = True
        coords = coordinates[best_id]
        if reverse:
            coords.reverse()
        ordered.append(LineString(coords))
        cursor = np.array(coords[-1])
    return ordered


def convert(args):
    panels = read_panels(args.input, args.keep_highlights)
    print(f"Clipping hidden geometry across {len(panels):,} depth-sorted panels...", flush=True)
    lines = visible_lines(panels)
    if not lines:
        raise ValueError("No visible strokes")
    minx, miny, maxx, maxy = (getattr(args, 'frame_bounds', None)
                               or MultiLineString(lines).bounds)
    factor = min((args.width - 2 * args.margin) / (maxx - minx),
                 (args.height - 2 * args.margin) / (maxy - miny))
    dx = (args.width - (maxx - minx) * factor) / 2 - minx * factor
    dy = (args.height - (maxy - miny) * factor) / 2 - miny * factor
    loop_spacing = args.loop_spacing if args.loop_spacing is not None else args.pen * 0.7
    border_spacing = args.border_spacing if args.border_spacing is not None else args.pen * 0.4
    smooth = all(panel.loop_blend is not None for panel in panels)
    if smooth:
        count, total = continuous_tracks(panels, factor, args.fill_loops, loop_spacing,
                                         args.border_passes, border_spacing)
        print(f"Continuous transition profiles: {count} tracks, {total:,} segments", flush=True)
    else:
        if args.fill_loops:
            added = hatch_loops(panels, factor, loop_spacing)
            print(f"Added {added:,} loop-fill segments, maximum spacing {loop_spacing:g} mm", flush=True)
        if args.border_passes > 1:
            added = reinforce_borders(panels, factor, args.border_passes, border_spacing)
            print(f"Added {added:,} border segments: {args.border_passes} passes per edge, {border_spacing:g} mm spacing", flush=True)
    # Clip together, but never join across colors or (in directed mode) backbones.
    directed = getattr(args, 'stroke_direction', 'backbone') == 'backbone'
    if directed and any(panel.chain_id is None for panel in panels):
        raise ValueError('Backbone-directed output requires data-chain metadata; regenerate the source with render.py')
    groups = visible_lines(panels, by_color=True, by_chain=directed,
                           preserve_direction=directed)
    before_merge = sum(len(lines) for lines in groups.values())
    color_lines = defaultdict(list)
    for key, lines in groups.items():
        color = key[0] if directed else key
        lines = [affinity.affine_transform(line, [factor, 0, 0, factor, dx, dy]) for line in lines]
        lines = (merge_directed_lines if directed else merge_lines)(lines, args.join)
        lines = [line.simplify(args.simplify) for line in lines]
        # Filter after merging, so short spline steps do not delete long curves.
        color_lines[color].extend(line for line in lines if line.length >= args.min_length)
    for color, lines in color_lines.items():
        color_lines[color] = order_paths(lines, allow_reverse=not directed)
    length = sum(line.length for lines in color_lines.values() for line in lines) / 1000
    stroke_count = sum(len(lines) for lines in color_lines.values())
    print(f"{before_merge:,} visible pieces -> {stroke_count:,} pen strokes; {length:.2f} m of drawing")
    source_title = ET.parse(args.input).getroot().findtext(SVG + 'title') or args.input.stem
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{args.width:g}mm" height="{args.height:g}mm" viewBox="0 0 {args.width:g} {args.height:g}">',
           f'<title>{escape(source_title)} / pen plot</title>',
           '<desc>Visible vector centerlines only. Hidden strokes geometrically removed '
           'using the source illustration panels. No filled shapes, clipping masks, '
           'fonts, or raster data. Stroke width is a physical pen preview, not shading. '
           + ('Backbone-directed strokes, smoothly joined through branches; no travel-driven reversal.'
              if directed else 'Stroke directions optimized for pen-up travel.') + '</desc>',
           f'<g fill="none" stroke="black" stroke-width="{args.pen:g}" stroke-linecap="round" stroke-linejoin="round">']
    for index, (color, lines) in enumerate(sorted(color_lines.items())):
        svg.append(f'<g id="pen-{index + 1}" stroke="{escape(color, quote=True)}">')
        for line in lines:
            coords = list(line.coords)
            d = f'M{coords[0][0]:.4f},{coords[0][1]:.4f}'
            d += "".join(f'L{x:.4f},{y:.4f}' for x, y in coords[1:])
            svg.append(f'<path d="{d}"/>')
        svg.append('</g>')
    svg += ['</g>', '</svg>']
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(svg) + "\n")
    print(f"SVG: {args.output.resolve()} ({args.width:g} x {args.height:g} mm, {args.pen:g} mm pen)")


def add_plot_options(parser):
    """Shared defaults for direct CIF rendering and standalone SVG conversion."""
    parser.add_argument("--width", type=float, default=210, help="Page width in mm")
    parser.add_argument("--height", type=float, default=297, help="Page height in mm")
    parser.add_argument("--margin", type=float, default=15, help="Page margin in mm")
    parser.add_argument("--pen", type=float, default=0.25, help="Preview pen width in mm; paths remain centerlines")
    parser.add_argument("--join", type=float, default=0.01, help="Weld microscopic endpoint gaps within this radius in mm")
    parser.add_argument("--stroke-direction", choices=('travel', 'backbone'), default='backbone',
                        help="backbone (default): smooth forward joins and no path reversal; travel: minimize pen-up travel with reversals")
    parser.add_argument("--simplify", type=float, default=0.015, help="Path simplification tolerance in mm")
    parser.add_argument("--min-length", type=float, default=0.08, help="Drop isolated strokes shorter than this many mm")
    parser.add_argument("--keep-highlights", action="store_true", help="Also plot the loop-tube highlight lines")
    parser.add_argument("--border-passes", type=int, choices=(1, 2, 3), default=3, help="Parallel paths per helix/sheet border, including the original outline")
    parser.add_argument("--border-spacing", type=float, default=0.10, help="Inset spacing of border paths in mm (default: 0.10)")
    parser.add_argument("--fill-loops", action=argparse.BooleanOptionalAction, default=True, help="Fill unstructured loop tubes with close parallel pen paths (default: enabled)")
    parser.add_argument("--loop-spacing", type=float, default=0.15, help="Maximum loop-fill spacing in mm (default: 0.15)")


def build_parser():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="Illustration SVG produced by render.py --illustration or --source")
    parser.add_argument("-o", "--output", type=Path, required=True)
    add_plot_options(parser)
    return parser


def validate_options(args, parser):
    if args.margin < 0 or args.width <= 2 * args.margin or args.height <= 2 * args.margin:
        parser.error("Page dimensions must exceed twice the nonnegative margin")
    if args.pen <= 0 or args.join <= 0 or args.simplify < 0 or args.min_length < 0:
        parser.error("Pen width and join tolerance must be positive; other tolerances must be nonnegative")
    if args.border_spacing is not None and args.border_spacing <= 0:
        parser.error("Border spacing must be positive")
    if args.loop_spacing is not None and args.loop_spacing <= 0:
        parser.error("Loop spacing must be positive")
    if args.input.resolve() == args.output.resolve():
        parser.error("Keep the source illustration: choose a different output path")


def main():
    parser = build_parser()
    args = parser.parse_args()
    validate_options(args, parser)
    convert(args)


if __name__ == "__main__":
    main()
