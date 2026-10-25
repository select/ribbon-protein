#!/usr/bin/env python3
"""Render all candidates at -30 Y, choose nine diverse views, then sort a 3x3 grid."""
from concurrent.futures import ThreadPoolExecutor, as_completed
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET

import numpy as np
from scipy.spatial.distance import pdist, squareform

from sort_complex_grid import SVG, arrange, binder_features

ET.register_namespace('', SVG)


def diverse_indices(distances, count):
    """Maximin visual coverage, seeded by the most central candidate."""
    distances = np.asarray(distances)
    if not 1 <= count <= len(distances):
        raise ValueError('Invalid representative count')
    chosen = [int(np.argmin(distances.sum(axis=1)))]
    while len(chosen) < count:
        nearest = distances[:, chosen].min(axis=1)
        nearest[chosen] = -np.inf
        chosen.append(int(np.argmax(nearest)))
    return chosen


def main():
    inputs = Path('output/domain3-aligned-yminus30-inputs')
    candidates = Path('output/domain3-yminus30-sixline-candidates')
    output = Path('output/complexes-domain3-yminus30-diverse9-sorted')
    candidates.mkdir(parents=True, exist_ok=True)
    output.mkdir(parents=True, exist_ok=True)
    entries = sorted(json.loads((inputs / 'manifest.json').read_text()), key=lambda e: e['rank'])
    alignment = json.loads((inputs / 'alignment-report.json').read_text())
    if alignment['view_rotation_degrees_xyz'] != [0, -30, 0] or len(entries) != 40:
        raise ValueError('Expected all 40 candidates in the -30 Y reference view')
    bounds = alignment['view_bounds']
    columns = rows = 3
    width, height, margin, gap, pen = 297, 420, 12, 3, 0.4
    cell_w = (width - 2 * margin - 2 * gap) / 3
    cell_h = (height - 2 * margin - 2 * gap) / 3

    def render_candidate(entry):
        destination = candidates / (entry['rank'] + '.svg')
        command = [sys.executable, str(Path(__file__).with_name('render.py')),
                   str(inputs / entry['file']), '-o', str(destination),
                   '--chain-colors', f"{entry['target_chain']}=black", f"{entry['binder_chain']}=red",
                   '--fixed-camera', '--view-bounds', *map(str, bounds),
                   '--width', str(cell_w), '--height', str(cell_h), '--margin', '1.5',
                   '--pen', str(pen), '--lines', '6', '--border-passes', '1',
                   '--loop-spacing', '0.45', '--title', f"{entry['rank']}: domain III / Y -30 / six contours"]
        process = subprocess.run(command, capture_output=True, text=True,
                                 env=os.environ | {'OPENBLAS_NUM_THREADS': '1', 'OMP_NUM_THREADS': '1'})
        (candidates / (entry['rank'] + '.log')).write_text(process.stdout + process.stderr)
        if process.returncode:
            raise RuntimeError(f"Failed rendering {entry['rank']}; see candidate log")
        print(f"Rendered {entry['rank']}", flush=True)

    with ThreadPoolExecutor(max_workers=4) as pool:
        tasks = [pool.submit(render_candidate, entry) for entry in entries]
        for task in as_completed(tasks):
            task.result()
    features = np.array([binder_features(candidates / (e['rank'] + '.svg')) for e in entries])
    distances = squareform(pdist(features))
    selected = diverse_indices(distances, 9)
    selected_distances = distances[np.ix_(selected, selected)]
    layout, cost = arrange(selected_distances, rows=3, columns=3)
    indices = [selected[int(i)] for i in layout.ravel()]
    root = ET.Element(f'{{{SVG}}}svg', width='297mm', height='420mm', viewBox='0 0 297 420')
    ET.SubElement(root, f'{{{SVG}}}title').text = 'Nine diverse EGFR domain III complexes — visually sorted / Y -30 / A3'
    ET.SubElement(root, f'{{{SVG}}}desc').text = (
        'Nine visually diverse representatives chosen from all 40 candidates in this exact -30 degree Y view. '
        'Selection: maximin multi-scale projected binder-shape coverage. '
        'Layout: visual-similarity clustering and 2D neighbor optimization, 3 columns by 3 rows. '
        'Shared domain III C-alpha superposition onto rank10; shared camera and molecular scale. '
        'EGFR black, binders red; 0.4 mm pen; six interior secondary-structure contours, one fewer than before. '
        'A3 portrait 297 x 420 mm, 12 mm margins, 3 mm gutters. Print at 100 percent. '
        'Vector paths only; no PDF, fonts, masks, or embedded raster data.')
    layers = {}
    for color in ('black', 'red'):
        layers[color] = ET.SubElement(root, f'{{{SVG}}}g', {
            'id': f'pen-{color}', 'fill': 'none', 'stroke': color,
            'stroke-width': str(pen), 'stroke-linecap': 'round', 'stroke-linejoin': 'round'})
    placements = []
    for position, index in enumerate(indices):
        entry = entries[index]
        rank = entry['rank']
        row, col = divmod(position, columns)
        x, y = margin + col * (cell_w + gap), margin + row * (cell_h + gap)
        shutil.copyfile(candidates / (rank + '.svg'), output / (rank + '.svg'))
        source = ET.parse(candidates / (rank + '.svg')).getroot()
        for source_group in source.findall(f'.//{{{SVG}}}g[@id]'):
            color = source_group.attrib['stroke']
            group = ET.SubElement(layers[color], f'{{{SVG}}}g', {
                'id': f'{rank}-{color}', 'transform': f'translate({x:.6f} {y:.6f})'})
            for path in source_group.findall(f'{{{SVG}}}path'):
                group.append(copy.deepcopy(path))
        placements.append({'rank': rank, 'source': entry['file'], 'svg': rank + '.svg',
                           'row': row + 1, 'column': col + 1, 'x_mm': x, 'y_mm': y})
    destination = output / 'top9-a3-portrait.svg'
    ET.ElementTree(root).write(destination, encoding='utf-8', xml_declaration=True)
    coverage = []
    for index, entry in enumerate(entries):
        nearest = min(selected, key=lambda j: distances[index, j])
        coverage.append({'rank': entry['rank'], 'kept': index in selected,
                         'representative': entries[nearest]['rank'],
                         'visual_distance': float(distances[index, nearest])})
    manifest = {'page_mm': [width, height], 'grid': [columns, rows], 'cell_mm': [cell_w, cell_h],
                'margin_mm': margin, 'gap_mm': gap, 'pen_mm': pen,
                'style': {'lines': 6, 'border_passes': 1, 'loop_spacing_mm': 0.45},
                'reference': 'rank10', 'view_rotation_degrees_xyz': [0, -30, 0],
                'view_bounds': bounds, 'fixed_camera': True,
                'selection': 'Maximin visual diversity from all 40 in the -30 Y view',
                'sorting': 'Projected binder occupancy clustering + 2D neighbor optimization',
                'layout_neighbor_cost': cost, 'entries': placements, 'coverage': coverage}
    (output / 'grid-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    np.save(candidates / 'visual-distance-matrix.npy', distances)
    lines = ['# Nine diverse complexes — visually sorted', '',
             'Selected from all 40, using projected binder shapes in the final −30° Y view.',
             'A3 portrait, 3 × 3, 0.4 mm pen, six interior secondary-structure contours (previously seven).',
             'Common reference alignment, camera, and molecular scale. Similar views arranged as neighbors.', '',
             '| Row | Column 1 | Column 2 | Column 3 |', '|---|---|---|---|']
    for row in range(3):
        lines.append(f'| {row + 1} | ' + ' | '.join(entries[i]['rank'] for i in indices[row*3:(row+1)*3]) + ' |')
    lines += ['', '## Removed similar views', '', '| Removed | Closest retained view |', '|---|---|']
    for record in coverage:
        if not record['kept']:
            lines.append(f"| {record['rank']} | {record['representative']} |")
    (output / 'selection-and-order.md').write_text('\n'.join(lines) + '\n')
    print('\n'.join(lines[:13]), flush=True)
    print(f'Grid SVG: {destination.resolve()}', flush=True)


if __name__ == '__main__':
    main()
