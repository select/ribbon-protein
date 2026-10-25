import { Structure, Unit } from "molstar/lib/mol-model/structure";
import { Model } from "molstar/lib/mol-model/structure/model";
import type { ElementIndex } from "molstar/lib/mol-model/structure/model";
import { SortedArray } from "molstar/lib/mol-data/int";
import type { Molecule, Vec3 } from "../types/render";

/** Mol* recognizes many simulation lipid names; CCD names cover experimental
 * structures such as PTY, whose chem_comp.type is only `non-polymer`. */
function isLipid(model: Model, compId: string, type: number | undefined): boolean {
  if (type === 4 || compId === "PTY") return true;
  const component = model.properties.chemicalComponentMap.get(compId);
  return /phosphatid|phosphocholine|cardiolipin|cholesterol|sphingomyelin|ceramide|diacylglycerol|triacylglycerol|glycolipid|lipopolysaccharide/i.test(
    component?.name ?? "",
  );
}

/** Heavy atoms only. No network/CCD fetches: Mol* uses file connectivity and
 * local component tables, with distance inference where definitions are absent.
 * Inferred contacts never join separate non-polymer residues. */
export function readMolecules(model: Model): Molecule[] {
  const h = model.atomicHierarchy;
  const conf = model.atomicConformation;
  const molecules: Molecule[] = [];
  const selected: ElementIndex[] = [];
  const mapping = new Map<number, { molecule: Molecule; atom: number }>();
  // Reserve every original author/label ID, even protein chains without ribbons.
  const used = new Set<string>();
  for (let c = 0; c < h.chains._rowCount; c++) {
    used.add(h.chains.auth_asym_id.value(c));
    used.add(h.chains.label_asym_id.value(c));
  }
  let bondCount = 0;
  for (let c = 0; c < h.chains._rowCount; c++) {
    const entity = model.entities.getEntityIndex(h.chains.label_entity_id.value(c));
    const polymer = entity >= 0 && model.entities.data.type.value(entity) === "polymer";
    const author =
      h.chains.auth_asym_id.valueKind(c) === 0
        ? h.chains.auth_asym_id.value(c)
        : h.chains.label_asym_id.value(c);
    const first = h.residueAtomSegments.index[h.chainAtomSegments.offsets[c]!]!;
    const last = h.residueAtomSegments.index[h.chainAtomSegments.offsets[c + 1]! - 1]!;
    let polymerMolecule: Molecule | undefined;
    for (let r = first; r <= last; r++) {
      const type = h.derived.residue.moleculeType[r];
      if (polymer && type === 5 /* Protein */) continue;
      const start = h.residueAtomSegments.offsets[r]!;
      const end = h.residueAtomSegments.offsets[r + 1]!;
      const groups = new Map<string, Map<string, number>>();
      for (let a = start; a < end; a++) {
        if (["H", "D", "T"].includes(h.atoms.type_symbol.value(a))) continue;
        const p = [conf.x[a]!, conf.y[a]!, conf.z[a]!];
        if (p.some((v) => !Number.isFinite(v) || Math.abs(v) > 1e6))
          throw new Error("Non-protein atom has invalid or excessive coordinates.");
        if (conf.occupancy.value(a) <= 0) continue;
        const alt = h.atoms.label_alt_id.value(a) || "";
        if (!groups.has(alt)) groups.set(alt, new Map());
        const group = groups.get(alt)!;
        const name = h.atoms.auth_atom_id.value(a) || h.atoms.label_atom_id.value(a);
        const old = group.get(name);
        if (old === undefined || conf.occupancy.value(a) > conf.occupancy.value(old))
          group.set(name, a);
      }
      const blank = groups.get("") ?? new Map<string, number>();
      const alternates = [...groups.keys()].filter(Boolean).sort();
      let chosen = blank;
      let score = -Infinity;
      for (const alt of alternates) {
        const merged = new Map([...blank, ...groups.get(alt)!]);
        const occupancy = [...merged.values()].reduce((sum, a) => sum + conf.occupancy.value(a), 0);
        if (occupancy > score) {
          chosen = merged;
          score = occupancy;
        }
      }
      const indices = [...chosen.values()].sort((a, b) => a - b);
      if (!indices.length) continue;
      let molecule = polymer ? polymerMolecule : undefined;
      if (!molecule) {
        let id = `molecule:${molecules.length + 1}`;
        while (used.has(id)) id = `_${id}`;
        used.add(id);
        molecule = {
          chainId: id,
          label: polymer
            ? `${author || "(blank)"} · nucleic/other polymer`
            : `${h.atoms.label_comp_id.value(start)} · ${author || "(blank)"} ${h.residues.auth_seq_id.value(r)}${h.residues.pdbx_PDB_ins_code.value(r) || ""}`,
          category:
            type === 2
              ? "water"
              : polymer
                ? "nucleic"
                : isLipid(model, h.atoms.label_comp_id.value(start), type)
                  ? "lipid"
                  : indices.length === 1
                    ? "ion"
                    : "molecule",
          atoms: [],
          bonds: [],
        };
        molecules.push(molecule);
        if (molecules.length > 4096)
          throw new Error(
            "Too many molecular components (limit 4,096). Export a smaller structure.",
          );
        if (polymer) polymerMolecule = molecule;
      }
      for (const a of indices) {
        mapping.set(a, { molecule, atom: molecule.atoms.length });
        molecule.atoms.push([conf.x[a]!, conf.y[a]!, conf.z[a]!] as Vec3);
        selected.push(a as ElementIndex);
      }
    }
  }
  if (!selected.length) return [];
  // Sparse atomic files are sometimes heuristically classified as coarse
  // models by Mol*. Do not invent coarse trace bonds for incomplete ligands/DNA.
  Model.CoarseGrained.set(model, false);
  const base = Structure.ofModel(model).units.find(Unit.isAtomic);
  if (!base) return [];
  const unit = Unit.create(
    0,
    0,
    0,
    base.traits,
    0,
    model,
    base.conformation.operator,
    SortedArray.ofSortedArray(selected.sort((a, b) => a - b)),
  );
  if (!Unit.isAtomic(unit)) throw new Error("Expected atomic molecular coordinates.");
  // Bound Mol*'s neighbor work before it allocates an adjacency graph. Six
  // angstroms also covers its coarse-model fallback radius.
  let neighbors = 0;
  for (let i = 0; i < unit.elements.length; i++) {
    const a = unit.elements[i]!;
    neighbors += unit.lookup3d.find(conf.x[a]!, conf.y[a]!, conf.z[a]!, 6).count;
    if (neighbors > 5_000_000)
      throw new Error("Molecular bond-search work limit exceeded; export fewer components.");
  }
  const bonds = unit.bonds;
  for (let a = 0; a < unit.elements.length; a++) {
    for (let e = bonds.offset[a]!; e < bonds.offset[a + 1]!; e++) {
      const b = bonds.b[e]!;
      if (a >= b || !((bonds.edgeProps.flags[e]! & 1) /* Covalent */)) continue;
      const left = mapping.get(unit.elements[a]!)!;
      const right = mapping.get(unit.elements[b]!)!;
      // Do not invent bridges between separate ligands, waters or ions.
      // Non-protein polymer residues share a component and retain backbone bonds.
      if (left.molecule !== right.molecule) continue;
      left.molecule.bonds.push([left.atom, right.atom]);
      if (++bondCount > 250_000) throw new Error("Too many molecular bonds (limit 250,000).");
    }
  }
  return molecules;
}
