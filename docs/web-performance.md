# Persistent rendering and interaction performance

## Scope and exact-output requirement

This follow-up optimizes the corrected hydrogen-bond-DSSP renderer, starting at
`deefab36f337bd7569005584c37fb04803e976ba`. It does not change parsing, DSSP,
residue chemistry, PCA conventions, full-quality sampling, ribbon shape, clipping,
joining, simplification, path ordering, page fitting or pen clearance. In
particular, 1EZG retains **H0/E38/C126**, twelve continuous EEE runs and its broad
sheet ribbons. The older cross-language comparison in [validation notes](web-validation.md)
is a separate issue: this optimization requires exact equality with the existing
Rust/WASM artwork, not a new approximation to it.

All molecular work remains in the browser. No input is sent to a service, and no
CDN/runtime calculation endpoint is required. GitHub Pages still uses
`/ribbon-protein/`, including worker, parser chunks, WASM and its timing helper.
Generated artifacts and local structure bundles remain ignored.

## Six changes

### 1. Persistent worker and initialized WASM instance

`app/lib/worker-client.ts` creates a worker lazily and normally keeps it alive.
The worker lazily imports the molecular parser and initializes one WASM module
promise. A normalized structure and its Rust `RenderSession` survive subsequent
renders. This reuses the **initialized instance**, not merely cached HTTP bytes.
Replacing an idle structure frees its old Rust session. The explicit Nuxt Mol*
prebundling stays in place: lazy parser loading must not cause the first file to
trigger a Vite dependency-discovery page reload.

### 2. Bounded latest-request-wins scheduling

There is at most **one dispatched job and one newest pending job**. Superseded
promises settle with `AbortError`; old replies cannot update the current UI.
Normal option invalidation does not terminate synchronous work already executing
in WASM. Its reply releases the active slot and dispatches only the latest options.
Job IDs, worker identity, structure revision and studio epochs reject stale events.

Explicit **Cancel**, active file replacement, a worker crash, fatal WASM trap,
active timeout or disposal terminates the worker. The next request creates a new
one. The 120-second timeout starts at dispatch, not while a request waits in the
pending slot. Construction/post failures, errors, disposal and replacement settle
both active and queued callers; no unbounded task queue is retained.

### 3. Bounded prepared geometry and PCA

`crates/ribbon-core/src/lib.rs` keeps one normalized structure and an LRU of at
most **two** prepared variants. A variant is keyed by sample count and selected
segment indices in source order (duplicate/reordered chain selection does not
repeat geometry). It retains smoothing/interpolation, ribbon frames, width/alpha
profiles, full-resolution samples and PCA basis. Rotation applies to this basis
without repeating preparation. A new structure creates a new session and cannot
reuse geometry solely because its chain IDs match.

There is also **one last exact joined-view cache**, not an all-results memo table.
Existing input/work limits remain enforced, including on cache hits. Memory is
bounded by these fixed cache capacities and the existing structure/sample limits;
this is not an attempt to cache every rotation.

### 4. Options-only warm requests

After parsing, the worker owns its normalized structure and Rust session. Warm
render messages contain a structure revision, options and quality, not repeated
coordinates. After hard cancellation/crash, the client can rehydrate the new worker
once from its retained normalized structure; atom parsing and DSSP are not repeated.
Changing files advances the revision and discards structure-dependent state.

### 5. Exact color-only fast path

The last joined view is keyed by **every render option except chain colors**.
A color edit reuses per-segment joined paths, then rebuilds color groups and reruns
the original ordering/statistics/SVG finish step. This is not string recoloring:
splitting or merging color groups can change traversal, travel distance, stroke
order and floating-point accumulation order. Those semantics must stay exact.
Pen width remains in the view key because it affects fitting and half-pen edge
clearance. Changing it is never treated as color-only.

### 6. Actual-3D preview, unchanged exact final

A small scene of actual prepared 3D ribbon samples and the full-sample PCA basis
is transferred once per structure/sample/selection key. It targets 800 samples
plus per-segment endpoints. `app/lib/interaction-preview.ts` projects this scene
on the main thread during dragging, so it can display while synchronous exact
WASM work is still busy. Updates are frame-coalesced at 16 ms, **not** trailing
input-debounced; a continuous pointer stream must not starve every preview.

The interaction view draws sparse contours without hidden-line removal, dense
interior tracks, joining or plotter ordering. It is not a rotation of an old 2D
image. If the prepared scene is unavailable, a bounded worker contour preview is
the fallback. Preview state is separate from the last exact result and cannot
poison the exact joined cache. Empty selections and fragment boundaries remain
explicit. Displayed dimensions, colors and rotation use the current options.

**Preview · exports paused** and **Refining** identify approximate/unfinished
states. Pointer release, touch end/cancel, lost capture or a discrete keyboard edit
requests the latest exact drawing. Explicit Cancel/reset/file replacement settles
captured gestures too. SVG and PNG require a current exact final; both export
boundaries reject preview quality, and asynchronous PNG completion rechecks
staleness. A late preview can never overwrite a newer final or another structure.

## Cache dependency summary

| Change | Prepared geometry/PCA | Joined view | Main-thread preview scene |
| --- | --- | --- | --- |
| Rotation | Reuse | Recompute | Reuse 3D samples; reproject |
| Colors only | Reuse | Reuse paths; regroup/order/serialize | Reuse; recolor |
| Lines, pen width, page/margin, loop/border controls, direction | Reuse | Recompute | Reuse contours; apply current fit/style |
| Samples or selected segment set | Lookup/create bounded variant | Recompute | Request matching scene |
| New structure | Drop old session | Drop | Drop |
| Hard cancellation/crash | Rebuild after rehydration | Drop | Matching client scene may remain usable |

Conservative joined-view invalidation also occurs if otherwise equivalent option
representations differ. No approximation is used to accelerate an exact final.

## What the timings mean

The stats bar's **Response** is options/file-to-current-result latency, including
input debounce, queue wait, worker/module startup, parsing when applicable,
structure handoff, WASM calls and result delivery. After the SVG image loads,
`visibleMs` also includes image load/decode and a next-animation-frame estimate.
This is an approximate presentation boundary, **not a hardware paint timestamp**.
While stale, the muted statistics belong to the last exact final, not the sparse
preview. Hover exposes the separate WASM call and stage timings.

Diagnostics distinguish parser/DSSP, module initialization, structure loading,
preparation, projection, clipping, joining, finish and image delay. `elapsedMs`
for finals is the WASM call(s) plus JSON decoding, not end-to-end latency. Stage
measurements are wall-clock intervals and include scheduling pauses. `startupMs`
covers the worker's first-use interval and overlaps parser/module/load stages;
it must not be added to them. Per-worker initialization/load/parse counters and
cache-hit flags distinguish reuse from a fast HTTP cache hit.

## Measured before/after

Local measurement on 2026-10-25: Linux x86_64, Intel i7-1185G7, headless
Chrome 155.0.8059.84, 1440 × 1000. Two actual static Pages builds were served over
HTTP: the untouched baseline above and the optimized build. Each structure had
its own fresh browser context with HTTP cache disabled for the first load; cache
was then enabled for warm requests. The baseline and optimized pages alternated
order for each of **16 rotations, 8 color edits and 6 real pointer drags per
structure**. Both used identical coordinates/options, actual generated WASM and
current-image decoding, not mocked workers or trivial synthetic geometry.

**Important limits:** unrelated desktop workloads caused substantial, variable
CPU contention. These are observed wall times, not a controlled performance
claim or a guaranteed speedup. Cold values are one sample each. Small-sample p95
uses the nearest observed rank (for 16 observations, the maximum); the median is
the lower observed middle rank. UI timings include the 180 ms discrete-edit
debounce and image decoding. Drag-start timings include automation dispatch;
release timings start at pointer release. No machine/process isolation or CPU
throttling was applied. The cache/message counters and exact-output comparisons
are stronger architectural evidence than any single noisy timing.

All values below are **milliseconds**, before → after. Paired values are
**median / p95** unless noted.

| Measurement | 1EZG, 164 residues | 1TIM, 494 residues |
| --- | ---: | ---: |
| First file → exact displayed result, single cold sample | 5458 → 3249 | 5736 → 5031 |
| Warm rotation → exact displayed result | 2385 / 3513 → 515 / 1814 | 4304 / 5080 → 2178 / 2731 |
| Warm rotation WASM call + JSON only | 787 / 1865 → 294 / 1166 | 2474 / 2896 → 1693 / 2016 |
| Color edit → exact displayed result | 789 / 3394 → 306 / 709 | 3845 / 4898 → 846 / 1021 |
| Color WASM call + JSON, median | 305 → 93 | 2334 → 424 |
| Drag release → exact displayed result | 3085 / 3916 → 1242 / 1492 | 3018 / 5384 → 1556 / 2204 |
| Drag start → first decoded approximate preview | None → 272 / 487 | None → 236 / 425 |
| Workers constructed across the sequence | 33 → 1 | 33 → 1 |
| Typical warm render message, JSON bytes | 24,931 → 447–450 | 73,356 → 447–450 |

Every optimized drag displayed a preview before release. A separate continuous
8 ms pointer-input smoke run displayed **17 preview frames**, first at **55 ms**
(measured after pointer-down, so not directly comparable with the table).
Normal rotations/colors/drags retained one module initialization, one structure
load and one parse per context. Warm parse/init/load times were zero; rotations
hit prepared geometry, and color edits hit joined paths with zero projection,
clipping and joining work. Warm messages contained no coordinates.

This does **not** make exact rendering realtime. Clipping, joining, color-group
ordering and SVG creation still dominate complex views; for example, one loaded
1TIM warm sample spent about 797 ms clipping, 460 ms joining and 509 ms finishing.
Initial preparation was only about 10–18 ms in the sampled cold requests. The
prepared cache alone does not explain all improvement: avoiding worker/module
reconstruction also retains engine warmup, and timings remain load-sensitive.
Color grouping/order/SVG are intentionally not skipped.

## Validation and reproduction

- Saved the original compiled WASM before editing. Compared **74 requests**
  (37 each for real 1EZG/1TIM): old vs new stateless JSON and cached-session results.
  SVG bytes, stroke count, drawing/travel statistics, dimensions, warnings and
  ordering matched exactly, excluding only the new timing wrapper. Cases include
  rotations, zero/max lines, samples 2/8/32, empty/subset/reordered/duplicate chains,
  paper/orientation/margins, pen extremes, loop/border controls, color split/merge
  and both direction modes. **34 production-browser SVGs** were also byte-identical.
  Four further smoke outputs (including a final after dragging and file replacement)
  matched the old WASM exactly.
- Native tests cover cache invalidation/eviction, replacement structures, option
  errors/recovery, preview/final separation and the existing numerical/physical
  regressions: **32 passed**. Lint/clippy use warnings-as-errors.
- Runtime tests: **63 passed**, including real generated WASM, scientific fixtures,
  scene bounds, active/newest scheduling, settlement, crash/fatal trap, timeout,
  disposal, rehydration, file/options races and stale/preview/PNG export guards.
  Generated-module tests were not skipped. The scientific multi-render matrix
  has an explicit 30-second timeout to avoid its former 5-second timeout on a
  heavily loaded host; its equality assertions are unchanged.
- Local Pages browser checks exercised 1EZG's broad sheets and H0/E38/C126,
  rapid edits (21 changes coalesced to two messages; at most one active job),
  continuous dragging, exact refinement, actual SVG/150-dpi PNG downloads,
  stale PNG completion, hard cancellation/rehydration, injected worker-error
  recovery and structure replacement during active rendering. No browser/HTTP
  errors or molecular uploads/remote calculation requests occurred.
- A fresh development server in an isolated source copy with a fresh Vite cache
  passed the same smoke checks: exactly one document request per page, no
  dependency-discovery reload during the first file, and no browser/HTTP errors.
  The copy used an isolated Nuxt build directory and an allowlist for symlinked
  dependencies; the checked-in startup prebundling is unchanged.
- At 390 × 844 with touch emulation: no horizontal overflow, accessible settings
  drawer/Escape focus return, top-right MDI repository link, real touch preview,
  touch cancellation/lost capture and keyboard Home exact reset. Screenshots and
  an exported PNG were opened and visually inspected. The downloaded PNG also
  matched a fresh rasterization of its exact SVG with zero differing RGBA channels
  across 8,706,856 channels (1241 × 1754 pixels).

The normal reproducible checks are the commands in the root README plus
`GITHUB_PAGES=true pnpm generate`. Real-WASM tests require building WASM first.
For a benchmark, archive the baseline and build both revisions with the same
compiler/toolchain, serve each at `/ribbon-protein/`, wrap `Worker` before app load
to record creations/messages/results, disable cache in fresh contexts, then drive
the same options with actual DOM input and pointer events. Wait for **Up to date**
and the current image's `decode()`; record previews from the image's
`data-quality="preview"`, not merely from worker replies. Check exact SVG hashes
before comparing time. Keep fixtures/toolchain/browser/load conditions in the
report, and distinguish one cold sample from warm distributions.

Session scripts, raw measurements, original WASM, screenshots and downloads are
local artifacts under `/tmp/ribbon-perf-20260721/`, not committed generated output
or local coordinate bundles. The directory name is only an artifact identifier.
Builds retain non-failing upstream Nuxt sourcemap/SPA-prerender notices and
wasm-pack metadata/version notices. Browser validation is Chrome-based, including
mobile emulation, not a claim of physical-device or Firefox/Safari coverage.
