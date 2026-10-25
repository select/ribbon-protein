/** Shared JSON contract between the browser worker and Rust renderer. Coordinates are Å. */
export type Vec3 = [number, number, number];
export type SecondaryStructure = "H" | "E" | "C";
export interface BackboneResidue {
  ca: Vec3;
  carbonyl: Vec3;
  kind: SecondaryStructure;
}
export interface BackboneSegment {
  chainId: string;
  residues: BackboneResidue[];
}
export type MoleculeCategory = "lipid" | "molecule" | "nucleic" | "water" | "ion";
export interface MoleculeStyle {
  /** Real longitudinal tracks between two bond outlines. */
  lines: number;
  /** Full artistic bond-band width in Å; zero keeps the original wire. */
  bondWidthAngstrom: number;
}
export function defaultMoleculeStyle(category: MoleculeCategory): MoleculeStyle {
  return category === "lipid"
    ? { lines: 0, bondWidthAngstrom: 0.4 }
    : { lines: 1, bondWidthAngstrom: 0.5 };
}
export interface Molecule {
  /** Selection/color ID; distinct from original protein chain IDs. */
  chainId: string;
  label: string;
  category: MoleculeCategory;
  atoms: Vec3[];
  /** Heavy-atom covalent bonds, irrespective of bond order. */
  bonds: [number, number][];
}
export interface StructureData {
  name: string;
  segments: BackboneSegment[];
  molecules?: Molecule[];
  chains: { id: string; residueCount: number }[];
  residueCount: number;
  warnings: string[];
}
export interface RenderOptions {
  paper: "a4" | "a3" | "bierdeckel" | "custom";
  landscape: boolean;
  widthMm: number;
  heightMm: number;
  marginMm: number;
  rotation: Vec3;
  lines: number;
  penWidthMm: number;
  lipidWidthMm: number;
  moleculeStyles: Record<MoleculeCategory, MoleculeStyle>;
  samples: number;
  fillLoops: boolean;
  loopSpacingMm: number;
  borderPasses: number;
  borderSpacingMm: number;
  strokeDirection: "backbone" | "travel";
  selectedChains: string[];
  chainColors: Record<string, string>;
}
export interface RenderRequest {
  structure: StructureData;
  options: RenderOptions;
}
export interface PreviewMolecule extends Molecule {
  /** True source terminals only; sampled preview cuts must not gain caps. */
  terminalAtoms?: number[];
}
export interface PreviewScene {
  center: Vec3;
  basis: [Vec3, Vec3, Vec3];
  molecules?: PreviewMolecule[];
  ribbons: {
    chainId: string;
    samples: { point: Vec3; side: Vec3; width: number; alpha: number }[];
  }[];
}
export interface RenderTimings {
  /** Per-worker diagnostic counters; reset only after hard recreation. */
  moduleInitializations?: number;
  structureLoads?: number;
  parses?: number;
  structureRevision?: number;
  startupMs?: number;
  moduleInitMs?: number;
  structureLoadMs?: number;
  parseDsspMs?: number;
  prepareMs?: number;
  projectMs?: number;
  clipMs?: number;
  pathsMs?: number;
  finishMs?: number;
  geometryCacheHit?: boolean;
  pathCacheHit?: boolean;
  roundtripMs?: number;
  responseMs?: number;
  imageDecodeMs?: number;
  visibleMs?: number;
}
export interface RenderResult {
  svg: string;
  widthMm: number;
  heightMm: number;
  strokeCount: number;
  lengthMm: number;
  residueCount: number;
  /** Final: WASM calls + JSON decoding; preview: local contour compute. Not end-to-end. */
  elapsedMs: number;
  quality?: "preview" | "final";
  /** Bounded prepared 3D outline scene, transferred only when its key changes. */
  previewScene?: PreviewScene;
  timings?: RenderTimings;
  warnings: string[];
}
export type RenderStatus = "idle" | "loading" | "rendering" | "ready" | "error";

export const PAPER_PRESETS = {
  a4: { label: "A4", widthMm: 210, heightMm: 297, marginMm: 15 },
  a3: { label: "A3", widthMm: 297, heightMm: 420, marginMm: 15 },
  bierdeckel: { label: "Bierdeckel", widthMm: 93.14, heightMm: 93.14, marginMm: 5 },
} as const;

export function defaultRenderOptions(): RenderOptions {
  return {
    paper: "a4",
    landscape: false,
    widthMm: 210,
    heightMm: 297,
    marginMm: 15,
    rotation: [0, 0, 0],
    lines: 7,
    penWidthMm: 0.4,
    lipidWidthMm: 0.4,
    moleculeStyles: {
      lipid: defaultMoleculeStyle("lipid"),
      molecule: defaultMoleculeStyle("molecule"),
      nucleic: defaultMoleculeStyle("nucleic"),
      water: defaultMoleculeStyle("water"),
      ion: defaultMoleculeStyle("ion"),
    },
    samples: 16,
    fillLoops: true,
    loopSpacingMm: 0.15,
    borderPasses: 3,
    borderSpacingMm: 0.1,
    strokeDirection: "backbone",
    selectedChains: [],
    chainColors: {},
  };
}
