import { readFile } from "node:fs/promises";
import { describe, expect, it, vi } from "vitest";
import { parseStructure } from "../app/lib/parser";

const header = `data_molecules
loop_
_entity.id
_entity.type
1 non-polymer
2 water
3 polymer
loop_
_chem_comp.id
_chem_comp.type
LIG non-polymer
HOH non-polymer
NA non-polymer
DA 'DNA linking'
loop_
_atom_site.group_PDB
_atom_site.id
_atom_site.type_symbol
_atom_site.label_atom_id
_atom_site.label_alt_id
_atom_site.label_comp_id
_atom_site.label_asym_id
_atom_site.label_entity_id
_atom_site.label_seq_id
_atom_site.pdbx_PDB_ins_code
_atom_site.Cartn_x
_atom_site.Cartn_y
_atom_site.Cartn_z
_atom_site.occupancy
_atom_site.B_iso_or_equiv
_atom_site.auth_seq_id
_atom_site.auth_comp_id
_atom_site.auth_asym_id
_atom_site.auth_atom_id
_atom_site.pdbx_PDB_model_num
`;
const molecularCif =
  header +
  `HETATM 1 C C1 . LIG L 1 . ? 0 0 0 1 0 1 LIG A C1 1
HETATM 2 C C2 . LIG L 1 . ? 1.5 0 0 1 0 1 LIG A C2 1
HETATM 3 C C1 . LIG M 1 . ? 3 0 0 1 0 2 LIG A C1 1
HETATM 4 C C2 . LIG M 1 . ? 4.5 0 0 1 0 2 LIG A C2 1
HETATM 5 O O . HOH W 2 . ? 0 5 0 1 0 3 HOH W O 1
HETATM 6 H H1 . HOH W 2 . ? 0 5.9 0 1 0 3 HOH W H1 1
HETATM 7 Na NA . NA I 1 . ? 6 0 0 1 0 4 NA I NA 1
`;

describe("general non-protein molecules", () => {
  it("retains a real 6BAJ lipid with its file-supplied chemical connectivity, without needing protein", async () => {
    const text = await readFile(new URL("./fixtures/6baj-lipid.cif", import.meta.url), "utf8");
    const s = await parseStructure(text, "lipid.cif");
    expect(s.segments).toEqual([]);
    expect(s.residueCount).toBe(0);
    expect(s.molecules).toHaveLength(1);
    const lipid = s.molecules![0]!;
    expect(lipid.label).toBe("PTY · A 1101");
    expect(lipid.atoms).toHaveLength(50);
    expect(lipid.bonds).toHaveLength(49);
    [86.932, 83.763, 67.872].forEach((v, i) => expect(lipid.atoms[0]![i]).toBeCloseTo(v, 4));
    expect(s.warnings.join(" ")).not.toContain("DSSP");
    // A local chemical dictionary, not proximity alone, controls connectivity.
    const edited = await parseStructure(text.replace(/^PTY C1  O4 .*\n/m, ""), "edited.cif");
    expect(edited.molecules![0]!.bonds).toHaveLength(48);
    const incomplete = await parseStructure(text.replace(/^HETATM 23909 .*\n/m, ""), "partial.cif");
    expect(incomplete.molecules![0]!.atoms).toHaveLength(49);
    expect(incomplete.molecules![0]!.bonds).toHaveLength(47);
  });
  it("keeps close ligands separate, omits hydrogen and exposes water and isolated atoms", async () => {
    const s = await parseStructure(molecularCif, "molecules.cif");
    expect(s.chains).toHaveLength(4);
    expect(new Set(s.chains.map((c) => c.id)).size).toBe(4);
    expect(s.molecules!.map((m) => [m.category, m.atoms.length, m.bonds.length])).toEqual([
      ["molecule", 2, 1],
      ["molecule", 2, 1],
      ["ion", 1, 0],
      ["water", 1, 0],
    ]);
    expect(s.molecules![0]!.bonds).toEqual([[0, 1]]);
    expect(s.molecules![1]!.bonds).toEqual([[0, 1]]);
  });
  it("uses one coherent high-occupancy alternate with shared blank atoms", async () => {
    const source = molecularCif.replace(
      "HETATM 2 C C2 . LIG L 1 . ? 1.5 0 0 1 0 1 LIG A C2 1",
      "HETATM 2 C C2 A LIG L 1 . ? 1.5 0 0 0.2 0 1 LIG A C2 1\nHETATM 8 C C2 B LIG L 1 . ? 1.6 0 0 0.8 0 1 LIG A C2 1",
    );
    const s = await parseStructure(source, "alternate.cif");
    expect(s.molecules![0]!.atoms).toHaveLength(2);
    expect(s.molecules![0]!.atoms[0]).toEqual([0, 0, 0]);
    expect(s.molecules![0]!.atoms[1]![0]).toBeCloseTo(1.6, 5);
    expect(s.molecules![0]!.bonds).toEqual([[0, 1]]);
  });
  it("draws DNA as a molecular chain, retaining bonds between nucleotide residues", async () => {
    const s = await parseStructure(
      header +
        `ATOM 1 P P . DA D 3 1 ? 0 0 0 1 0 1 DA D P 1
ATOM 2 O "O3'" . DA D 3 1 ? 1.6 0 0 1 0 1 DA D "O3'" 1
ATOM 3 P P . DA D 3 2 ? 3.2 0 0 1 0 2 DA D P 1
ATOM 4 O "O3'" . DA D 3 2 ? 4.8 0 0 1 0 2 DA D "O3'" 1
`,
      "dna.cif",
    );
    expect(s.molecules).toHaveLength(1);
    expect(s.molecules![0]!.category).toBe("nucleic");
    expect(s.molecules![0]!.atoms).toHaveLength(4);
    const molecule = s.molecules![0]!;
    const a = molecule.atoms.findIndex((p) => Math.abs(p[0] - 1.6) < 1e-5);
    const b = molecule.atoms.findIndex((p) => Math.abs(p[0] - 3.2) < 1e-5);
    expect(molecule.bonds.some((pair) => pair.includes(a) && pair.includes(b))).toBe(true);
  });
  it("reads ligand-only PDB files and keeps disconnected atoms instead of inventing long bonds", async () => {
    const s = await parseStructure(
      `HETATM    1  C1  LIG A   1       0.000   0.000   0.000  1.00 20.00           C
HETATM    2  C2  LIG A   1       1.500   0.000   0.000  1.00 20.00           C
HETATM    3  C3  LIG A   1      10.000   0.000   0.000  1.00 20.00           C
END
`,
      "ligand.pdb",
    );
    expect(s.molecules).toHaveLength(1);
    expect(s.molecules![0]!.atoms).toHaveLength(3);
    expect(s.molecules![0]!.bonds).toEqual([[0, 1]]);
  });
  it("bounds dense molecular bond-search work before building the bond graph", async () => {
    const { Unit } = await import("molstar/lib/mol-model/structure");
    const create = Unit.create;
    const spy = vi.spyOn(Unit, "create").mockImplementation((...args) => {
      const unit = create(...args);
      if (Unit.isAtomic(unit))
        vi.spyOn(unit.lookup3d, "find").mockReturnValue({
          count: 5_000_001,
          indices: [],
          squaredDistances: [],
        });
      return unit;
    });
    try {
      await expect(parseStructure(molecularCif, "dense.cif")).rejects.toThrow(
        /bond-search work limit/,
      );
    } finally {
      vi.restoreAllMocks();
      spy.mockRestore();
    }
  });
});
