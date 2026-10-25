import { LIMITS } from "./limits";
import { SortedArray } from "molstar/lib/mol-data/int";
import { IntAdjacencyGraph } from "molstar/lib/mol-math/graph";
import { Vec3 } from "molstar/lib/mol-math/linear-algebra";
import type { Unit } from "molstar/lib/mol-model/structure";
import { SecondaryStructureType } from "molstar/lib/mol-model/structure/model/types";
import { DefaultDSSPComputationProps } from "molstar/lib/mol-model-props/computed/secondary-structure/dssp";
import { getUnitProteinInfo } from "molstar/lib/mol-model-props/computed/secondary-structure/dssp/protein-info";
import { calcUnitProteinTraceLookup3D } from "molstar/lib/mol-model-props/computed/secondary-structure/dssp/trace-lookup";
import { assignTurns } from "molstar/lib/mol-model-props/computed/secondary-structure/dssp/turns";
import { assignHelices } from "molstar/lib/mol-model-props/computed/secondary-structure/dssp/helices";
import { assignLadders } from "molstar/lib/mol-model-props/computed/secondary-structure/dssp/ladders";
import { assignSheets } from "molstar/lib/mol-model-props/computed/secondary-structure/dssp/sheets";
import {
  Bridge,
  type DSSPContext,
} from "molstar/lib/mol-model-props/computed/secondary-structure/dssp/common";

/** Mol* 5.11's computeUnitDSSP assumes a continuous residue array: its donor
 * reconstruction crosses chains/gaps and treats PRO as an NH donor. Reuse its
 * original-convention turn/helix/ladder/sheet stages with a chemical, break-aware
 * H-bond graph instead. All chains share one spatial lookup (inter-chain sheets).
 * Bends/dihedrals are unnecessary for the simplified H/E/C result.
 */
// Mol* DSSPType.Flag is an ambient const enum; numeric values are its pinned
// 5.11 API (not SecondaryStructureType.Flag, which uses different bit values).
const DSSP = { H: 1, B: 2, E: 4, G: 8, I: 16 } as const;

export function computeBackboneDSSP(unit: Unit.Atomic) {
  const proteinInfo = getUnitProteinInfo(unit);
  const { residueIndices, cIndices, nIndices, oIndices } = proteinInfo;
  const count = residueIndices.length;
  const { atomicHierarchy: h, atomicConformation: xyz } = unit.model;
  const chain = Array.from(
    residueIndices,
    (r) => h.chainAtomSegments.index[h.residueAtomSegments.offsets[r]!]!,
  );
  const lookup = calcUnitProteinTraceLookup3D(unit, residueIndices);
  const position = (a: number): Vec3 => Vec3.create(xyz.x[a]!, xyz.y[a]!, xyz.z[a]!);
  const sameChain = (a: number, b: number) => a >= 0 && b < count && chain[a] === chain[b];
  const acceptors: number[] = [];
  const energies: number[] = [];
  const hydrogen = Array.from({ length: count }, (_, i) => {
    const r = residueIndices[i]!;
    const atom = h.residueAtomSegments.offsets[r]!;
    // DSSP uses an inferred amide H, not arbitrary explicit hydrogen naming.
    // PRO has no backbone amide H; chain starts have no previous carbonyl.
    if (!sameChain(i - 1, i) || h.atoms.label_comp_id.value(atom) === "PRO") return undefined;
    const direction = Vec3.sub(Vec3(), position(cIndices[i - 1]!), position(oIndices[i - 1]!));
    return Vec3.add(direction, position(nIndices[i]!), Vec3.normalize(direction, direction));
  });
  // Same electrostatic expression, 9 Å CA lookup and -0.5 kcal/mol threshold as
  // Mol*'s backbone-hbonds (Kabsch–Sander). No coordinates leave this worker.
  const from: number[] = [];
  let pairs = 0;
  for (let i = 0; i < count; i++) {
    const ca = position(h.derived.residue.traceElementIndex[residueIndices[i]!]!);
    if (h.index.findAtomOnResidue(residueIndices[i]!, "OXT") !== -1) continue;
    const nearby = lookup.find(ca[0]!, ca[1]!, ca[2]!, 9);
    pairs += nearby.count;
    if (pairs > LIMITS.dsspPairs)
      throw new Error("DSSP contact-search work limit exceeded. Export a smaller protein.");
    const o = position(oIndices[i]!);
    const c = position(cIndices[i]!);
    for (let k = 0; k < nearby.count; k++) {
      const j = nearby.indices[k]!;
      const hp = hydrogen[j];
      if (!hp || i === j || (sameChain(Math.min(i, j), Math.max(i, j)) && Math.abs(i - j) === 1))
        continue;
      const n = position(nIndices[j]!);
      const energy =
        -27.888 *
        (1 / Vec3.distance(o, hp) -
          1 / Vec3.distance(c, hp) +
          1 / Vec3.distance(c, n) -
          1 / Vec3.distance(o, n));
      if (!Number.isFinite(energy) || energy > -0.5) continue;
      if (from.length >= LIMITS.dsspBonds)
        throw new Error("DSSP hydrogen-bond work limit exceeded. Check for overlapping chains.");
      from.push(i);
      acceptors.push(j);
      energies.push(Math.max(-9.9, energy));
    }
  }
  const builder = new IntAdjacencyGraph.DirectedEdgeBuilder(count, from, acceptors);
  const edgeEnergies = new Float32Array(builder.slotCount);
  for (let i = 0; i < builder.edgeCount; i++) {
    builder.addNextEdge();
    builder.assignProperty(edgeEnergies, energies[i]!);
  }
  const hbonds = builder.createGraph({ energies: edgeEnergies });
  const flags = new Uint32Array(count);
  const ctx: DSSPContext = {
    params: DefaultDSSPComputationProps, // oldDefinition=true, oldOrdering=true
    getResidueFlag: () => SecondaryStructureType.create(0),
    getFlagName: () => "", // not used by these stages
    unit,
    proteinInfo,
    flags,
    hbonds,
    torsionAngles: { phi: new Float32Array(0), psi: new Float32Array(0) },
    bridges: [],
    ladders: [],
  };
  // Turn and helix sequence arithmetic must never cross a genuine break.
  for (let start = 0; start < count; ) {
    let end = start + 1;
    while (end < count && sameChain(start, end)) end++;
    const local: DSSPContext = {
      ...ctx,
      proteinInfo: {
        ...proteinInfo,
        residueIndices: SortedArray.ofSortedArray(Array.from(residueIndices).slice(start, end)),
      },
      flags: flags.subarray(start, end),
      hbonds: {
        ...hbonds,
        getDirectedEdgeIndex: (a, b) =>
          a >= 0 && b < end - start ? hbonds.getDirectedEdgeIndex(a + start, b + start) : -1,
      },
    };
    assignTurns(local);
    assignHelices(local);
    start = end;
  }
  // Mol* bridge patterns, with explicit peptide-neighbor checks. Reciprocal
  // bonds can form a bridge even at termini; offset patterns need real neighbors.
  const groups = new Map<string, Bridge[]>();
  const seen = new Set<string>();
  const add = (a: number, b: number, type: 0 | 1) => {
    if (a === b || (sameChain(Math.min(a, b), Math.max(a, b)) && Math.abs(a - b) < 3)) return;
    const bridge = new Bridge(a, b, type);
    const key = `${bridge.partner1}:${bridge.partner2}:${type}`;
    if (seen.has(key)) return;
    seen.add(key);
    flags[a]! |= DSSP.B;
    flags[b]! |= DSSP.B;
    const pair = `${chain[bridge.partner1]}:${chain[bridge.partner2]}`;
    if (!groups.has(pair)) groups.set(pair, []);
    groups.get(pair)!.push(bridge);
  };
  const bonded = (a: number, b: number) =>
    a >= 0 && b >= 0 && a < count && b < count && hbonds.getDirectedEdgeIndex(a, b) !== -1;
  for (let e = 0; e < from.length; e++) {
    const k = from[e]!,
      l = acceptors[e]!;
    if (k > l) continue;
    if (sameChain(k, k + 2) && bonded(l, k + 2)) add(k + 1, l, 0);
    if (sameChain(l - 2, l) && bonded(l - 2, k)) add(k, l - 1, 0);
    if (bonded(l, k)) add(k, l, 1);
    if (sameChain(k, k + 2) && sameChain(l - 2, l) && bonded(l - 2, k + 2)) add(k + 1, l - 1, 1);
  }
  // Prevent ladder extension/bulge filling from jumping between fragments.
  let ladderWork = 0;
  for (const bridges of groups.values()) {
    ladderWork += bridges.length ** 2;
    if (ladderWork > LIMITS.renderWork)
      throw new Error("DSSP ladder-search work limit exceeded. Export a smaller protein.");
    bridges.sort((a, b) => a.partner1 - b.partner1);
    const sheet = { ...ctx, bridges, ladders: [] };
    assignLadders(sheet);
    assignSheets(sheet);
  }
  // Mol* original priority H > E > B > G > I > T > S; simplify like DSSP:
  // H/G/I → H, E/B → E, turns/bends/unassigned → C (not raw DSSP flag bits).
  const kinds = Array.from(flags, (f): "H" | "E" | "C" =>
    f & DSSP.H ? "H" : f & (DSSP.E | DSSP.B) ? "E" : f & (DSSP.G | DSSP.I) ? "H" : "C",
  );
  return { residueIndices, kinds };
}
