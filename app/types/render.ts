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
export interface StructureData {
  name: string;
  segments: BackboneSegment[];
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
export interface RenderResult {
  svg: string;
  widthMm: number;
  heightMm: number;
  strokeCount: number;
  lengthMm: number;
  residueCount: number;
  elapsedMs: number;
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
    penWidthMm: 0.25,
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
