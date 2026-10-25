# Protein pen plots

Create editable, line-only SVG artwork directly from local mmCIF/PDB structures. The default style is **smooth ribbon/loop transitions, closely spaced parallel strokes filling unstructured loops, and three-pass helix/sheet borders**.

Standalone Python renderer; not a wrapper around `fogleman/ribbon`.

## Render a structure

```bash
uv run render.py ~/Dev/hub/ui/public/examples/7x81-ground-truth.cif \
  -o output/my-protein.svg --png output/my-protein.png
```

That's all: no style flags or separate conversion step required. Dependencies are declared in the script (PEP 723) and installed automatically by `uv`. You can also use the existing environment: `.venv/bin/python render.py ...`.

Default output:

- Actual vector pen paths, **no filled shapes, masks, clipping paths, fonts, or embedded images**.
- Hidden strokes geometrically trimmed using intermediate depth-sorted surfaces.
- Shared contour paths that spread continuously from dark loops into ribbons, without cut-end caps at joins.
- Loops filled with longitudinal tracks at up to **0.15 mm spacing** in stable loop regions.
- Each helix/sheet edge has the original outline plus **two inset paths, 0.10 mm apart** where ribbon width allows.
- **A4 portrait, 15 mm margins, 0.25 mm preview pen width**.
- **16 spline samples per residue** and seven interior ribbon contour lines.
- Optional PNG previews on white paper, 1400 px wide.

Open SVGs in a browser, Inkscape, or your plotter's SVG import workflow. These are not machine-specific G-code. Your physical pen determines the plotted width; the SVG paths remain centerlines.

## Gallery

Open **`output/gallery/index.html`** to browse ten designs and download SVGs. **`output/gallery/contact-sheet.png`** shows all ten at a glance.

| Design | Helix residues | Sheet residues | Loop residues |
|---|---:|---:|---:|
| 7NLM | 118 | 48 | 132 |
| 5SG6 | 222 | 0 | 91 |
| 7X81 | 14 | 182 | 90 |
| 7F6J | 63 | 45 | 63 |
| 5SIF | 225 | 0 | 88 |
| 7GKH | 147 | 179 | 273 |
| Hemoglobin | 448 | 0 | 126 |
| 7BEA | 32 | 113 | 104 |
| 8T6D | 293 | 243 | 459 |
| 7M16 | 65 | 0 | 58 |

Counts are computed from the selected model's coordinates with DSSP, not inferred from structure names. Full statistics, input paths, and output filenames are in `output/gallery/manifest.json`. Each design has a matching SVG, PNG, and render log. **7X81** is the most beta-rich example; **7GKH** and **8T6D** are larger mixed structures.

Rebuild the gallery:

```bash
uv run make_gallery.py
# Optional paths and concurrency:
uv run make_gallery.py --examples ~/Dev/hub/ui/public/examples -o output/gallery --jobs 2
```

## Customize

```bash
# Two-protein complex: target chain A black, binder chain B red:
uv run render.py complex.pdb -o output/complex.svg --chain-colors A=black B=red

# Different camera, A3 paper, thinner pen preview:
uv run render.py input.cif -o output/a3.svg --png output/a3.png \
  --rotate 15 -20 0 --width 297 --height 420 --pen 0.2

# Less dense ribbons; loops left unfilled; one contour per border:
uv run render.py input.cif -o output/light.svg \
  --lines 4 --no-fill-loops --border-passes 1

# Smooth forward joins and backbone stroke direction are the default:
uv run render.py input.cif -o output/directed.svg

# Opt into travel-optimized ordering with path reversals:
uv run render.py input.cif -o output/travel.svg --stroke-direction travel

# Try the same routing on an existing smooth illustration, without rerendering:
uv run plotter.py output/7x81-smooth.svg -o output/7x81-plotter-directed.svg \
  --stroke-direction backbone

# Keep the intermediate illustration for later restyling:
uv run render.py input.cif -o output/plot.svg --source output/source.svg

# Restyle that intermediate SVG without recomputing secondary structure:
uv run plotter.py output/source.svg -o output/restyled.svg \
  --loop-spacing 0.12 --border-passes 2 --border-spacing 0.08

# Explicitly request the older filled, captioned illustration:
uv run render.py input.cif -o output/illustration.svg --illustration \
  --title 'Protein ribbon study'
```

Additional options:

- `--chains A B`: select original mmCIF chain IDs.
- `--chain-colors A=black B=red`: color paths by original chain ID (black, red, or `#RRGGBB`). Unspecified chains remain black. Colors are exported in separate pen groups; hidden-line removal still uses the entire complex.
- `--rotate X Y Z`: degrees relative to the principal-axis camera.
- `--size 2000`: PNG preview width and intermediate illustration width, in pixels. Use `--width` / `--height` for the actual plot size in millimeters.
- `--samples 20`: smoother splines at the cost of larger intermediate geometry.
- `--lines 5`: number of interior ribbon contours.
- `--width 210 --height 297 --margin 15`: paper dimensions and margin, in millimeters.
- `--pen 0.25`: physical pen preview width. **Explicit loop/border spacings remain unchanged**; adjust those too when changing pens.
- `--loop-spacing 0.15`: maximum spacing of the parallel fill tracks in stable loops. Transitions gradually spread these into the ribbon contours.
- `--border-passes 1|2|3`: number of paths per ribbon boundary, including the outline.
- `--border-spacing 0.10`: inset border spacing, tapering at narrow tips.
- `--join 0.01`: endpoint welding radius in millimeters.
- `--stroke-direction backbone|travel`: default `backbone` preserves each source segment's forward direction, pairs incoming/outgoing edges by smoothest continuation through branching endpoints, and never reverses completed strokes for travel. Use `travel` to permit path reversal and shorten pen-up travel. It joins separately within each backbone/color and does not add connecting chords or bridge genuine gaps. Requires source `data-chain` metadata. This means consistent direction **along the backbone**, not monotonic movement in one page direction; antiparallel beta strands can still run opposite ways on paper. Fewer pen lifts can come at the cost of substantially longer pen-up travel.
- `--simplify 0.015`: path simplification tolerance in millimeters.
- `--min-length 0.08`: discard isolated strokes below this length after merging.
- `--title`: SVG metadata title; also a visible caption in `--illustration` mode. Plotter SVGs intentionally omit text. A plotted caption would require single-stroke lettering rather than a normal font outline.

`plotter.py` shares the same plot defaults and supports older intermediate SVGs from this project. It is **not a general arbitrary-SVG converter**. Old sources still work, but must be regenerated with `render.py --source ...` or `--illustration` to gain continuous-transition metadata. `--keep-highlights` is only useful with legacy tube-highlight geometry.

## Method and limitations

1. Gemmi reads the first model and optionally selects chains.
2. A temporary PDB loads the coordinates into MDTraj; long chain IDs are shortened only for this interchange. Original names are recovered by actual chain ID, not chain-list position: MDTraj may split polymer and solvent into separate chains at TER records, and these must not truncate or mislabel later protein chains.
3. MDTraj assigns DSSP secondary structure directly from the backbone; input HELIX/SHEET annotations are not required.
4. Smoothed C-alpha splines and carbonyl directions define approximate ribbon geometry. Shape-preserving cubic width interpolation and continuously blended projected cross-sections avoid the old hard tube/ribbon switch.
5. Shared contour families flow through ribbon/loop joins. Additional loop tracks merge onto existing ribbon tracks; inset borders flow into the loop interior.
6. Shapely subtracts nearer surface polygons from each line. Intermediate filled polygons are discarded, never exported in plot mode.
7. Duplicate segments are removed, nearby endpoints welded, and curves simplified. Greedy endpoint ordering/reversal reduces pen-up travel.

This is an artistic illustration, not an exact molecular surface renderer. Average-depth sorting can produce artifacts at complex crossings and edge-on surfaces. Geometry is smoothed; incomplete backbone residues are skipped. C-alpha gaps over 4.5 Angstrom split the backbone. Only segments of at least four complete residues are drawn. Real chain gaps and occluded crossings still interrupt visible strokes. Ligands, waters, and hydrogens are omitted; biological assemblies are not expanded. The dummy-unit-cell warning from MDTraj is harmless for these nonperiodic drawings.

At narrow tips or edge-on ribbons, ink lines concentrate. Try a small crop with your actual pen and paper before plotting a whole sheet; use fewer contour lines, a finer pen, or larger paper if needed. Microscopic endpoint welding and simplification can slightly alter clipped boundaries below normal pen width.

Cairo is needed only for PNG previews and is available on this machine.

## Tests

```bash
uv run --with shapely python -m unittest -v
```

Tests cover hidden-line clipping, endpoint welding, parallel loop fill, inset borders, identical shared transition endpoints, absence of crosswise caps, default style settings, and line-only generated SVGs.
