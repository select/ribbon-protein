import { describe, expect, it } from "vitest";
import {
  colorMoleculeGroup,
  moleculeGroups,
  selectMoleculeGroup,
} from "../app/lib/molecule-groups";
import { drawingPenWidth, validateOptions } from "../app/lib/options";
import { defaultRenderOptions } from "../app/types/render";
import type { Molecule } from "../app/types/render";
import { structure } from "./helpers";

const molecules: Molecule[] = [
  { chainId: "l1", label: "PTY 1", category: "lipid", atoms: [[0, 0, 0]], bonds: [] },
  { chainId: "l2", label: "PTY 2", category: "lipid", atoms: [[2, 0, 0]], bonds: [] },
  { chainId: "other", label: "ligand", category: "molecule", atoms: [[4, 0, 0]], bonds: [] },
  { chainId: "water", label: "HOH", category: "water", atoms: [[6, 0, 0]], bonds: [] },
];
const mixed = {
  ...structure,
  molecules,
  chains: [...structure.chains, ...molecules.map((m) => ({ id: m.chainId, residueCount: 0 }))],
};

describe("molecule group controls", () => {
  it("groups all lipids distinctly from other molecules and water", () => {
    expect(moleculeGroups(molecules)).toEqual([
      { category: "lipid", label: "Lipids", ids: ["l1", "l2"] },
      { category: "molecule", label: "Other molecules", ids: ["other"] },
      { category: "water", label: "Water", ids: ["water"] },
    ]);
  });
  it("hides/shows every lipid without changing protein, ligand or water selection", () => {
    const o = defaultRenderOptions();
    o.selectedChains = ["A", "l1", "other"];
    selectMoleculeGroup(o, mixed, ["l1", "l2"], true);
    expect(o.selectedChains).toEqual(["A", "l1", "l2", "other"]);
    selectMoleculeGroup(o, mixed, ["l1", "l2"], false);
    expect(o.selectedChains).toEqual(["A", "other"]);
  });
  it("applies a group color to hidden lipids too and preserves other pen colors", () => {
    const o = defaultRenderOptions();
    o.selectedChains = ["A", "l1"];
    o.chainColors = { A: "black", other: "#123456", l1: "red" };
    colorMoleculeGroup(o, ["l1", "l2"], "#AABBCC");
    expect(o.chainColors).toEqual({ A: "black", other: "#123456", l1: "#aabbcc", l2: "#aabbcc" });
    expect(o.selectedChains).toEqual(["A", "l1"]);
  });
  it("keeps each group drawing style independent and validates hidden groups", () => {
    const o = defaultRenderOptions();
    o.selectedChains = ["A"];
    expect(o.moleculeStyles.lipid).toEqual({ lines: 0, bondWidthAngstrom: 0.4 });
    o.moleculeStyles.lipid.lines = 8;
    o.moleculeStyles.lipid.bondWidthAngstrom = 2;
    expect(o.lines).toBe(7);
    expect(o.moleculeStyles.molecule).toEqual({ lines: 1, bondWidthAngstrom: 0.5 });
    expect(defaultRenderOptions().moleculeStyles.lipid.lines).toBe(0);
    for (const lines of [-1, 1.5, 21, NaN]) {
      o.moleculeStyles.water.lines = lines;
      expect(() => validateOptions(o, mixed)).toThrow(/water interior lines/);
    }
    o.moleculeStyles.water.lines = 0;
    for (const width of [-0.1, 4.1, Infinity, NaN]) {
      o.moleculeStyles.water.bondWidthAngstrom = width;
      expect(() => validateOptions(o, mixed)).toThrow(/water bond width/);
    }
  });
  it("reserves the thicker lipid stroke in page fitting only when lipids are selected", () => {
    const o = defaultRenderOptions();
    expect(o.penWidthMm).toBe(0.4);
    expect(o.lipidWidthMm).toBe(o.penWidthMm);
    o.selectedChains = ["A"];
    expect(drawingPenWidth(o, mixed)).toBe(0.4);
    o.selectedChains.push("l1");
    expect(drawingPenWidth(o, mixed)).toBe(0.4);
    o.lipidWidthMm = 1.2;
    expect(drawingPenWidth(o, mixed)).toBe(1.2);
    o.selectedChains = ["A"];
    expect(drawingPenWidth(o, mixed)).toBe(0.4);
    o.lipidWidthMm = 0;
    expect(() => validateOptions(o, mixed)).toThrow(/Lipid stroke width/);
  });
});
