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
  it("reserves the thicker lipid stroke in page fitting only when lipids are selected", () => {
    const o = defaultRenderOptions();
    expect(o.lipidWidthMm).toBe(0.8);
    o.selectedChains = ["A"];
    expect(drawingPenWidth(o, mixed)).toBe(0.25);
    o.selectedChains.push("l1");
    expect(drawingPenWidth(o, mixed)).toBe(0.8);
    o.lipidWidthMm = 1.2;
    expect(drawingPenWidth(o, mixed)).toBe(1.2);
    o.lipidWidthMm = 0;
    expect(() => validateOptions(o, mixed)).toThrow(/Lipid stroke width/);
  });
});
