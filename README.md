# Ribbon Studio

Turn local protein structures into editable, line-only SVG artwork for a pen plotter. A minimal **Nuxt 4 / Vue / UnoCSS** studio wraps a **Rust/WebAssembly** rendering engine.

**[Open the live studio →](https://select.github.io/ribbon-protein/)** · [Source on GitHub](https://github.com/select/ribbon-protein)

Inspired by Michael Fogleman's [plotter work](https://www.michaelfogleman.com/plotter/) and [original ribbon repository](https://github.com/fogleman/ribbon). This is a separate browser-based implementation, not his Go library.

**Files stay in your browser.** Mol* parses coordinates and computes secondary structure locally; a Web Worker runs the Rust renderer. Nuxt/Nitro only serves the app and its assets—there is no upload or calculation endpoint.

## Run locally

Requires Node.js 24, pnpm 11, Rust (stable) and wasm-pack:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.13.1 --locked
pnpm install
pnpm dev
```

Open the URL printed by Nuxt (normally `http://localhost:3000`). `pnpm dev` builds the WASM module first. After changing Rust, run `pnpm build:wasm` and reload the page; Vue/TypeScript changes use normal Nuxt hot reload. `pnpm dev:nuxt` skips the initial WASM build.

### Production

```bash
pnpm build
node .output/server/index.mjs
```

Alternatively, `pnpm generate` creates a static app in `.output/public/`; serve that directory over HTTP(S). No Rust runtime or calculation server is needed on the host. For a custom subdirectory deployment, set `NUXT_APP_BASE_URL` at build time (and when running a Nitro server), for example `/studio/`.

### GitHub Pages

The [Pages workflow](.github/workflows/deploy.yml) builds and deploys [the live studio](https://select.github.io/ribbon-protein/) on pushes to `main`, or manually via **Actions → Deploy to GitHub Pages → Run workflow**. Repository **Settings → Pages → Build and deployment → Source** must be **GitHub Actions**. It uses Node 24, the pinned pnpm lockfile and Rust/wasm-pack; generated assets are uploaded as a Pages artifact, never committed.

To reproduce the Pages build:

```bash
GITHUB_PAGES=true pnpm generate
```

This selects the GitHub Pages preset and `/ribbon-protein/` base path, including the worker, examples and WASM assets. Local development still defaults to `/`. The artifact is `.output/public/`; serve it mounted at `/ribbon-protein/` to test locally.

## Using the studio

1. Choose or drop a `.cif`, `.mmcif`, `.pdb` or `.ent` file. **Load example** opens a small bundled synthetic structure without contacting a molecular database.
2. Select chains and optional per-chain pen colors.
3. Choose paper, orientation and margin:
   - **A4:** 210 × 297 mm.
   - **A3:** 297 × 420 mm.
   - **Bierdeckel / Coaster:** 93.14 × 93.14 mm canvas containing one 92.74 mm rounded-square guide with 10 mm corners, extracted from the supplied plotter template. Default artwork margin: 5 mm.
   - **Custom:** explicit width and height; swap dimensions to rotate the page.
4. Drag the paper or adjust X/Y/Z rotation. With the preview focused, use arrow keys, Shift for 15° steps, `[` / `]` for Z, and Home or `0` to reset.
5. Set **Interior lines** (0–20) and **Pen width** (0.1–2 mm; default 0.25 mm). Advanced settings control loop fill, loop/border spacing, border passes, spline samples and path direction.
6. Download **SVG** for plotting/editing or **PNG** for a white-paper preview. Exports remain disabled until the preview matches the current settings. Cancel interrupts the worker; changing options or retrying starts a fresh calculation.

Guides belong to the preview and are **never exported**. The coaster export is a square SVG canvas, not a mask or a cutting template. The default 5 mm margin keeps the artwork inside the rounded guide; reducing the margin can put ink outside the physical coaster near its corners.

### Plotter semantics

SVGs contain visible vector centerlines, grouped by pen color, with physical millimeter dimensions. There are **no filled surfaces, masks, clipping paths, fonts or embedded raster images**. Temporary ribbon surfaces are only used for geometric hidden-line removal.

Pen width changes preview/export stroke width; your physical pen determines actual ink width. It does not automatically change loop or border spacing. Thick pens and closely spaced tracks can overlap—try a small physical sample before plotting. The default `backbone` path ordering retains source direction and smooth joins; `travel` allows reversal to reduce pen-up travel. Neither intentionally bridges real backbone gaps.

## Calculation and limitations

- First molecular model only. Original author chain IDs are preserved. Incomplete protein backbones, sequence/CA gaps and alternate conformers are handled explicitly. At least four complete residues are needed for a drawable fragment. Ligands, waters and biological-assembly expansion are not rendered.
- **Secondary structure uses Mol* Zhang–Skolnick CA geometry, not hydrogen-bond DSSP.** It is computed from coordinates even without HELIX/SHEET annotations, but helix/sheet boundaries and inter-chain sheets can differ from DSSP assignments. The UI exposes this warning; computation failures do not silently fall back to all loops.
- Rust handles ribbon smoothing, continuous loop/ribbon tracks, inset borders, projection, spatially indexed hidden-line trimming, stroke welding/joining, simplification and ordering. This is an artistic ribbon illustration, not a molecular surface renderer.
- Visibility uses **average panel depth**, so intersecting or strongly twisted surfaces may have local occlusion artifacts. PCA orientation, full-precision geometry, page fitting and secondary-structure assignment all affect the final illustration.
- Rendering is debounced rather than guaranteed realtime during dragging. The previous preview remains visible but marked stale during calculation.
- Browser input limits: 20 MiB per file, 250,000 atoms, 10,000 residues and 512 drawable fragments. The engine additionally caps spline samples, track count, clipping and path-processing work. Oversized drawings fail with a message instead of silently reducing quality. Browser jobs time out after two minutes.
- No multi-structure alignment, G-code, file persistence or account system in this first version. Mol* remains available for future alignment features.

## Repository layout

```text
app/                    Vue UI, local parser, worker and state lifecycle
crates/ribbon-core/      Pure Rust renderer, WASM boundary and native tests
public/examples/        Small synthetic PDB/mmCIF examples and provenance
public/templates/       Standalone preview-only coaster guide
public/wasm/            Generated wasm-pack output (ignored)
scripts/                WASM build, smoke check and fixture generation
tests/                  Parser, lifecycle, export and real-WASM integration tests
docs/                   Implementation plan and validation notes
output/                 Existing local structures/renders (ignored, not moved)
```

See [the implementation plan](docs/web-plan.md) for the architecture and milestone scope, and [validation notes](docs/web-validation.md) for verified checks and rendering limitations.

## Checks and Git hooks

```bash
pnpm build:wasm
pnpm test:wasm
pnpm check              # lint, typecheck, runtime tests
pnpm fmt:check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm build
```

Real-WASM Vitest cases require `pnpm build:wasm`; they are skipped if the generated module is absent. CI builds WASM before running them. Native tests cover spline interpolation, interval clipping, stroke continuity, physical dimensions and workload limits, using small committed synthetic fixtures.

`simple-git-hooks` runs lint/format checks before commits and `commitlint` enforces Conventional Commits. Use `pnpm fmt` to format application/config files. Build artifacts, local structure bundles and secrets are ignored and must not be force-added.
