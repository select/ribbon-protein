"""Regenerate the small numerical regression oracle, not a runtime dependency.

Run from repository root:
    .venv/bin/python crates/ribbon-core/tests/generate_reference.py
Requires the optional dependencies of legacy/python/render.py.
"""
from pathlib import Path
import json
import math
import random
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "legacy" / "python"))
import render  # noqa: E402
from shapely import make_valid, union_all  # noqa: E402
from shapely.geometry import LineString, Polygon  # noqa: E402

fixtures = Path(__file__).parent / "fixtures"
fixtures.mkdir(exist_ok=True)

residues = []
segment = []
for i in range(12):
    t = i * 0.55
    ca = [2 * math.cos(t), 2 * math.sin(t), i * 2.7]
    carbonyl = [math.cos(t), math.sin(t), 0.3]
    kind = "H" if i < 4 else "E" if i < 8 else "C"
    residues.append(dict(ca=ca, carbonyl=carbonyl, kind=kind))
    segment.append((ca, carbonyl, kind, "reference"))
ribbon = render.make_ribbon(segment, 8)
samples = []
for index in [0, 1, 4, 8, 11, 19, 24, 32, 40, 48, 56, 64, 72, 80, 87, 88]:
    point, side, tangent, width, alpha = (part[index].tolist() for part in ribbon)
    samples.append(dict(index=index, point=point, side=side, width=width, alpha=alpha))
(fixtures / "python-ribbon.json").write_text(json.dumps(dict(
    provenance="legacy/python/render.py make_ribbon; SciPy CubicSpline/PchipInterpolator; samples=8",
    segment=dict(chainId="reference", residues=residues),
    samples=samples,
), indent=2) + "\n")

rng = random.Random(8274)
cases = []
for index in range(96):
    polygons = []
    for _ in range(1 + index % 3):
        # Some are concave or self-crossing; Shapely's make_valid repairs them.
        polygons.append([[rng.uniform(-3, 3), rng.uniform(-3, 3)] for _ in range(4)])
    source = [[-4.0, rng.uniform(-3, 3)], [4.0, rng.uniform(-3, 3)]]
    if index % 2:
        source.reverse()
    shape = LineString(source).difference(union_all([make_valid(Polygon(p)) for p in polygons]))
    lines = [shape] if shape.geom_type == "LineString" else list(shape.geoms)
    dx, dy = (source[1][i] - source[0][i] for i in range(2))
    length2 = dx * dx + dy * dy
    def parameter(p):
        return ((p[0] - source[0][0]) * dx + (p[1] - source[0][1]) * dy) / length2
    visible = []
    for line in lines:
        if line.is_empty or line.geom_type != "LineString":
            continue
        a, b = list(line.coords)[0], list(line.coords)[-1]
        if parameter(a) > parameter(b):
            a, b = b, a
        visible.append([a, b])
    visible.sort(key=lambda edge: parameter(edge[0]))
    cases.append(dict(source=source, polygons=polygons, visible=visible))
(fixtures / "shapely-clip.json").write_text(json.dumps(dict(
    provenance="Shapely make_valid / union_all / LineString.difference; seed=8274",
    cases=cases,
), separators=(",", ":")) + "\n")
print("Wrote numerical ribbon and 96 Shapely interval fixtures")
