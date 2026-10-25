import type { RenderResult, StructureData } from "../app/types/render";
export const structure: StructureData = {
  name: "test.pdb",
  segments: [
    {
      chainId: "A",
      residues: Array.from({ length: 4 }, (_, i) => ({
        ca: [i * 3.5, 0, 0],
        carbonyl: [0, 1, 0],
        kind: "C",
      })),
    },
  ],
  chains: [{ id: "A", residueCount: 4 }],
  residueCount: 4,
  warnings: [],
};
export const result: RenderResult = {
  svg: '<svg xmlns="http://www.w3.org/2000/svg" width="210mm" height="297mm" viewBox="0 0 210 297" fill="none"><path d="M1 2L3 4" stroke="#000000"/></svg>',
  widthMm: 210,
  heightMm: 297,
  strokeCount: 1,
  lengthMm: Math.SQRT2 * 2,
  residueCount: 4,
  elapsedMs: 2,
  warnings: [],
};
export function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (cause: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
