// Run after wasm-pack build:
// node crates/ribbon-core/tests/wasm-smoke.mjs [path/to/request.json ...]
// Additional requests can have a sibling *-native.json for numeric comparison.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";

const wasmUrl = new URL("../../../public/wasm/ribbon_core_bg.wasm", import.meta.url);
const { default: init, render_json } = await import(
  new URL("../../../public/wasm/ribbon_core.js", import.meta.url)
);
await init({ module_or_path: await readFile(wasmUrl) });
const fixture = JSON.parse(
  await readFile(new URL("./fixtures/python-ribbon.json", import.meta.url), "utf8"),
);
const request = {
  structure: {
    name: "Actual WASM smoke",
    segments: [fixture.segment],
    chains: [{ id: "reference", residueCount: 12 }],
    residueCount: 12,
    warnings: [],
  },
  options: {
    paper: "a4",
    landscape: true,
    widthMm: 297,
    heightMm: 210,
    marginMm: 15,
    rotation: [0, 0, 0],
    lines: 7,
    penWidthMm: 0.25,
    samples: 16,
    fillLoops: true,
    loopSpacingMm: 0.15,
    borderPasses: 3,
    borderSpacingMm: 0.1,
    strokeDirection: "backbone",
    selectedChains: ["reference"],
    chainColors: { reference: "#aB1234" },
  },
};
function render(request) {
  return JSON.parse(render_json(JSON.stringify(request)));
}
function validate(result) {
  assert.deepEqual(Object.keys(result).sort(), [
    "elapsedMs",
    "heightMm",
    "lengthMm",
    "residueCount",
    "strokeCount",
    "svg",
    "warnings",
    "widthMm",
  ]);
  assert.ok(result.strokeCount > 0 && result.lengthMm > 0);
  assert.ok(Number.isFinite(result.lengthMm));
  assert.equal(result.elapsedMs, 0);
  assert.equal(result.svg.match(/<path /g).length, result.strokeCount);
  assert.ok(!/<(?:polygon|rect|circle|mask|clipPath|image|text)\b/.test(result.svg));
  assert.ok(!/NaN|Infinity/.test(result.svg));
}
const first = render(request);
validate(first);
assert.equal(first.widthMm, 297);
assert.equal(first.heightMm, 210);
assert.ok(first.svg.includes('stroke="#aB1234"'));
assert.equal(first.svg, render(request).svg);
const none = structuredClone(request);
none.options.selectedChains = [];
assert.equal(render(none).strokeCount, 0);
const mutations = [
  (r) => {
    r.options.samples = 0;
  },
  (r) => {
    r.options.widthMm = NaN;
  },
  (r) => {
    r.options.penWidthMm = 20;
  },
  (r) => {
    r.structure.segments[0].residues[1].ca = r.structure.segments[0].residues[0].ca;
  },
  (r) => {
    r.options.chainColors.reference = 'black" onload="alert(1)';
  },
];
for (const mutate of mutations) {
  const invalid = structuredClone(request);
  mutate(invalid);
  assert.throws(() => render(invalid));
  // A rejected request must not poison the WASM instance.
  assert.equal(render(request).svg, first.svg);
}
assert.throws(() => render_json("{invalid JSON"));
console.log(
  JSON.stringify({
    test: "compiled wasm boundary",
    strokes: first.strokeCount,
    lengthMm: first.lengthMm,
    invalidCases: mutations.length + 1,
  }),
);
for (const path of process.argv.slice(2)) {
  const request = JSON.parse(await readFile(path, "utf8"));
  const start = performance.now();
  const result = render(request);
  validate(result);
  const elapsedMs = performance.now() - start;
  let native;
  if (/-request\.json$/.test(path)) {
    try {
      native = JSON.parse(await readFile(path.replace(/-request\.json$/, "-native.json"), "utf8"));
    } catch {
      /* Native comparison is optional. */
    }
  }
  if (native) {
    assert.equal(result.residueCount, native.residueCount);
    assert.equal(result.strokeCount, native.strokeCount);
    // Native and WASM libm may make sub-micron total-length differences at
    // coincident panel boundaries; byte equality is not a parity requirement.
    assert.ok(
      Math.abs(result.lengthMm - native.lengthMm) < 0.001,
      `length delta: ${result.lengthMm - native.lengthMm}; WASM: ${result.lengthMm}, native: ${native.lengthMm}`,
    );
  }
  console.log(
    JSON.stringify({
      path,
      residues: result.residueCount,
      strokes: result.strokeCount,
      lengthMm: result.lengthMm,
      elapsedMs: Math.round(elapsedMs),
      nativeParity: Boolean(native),
    }),
  );
}
