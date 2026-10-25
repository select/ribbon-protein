#!/usr/bin/env python3
"""Overwrite a grid's contour density without changing selection or placement."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

SVG = 'http://www.w3.org/2000/svg'
ET.register_namespace('', SVG)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--structures', type=Path, required=True)
    parser.add_argument('--lines', type=int, required=True)
    parser.add_argument('--compact-grid', action='store_true', help='Square cells with equal row/column pitch, centered on the page')
    parser.add_argument('--outer-margin', type=float, default=5)
    parser.add_argument('--gap', type=float, default=2)
    parser.add_argument('--drawing-margin', type=float)
    args = parser.parse_args()
    if args.lines < 0:
        parser.error('--lines must be nonnegative')
    directory = args.directory
    manifest_path = directory / 'grid-manifest.json'
    manifest = json.loads(manifest_path.read_text())
    entries = manifest['entries']
    previous = manifest['style']['lines']
    if args.drawing_margin is not None:
        if args.drawing_margin <= 0:
            parser.error('--drawing-margin must be positive')
        manifest['drawing_margin_mm'] = args.drawing_margin
    if args.compact_grid:
        if args.outer_margin < 0 or args.gap < 0:
            parser.error('Outer margin and gap must be nonnegative')
        columns, rows = manifest['grid']
        page_w, page_h = manifest['page_mm']
        side = min((page_w - 2 * args.outer_margin - (columns - 1) * args.gap) / columns,
                   (page_h - 2 * args.outer_margin - (rows - 1) * args.gap) / rows)
        if side <= 2 * manifest.get('drawing_margin_mm', 1.5):
            parser.error('Requested layout leaves no drawing area')
        origin_x = (page_w - columns * side - (columns - 1) * args.gap) / 2
        origin_y = (page_h - rows * side - (rows - 1) * args.gap) / 2
        manifest.update(cell_mm=[side, side], margin_mm=args.outer_margin,
                        gap_mm=args.gap, grid_origin_mm=[origin_x, origin_y],
                        layout='Centered square cells; equal horizontal and vertical pitch')
        for entry in entries:
            entry['x_mm'] = origin_x + (entry['column'] - 1) * (side + args.gap)
            entry['y_mm'] = origin_y + (entry['row'] - 1) * (side + args.gap)
    positions = {entry['rank']: entry for entry in entries}
    destination = directory / f'top{len(entries)}-a3-portrait.svg'
    root = ET.parse(destination).getroot()
    with tempfile.TemporaryDirectory() as temporary:
        temporary = Path(temporary)
        def generate(entry):
            output = temporary / entry['svg']
            command = [sys.executable, str(Path(__file__).with_name('render.py')),
                       str(args.structures / entry['source']), '-o', str(output),
                       '--chain-colors', 'A=black', 'B=red', '--fixed-camera',
                       '--view-bounds', *map(str, manifest['view_bounds']),
                       '--width', str(manifest['cell_mm'][0]), '--height', str(manifest['cell_mm'][1]),
                       '--margin', str(manifest.get('drawing_margin_mm', 1.5)), '--pen', str(manifest['pen_mm']),
                       '--lines', str(args.lines), '--border-passes', str(manifest['style']['border_passes']),
                       '--loop-spacing', str(manifest['style']['loop_spacing_mm']),
                       '--title', f"{entry['rank']}: domain III / {args.lines} contours"]
            process = subprocess.run(command, capture_output=True, text=True,
                                     env=os.environ | {'OPENBLAS_NUM_THREADS': '1', 'OMP_NUM_THREADS': '1'})
            if process.returncode:
                raise RuntimeError(f"{entry['rank']}: {process.stdout}\n{process.stderr}")
            print(f"Restyled {entry['rank']}", flush=True)
            return ET.parse(output).getroot()
        with ThreadPoolExecutor(max_workers=4) as pool:
            sources = dict(zip((e['rank'] for e in entries), pool.map(generate, entries)))
        for layer in root.findall(f'{{{SVG}}}g'):
            color = layer.attrib['stroke']
            for group in layer:
                rank = group.attrib['id'].rsplit('-', 1)[0]
                position = positions[rank]
                group.set('transform', f"translate({position['x_mm']:.6f} {position['y_mm']:.6f})")
                source_group = next(g for g in sources[rank].findall(f'.//{{{SVG}}}g[@id]')
                                    if g.attrib['stroke'] == color)
                for child in list(group):
                    group.remove(child)
                for path in source_group.findall(f'{{{SVG}}}path'):
                    group.append(copy.deepcopy(path))
        root.find(f'{{{SVG}}}desc').text = (
            f"{len(entries)} complexes with unchanged selection, similarity-sorted layout, reference alignment, "
            f"shared scale and {manifest['view_rotation_degrees_xyz']} degree XYZ camera rotation. "
            f"{args.lines} interior secondary-structure contours (previously {previous}). "
            f"EGFR black / binder red; {manifest['pen_mm']} mm physical pen. "
            f"A3 portrait, grid {manifest['grid']}, cell dimensions {manifest['cell_mm']} mm, "
            f"equal grid gaps {manifest['gap_mm']} mm. Visible vector paths only. Print at actual size.")
        # All rendering succeeded before replacing any of the requested artifacts.
        for entry in entries:
            (directory / entry['svg']).write_bytes((temporary / entry['svg']).read_bytes())
        ET.ElementTree(root).write(destination, encoding='utf-8', xml_declaration=True)
    manifest['style']['lines'] = args.lines
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    order_path = directory / 'selection-and-order.md'
    if order_path.exists():
        text = order_path.read_text()
        text = re.sub(r'(?:\d+|six) interior secondary-structure contours \([^)]*\)',
                      f'{args.lines} interior secondary-structure contours (previously {previous})', text)
        if args.compact_grid:
            text += (f"\nLayout: centered square cells ({manifest['cell_mm'][0]:.2f} mm), "
                     f"{manifest['gap_mm']:g} mm horizontal and vertical gutters; "
                     f"{args.outer_margin:g} mm side margins.\n")
        order_path.write_text(text)
    print(f'Overwritten: {destination.resolve()} ({previous} -> {args.lines} contours)', flush=True)


if __name__ == '__main__':
    main()
