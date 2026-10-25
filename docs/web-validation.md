# Web implementation validation

## Performance follow-up (current)

The persistent worker/WASM, bounded latest-request-wins scheduler, prepared
geometry/PCA cache, options-only boundary, exact color fast path and actual-3D
interaction preview are implemented. See [architecture, timings, benchmark method
and exact-output validation](web-performance.md). This supersedes the initial
cold-worker/dragging behavior described by the historical sections below.

- `pnpm check`: **63 tests passed**, including generated-WASM cases (none skipped),
  lint with zero warnings/errors and app/test typechecking.
- `cargo fmt --all -- --check`, clippy with `-D warnings` and **32 native tests**
  passed. Actual `pnpm build:wasm` / `pnpm test:wasm`, formatting, production build
  and Pages generation were checked; retained upstream build notices are non-failing.
- **74 real-structure old/new stateless/session cases** and **34 production-browser
  SVGs** matched exactly. No DSSP code or labels were changed; 1EZG still has
  H0/E38/C126 and broad sheets. Empty/reordered/duplicate selections, pen extremes,
  paper, color merge/split, travel ordering and invalidation are covered.
- Browser checks passed persistent module/structure reuse, coordinate-free warm
  messages, bounded rapid changes, continuous preview and exact release, stale
  SVG/PNG prevention, real downloads, hard cancellation/rehydration, injected
  worker-error recovery and replacement during active work. Mobile 390 × 844
  checks included real touch, touch cancellation/lost capture, keyboard reset,
  drawer/focus and top-right repository link. No browser/HTTP errors or uploads
  occurred in the local static Pages run; screenshots/exported PNG were opened.
  Fresh-cache isolated development loading also passed with no document or
  dependency-discovery reload. PNG pixels matched the exported exact SVG raster.

This section records local validation; remote workflow/public-site results are
reported separately after publication. The timings are load-sensitive, not a
promise of realtime exact rendering. No reference-language behavior was changed.

## Hydrogen-bond DSSP correction (preceding validation)

The CA-geometry assignment in the initial validation below is **superseded**.
The current worker computes hydrogen-bond DSSP using Mol* stages with explicit
proline chemistry, peptide breaks and shared inter-chain contacts. See
[algorithm details, exact counts and investigated oracle differences](web-dssp.md).
No Rust rendering/camera/pen-width behavior was changed.

Locally validated with Node 24.21, pnpm 11.22, Rust 1.96 and headless Chrome:

- `pnpm build:wasm` and `pnpm test:wasm`: actual generated web-target module passed.
- `pnpm check`: lint/typecheck and **42 tests passed**, including actual parser →
  WASM integration (none skipped). New public scientific fixtures replace
  illustrative CA-geometry assumptions; continuous beta runs, H/G simplification,
  PRO donors, cross-chain/short-fragment contacts, coherent alternatives, gaps,
  work limits and assignment failure are covered. Cancellation/lifecycle tests
  remain passing.
- `pnpm fmt:check`, `pnpm build` and **`GITHUB_PAGES=true pnpm generate`** passed.
  The Pages preset/base path remains `/ribbon-protein/`; Vite Environment API and
  startup prebundling include the actual DSSP imports.
- Chrome passed real first-file 1EZG mmCIF loading in a fresh development app
  (isolated source copy to avoid interfering with the user's existing server),
  then 1TEN/1MBN PDB and 1CRN mmCIF loading. No mid-calculation document reload or
  newly discovered dependency reload occurred. A generated Pages-base-path build
  served by a plain static file server passed the same checks.
- Browser-observed worker results matched the documented H/E/C counts, including
  1EZG **0/38/126** and all twelve continuous EEE runs. Real SVG/PNG downloads,
  line-only SVG, rotation/stale-export blocking, hard cancellation, invalid-file
  recovery and the offline physical 1CRN demo passed. No upload/remote calculation
  requests, browser errors or HTTP error responses occurred.
- At **390 × 844** with touch/mobile emulation, real 1EZG renders without overflow;
  settings expose the updated DSSP message, Escape restores focus, and the MDI
  GitHub link remains in the top-right toolbar. Sidebar attribution is unchanged.
- Matched-view WASM before/after images for **1EZG, 1TEN, 1CRN and 1MBN** were
  opened and visually inspected. CA/carbonyl arrays, camera and options were
  asserted equal; only SSE labels changed. 1EZG's broad, spreading sheet sections
  visibly return (675 → 955 strokes; 20.22 → 24.21 m). Its default PCA orientation
  still differs from the legacy reference; that is deliberately not "fixed" here.
  Browser/export PNGs were also opened, including mobile controls.

Artifacts and scripts for this correction are under `/tmp/ribbon-dssp-validate/`,
not committed renders or local input bundles. Upstream sourcemap and SPA
prerender warnings remain non-failing. Rust and legacy behavior were untouched;
the historical native/reference test results below were not re-run for this fix.
Remote CI/Pages results are reported separately after the authorized publication;
this section records local validation rather than pre-claiming remote success.

## Initial implementation baseline (historical)

The following records the initial implementation's local checks on 2026-10-25,
before the DSSP correction and subsequent GitHub Pages publication.

### Automated checks

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

### Browser checks (initial baseline)

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

### Python comparison and deliberate differences (initial baseline)

1TEN contains 89 drawable residues. Using the Python-derived DSSP labels as the Rust input isolates geometry from assignment differences:

| Pipeline | Pen strokes | Drawing length |
| --- | ---: | ---: |
| Python reference, default A4 | 608 | 37.14 m |
| Rust with the same normalized DSSP-labelled backbone | 591 | 36.24 m |
| Browser: Mol* Zhang–Skolnick assignment + Rust | 486 | 42.09 m |

The common ribbon/loop style is retained, but **this is not exact output parity**. Rust uses a deterministic PCA sign convention, unrounded intermediate coordinates and fitting based on actual blended outlines with half-pen allowance. This can change the initial view, visibility, track count and joined paths compared with Python's rounded intermediate SVG. The initial Mol* CA-geometry assignments further changed ribbon boundaries; that algorithm and its old UI/README warning are now superseded by the hydrogen-bond DSSP correction above.

## Follow-up scope

- **Completed:** hydrogen-bond DSSP in the browser, preserving residue identity and inter-chain contacts. Remaining implementation-specific boundaries/bulges are documented in [DSSP notes](web-dssp.md).
- **Completed:** persistent worker/WASM, bounded scheduling and caches, options-only warm messages, exact color fast path and actual-3D preview with exact-final-only exports. See [performance notes](web-performance.md). Explicit hard cancellation still drops worker-local caches; normal option edits do not.
- Add a maintained automated browser suite and test Firefox/Safari; current browser validation is Chrome-based.
- Multi-structure alignment, biological assemblies, persistent projects and plotter-specific machine output remain outside this first version.
- At very small coaster margins, fitting is still to a rectangle, not geometrically clipped to the rounded substrate; the UI warns below 3.14 mm. Default 5 mm is conservative.
