import { readFile } from "node:fs/promises";
import { describe, expect, it, vi } from "vitest";
import { parseStructure } from "../app/lib/parser";
import { LIMITS } from "../app/lib/limits";
import { pdbAtom, syntheticResidues } from "../scripts/generate-fixtures.mjs";

const fixture = (name: string) =>
  readFile(new URL(`../public/examples/${name}`, import.meta.url), "utf8");
function protein(
  count = 16,
  modify?: (
    seq: number,
    atom: string,
  ) => { point?: number[]; alt?: string; occupancy?: number; skip?: boolean; insertion?: string },
) {
  let serial = 0;
  return (
    syntheticResidues("H", count)
      .flatMap((residue, r) =>
        Object.entries(residue).flatMap(([atom, original]) => {
          const m = modify?.(r + 1, atom) ?? {};
          return m.skip
            ? []
            : [
                pdbAtom(
                  ++serial,
                  atom,
                  r + 1,
                  "A",
                  m.point ?? original,
                  m.alt,
                  m.occupancy,
                  m.insertion,
                ),
              ];
        }),
      )
      .join("\n") + "\nEND\n"
  );
}
describe("local Mol* protein parser", () => {
  it.each(["synthetic.pdb", "synthetic.cif"])(
    "computes H and E from unannotated %s coordinates",
    async (name) => {
      const s = await parseStructure(await fixture(name), name);
      expect(s.residueCount).toBe(32);
      expect(s.segments).toHaveLength(2);
      expect(s.segments[0]!.residues.filter((r) => r.kind === "H").length).toBeGreaterThan(6);
      expect(s.segments[1]!.residues.filter((r) => r.kind === "E").length).toBeGreaterThan(6);
      s.segments[0]!.residues[0]!.carbonyl.forEach((v, i) =>
        expect(v).toBeCloseTo([0, 0.6, 0.9][i]!, 5),
      );
      expect(s.warnings.join(" ")).toMatch(/computed locally.*not DSSP/);
      expect(s.chains.map((c) => c.id)).toEqual(
        name.endsWith("cif") ? ["helix-original", "strand.original"] : ["H", "E"],
      );
    },
  );
  it("preserves blank author chain IDs instead of replacing them with generated label IDs", async () => {
    const blank = protein()
      .split("\n")
      .map((line) => (line.startsWith("ATOM") ? line.slice(0, 21) + " " + line.slice(22) : line))
      .join("\n");
    const s = await parseStructure(blank, "blank-chain.pdb");
    // PDB author IDs are one-character fields; a blank is literally a space.
    expect(s.chains.map((c) => c.id)).toEqual([" "]);
    expect(s.segments[0]!.chainId).toBe(" ");
  });
  it("excludes non-polymer amino-acid ligands even when they contain backbone atom names", async () => {
    const source = await fixture("synthetic.cif");
    const entities = "loop_\n_entity.id\n_entity.type\n1 polymer\n2 polymer\n3 non-polymer\n";
    const ligand = Object.entries(syntheticResidues("H", 1)[0]!)
      .map(
        ([atom, point], i) =>
          `HETATM ${1000 + i} ${atom[0]} ${atom} . ALA LIG 3 . ? ${point.join(" ")} 1 20 1 ALA ligand ${atom} 1`,
      )
      .join("\n");
    const s = await parseStructure(
      source.replace("loop_", entities + "loop_") + ligand + "\n",
      "ligand.cif",
    );
    expect(s.residueCount).toBe(32);
    expect(s.chains.map((c) => c.id)).not.toContain("ligand");
  });
  it("uses only the first model", async () => {
    const body = protein().replace(/\nEND\n$/, "");
    const s = await parseStructure(
      `MODEL        1\n${body}\nENDMDL\nMODEL        2\n${body}\nENDMDL\nEND\n`,
      "two.pdb",
    );
    expect(s.residueCount).toBe(16);
    expect(s.warnings.join(" ")).toMatch(/first molecular model/);
  });
  it("splits incomplete backbones and excludes short fragments", async () => {
    const s = await parseStructure(
      protein(16, (seq, atom) => ({ skip: seq === 9 && atom === "O" })),
      "gaps.pdb",
    );
    expect(s.segments.map((seg) => seg.residues.length)).toEqual([8, 7]);
    expect(s.warnings.join(" ")).toMatch(/1 incomplete/);
    await expect(parseStructure(protein(3), "tiny.pdb")).rejects.toThrow(/at least four/);
  });
  it("splits CA gaps, numbering gaps and duplicate CA instead of bridging them", async () => {
    const body = protein(16, (seq, atom) =>
      seq > 8
        ? {
            point: syntheticResidues("H")[seq - 1]![atom as "N" | "CA" | "C" | "O"].map(
              (v: number, i: number) => v + (i === 2 ? 20 : 0),
            ),
          }
        : {},
    );
    const s = await parseStructure(body, "gap.pdb");
    expect(s.segments.map((seg) => seg.residues.length)).toEqual([8, 8]);
    const numbering = protein()
      .split("\n")
      .map((line) =>
        line.startsWith("ATOM") && Number(line.slice(22, 26)) > 8
          ? line.slice(0, 22) + String(Number(line.slice(22, 26)) + 2).padStart(4) + line.slice(26)
          : line,
      )
      .join("\n");
    expect((await parseStructure(numbering, "numbering.pdb")).segments).toHaveLength(2);
  });
  it("selects a complete high-occupancy alt without mixing incomplete conformers", async () => {
    const source = protein();
    const ca = source.split("\n").find((line) => line.slice(12, 16).trim() === "CA")!;
    const altA = ca.slice(0, 16) + "A" + ca.slice(17, 54) + "  0.20" + ca.slice(60);
    const point = syntheticResidues("H")[0]!.CA.map((v: number) => v + 0.05);
    const altB = pdbAtom(999, "CA", 1, "A", point, "B", 0.8);
    const s = await parseStructure(source.replace(ca, `${altA}\n${altB}`), "alt.pdb");
    s.segments[0]!.residues[0]!.ca.forEach((v, i) => expect(v).toBeCloseTo(point[i]!, 5));
    expect(s.warnings.join(" ")).toMatch(/alternate-location/);
    expect(s.segments[0]!.residues.some((r) => r.kind === "H")).toBe(true);
  });
  it("ignores water and ligands, and rejects unreadable/non-protein inputs", async () => {
    const body = await fixture("synthetic.pdb");
    const water = "HETATM  999  O   HOH W   1       0.000   0.000   0.000  1.00 20.00           O";
    expect(
      (await parseStructure(body.replace("END", water + "\nEND"), "with-water.pdb")).residueCount,
    ).toBe(32);
    await expect(parseStructure("not a molecule", "bad.pdb")).rejects.toThrow();
    await expect(parseStructure("data_empty\n", "bad.cif")).rejects.toThrow(/atom_site/);
    await expect(parseStructure("hi", "a.exe")).rejects.toThrow(/Choose a/);
    await expect(parseStructure(" ".repeat(LIMITS.fileBytes + 1), "large.pdb")).rejects.toThrow(
      /20 MiB/,
    );
  });
  it("surfaces computation errors instead of assigning every residue to loops", async () => {
    const computed =
      await import("molstar/lib/mol-model-props/computed/secondary-structure/zhang-skolnik");
    const spy = vi
      .spyOn(computed, "computeUnitZhangSkolnik")
      .mockRejectedValueOnce(new Error("assignment unavailable"));
    try {
      await expect(parseStructure(protein(), "failure.pdb")).rejects.toThrow(
        /computation failed.*No loop-only fallback/,
      );
    } finally {
      spy.mockRestore();
    }
  });
});
