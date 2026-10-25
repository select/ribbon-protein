import type { Molecule, MoleculeCategory, RenderOptions, StructureData } from "../types/render";

const labels: Record<MoleculeCategory, string> = {
  lipid: "Lipids",
  molecule: "Other molecules",
  nucleic: "DNA/RNA / other polymers",
  ion: "Single atoms",
  water: "Water",
};
export function moleculeGroups(molecules: Molecule[]) {
  return (Object.entries(labels) as [MoleculeCategory, string][])
    .map(([category, label]) => ({
      category,
      label,
      ids: molecules.filter((m) => m.category === category).map((m) => m.chainId),
    }))
    .filter((group) => group.ids.length > 0);
}

export function selectMoleculeGroup(
  o: RenderOptions,
  s: StructureData,
  ids: string[],
  on: boolean,
): void {
  const group = new Set(ids);
  const selected = new Set(o.selectedChains);
  o.selectedChains = s.chains
    .filter((c) => (group.has(c.id) ? on : selected.has(c.id)))
    .map((c) => c.id);
}

/** A group color applies to hidden members too; per-component edits remain possible. */
export function colorMoleculeGroup(o: RenderOptions, ids: string[], color: string): void {
  o.chainColors = {
    ...o.chainColors,
    ...Object.fromEntries(ids.map((id) => [id, color.toLowerCase()])),
  };
}
