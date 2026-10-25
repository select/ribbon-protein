import type { RenderOptions, StructureData } from "../types/render";
import { LIMITS } from "./limits";

/** Mirror the strict Rust boundary early so an invalid edit keeps the preview. */
export function validateOptions(o: RenderOptions, s: StructureData): void {
  const range = (label: string, value: number, min: number, max: number, integer = false) => {
    if (
      !Number.isFinite(value) ||
      value < min ||
      value > max ||
      (integer && !Number.isInteger(value))
    )
      throw new Error(`${label} must be ${integer ? "an integer " : ""}between ${min} and ${max}.`);
  };
  if (!["a4", "a3", "bierdeckel", "custom"].includes(o.paper))
    throw new Error("Choose a valid paper preset.");
  range("Width (mm)", o.widthMm, 10, 2000);
  range("Height (mm)", o.heightMm, 10, 2000);
  range("Margin (mm)", o.marginMm, 0, 1000);
  range("Pen width (mm)", o.penWidthMm, 0.1, 2);
  if (o.marginMm * 2 + o.penWidthMm >= Math.min(o.widthMm, o.heightMm))
    throw new Error("Paper must be larger than twice the margin plus the pen width.");
  range("Interior lines", o.lines, 0, 20, true);
  range("Samples", o.samples, 2, 64, true);
  range("Border passes", o.borderPasses, 1, 3, true);
  range("Loop spacing (mm)", o.loopSpacingMm, 0.02, 10);
  range("Border spacing (mm)", o.borderSpacingMm, 0.01, 10);
  if (o.rotation.length !== 3) throw new Error("Rotation requires three angles.");
  o.rotation.forEach((v) => range("Rotation (degrees)", v, -360000, 360000));
  if (!["backbone", "travel"].includes(o.strokeDirection))
    throw new Error("Choose backbone or travel ordering.");
  const chains = new Set(s.chains.map((c) => c.id));
  if (o.selectedChains.some((id) => !chains.has(id)))
    throw new Error("An unknown chain is selected.");
  // An empty selection is valid: Rust returns an empty drawing, never all chains.
  if (
    Object.entries(o.chainColors).some(
      ([id, color]) => !chains.has(id) || !/^(#[\da-fA-F]{6}|black|red)$/.test(color),
    )
  )
    throw new Error(
      "Chain colors must be black, red or six-digit #RRGGBB values for known chains.",
    );
  const samples = s.segments.reduce(
    (n, segment) => n + (segment.residues.length - 1) * o.samples + 1,
    0,
  );
  if (samples > 200_000)
    throw new Error("Too many spline samples (limit 200,000). Reduce quality or protein size.");
  const work = samples * (o.lines + 2 * o.borderPasses + (o.fillLoops ? 24 : 0));
  if (work > LIMITS.renderWork)
    throw new Error(
      "This structure/quality combination exceeds the local workload limit. Reduce samples, lines, or disable loop fill.",
    );
}
