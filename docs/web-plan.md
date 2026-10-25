# Client-side ribbon studio — implementation plan

## Goal

Replace the Python-first entry point with a minimal Nuxt 4 application. Nuxt/Nitro serves the application; molecular files and rendering never leave the browser. Use Rust compiled to WebAssembly in a module Web Worker for the custom ribbon/plotter calculation. Keep Python as a readable reference and regression oracle, not a web dependency.

## Product scope

- Local `.cif`, `.mmcif`, `.pdb`, `.ent` file picker and drag/drop, with limits, validation, actionable errors and no upload endpoint.
- First model, original chain IDs, chain selection and optional chain colors. Exclude ligands/waters; split incomplete backbones and CA gaps >4.5 Å; require at least four complete residues per segment.
- Molecular secondary structure calculated locally through Mol*; disclose assignment differences from MDTraj DSSP. Do not silently label every unannotated structure as loops.
- Rotation around X/Y/Z, pointer-drag rotation and reset to a principal-axis view. Debounce re-renders; keep the page responsive; cancellation terminates/recreates the worker and invalidates obsolete results.
- A4 (210 × 297 mm), A3 (297 × 420 mm), portrait/landscape and a single rounded-square Bierdeckel guide. Allow custom dimensions and margin if inexpensive.
- Bierdeckel: extract `rect6` from the supplied template, not its unrelated circle or full multi-coaster sheet. Guide centerline is 92.74 × 92.74 mm with 10 mm corner radii, inset 0.2 mm in a 93.14 × 93.14 mm canvas including the original 0.4 mm stroke. Default 5 mm artwork margin keeps the rectangular fit within the rounded corners. Preview guide is not plotted/exported by default. Preserve provenance in a small standalone SVG; do not copy embedded images or other artwork.
- Ribbon interior-line count (0–20); pen preview/export width 0.1–2.0 mm, default 0.25 mm. Width is centerline stroke width, not a filled outline; physical pen determines actual ink. Do not silently change loop/border spacing when changing width.
- Advanced: loop fill and spacing (default 0.15 mm), border passes (1–3, default 3), border spacing (0.10 mm), quality/samples, directed vs travel path ordering. Warn when thick pens overlap dense tracks.
- True line-only SVG export in millimeters: no filled surfaces, masks, clipping paths, fonts or raster content. PNG preview export optional but desirable. Export must correspond to current options, never a stale render.
- Render status, cancellation, errors, dimensions, stroke count and drawing length. Basic accessible keyboard controls and responsive layout.

## UI direction

Notion-like, not a molecular CAD application: warm white/gray surfaces, restrained black typography, hairline borders, generous whitespace, small monochrome icons, compact settings sidebar and a large paper preview. Avoid gradients, heavy card shadows, marketing landing pages and a permanently visible Mol* viewer. Controls grouped as Structure, Paper, View, Ink; Advanced collapsed initially. Drag/drop empty state and a local bundled synthetic example for an immediately testable app.

## Architecture and ownership

```
app/
  app.vue                   # UI agent
  components/               # UI agent
  assets/css/main.css       # UI agent
  composables/useRibbon.ts  # runtime agent
  lib/                      # parser, worker client, export helpers: runtime agent
  workers/                  # parser + WASM worker: runtime agent
  types/render.ts           # shared contract, coordinator-owned
crates/ribbon-core/         # Rust engine and native tests: core agent
public/templates/          # standalone coaster guide: coordinator
public/examples/           # small synthetic fixtures: runtime agent
public/wasm/               # generated wasm-pack artifacts, ignored
scripts/                   # WASM build + verification helpers: runtime agent
 tests/                    # TS tests: runtime agent
legacy/python/             # original Python scripts and tests: coordinator
legacy/python/README.md    # original usage documentation
 docs/                     # design, limitations and migration notes
```

Root Nuxt configuration, package scripts and pnpm lockfile. Rust workspace at root, with engine in `crates/ribbon-core`. Prefer `wasm-pack --target web`, explicit browser module loading compatible with workers and Nuxt base URLs. No Python subprocess, server-side calculation, SaaS, CDN scripts or external font dependency. Transfer normalized structures into Rust; keep geometry in memory rather than serializing an intermediate SVG.

### Shared boundary

`app/types/render.ts` is authoritative for JSON field names. Rust exports `render_json(request: &str) -> Result<String, JsValue>`; native tests use the same request/response structs and pure render function. Request consists of normalized `StructureData` and `RenderOptions`. Result includes SVG, dimensions, statistics and warnings. Use finite f64 math internally and strict bounded option validation at the Rust boundary.

`useRibbon()` exposes reactive `options`, refs `structure`, `result`, `status`, `error`, `fileName`, `isStale`, and methods `loadFile(File)`, `loadExample()`, `render()`, `cancel()`, `resetView()`, `downloadSvg()`, `downloadPng()`. Status is `idle | loading | rendering | ready | error`. Options changes trigger debounced rendering after a structure is loaded. UI may mutate option properties; changing chains uses a new array. UI must not import Mol* or WASM directly.

## Rendering strategy

Port the custom Python behavior: smoothed CA/carbonyl frames, stable side signs, cubic center interpolation and shape-preserving widths, beta tips, blended loop/ribbon cross-sections, continuous longitudinal tracks, inset borders, projected/depth-sorted panels, hidden stroke removal, direction-preserving deduplication/welding/joining, simplification and travel ordering. PCA sign ambiguity and float serialization mean exact byte equality is not a parity goal.

Hidden-line clipping is the major risk. A Rust spatial index plus robust segment/polygon interval clipping can avoid a full GEOS dependency: subtract the union of occluded parameter intervals against nearer panels. Test boundary contacts, overlaps, degenerate and crossing panels, direction preservation and ordering. Explicitly document remaining average-depth limitations. Never replace hidden-line removal with SVG masks or a raster image.

Mol* is for parsing and local computed secondary structure, using existing projects as references. Preserve auth/original chain IDs and alternate-location handling. Structural alignment is a later extension: not needed for single-file rotation/export, and deliberately outside the initial acceptance gate.

## Tooling

Follow `ffito` for Nuxt 4, `ssr: false`, UnoCSS (`presetWind4`, local Iconify icons if useful), pnpm, Oxlint/Oxfmt, and TypeScript checks. Follow `movies-deluxe` for commitlint conventional configuration and simple-git-hooks. Add sensible ignore rules, pinned lockfiles, Rust fmt/clippy/tests and browser/runtime tests. CI must compile actual WASM and build Nuxt; generated outputs are not committed. Do not copy secrets, application-specific config or unrelated dependencies from reference projects.

## Milestones

1. **Foundation:** preserve unrelated user changes; move Python under `legacy/python`; commit this plan, contract and extracted guide. Maintain a runnable Python reference and document changed invocation paths.
2. **Parallel implementation:**
   - `github-copilot/gpt-6.1-sol`: Rust engine, clipping/math/export tests, actual WASM build.
   - `github-copilot/gpt-6.1-sol`: Nuxt/tooling, Mol* parser, worker lifecycle, composable, integration tests and build wiring.
   - `amazon-bedrock/eu.anthropic.claude-opus-5-5`: polished Vue UI and styling against the shared contract.
   Workers have disjoint file ownership; coordinator reviews/stages/commits each coherent milestone to avoid shared Git index races.
3. **Integration:** coordinator resolves contract differences, checks actual end-to-end rendering of PDB and CIF, verifies guide and dimensions, tests invalid files/cancellation/stale exports, and reviews UI in a browser.
4. **Validation/documentation:** native Rust tests + fmt/clippy; WASM build; TS tests + formatting/lint/typecheck; Nuxt production build/static generation; Python regression tests from the legacy directory; browser smoke checks. Compare representative web/Python renders and explicitly report parity limitations rather than claiming an exact port.
5. **Delivery:** usable app and development commands, small conventional commits, remaining limitations and preserved uncommitted user work listed. No remote push.

## Acceptance and explicit non-goals

Accept only when a real local PDB and mmCIF can be parsed and rendered by WASM in the browser; controls change output; SVG has correct physical dimensions and only line paths; a worker failure is recoverable; the built app works without a rendering backend. No account system, persistence of uploaded molecular files, biological-assembly expansion, ligand drawing, G-code, multi-structure alignment, exact DSSP equivalence or guaranteed realtime full-quality dragging in this first version.
