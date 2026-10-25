import { CIF } from "molstar/lib/mol-io/reader/cif";
import { parsePDB } from "molstar/lib/mol-io/reader/pdb/parser";
import { trajectoryFromMmCIF } from "molstar/lib/mol-model-formats/structure/mmcif";
import { trajectoryFromPDB } from "molstar/lib/mol-model-formats/structure/pdb";
import { Structure, Unit } from "molstar/lib/mol-model/structure";
import type { Model, ElementIndex } from "molstar/lib/mol-model/structure/model";
import { SecondaryStructureType } from "molstar/lib/mol-model/structure/model/types";
import { SortedArray } from "molstar/lib/mol-data/int";
import { computeUnitZhangSkolnik } from "molstar/lib/mol-model-props/computed/secondary-structure/zhang-skolnik";
import type { BackboneSegment, StructureData, Vec3 } from "../types/render";
import { LIMITS, validateFile } from "./limits";

type Atoms = Record<"CA" | "C" | "O" | "N", Vec3>;
interface SelectedSegment {
  chainId: string;
  atoms: Atoms[];
}
const backboneNames = ["CA", "C", "O", "N"] as const;
export const ASSIGNMENT_WARNING =
  "Secondary structure is computed locally by Mol* Zhang–Skolnick (CA geometry), not DSSP. Helices/sheets, especially termini and inter-chain sheets, may differ from hydrogen-bond DSSP assignments.";

async function readModel(text: string, name: string): Promise<{ model: Model; multiple: boolean }> {
  if (/\.(pdb|ent)$/i.test(name)) {
    const parsed = await parsePDB(text).run();
    if (parsed.isError) throw new Error(`PDB parsing failed: ${parsed.message}`);
    const trajectory = await trajectoryFromPDB(parsed.result).run();
    if (!trajectory.frameCount) throw new Error("No molecular model found in the PDB file.");
    return { model: trajectory.representative, multiple: trajectory.frameCount > 1 };
  }
  const parsed = await CIF.parseText(text).run();
  if (parsed.isError) throw new Error(`mmCIF parsing failed: ${parsed.message}`);
  const frame = parsed.result.blocks.find((block) => block.categories.atom_site);
  if (!frame) throw new Error("The mmCIF file has no atom_site coordinate table.");
  const trajectory = await trajectoryFromMmCIF(frame).run();
  if (!trajectory.frameCount) throw new Error("No molecular model found in the mmCIF file.");
  return { model: trajectory.representative, multiple: trajectory.frameCount > 1 };
}

/** Choose one coherent, complete conformer; shared blank-alt atoms are reusable.
 * Prefer the complete conformer with highest summed occupancy; ties are lexical.
 */
function residueAtoms(
  model: Model,
  start: number,
  end: number,
): { atoms?: Atoms; alternate: boolean } {
  const { atoms } = model.atomicHierarchy;
  const conf = model.atomicConformation;
  const candidates = new Map<string, Map<string, number>>();
  for (let a = start; a < end; a++) {
    // Mol* makes duplicate PDB label names unique (CA_1, etc.), including
    // alternate locations. Auth names retain the original atom identity.
    const original = atoms.auth_atom_id.value(a);
    const name = (backboneNames as readonly string[]).includes(original)
      ? original
      : atoms.label_atom_id.value(a);
    if (!(backboneNames as readonly string[]).includes(name)) continue;
    const alt = atoms.label_alt_id.value(a) || "";
    if (!candidates.has(alt)) candidates.set(alt, new Map());
    const group = candidates.get(alt)!;
    const old = group.get(name);
    if (old === undefined || conf.occupancy.value(a) > conf.occupancy.value(old))
      group.set(name, a);
  }
  const blank = candidates.get("") ?? new Map<string, number>();
  const ids = [...candidates.keys()].filter(Boolean).sort();
  if (!ids.length) ids.push("");
  let best: Atoms | undefined;
  let score = -Infinity;
  for (const alt of ids) {
    const group = candidates.get(alt);
    const indices = backboneNames.map((name) => group?.get(name) ?? blank.get(name));
    if (indices.some((index) => index === undefined)) continue;
    const selected = indices as number[];
    const points = selected.map((a): Vec3 => [conf.x[a]!, conf.y[a]!, conf.z[a]!]);
    if (
      points.some((p) => p.some((v) => !Number.isFinite(v) || Math.abs(v) > 1e6)) ||
      Math.hypot(...points[2]!.map((v, i) => v - points[1]![i]!)) < 1e-8
    )
      continue;
    const occupancy = selected.reduce((sum, a) => sum + conf.occupancy.value(a), 0);
    if (occupancy > score) {
      best = Object.fromEntries(backboneNames.map((name, i) => [name, points[i]])) as Atoms;
      score = occupancy;
    }
  }
  return { atoms: best, alternate: candidates.size > 1 || candidates.has("") === false };
}

const cifHeader = [
  "data_selected_backbone",
  "loop_",
  "_atom_site.group_PDB",
  "_atom_site.id",
  "_atom_site.type_symbol",
  "_atom_site.label_atom_id",
  "_atom_site.label_alt_id",
  "_atom_site.label_comp_id",
  "_atom_site.label_asym_id",
  "_atom_site.label_entity_id",
  "_atom_site.label_seq_id",
  "_atom_site.pdbx_PDB_ins_code",
  "_atom_site.Cartn_x",
  "_atom_site.Cartn_y",
  "_atom_site.Cartn_z",
  "_atom_site.occupancy",
  "_atom_site.B_iso_or_equiv",
  "_atom_site.auth_seq_id",
  "_atom_site.auth_comp_id",
  "_atom_site.auth_asym_id",
  "_atom_site.auth_atom_id",
  "_atom_site.pdbx_PDB_model_num",
].join("\n");

/** Rebuild only the selected complete backbones for assignment. This prevents
 * alternate conformers, missing atoms or gaps from contaminating Mol*'s CA trace.
 * Synthetic labels are private to computation; exported chains keep original IDs.
 */
async function assignSecondary(segments: SelectedSegment[]): Promise<BackboneSegment[]> {
  const rows = [cifHeader];
  let atomId = 0;
  segments.forEach((segment, s) =>
    segment.atoms.forEach((residue, r) => {
      for (const name of backboneNames) {
        rows.push(
          `ATOM ${++atomId} ${name[0]} ${name} . ALA S${s} ${s + 1} ${r + 1} ? ${residue[name].join(" ")} 1 0 ${r + 1} ALA S${s} ${name} 1`,
        );
      }
    }),
  );
  const { model } = await readModel(rows.join("\n"), "selected.cif");
  const structure = Structure.ofModel(model);
  const output = segments.map((segment) => ({
    chainId: segment.chainId,
    residues: segment.atoms.map((atoms) => ({
      ca: atoms.CA,
      carbonyl: atoms.O.map((v, i) => v - atoms.C[i]!) as Vec3,
      kind: "C" as "H" | "E" | "C",
    })),
  }));
  let assigned = 0;
  for (const unit of structure.units) {
    if (!Unit.isAtomic(unit)) continue;
    // Mol* may group multiple small chains into one unit. Split by original
    // normalized segment before computation so assignment never spans a gap.
    const byChain = new Map<number, ElementIndex[]>();
    for (let i = 0; i < unit.elements.length; i++) {
      const a = unit.elements[i]!;
      const c = unit.chainIndex[a]!;
      if (!byChain.has(c)) byChain.set(c, []);
      byChain.get(c)!.push(a);
    }
    for (const [chain, elements] of byChain) {
      const child = unit.getChild(SortedArray.ofSortedArray(elements));
      if (!Unit.isAtomic(child)) throw new Error("Expected an atomic protein backbone.");
      const ss = await computeUnitZhangSkolnik(child);
      const s = Number(model.atomicHierarchy.chains.label_asym_id.value(chain).slice(1));
      const target = output[s];
      if (!target) throw new Error("Computed secondary-structure chain mapping failed.");
      for (let i = 0; i < child.proteinElements.length; i++) {
        const residue = child.residueIndex[child.proteinElements[i]!]!;
        const index = model.atomicHierarchy.residues.label_seq_id.value(residue) - 1;
        const flag = ss.type[ss.getIndex(residue)];
        if (flag === undefined || !target.residues[index])
          throw new Error("Computed secondary-structure residue mapping failed.");
        target.residues[index]!.kind = SecondaryStructureType.is(
          flag,
          2 /* Mol* SecondaryStructureType.Flag.Helix */,
        )
          ? "H"
          : SecondaryStructureType.is(flag, 4 /* Mol* SecondaryStructureType.Flag.Beta */)
            ? "E"
            : "C";
        assigned++;
      }
    }
  }
  if (assigned !== segments.reduce((n, s) => n + s.atoms.length, 0))
    throw new Error("Secondary-structure assignment did not cover every selected residue.");
  return output;
}

/** Local-only parser; deliberately not imported by the main/UI bundle. */
export async function parseStructure(text: string, name: string): Promise<StructureData> {
  validateFile(name, new TextEncoder().encode(text).byteLength);
  const { model, multiple } = await readModel(text, name);
  const h = model.atomicHierarchy;
  if (!h.atoms._rowCount) throw new Error("The file contains no atomic coordinates.");
  if (h.atoms._rowCount > LIMITS.atoms)
    throw new Error("Too many atoms (limit 250,000). Export only the protein chains you need.");
  if (h.residues._rowCount > LIMITS.residues)
    throw new Error("Too many residues (limit 10,000). Export a smaller protein.");
  const segments: SelectedSegment[] = [];
  let incomplete = 0,
    short = 0,
    alternates = 0,
    gaps = 0;
  for (let c = 0; c < h.chains._rowCount; c++) {
    const entity = model.entities.getEntityIndex(h.chains.label_entity_id.value(c));
    if (entity < 0 || model.entities.data.type.value(entity) !== "polymer") continue;
    // Blank author IDs are meaningful (a literal space in PDB). Only use
    // original label IDs when mmCIF marks author IDs as missing, not falsy.
    const chainId =
      h.chains.auth_asym_id.valueKind(c) === 0
        ? h.chains.auth_asym_id.value(c)
        : h.chains.label_asym_id.value(c);
    const startAtom = h.chainAtomSegments.offsets[c]!;
    const endAtom = h.chainAtomSegments.offsets[c + 1]!;
    const start = h.residueAtomSegments.index[startAtom]!;
    const end = h.residueAtomSegments.index[endAtom - 1]! + 1;
    let current: Atoms[] = [];
    let previousSeq: number | undefined;
    const flush = () => {
      if (current.length >= 4) segments.push({ chainId, atoms: current });
      else short += current.length;
      current = [];
      previousSeq = undefined;
    };
    for (let r = start; r < end; r++) {
      if (h.derived.residue.moleculeType[r] !== 5 /* Mol* MoleculeType.Protein */) {
        flush();
        continue;
      }
      const selected = residueAtoms(
        model,
        h.residueAtomSegments.offsets[r]!,
        h.residueAtomSegments.offsets[r + 1]!,
      );
      if (selected.alternate) alternates++;
      if (!selected.atoms) {
        incomplete++;
        flush();
        continue;
      }
      const seq =
        h.residues.label_seq_id.valueKind(r) === 0
          ? h.residues.label_seq_id.value(r)
          : h.residues.auth_seq_id.value(r);
      const previous = current.at(-1);
      const distance = previous
        ? Math.hypot(...selected.atoms.CA.map((v, i) => v - previous.CA[i]!))
        : 0;
      if (
        previous &&
        (distance > 4.5 ||
          distance < 1e-6 ||
          (previousSeq !== undefined && (seq > previousSeq + 1 || seq < previousSeq)))
      ) {
        gaps++;
        flush();
      }
      current.push(selected.atoms);
      previousSeq = seq;
    }
    flush();
  }
  if (!segments.length)
    throw new Error(
      "No drawable protein backbone. Need at least four consecutive residues with CA, C, O and N atoms, and CA gaps ≤4.5 Å.",
    );
  if (segments.length > LIMITS.segments)
    throw new Error("Too many backbone fragments (limit 512). Choose a less fragmented protein.");
  let assigned: BackboneSegment[];
  try {
    assigned = await assignSecondary(segments);
  } catch (cause) {
    throw new Error(
      `Local secondary-structure computation failed: ${cause instanceof Error ? cause.message : String(cause)}. No loop-only fallback was used.`,
      { cause },
    );
  }
  const counts = new Map<string, number>();
  for (const segment of assigned)
    counts.set(segment.chainId, (counts.get(segment.chainId) ?? 0) + segment.residues.length);
  const warnings = [ASSIGNMENT_WARNING];
  if (multiple) warnings.push("Only the first molecular model is used.");
  if (incomplete)
    warnings.push(
      `${incomplete} incomplete or invalid protein residues excluded; backbones split at these positions.`,
    );
  if (gaps) warnings.push(`${gaps} sequence/CA gaps split into separate backbones.`);
  if (short) warnings.push(`${short} residues in fragments shorter than four excluded.`);
  if (alternates)
    warnings.push(
      `${alternates} alternate-location residues: one complete conformer selected by occupancy (lexical ties), with shared blank atoms.`,
    );
  return {
    name,
    segments: assigned,
    chains: [...counts].map(([id, residueCount]) => ({ id, residueCount })),
    residueCount: [...counts.values()].reduce((a, b) => a + b, 0),
    warnings,
  };
}
