#!/usr/bin/env python3
"""Render an A4 angle comparison without overwriting the chosen main view."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from html import escape
import json
import os
from pathlib import Path
import subprocess
import sys

from PIL import Image, ImageDraw, ImageFont

VIEWS = [
    ('00-original', 'Original', (0, 0, 0)),
    ('01-current', 'Current tilt', (20, 15, 0)),
    ('02-tilt-x35', 'X tilt 35 degrees', (35, 0, 0)),
    ('03-tilt-y35', 'Y tilt 35 degrees', (0, 35, 0)),
    ('04-diagonal', 'Diagonal tilt', (35, 30, 0)),
    ('05-opposite', 'Opposite tilt', (-30, -20, 0)),
    ('06-steep-x60', 'Steep X tilt', (60, 0, 0)),
    ('07-steep-y60', 'Steep Y tilt', (0, 60, 0)),
    ('08-reverse', 'Reverse side', (0, 180, 0)),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path)
    parser.add_argument('-o', '--output', type=Path, default=Path('output/new-targets/1axc-angles'))
    parser.add_argument('--title', default='1AXC / human PCNA complex')
    parser.add_argument('--jobs', type=int, default=2)
    args = parser.parse_args()
    if not args.input.is_file():
        parser.error('Structure file not found')
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    args.output.mkdir(parents=True, exist_ok=True)
    def generate(view):
        name, label, rotation = view
        command = [sys.executable, str(Path(__file__).with_name('render.py')), str(args.input),
                   '-o', str(args.output / (name + '.svg')), '--png', str(args.output / (name + '.png')),
                   '--title', f'{args.title} / {label}', '--rotate', *map(str, rotation),
                   '--width', '210', '--height', '297', '--margin', '15', '--pen', '0.25',
                   '--samples', '16', '--lines', '7', '--border-passes', '3',
                   '--border-spacing', '0.10', '--loop-spacing', '0.15']
        result = subprocess.run(command, capture_output=True, text=True,
                                env=os.environ | {'OPENBLAS_NUM_THREADS': '1', 'OMP_NUM_THREADS': '1'})
        (args.output / (name + '.log')).write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f'{name}: render failed; see its log')
        print(f'Done {name}: XYZ {rotation}', flush=True)
        return {'id': name, 'label': label, 'rotation_degrees_xyz': list(rotation),
                'svg': name + '.svg', 'png': name + '.png'}
    entries = {}
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        tasks = {pool.submit(generate, view): view[0] for view in VIEWS}
        for task in as_completed(tasks):
            entries[tasks[task]] = task.result()
    entries = [entries[name] for name, _, _ in VIEWS]
    manifest = {'source': str(args.input), 'title': args.title,
                'settings': {'page_mm': [210, 297], 'margin_mm': 15, 'pen_mm': 0.25,
                             'samples': 16, 'lines': 7, 'border_passes': 3,
                             'border_spacing_mm': 0.10, 'loop_spacing_mm': 0.15,
                             'stroke_direction': 'backbone'},
                'rotation_convention': 'XYZ degrees relative to the same principal-axis camera; original is zero rotation',
                'entries': entries}
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    html = ('<!doctype html><html lang="en"><meta charset="utf-8">'
            '<meta name="viewport" content="width=device-width,initial-scale=1">'
            '<title>PCNA angle comparison</title><style>'
            'body{font:16px system-ui;margin:30px;background:#f5f5f2}'
            'main{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:24px}'
            'article{background:white;padding:18px}img{width:100%;height:440px;object-fit:contain}'
            '</style><h1>' + escape(args.title) + ' — angle comparison</h1>'
            '<p>Each SVG is A4 portrait, 0.25 mm pen, 7 interior contours, three-pass borders, '
            'and backbone-directed strokes. XYZ angles are relative to the same original camera.</p><main>')
    for entry in entries:
        html += (f'<article><h2>{escape(entry["label"])}</h2>'
                 f'<p>XYZ: {entry["rotation_degrees_xyz"]}</p>'
                 f'<a href="{entry["svg"]}"><img src="{entry["png"]}" alt="{escape(entry["label"])}"></a>'
                 f'<a href="{entry["svg"]}">A4 SVG</a> · <a href="{entry["png"]}">PNG preview</a></article>')
    (args.output / 'index.html').write_text(html + '</main></html>\n')
    cw, ch = 480, 740
    sheet = Image.new('RGB', (3*cw, 3*ch), '#f5f5f2')
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.truetype('DejaVuSans.ttf', 17)
    small = ImageFont.truetype('DejaVuSans.ttf', 14)
    for i, entry in enumerate(entries):
        x, y = (i % 3)*cw, (i // 3)*ch
        with Image.open(args.output / entry['png']) as original:
            image = original.convert('RGB')
            image.thumbnail((cw - 20, ch - 75))
            sheet.paste(image, (x + (cw - image.width)//2, y + 8))
        draw.text((x + 16, y + ch - 54), f'{i}: {entry["label"]}', fill='#222', font=font)
        draw.text((x + 16, y + ch - 29), f'XYZ {entry["rotation_degrees_xyz"]}', fill='#444', font=small)
    sheet.save(args.output / 'contact-sheet.png')
    print(f'Gallery: {(args.output / "index.html").resolve()}', flush=True)


if __name__ == '__main__':
    main()
