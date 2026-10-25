#!/usr/bin/env python3
"""Arrange a rendered two-color protein grid by projected binder similarity."""
import argparse
import copy
import io
import json
from pathlib import Path
import xml.etree.ElementTree as ET

import cairosvg
import numpy as np
from PIL import Image
from scipy.cluster.hierarchy import leaves_list, linkage
from scipy.ndimage import gaussian_filter
from scipy.spatial.distance import pdist, squareform

SVG = 'http://www.w3.org/2000/svg'
ET.register_namespace('', SVG)


def binder_features(path):
    """Multi-scale occupancy descriptors; retain position in shared target frame."""
    root = ET.parse(path).getroot()
    red = next(g for g in root.findall(f'.//{{{SVG}}}g[@id]') if g.attrib['stroke'] == 'red')
    isolated = ET.Element(f'{{{SVG}}}svg', dict(root.attrib))
    ink = ET.SubElement(isolated, f'{{{SVG}}}g', {
        'fill': 'none', 'stroke': 'black', 'stroke-width': '0.4',
        'stroke-linecap': 'round', 'stroke-linejoin': 'round'})
    for path in red.findall(f'{{{SVG}}}path'):
        ink.append(copy.deepcopy(path))
    png = cairosvg.svg2png(bytestring=ET.tostring(isolated), output_width=400)
    alpha = np.asarray(Image.open(io.BytesIO(png)).convert('RGBA'))[:, :, 3]
    occupancy = Image.fromarray(alpha).resize((64, 64), Image.Resampling.LANCZOS)
    occupancy = np.asarray(occupancy, dtype=float) / 255
    features = []
    for sigma in (1.0, 2.5, 5.0):
        blurred = gaussian_filter(occupancy, sigma).ravel()
        norm = np.linalg.norm(blurred)
        if norm == 0:
            raise ValueError(f'No visible binder strokes: {path}')
        features.append(blurred / norm)
    return np.concatenate(features) / np.sqrt(len(features))


def arrange(distances, rows=8, columns=5, seed=1701):
    """Hierarchical seriation followed by reproducible 2D neighbor optimization."""
    condensed = squareform(distances, checks=False)
    leaves = leaves_list(linkage(condensed, method='average', optimal_ordering=True))
    layout = leaves.reshape(rows, columns).copy()
    layout[1::2] = layout[1::2, ::-1]
    edges = [(r * columns + c, r * columns + c + 1)
             for r in range(rows) for c in range(columns - 1)]
    edges += [(r * columns + c, (r + 1) * columns + c)
              for r in range(rows - 1) for c in range(columns)]
    edges = np.array(edges)
    flat = layout.ravel()
    def score(a):
        return float(distances[a[edges[:, 0]], a[edges[:, 1]]].sum())
    current = score(flat)
    best, best_score = flat.copy(), current
    rng = np.random.default_rng(seed)
    start_temperature = max(float(np.median(condensed)) * 0.2, 1e-6)
    for step in range(60000):
        a, b = rng.choice(len(flat), 2, replace=False)
        flat[a], flat[b] = flat[b], flat[a]
        proposed = score(flat)
        temperature = start_temperature * (0.0001 ** (step / 59999))
        delta = proposed - current
        if delta <= 0 or rng.random() < np.exp(-delta / temperature):
            current = proposed
            if current < best_score:
                best, best_score = flat.copy(), current
        else:
            flat[a], flat[b] = flat[b], flat[a]
    return best.reshape(rows, columns), best_score


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    directory = args.directory
    manifest = json.loads((directory / 'grid-manifest.json').read_text())
    entries = sorted(manifest['entries'], key=lambda e: e['rank'])
    features = np.array([binder_features(directory / entry['svg']) for entry in entries])
    distances = squareform(pdist(features, metric='euclidean'))
    layout, cost = arrange(distances)
    order = [entries[int(i)] for i in layout.ravel()]
    root = ET.parse(directory / 'top40-a3-portrait.svg').getroot()
    root.find(f'{{{SVG}}}title').text = 'EGFR domain III complexes — visual similarity / Y +30 degrees / A3'
    description = root.find(f'{{{SVG}}}desc')
    description.text = (description.text.replace('rank order left to right then top to bottom.',
                                                 'arranged by projected binder visual similarity.')
                        + ' Reference-camera view rotated +30 degrees about Y (counterclockwise viewed from +Y). '
                        'Multi-scale red-stroke occupancy similarity, hierarchical clustering and 2D neighbor optimization. '
                        'Original ranks retained in group IDs; see similarity-order.md for grid positions.')
    cell_w, cell_h = manifest['cell_mm']
    gap, margin = manifest['gap_mm'], manifest['margin_mm']
    placement = []
    for layer in root.findall(f'{{{SVG}}}g'):
        groups = {g.attrib['id'].rsplit('-', 1)[0]: g for g in layer}
        for child in list(layer):
            layer.remove(child)
        for index, entry in enumerate(order):
            row, col = divmod(index, 5)
            group = groups[entry['rank']]
            group.set('transform', f'translate({margin + col * (cell_w + gap):.6f} {margin + row * (cell_h + gap):.6f})')
            layer.append(group)
    for index, entry in enumerate(order):
        row, col = divmod(index, 5)
        placement.append(entry | {'row': row + 1, 'column': col + 1,
                                 'x_mm': margin + col * (cell_w + gap),
                                 'y_mm': margin + row * (cell_h + gap)})
    output = directory / 'top40-a3-portrait-similarity.svg'
    ET.ElementTree(root).write(output, encoding='utf-8', xml_declaration=True)
    positions = {entry['rank']: f"r{entry['row']}c{entry['column']}" for entry in placement}
    neighbors = {}
    for i, entry in enumerate(entries):
        nearest = np.argsort(distances[i])[1:5]
        neighbors[entry['rank']] = [{'rank': entries[int(j)]['rank'],
                                    'distance': float(distances[i, j])} for j in nearest]
    (directory / 'similarity-manifest.json').write_text(json.dumps(manifest | {
        'entries': placement, 'view_rotation_degrees_xyz': [0, 30, 0],
        'sorting': 'Multi-scale projected red binder occupancy; hierarchical clustering + 2D nearest-neighbor layout',
        'layout_neighbor_cost': cost, 'nearest_visual_neighbors': neighbors}, indent=2) + '\n')
    lines = ['# Visual similarity grid', '',
             'A3 portrait, 5 × 8, 0.4 mm. EGFR black / binder red. Reference: rank10 domain III.',
             'Shared molecular scale and orientation; reference-camera Y rotation +30°.', '',
             '| Row | Column 1 | Column 2 | Column 3 | Column 4 | Column 5 |',
             '|---|---|---|---|---|---|']
    for row in range(8):
        lines.append(f'| {row + 1} | ' + ' | '.join(e['rank'] for e in order[row * 5:(row + 1) * 5]) + ' |')
    lines += ['', 'Order is based on appearance in this view, not sequence similarity or model confidence.',
              f"Original r4c5 (rank20) → {positions['rank20']}; original r7c1 (rank31) → {positions['rank31']}."]
    (directory / 'similarity-order.md').write_text('\n'.join(lines) + '\n')
    print('\n'.join(lines), flush=True)
    print(f'Sorted grid: {output.resolve()}', flush=True)


if __name__ == '__main__':
    main()
