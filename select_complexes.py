#!/usr/bin/env python3
"""Select visual representatives while preserving a similarity-sorted grid order."""
import argparse
import json
from pathlib import Path
import shutil

import numpy as np
from scipy.cluster.hierarchy import cut_tree, linkage
from scipy.spatial.distance import pdist, squareform

from render import load_structure, make_ribbon
from sort_complex_grid import binder_features


def representatives(distances, count):
    """One medoid per average-linkage visual cluster, with stable tie breaking."""
    if not 1 <= count <= len(distances):
        raise ValueError('Representative count must be between 1 and input count')
    if count == len(distances):
        return list(range(count))
    tree = linkage(squareform(distances, checks=False), method='average')
    labels = cut_tree(tree, n_clusters=[count]).ravel()
    selected = []
    for label in range(count):
        members = np.flatnonzero(labels == label)
        costs = distances[np.ix_(members, members)].sum(axis=1)
        selected.append(int(members[np.argmin(costs)]))
    return sorted(selected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--grid', type=Path, default=Path('output/complexes-domain3-y30-similarity'))
    parser.add_argument('--structures', type=Path, default=Path('output/domain3-aligned-y30-inputs'))
    parser.add_argument('--output', type=Path, default=Path('output/domain3-y30-selected20-inputs'))
    parser.add_argument('--count', type=int, default=20)
    args = parser.parse_args()
    grid_path = args.grid / 'similarity-manifest.json'
    if not grid_path.exists():
        grid_path = args.grid / 'grid-manifest.json'
    grid = json.loads(grid_path.read_text())
    entries = grid['entries']  # This is the approved visual order, NOT rank order.
    features = np.array([binder_features(args.grid / e['svg']) for e in entries])
    distances = squareform(pdist(features))
    selected = representatives(distances, args.count)
    originals = {e['rank']: e for e in json.loads((args.structures / 'manifest.json').read_text())}
    args.output.mkdir(parents=True, exist_ok=True)
    bounds, prepared = [], []
    for index in selected:
        entry = originals[entries[index]['rank']]
        shutil.copyfile(args.structures / entry['file'], args.output / entry['file'])
        prepared.append(entry)
        segments, _ = load_structure(args.output / entry['file'], None)
        for segment in segments:
            points, _, _, width, _ = make_ribbon(segment, 16)
            bounds.extend([points[:, :2] - width[:, None], points[:, :2] + width[:, None]])
    bounds = np.concatenate(bounds)
    low, high = bounds.min(axis=0) - 0.5, bounds.max(axis=0) + 0.5
    view_bounds = [*low.tolist(), *high.tolist()]
    alignment = json.loads((args.structures / 'alignment-report.json').read_text())
    ranks = {e['rank'] for e in prepared}
    alignment['entries'] = [e for e in alignment['entries'] if e['rank'] in ranks]
    alignment['view_bounds'] = view_bounds
    report = []
    for index, entry in enumerate(entries):
        nearest = min(selected, key=lambda j: distances[index, j])
        report.append({'rank': entry['rank'], 'kept': index in selected,
                       'previous_position': f"r{entry['row']}c{entry['column']}",
                       'representative': entries[nearest]['rank'],
                       'visual_distance_to_representative': float(distances[index, nearest])})
    (args.output / 'manifest.json').write_text(json.dumps(prepared, indent=2) + '\n')
    (args.output / 'alignment-report.json').write_text(json.dumps(alignment, indent=2) + '\n')
    (args.output / 'selection-report.json').write_text(json.dumps({
        'method': f'Average-linkage clustering of projected multi-scale binder occupancy; {args.count} cluster medoids',
        'preserved_order': 'Relative order in approved similarity-sorted grid',
        'selected_ranks': [e['rank'] for e in prepared], 'entries': report}, indent=2) + '\n')
    print('Selected in visual order:', ', '.join(e['rank'] for e in prepared), flush=True)
    for entry in report:
        if not entry['kept']:
            print(f"Remove {entry['rank']} (similar to {entry['representative']})", flush=True)


if __name__ == '__main__':
    main()
