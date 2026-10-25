# Web implementation validation

Validated locally on 2026-10-25 with Node 24.21, pnpm 11.22, Rust 1.96 and headless Chrome on Linux. These are local results, not a claim that remote CI has run. The GitHub workflow is configured; nothing was pushed.

## Automated checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 29 passed, no skipped tests |
| `pnpm build:wasm` / actual web-target WASM smoke | Passed |
| `pnpm check` | Lint and app/test typecheck passed; 35 tests passed with generated WASM present |
| `pnpm fmt:check` | Passed |
| `pnpm build` | Passed |
| `pnpm generate` | Passed |
| `.venv/bin/python -m unittest discover -s legacy/python -v` | 28 passed, no skipped tests |

Rust tests cover spline/PCHIP numerical parity with Python/SciPy, 96 Shapely clipping reference cases, degenerate panels, preserved stroke direction, joining/gaps, physical dimensions, option validation and workload limits. Runtime tests cover computed helix/sheet assignment, original chain IDs and conformers, stale jobs, cancellation/recovery, dimensions and safe exports. Real parser-to-WASM tests are skipped when the generated module is absent; it was present for this validation.

Non-failing upstream warnings remain: Nuxt's component-name plugin reports missing transformation sourcemaps during build; static generation notes that an `ssr: false` app does not prerender its content. Python MDTraj emits its existing dummy-unit-cell warning.

## Browser checks

Verified against the Nitro production build and against the generated assets served by a plain HTTP file server (no calculation backend). The runtime agent also checked the `/studio/` base-path deployment. The coordinator independently checked:

- Local real **1TEN mmCIF and PDB** loading, plus the bundled synthetic examples.
- Actual WASM-produced SVG paths, not placeholder images or raster rendering.
- Rotation changes the SVG; exports are disabled immediately while settings are stale.
- A3 landscape exports **420 × 297 mm**; coaster exports **93.14 × 93.14 mm**.
- Coaster preview uses the extracted 92.74 mm rounded-square guide with 10 mm radii. The guide and all shape/mask elements are absent from the exported plot.
- Pen-width extremes **0.1 mm / 2 mm** and **zero interior lines** produce valid output.
- Actual browser SVG and PNG downloads complete; SVG dimensions and PNG signature checked from the saved files.
- Invalid mmCIF produces an actionable error; loading a valid file recovers.
- No external resource requests, POSTs or molecular uploads observed in production/static browser runs.
- Nuxt development startup and first example load succeed without a mid-calculation reload. The explicit Vite Environment API setting and Mol* prebundling are required with the pinned Nuxt version.
- At **390 × 844** mobile dimensions, a real 1TEN render fits without horizontal overflow. Drawer focus wraps, background controls are inert, Escape closes it and returns focus. Screenshots were opened and visually inspected, not only captured.

Browser scripts, screenshots and downloaded test artifacts are local session artifacts under `/tmp/ribbon-web-20261025/`, not committed source or portable test fixtures. The normal automated suite is in `tests/` and `crates/ribbon-core/`.

## Python comparison and deliberate differences

1TEN contains 89 drawable residues. Using the Python-derived DSSP labels as the Rust input isolates geometry from assignment differences:

| Pipeline | Pen strokes | Drawing length |
| --- | ---: | ---: |
| Python reference, default A4 | 608 | 37.14 m |
| Rust with the same normalized DSSP-labelled backbone | 591 | 36.24 m |
| Browser: Mol* Zhang–Skolnick assignment + Rust | 486 | 42.09 m |

The common ribbon/loop style is retained, but **this is not exact output parity**. Rust uses a deterministic PCA sign convention, unrounded intermediate coordinates and fitting based on actual blended outlines with half-pen allowance. This can change the initial view, visibility, track count and joined paths compared with Python's rounded intermediate SVG. Mol*'s CA-geometry assignments further change ribbon boundaries; notably they are not hydrogen-bond DSSP. The application and README disclose this explicitly.

## Follow-up scope

- Evaluate hydrogen-bond DSSP in the browser if closer scientific/visual parity is required; preserve residue identity and inter-chain contacts in such an implementation.
- Reuse idle workers/cache geometry or add a cheaper interaction preview if full-quality rotation latency is too high. Current hard cancellation is correct but can incur worker startup costs.
- Add a maintained automated browser suite and test Firefox/Safari; current browser validation is Chrome-based.
- Multi-structure alignment, biological assemblies, persistent projects and plotter-specific machine output remain outside this first version.
- At very small coaster margins, fitting is still to a rectangle, not geometrically clipped to the rounded substrate; the UI warns below 3.14 mm. Default 5 mm is conservative.
