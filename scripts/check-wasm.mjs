import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import init, { render_json } from "../public/wasm/ribbon_core.js";
import { syntheticResidues } from "./generate-fixtures.mjs";

await init({
  module_or_path: await readFile(new URL("../public/wasm/ribbon_core_bg.wasm", import.meta.url)),
});
const residues = syntheticResidues("H").map((r) => ({
  ca: r.CA,
  carbonyl: r.O.map((v, i) => v - r.C[i]),
  kind: "H",
}));
const structure = {
  name: "wasm-smoke.pdb",
  segments: [{ chainId: "A", residues }],
  chains: [{ id: "A", residueCount: residues.length }],
  residueCount: residues.length,
  warnings: [],
};
const options = {
  paper: "a4",
  landscape: false,
  widthMm: 210,
  heightMm: 297,
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
  selectedChains: ["A"],
  chainColors: {},
};
const render = (changes = {}) =>
  JSON.parse(render_json(JSON.stringify({ structure, options: { ...options, ...changes } })));
const first = render();
assert(
  first.strokeCount > 0 && first.lengthMm > 0,
  "WASM must produce actual visible line geometry",
);
assert.equal(first.residueCount, 16);
assert.match(first.svg, /width="210mm" height="297mm"/);
assert.match(first.svg, /<path d="M/);
assert.doesNotMatch(first.svg, /<(image|mask|clipPath|text|polygon)\b/);
assert.notEqual(render({ rotation: [30, 20, 10] }).svg, first.svg);
const custom = render({
  paper: "custom",
  landscape: true,
  widthMm: 100,
  heightMm: 150,
  marginMm: 5,
});
assert.equal(custom.widthMm, 100, "final custom dimensions must never be swapped");
assert.equal(custom.heightMm, 150);
const empty = render({ selectedChains: [] });
assert.equal(empty.strokeCount, 0, "an empty chain selection must never select all");
assert.throws(() => render({ penWidthMm: 0 }));
console.log(
  JSON.stringify({
    wasm: "actual web-target module",
    strokes: first.strokeCount,
    lengthMm: first.lengthMm,
    custom: [custom.widthMm, custom.heightMm],
    emptySelectionStrokes: empty.strokeCount,
  }),
);
