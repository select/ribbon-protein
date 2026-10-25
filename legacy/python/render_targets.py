#!/usr/bin/env python3
"""Render the downloaded target assemblies with the smooth A4 pen-plot defaults."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from html import escape
import json
import os
from pathlib import Path
import re
import subprocess
import sys

import gemmi
from PIL import Image, ImageDraw, ImageFont

TARGETS = [
    ('1ten', 'Tenascin domain'),
    ('1ema', 'Green fluorescent protein'),
    ('1stp', 'Streptavidin'),
    ('1axc', 'Human PCNA complex'),
    ('1igt', 'Intact antibody'),
    ('1tim', 'Triosephosphate isomerase'),
]

TARGET_ROTATIONS = {'1ten': (-30, 90, 0), '1axc': (20, 15, 0)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--structures', type=Path, default=Path('output/structures'))
    parser.add_argument('--output', type=Path, default=Path('output/new-targets'))
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--targets-json', type=Path,
                        help='JSON array of objects with id, name, and optional note')
    parser.add_argument('--gallery-title', default='New protein targets')
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    specs = (json.loads(args.targets_json.read_text()) if args.targets_json else
             [{'id': pdb, 'name': name} for pdb, name in TARGETS])
    if (not isinstance(specs, list) or not specs or
            any(not isinstance(e, dict) or not isinstance(e.get('id'), str) or
                not re.fullmatch(r'[0-9][a-zA-Z0-9]{3}', e['id']) or
                not isinstance(e.get('name'), str) for e in specs)):
        parser.error('--targets-json must contain a nonempty array of {id, name, note?} objects')
    targets = [(e['id'].lower(), e['name']) for e in specs]
    if len({pdb for pdb, _ in targets}) != len(targets):
        parser.error('Duplicate target IDs')
    notes = {e['id'].lower(): e.get('note', '') for e in specs}
    for pdb, _ in targets:
        source = args.structures / f'{pdb}-assembly1.cif'
        if not source.is_file():
            raise FileNotFoundError(source)
    args.output.mkdir(parents=True, exist_ok=True)

    def generate(target):
        pdb, name = target
        source = args.structures / f'{pdb}-assembly1.cif'
        command = [sys.executable, str(Path(__file__).with_name('render.py')), str(source),
                   '-o', str(args.output / f'{pdb}.svg'), '--png', str(args.output / f'{pdb}.png'),
                   '--title', f'{name} / {pdb.upper()}',
                   '--rotate', *map(str, TARGET_ROTATIONS.get(pdb, (0, 0, 0))),
                   '--width', '210', '--height', '297', '--margin', '15', '--pen', '0.25',
                   '--samples', '16', '--lines', '7', '--border-passes', '3',
                   '--border-spacing', '0.10', '--loop-spacing', '0.15']
        # Check all modeled protein backbones, not just the asymmetric unit.
        structure = gemmi.read_structure(str(source))
        expected_by_chain = {}
        for chain in structure[0]:
            n = sum(gemmi.find_tabulated_residue(r.name).is_amino_acid() and
                    all(k in r for k in ('N', 'CA', 'C', 'O')) for r in chain)
            if n:
                expected_by_chain[chain.name] = expected_by_chain.get(chain.name, 0) + n
        # Deliberately use the renderer's backbone-direction default.
        process = subprocess.run(command, capture_output=True, text=True,
                                 env=os.environ | {'OPENBLAS_NUM_THREADS': '1', 'OMP_NUM_THREADS': '1'})
        (args.output / f'{pdb}.log').write_text(process.stdout + process.stderr)
        if process.returncode:
            raise RuntimeError(f'{pdb} rendering failed; see its log')
        match = re.search(r'DSSP: (\d+) helix, (\d+) sheet, (\d+) loop', process.stdout)
        counts = dict(zip(('helix', 'sheet', 'loop'), map(int, match.groups()))) if match else {}
        if not counts or sum(counts.values()) != sum(expected_by_chain.values()):
            raise RuntimeError(f'{pdb}: incomplete backbone import: {counts}; expected {expected_by_chain}')
        print(f'Done {pdb.upper()}: {name}', flush=True)
        return {'id': pdb, 'name': name, 'source': str(source),
                'note': notes[pdb], 'complete_backbone_residues_by_chain': expected_by_chain,
                'rotation_degrees_xyz': list(TARGET_ROTATIONS.get(pdb, (0, 0, 0))),
                'svg': f'{pdb}.svg', 'png': f'{pdb}.png', 'residues': counts}

    results = {}
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        tasks = {pool.submit(generate, target): target[0] for target in targets}
        for task in as_completed(tasks):
            results[tasks[task]] = task.result()
    entries = [results[pdb] for pdb, _ in targets]
    settings = {'page_mm': [210, 297], 'margin_mm': 15, 'pen_mm': 0.25,
                'samples': 16, 'interior_lines': 7, 'border_passes': 3,
                'border_spacing_mm': 0.10, 'loop_spacing_mm': 0.15,
                'stroke_direction': 'backbone', 'assembly': 1}
    (args.output / 'manifest.json').write_text(json.dumps({'settings': settings, 'entries': entries}, indent=2) + '\n')
    html = ('<!doctype html><html lang="en"><meta charset="utf-8">'
            '<meta name="viewport" content="width=device-width,initial-scale=1">'
            f'<title>{escape(args.gallery_title)}</title><style>'
            'body{font:16px system-ui;margin:30px;background:#f5f5f2} '
            'main{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:24px}'
            'article{background:white;padding:20px}img{width:100%;height:420px;object-fit:contain}'
            f'</style><h1>{escape(args.gallery_title)}</h1>'
            '<p>A4 portrait · 0.25 mm pen · 7 interior contours · 3-pass borders · backbone-directed strokes.</p>'
            '<p>Biological assembly 1, protein backbones only; RNA, sugars, ligands, and unmodeled residues are not drawn.</p><main>')
    for entry in entries:
        html += (f'<article><a href="{entry["svg"]}"><img src="{entry["png"]}" alt="{escape(entry["name"])}"></a>'
                 f'<h2>{entry["id"].upper()} — {escape(entry["name"])}</h2>'
                 f'<p>{escape(entry["note"])}</p>'
                 f'<a href="{entry["svg"]}">SVG</a> · <a href="{entry["png"]}">PNG preview</a></article>')
    (args.output / 'index.html').write_text(html + '</main></html>\n')
    cell_w, cell_h = 420, 650
    sheet = Image.new('RGB', (3 * cell_w, ((len(entries) + 2) // 3) * cell_h), '#f5f5f2')
    draw = ImageDraw.Draw(sheet)
    try:
        label_font = ImageFont.truetype('DejaVuSans.ttf', 20)
        name_font = ImageFont.truetype('DejaVuSans.ttf', 15)
    except OSError:
        label_font = ImageFont.load_default(size=20)
        name_font = ImageFont.load_default(size=15)
    for i, entry in enumerate(entries):
        x, y = (i % 3) * cell_w, (i // 3) * cell_h
        with Image.open(args.output / entry['png']) as original:
            image = original.convert('RGB')
            image.thumbnail((cell_w - 20, cell_h - 55))
            sheet.paste(image, (x + (cell_w - image.width) // 2, y + 8))
        draw.text((x + 15, y + cell_h - 49), entry['id'].upper(), fill='#222', font=label_font)
        draw.text((x + 15, y + cell_h - 23), entry['name'], fill='#444', font=name_font)
    sheet.save(args.output / 'contact-sheet.png')
    print(f'Gallery: {(args.output / "index.html").resolve()}', flush=True)


if __name__ == '__main__':
    main()
