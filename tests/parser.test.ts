import { readFile } from "node:fs/promises";
import { describe, expect, it, vi } from "vitest";
import { parseStructure } from "../app/lib/parser";
import { LIMITS } from "../app/lib/limits";
import type { StructureData } from "../app/types/render";
import { pdbAtom } from "../scripts/generate-fixtures.mjs";

const fixture = (name: string) =>
  readFile(new URL(`../public/examples/${name}`, import.meta.url), "utf8");
const reference = (id: string) =>
  readFile(new URL(`./fixtures/dssp/${id}-backbone.pdb`, import.meta.url), "utf8");
const crambin = await fixture("1crn-backbone.pdb");
const backboneNames = ["N", "CA", "C", "O"] as const;
// Real, physically consistent backbone for topology/conformer perturbations.
// ALA labels here are deliberate test mutations, not production normalization.
const points = Array.from(
  { length: 16 },
  (_, i) =>
    Object.fromEntries(
      backboneNames.map((atom) => {
        const line = crambin
          .split("\n")
          .find(
            (l) =>
              l.startsWith("ATOM") &&
              Number(l.slice(22, 26)) === i + 1 &&
              l.slice(12, 16).trim() === atom,
          )!;
        return [
          atom,
          [Number(line.slice(30, 38)), Number(line.slice(38, 46)), Number(line.slice(46, 54))],
        ];
      }),
    ) as Record<"N" | "CA" | "C" | "O", number[]>,
);
function protein(
  count = 16,
  modify?: (
    seq: number,
    atom: string,
  ) => { point?: number[]; alt?: string; occupancy?: number; skip?: boolean; insertion?: string },
) {
  let serial = 0;
  return (
    points
      .slice(0, count)
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
const counts = (s: StructureData) =>
  s.segments
    .flatMap((seg) => seg.residues)
    .reduce(
      (a, r) => {
        a[r.kind]++;
        return a;
      },
      { H: 0, E: 0, C: 0 },
    );
const labels = (s: StructureData) =>
  s.segments.map((seg) => seg.residues.map((r) => r.kind).join(""));

describe("local hydrogen-bond DSSP parser", () => {
  it.each(["1crn-backbone.pdb", "1crn-backbone.cif"])(
    "computes physical helix/sheet runs without annotations in %s",
    async (name) => {
      const s = await parseStructure(await fixture(name), name);
      expect(s.residueCount).toBe(46);
      expect(s.segments).toHaveLength(1);
      expect(counts(s)).toEqual({ H: 22, E: 4, C: 20 });
      expect(labels(s)[0]).toContain("HHHHHHHHHHH");
      expect(labels(s)[0]!.slice(41, 44)).toBe("HHH"); // DSSP G (3-10) simplifies to H
      expect(s.warnings.join(" ")).toMatch(/hydrogen-bond DSSP/);
      expect(s.chains.map((c) => c.id)).toEqual(
        name.endsWith("cif") ? ["crambin.original"] : ["A"],
      );
    },
  );
  it("recovers 1EZG's repeated broad beta runs, not isolated labels or forced sheets", async () => {
    const s = await parseStructure(await reference("1ezg"), "1ezg.pdb");
    expect(s.residueCount).toBe(164);
    expect(s.chains.map((c) => c.id)).toEqual(["A", "B"]);
    expect(counts(s)).toEqual({ H: 0, E: 38, C: 126 });
    for (const segment of labels(s)) {
      // Public coordinates + independently generated MDTraj simplified DSSP.
      expect(segment).toBe(
        "CCECCCCCCCCCCEEECCCCCCCCCEEECCCCCCCCCEEECCCCCCCCCEEECCCCCCCCCEEECCCCCCCCCEEECCCCCC",
      );
      expect(segment.match(/E{3,}/g)).toHaveLength(6);
    }
  });
  it.each([
    ["1ten", { H: 0, E: 51, C: 38 }, "EEEEEEEEEE"],
    ["1mbn", { H: 118, E: 0, C: 35 }, "HHHHHHHHHHHHHHH"],
  ] as const)(
    "recognizes independent %s reference with continuous SSE runs",
    async (id, expected, run) => {
      const s = await parseStructure(await reference(id), `${id}.pdb`);
      expect(counts(s)).toEqual(expected);
      expect(labels(s).join(" ")).toContain(run);
    },
  );
  it("keeps inter-chain hydrogen bonds rather than assigning each short strand independently", async () => {
    const strands =
      crambin
        .split("\n")
        .filter(
          (l) =>
            l.startsWith("ATOM") &&
            (Number(l.slice(22, 26)) <= 4 ||
              (Number(l.slice(22, 26)) >= 32 && Number(l.slice(22, 26)) <= 35)),
        )
        .map((l) => (Number(l.slice(22, 26)) >= 32 ? l.slice(0, 21) + "B" + l.slice(22) : l))
        .join("\n") + "\nEND\n";
    const s = await parseStructure(strands, "two-strands.pdb");
    expect(s.segments.map((seg) => seg.residues.length)).toEqual([4, 4]);
    expect(counts(s).E).toBe(4);
    for (const chain of ["A", "B"]) {
      const isolated = strands
        .split("\n")
        .filter((l) => l.startsWith("ATOM") && l[21] === chain)
        .join("\n");
      expect(counts(await parseStructure(isolated, "one-strand.pdb")).E).toBe(0);
    }
  });
  it("uses sub-four-residue fragments as sheet contacts without drawing them", async () => {
    const strands = crambin
      .split("\n")
      .filter(
        (l) =>
          l.startsWith("ATOM") &&
          (Number(l.slice(22, 26)) <= 4 ||
            (Number(l.slice(22, 26)) >= 32 && Number(l.slice(22, 26)) <= 34)),
      )
      .map((l) => (Number(l.slice(22, 26)) >= 32 ? l.slice(0, 21) + "B" + l.slice(22) : l))
      .join("\n");
    const s = await parseStructure(strands, "short-contact.pdb");
    expect(s.residueCount).toBe(4);
    expect(counts(s).E).toBe(1);
    expect(s.warnings.join(" ")).toMatch(/3 residues.*DSSP contacts only/);
  });
  it("bounds dense DSSP contact-search work with an actionable error", async () => {
    const computed =
      await import("molstar/lib/mol-model-props/computed/secondary-structure/dssp/trace-lookup");
    const original = computed.calcUnitProteinTraceLookup3D;
    const spy = vi
      .spyOn(computed, "calcUnitProteinTraceLookup3D")
      .mockImplementationOnce((...args) => {
        const lookup = original(...args);
        return {
          ...lookup,
          find: () => ({ ...lookup.find(0, 0, 0, 9), count: LIMITS.dsspPairs + 1 }),
        };
      });
    try {
      await expect(parseStructure(crambin, "dense.pdb")).rejects.toThrow(
        /DSSP contact-search work limit.*No loop-only fallback/,
      );
    } finally {
      spy.mockRestore();
    }
  });
  it("retains PRO chemistry: it cannot donate an amide hydrogen", async () => {
    const original = await parseStructure(crambin, "proline.pdb");
    const mutant = crambin
      .split("\n")
      .map((l) =>
        l.startsWith("ATOM") && l.slice(17, 20) === "PRO"
          ? l.slice(0, 17) + "ALA" + l.slice(20)
          : l,
      )
      .join("\n");
    const alanine = await parseStructure(mutant, "alanine-mutation.pdb");
    expect(counts(original).H).toBe(22);
    expect(counts(alanine).H).toBe(23);
    // A fictitious PRO 19 NH would incorrectly extend the preceding helix to 18.
    expect(labels(original)[0]![17]).toBe("C");
    expect(labels(alanine)[0]![17]).toBe("H");
  });
  it("preserves blank author chain IDs", async () => {
    const blank = protein()
      .split("\n")
      .map((line) => (line.startsWith("ATOM") ? line.slice(0, 21) + " " + line.slice(22) : line))
      .join("\n");
    const s = await parseStructure(blank, "blank-chain.pdb");
    expect(s.chains.map((c) => c.id)).toEqual([" "]);
  });
  it("excludes non-polymer amino-acid ligands even with backbone atom names", async () => {
    const source = await fixture("1crn-backbone.cif");
    const entities = "loop_\n_entity.id\n_entity.type\n1 polymer\n3 non-polymer\n";
    const ligand = Object.entries(points[0]!)
      .map(
        ([atom, point], i) =>
          `HETATM ${1000 + i} ${atom[0]} ${atom} . ALA LIG 3 . ? ${point.join(" ")} 1 20 1 ALA ligand ${atom} 1`,
      )
      .join("\n");
    const s = await parseStructure(
      source.replace("loop_", entities + "loop_") + ligand + "\n",
      "ligand.cif",
    );
    expect(s.residueCount).toBe(46);
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
  it("splits incomplete backbones and excludes short drawable fragments", async () => {
    const s = await parseStructure(
      protein(16, (seq, atom) => ({ skip: seq === 9 && atom === "O" })),
      "gaps.pdb",
    );
    expect(s.segments.map((seg) => seg.residues.length)).toEqual([8, 7]);
    expect(s.warnings.join(" ")).toMatch(/1 incomplete/);
    await expect(parseStructure(protein(3), "tiny.pdb")).rejects.toThrow(/at least four/);
  });
  it("splits CA, numbering and peptide gaps without inventing a helix across a break", async () => {
    const moved = protein(16, (seq, atom) =>
      seq > 8 ? { point: points[seq - 1]![atom as "N"].map((v, i) => v + (i === 2 ? 20 : 0)) } : {},
    );
    expect((await parseStructure(moved, "gap.pdb")).segments.map((s) => s.residues.length)).toEqual(
      [8, 8],
    );
    const numbering = protein()
      .split("\n")
      .map((l) =>
        l.startsWith("ATOM") && Number(l.slice(22, 26)) > 8
          ? l.slice(0, 22) + String(Number(l.slice(22, 26)) + 2).padStart(4) + l.slice(26)
          : l,
      )
      .join("\n");
    const split = await parseStructure(numbering, "numbering.pdb");
    expect(split.segments).toHaveLength(2);
    // Original physical alpha helix spans residues 7–17; no helix may cross 8/9.
    expect(labels(split)[0]!.slice(-2)).toBe("CC");
    const peptide = protein(16, (seq, atom) =>
      seq === 9 && atom === "N" ? { point: points[8]!.N.map((v) => v + 10) } : {},
    );
    expect((await parseStructure(peptide, "peptide-gap.pdb")).segments).toHaveLength(2);
    const duplicate = protein(16, (seq, atom) =>
      seq === 9 && atom === "CA" ? { point: points[7]!.CA } : {},
    );
    expect((await parseStructure(duplicate, "duplicate-ca.pdb")).warnings.join(" ")).toMatch(
      /gaps/,
    );
  });
  it("selects complete high-occupancy alternates with shared blank atoms and lexical ties", async () => {
    const source = protein();
    const ca = source.split("\n").find((line) => line.slice(12, 16).trim() === "CA")!;
    const altA = ca.slice(0, 16) + "A" + ca.slice(17, 54) + "  0.20" + ca.slice(60);
    const point = points[0]!.CA.map((v) => v + 0.05);
    const altB = pdbAtom(999, "CA", 1, "A", point, "B", 0.8);
    const s = await parseStructure(source.replace(ca, `${altA}\n${altB}`), "alt.pdb");
    s.segments[0]!.residues[0]!.ca.forEach((v, i) => expect(v).toBeCloseTo(point[i]!, 5));
    expect(s.warnings.join(" ")).toMatch(/alternate-location/);
    expect(counts(s).H).toBeGreaterThan(6);
    const tied = await parseStructure(
      source.replace(ca, `${altB.replace("  0.80", "  0.20")}\n${altA}`),
      "tie.pdb",
    );
    tied.segments[0]!.residues[0]!.ca.forEach((v, i) =>
      expect(v).toBeCloseTo(points[0]!.CA[i]!, 5),
    );
    // Complementary incomplete conformers must not be combined into a backbone.
    const incomplete = protein(16, (seq, atom) =>
      seq === 9 ? { alt: atom === "N" || atom === "CA" ? "A" : "B" } : {},
    );
    expect(
      (await parseStructure(incomplete, "incomplete-alt.pdb")).segments.map(
        (seg) => seg.residues.length,
      ),
    ).toEqual([8, 7]);
  });
  it("ignores water/ligands and rejects unreadable, non-protein and oversized inputs", async () => {
    const water = "HETATM  999  O   HOH W   1       0.000   0.000   0.000  1.00 20.00           O";
    expect(
      (await parseStructure(crambin.replace("END", water + "\nEND"), "water.pdb")).residueCount,
    ).toBe(46);
    await expect(parseStructure("not a molecule", "bad.pdb")).rejects.toThrow();
    await expect(parseStructure("data_empty\n", "bad.cif")).rejects.toThrow(/atom_site/);
    await expect(parseStructure("hi", "a.exe")).rejects.toThrow(/Choose a/);
    await expect(parseStructure(" ".repeat(LIMITS.fileBytes + 1), "large.pdb")).rejects.toThrow(
      /20 MiB/,
    );
  });
  it("surfaces computation errors instead of an all-loop fallback", async () => {
    const computed = await import("../app/lib/dssp");
    const spy = vi.spyOn(computed, "computeBackboneDSSP").mockImplementationOnce(() => {
      throw new Error("assignment unavailable");
    });
    try {
      await expect(parseStructure(protein(), "failure.pdb")).rejects.toThrow(
        /computation failed.*No loop-only fallback/,
      );
    } finally {
      spy.mockRestore();
    }
  });
});
