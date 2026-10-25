#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["gemmi", "mdtraj", "numpy", "scipy", "cairosvg", "shapely>=2.0", "pillow"]
# ///
"""Create a local SVG/PNG gallery with the default smooth pen-plot style."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from html import escape
import json
from pathlib import Path
import re
import subprocess
import sys

from PIL import Image, ImageDraw, ImageFont


EXAMPLES = [
    ("7nlm", "7nlm-ground-truth.cif", (0, 0, 0)),
    ("5sg6", "5sg6-ground-truth.cif", (15, -20, 0)),
    ("7x81", "7x81-ground-truth.cif", (0, 0, 0)),
    ("7f6j", "7f6j-ground-truth.cif", (0, 0, 0)),
    ("5sif", "5sif-ground-truth.cif", (0, 0, 0)),
    ("7gkh", "7gkh-ground-truth.cif", (0, 0, 0)),
    ("hemoglobin", "hemoglobin-demo.cif", (15, -15, 0)),
    ("7bea", "benchmark/7BEA.cif", (0, 0, 0)),
    ("8t6d", "benchmark/8T6D.cif", (0, 0, 0)),
    ("7m16", "benchmark/7M16.cif", (0, 0, 0)),
]


def generate(example, examples_dir, output):
    name, filename, rotation = example
    source = examples_dir / filename
    if not source.is_file():
        raise FileNotFoundError(source)
    command = [sys.executable, str(Path(__file__).with_name("render.py")), str(source),
               "-o", str(output / f"{name}.svg"), "--png", str(output / f"{name}.png"),
               "--title", name.upper(), "--rotate", *map(str, rotation)]
    process = subprocess.run(command, capture_output=True, text=True)
    (output / f"{name}.log").write_text(process.stdout + process.stderr)
    if process.returncode:
        raise RuntimeError(f"{name}: rendering failed; see {output / (name + '.log')}")
    match = re.search(r"DSSP: (\d+) helix, (\d+) sheet, (\d+) loop", process.stdout)
    if not match:
        raise RuntimeError(f"{name}: missing DSSP statistics")
    helix, sheet, loop = map(int, match.groups())
    category = "beta-rich" if sheet / (helix + sheet + loop) > 0.5 else "helix-rich" if helix / (helix + sheet + loop) > 0.55 else "mixed"
    result = {"id": name, "source": str(source), "helix": helix, "sheet": sheet, "loop": loop,
              "category": category, "svg": f"{name}.svg", "png": f"{name}.png"}
    print(f"Done: {name.upper()} / {category} / {sheet} sheet residues", flush=True)
    return result


def gallery(entries, output):
    cards = []
    for entry in entries:
        name = escape(entry["id"].upper())
        cards.append(f'<article><a href="{entry["svg"]}"><img src="{entry["png"]}" alt="{name} pen drawing" loading="lazy"></a>'
                     f'<h2>{name} <small>{entry["category"]}</small></h2>'
                     f'<p>{entry["helix"]} helix / {entry["sheet"]} sheet / {entry["loop"]} loop residues</p>'
                     f'<a href="{entry["svg"]}" download>Download SVG</a> · <a href="{entry["png"]}">PNG preview</a></article>')
    html = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">'
    html += '<title>Protein pen plots</title><style>body{font:15px system-ui;background:#f4f3ef;color:#252525;margin:35px}main{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:24px}article{background:white;padding:20px;border:1px solid #ddd}img{width:100%;height:400px;object-fit:contain}h2{font-size:20px}small{font-size:13px;font-weight:normal;color:#666}a{color:#334f64}p{font-size:13px}</style>'
    html += '<h1>Protein pen plots</h1><p>A4 · 0.25 mm pen · parallel-filled loops · three-pass borders · smooth transitions. SVGs contain only unfilled pen paths.</p><main>'
    html += "\n".join(cards) + '</main></html>\n'
    (output / "index.html").write_text(html)
    (output / "manifest.json").write_text(json.dumps(entries, indent=2) + "\n")
    columns, cell_w, cell_h = 5, 320, 490
    sheet = Image.new("RGB", (columns * cell_w, ((len(entries) + columns - 1) // columns) * cell_h), "#f4f3ef")
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default(size=17)
    small = ImageFont.load_default(size=12)
    for index, entry in enumerate(entries):
        x, y = (index % columns) * cell_w, (index // columns) * cell_h
        with Image.open(output / entry["png"]) as original:
            thumbnail = original.convert("RGB")
            thumbnail.thumbnail((cell_w - 24, cell_h - 65))
            sheet.paste(thumbnail, (x + (cell_w - thumbnail.width) // 2, y + 8))
        draw.text((x + 16, y + cell_h - 48), entry["id"].upper(), fill="#222", font=font)
        draw.text((x + 16, y + cell_h - 26), f'{entry["category"]} / {entry["sheet"]} sheet residues', fill="#555", font=small)
    sheet.save(output / "contact-sheet.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--examples", type=Path, default=Path.home() / "Dev/hub/ui/public/examples")
    parser.add_argument("-o", "--output", type=Path, default=Path("output/gallery"))
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("jobs must be at least 1")
    args.output.mkdir(parents=True, exist_ok=True)
    results, failures = {}, []
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        tasks = {pool.submit(generate, example, args.examples, args.output): example[0] for example in EXAMPLES}
        for task in as_completed(tasks):
            try:
                result = task.result()
                results[result["id"]] = result
            except Exception as error:
                failures.append(str(error))
                print(str(error), file=sys.stderr)
    if results:
        gallery([results[name] for name, _, _ in EXAMPLES if name in results], args.output)
        print(f"Gallery: {(args.output / 'index.html').resolve()}")
    if failures:
        raise SystemExit(f"{len(failures)} render(s) failed")


if __name__ == "__main__":
    main()
