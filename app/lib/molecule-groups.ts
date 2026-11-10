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

export interface ComponentTreeGroup {
  key: "protein" | MoleculeCategory;
  label: string;
  ids: string[];
  defaultExpanded: boolean;
}

export function componentGroups(
  s: Pick<StructureData, "chains" | "molecules">,
): ComponentTreeGroup[] {
  const moleculeIds = new Set((s.molecules ?? []).map((m) => m.chainId));
  const proteinIds = s.chains.filter((c) => !moleculeIds.has(c.id)).map((c) => c.id);
  const groups: ComponentTreeGroup[] = [];
  if (proteinIds.length > 0) {
    groups.push({
      key: "protein",
      label: "Protein chains",
      ids: proteinIds,
      defaultExpanded: true,
    });
  }
  for (const group of moleculeGroups(s.molecules ?? [])) {
    groups.push({
      key: group.category,
      label: group.label,
      ids: group.ids,
      defaultExpanded: group.category !== "water",
    });
  }
  return groups;
}

export function groupSwatchColor(
  chainColors: Record<string, string>,
  ids: string[],
  fallback = "#191918",
): string {
  if (!ids.length) return fallback;
  return chainColors[ids[0]!] ?? fallback;
}

/** A group color applies to hidden members too; per-component edits remain possible. */
export function colorMoleculeGroup(o: RenderOptions, ids: string[], color: string): void {
  o.chainColors = {
    ...o.chainColors,
    ...Object.fromEntries(ids.map((id) => [id, color.toLowerCase()])),
  };
}
