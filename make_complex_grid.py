#!/usr/bin/env python3
"""Render the 40 EGFR complexes and compose an A3 portrait, two-pen SVG."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import copy
import json
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET

SVG = 'http://www.w3.org/2000/svg'
ET.register_namespace('', SVG)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=Path('blender_message_top40'))
    parser.add_argument('--output', type=Path, default=Path('output/complexes-top40'))
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--columns', type=int, default=5)
    parser.add_argument('--rows', type=int, default=8)
    parser.add_argument('--lines', type=int, default=3)
    parser.add_argument('--preserve-order', action='store_true', help='Use manifest order instead of rank order')
    parser.add_argument('--fixed-camera', action='store_true')
    parser.add_argument('--view-bounds', nargs=4, type=float)
    parser.add_argument('--aligned-domain3', action='store_true', help='Describe a domain-III-superposed grid')
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    if args.columns < 1 or args.rows < 1 or args.lines < 0:
        parser.error('Grid dimensions must be positive and line count nonnegative')
    entries = json.loads((args.input / 'manifest.json').read_text())
    if not args.preserve_order:
        entries.sort(key=lambda e: e['rank'])
    count = args.columns * args.rows
    if len(entries) != count or len({e['rank'] for e in entries}) != count:
        raise ValueError(f'Expected {count} distinct complexes for the requested grid')
    for entry in entries:
        if not (args.input / entry['file']).is_file():
            raise FileNotFoundError(entry['file'])
    args.output.mkdir(parents=True, exist_ok=True)
    columns, rows = args.columns, args.rows
    page_w, page_h, margin, gap, pen = 297, 420, 12, 3, 0.4
    cell_w = (page_w - 2 * margin - (columns - 1) * gap) / columns
    cell_h = (page_h - 2 * margin - (rows - 1) * gap) / rows

    def generate(entry):
        name = entry['rank']
        destination = args.output / f'{name}.svg'
        command = [sys.executable, str(Path(__file__).with_name('render.py')),
                   str(args.input / entry['file']), '-o', str(destination),
                   '--chain-colors', f"{entry['target_chain']}=black", f"{entry['binder_chain']}=red",
                   '--width', str(cell_w), '--height', str(cell_h), '--margin', '1.5',
                   '--pen', str(pen), '--lines', str(args.lines), '--border-passes', '1',
                   '--loop-spacing', '0.45', '--title', f'{name}: EGFR black / binder red']
        if args.fixed_camera:
            command.append('--fixed-camera')
        if args.view_bounds is not None:
            command.extend(['--view-bounds', *map(str, args.view_bounds)])
        process = subprocess.run(command, capture_output=True, text=True,
                                 env=__import__('os').environ | {'OPENBLAS_NUM_THREADS': '1', 'OMP_NUM_THREADS': '1'})
        (args.output / f'{name}.log').write_text(process.stdout + process.stderr)
        if process.returncode:
            raise RuntimeError(f'{name} failed: see {args.output / (name + ".log")}')
        root = ET.parse(destination).getroot()
        colors = {g.attrib['stroke'] for g in root.findall(f'.//{{{SVG}}}g[@id]')}
        if colors != {'black', 'red'}:
            raise ValueError(f'{name}: expected visible black and red proteins, found {colors}')
        print(f'Done {name}', flush=True)
        return root

    rendered = {}
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        tasks = {pool.submit(generate, entry): entry['rank'] for entry in entries}
        for task in as_completed(tasks):
            rendered[tasks[task]] = task.result()

    root = ET.Element(f'{{{SVG}}}svg', width='297mm', height='420mm', viewBox='0 0 297 420')
    ET.SubElement(root, f'{{{SVG}}}title').text = (
        f'{count} predicted EGFR domain III complexes — superposed / A3 portrait'
        if args.aligned_domain3 else f'{count} predicted EGFR complexes — A3 portrait')
    ET.SubElement(root, f'{{{SVG}}}desc').text = (
        f'{columns} columns by {rows} rows, '
        + ('input manifest order' if args.preserve_order else 'rank order')
        + ' left to right then top to bottom. '
        'EGFR target: black; designed binder: red. Physical pen width: 0.4 mm. '
        '12 mm page margins; 3 mm cell gaps. All geometry is unfilled vector pen paths. '
        + ('EGFR cropped to the same 172-residue domain III sequence. C-alpha superposition '
           'onto rank10; binder transformed rigidly with its target. Shared viewing direction. '
           if args.aligned_domain3 else '')
        + ('Shared framing and molecular scale across all cells. '
           if args.view_bounds is not None else 'Each complex independently fitted; molecular scales differ. ')
        + 'Print at 100% / actual size. No scaling transforms or fonts.')
    layers = {}
    for color in ('black', 'red'):
        layers[color] = ET.SubElement(root, f'{{{SVG}}}g', {
            'id': f'pen-{color}', 'fill': 'none', 'stroke': color,
            'stroke-width': str(pen), 'stroke-linecap': 'round', 'stroke-linejoin': 'round'})
    placement = []
    for index, entry in enumerate(entries):
        x = margin + (index % columns) * (cell_w + gap)
        y = margin + (index // columns) * (cell_h + gap)
        source = rendered[entry['rank']]
        for source_group in source.findall(f'.//{{{SVG}}}g[@id]'):
            color = source_group.attrib['stroke']
            group = ET.SubElement(layers[color], f'{{{SVG}}}g', {
                'id': f"{entry['rank']}-{color}", 'transform': f'translate({x:.6f} {y:.6f})'})
            for path in source_group.findall(f'{{{SVG}}}path'):
                group.append(copy.deepcopy(path))
        placement.append({'rank': entry['rank'], 'source': entry['file'],
                          'svg': f"{entry['rank']}.svg", 'row': index // columns + 1,
                          'column': index % columns + 1, 'x_mm': x, 'y_mm': y})
    destination = args.output / f'top{count}-a3-portrait.svg'
    ET.ElementTree(root).write(destination, encoding='utf-8', xml_declaration=True)
    (args.output / 'grid-manifest.json').write_text(json.dumps({
        'page_mm': [page_w, page_h], 'grid': [columns, rows], 'pen_mm': pen,
        'margin_mm': margin, 'gap_mm': gap, 'cell_mm': [cell_w, cell_h],
        'style': {'lines': args.lines, 'border_passes': 1, 'loop_spacing_mm': 0.45},
        'fixed_camera': args.fixed_camera, 'view_bounds': args.view_bounds,
        'aligned_domain3': args.aligned_domain3,
        'entries': placement}, indent=2) + '\n')
    print(f'Grid SVG: {destination.resolve()}', flush=True)


if __name__ == '__main__':
    main()
