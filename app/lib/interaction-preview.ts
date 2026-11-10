import type { PreviewScene, RenderOptions, RenderResult, Vec3 } from "../types/render";
import { modeOf } from "./molecule-groups";
import { molecularPreviewContours } from "./molecular-preview";
import { drawingPenWidth } from "./options";

type P2 = [number, number];

export function previewGeometryKey(options: RenderOptions): string {
  const unique = [...new Set(options.selectedChains)].sort();
  const ribbonChains = unique.filter((id) => modeOf(options, id) === "ribbon");
  const sliceChains = unique.filter((id) => modeOf(options, id) === "slice");
  if (!sliceChains.length && !(unique.length === 0 && options.renderMode === "slice")) {
    return JSON.stringify([options.samples, ribbonChains]);
  }
  return JSON.stringify([
    "slice",
    ribbonChains.length ? options.samples : 0,
    ribbonChains,
    sliceChains.map((id) => [id, options.chainColors[id] ?? "black"]),
    options.sliceAxis,
    options.sliceAxis === "view" ? options.rotation : "fixed",
    options.sliceCount,
    options.sliceCoarseness,
    options.sliceProbeAngstrom,
  ]);
}

const dot = (a: number[], b: number[]) => a.reduce((s, v, i) => s + v * b[i]!, 0);
const combine = (a: Vec3, x: number, b: Vec3, y: number): Vec3 =>
  a.map((v, i) => v * x + b[i]! * y) as Vec3;

/** Display-only 3D ribbon and slice contours. This bounded scene comes from
 * Rust's exact smoothing/frames/widths and full-sample PCA.
 * Never use this result for export (both export boundaries reject it).
 */
export function renderInteractionPreview(scene: PreviewScene, o: RenderOptions): RenderResult {
  const started = performance.now();
  let basis = scene.basis;
  for (const [axis, degrees] of o.rotation.entries()) {
    const radians = (degrees * Math.PI) / 180;
    const s = Math.sin(radians),
      c = Math.cos(radians);
    const old = basis;
    basis = [...old];
    if (axis === 0) {
      basis[1] = combine(old[1], c, old[2], -s);
      basis[2] = combine(old[1], s, old[2], c);
    } else if (axis === 1) {
      basis[0] = combine(old[0], c, old[2], s);
      basis[2] = combine(old[2], c, old[0], -s);
    } else {
      basis[0] = combine(old[0], c, old[1], -s);
      basis[1] = combine(old[0], s, old[1], c);
    }
  }

  const low: P2 = [Infinity, Infinity],
    high: P2 = [-Infinity, -Infinity];

  const hasRibbons = scene.ribbons.length > 0 || (scene.molecules?.length ?? 0) > 0;
  const sliceGroups = scene.sliceGroups ?? [];
  const hasSlices = sliceGroups.length > 0;
  const pureSlice =
    (hasSlices && !hasRibbons) ||
    (!hasSlices && !hasRibbons && o.selectedChains.length === 0 && o.renderMode === "slice");

  const usePerspective = pureSlice && o.slicePerspective;
  const invR = 1 / Math.max(scene.sliceRadius ?? 1, 1e-6);
  const FOC = 3.4;

  const projectedSlices = sliceGroups.map((group) => ({
    color: group.color,
    loops: group.loops.flatMap((loop) => {
      if (loop.length < 3) return [];
      const pts: P2[] = loop.map((pt) => {
        const p = pt.map((v, i) => v - scene.center[i]!);
        const x = dot(basis[0], p);
        const y = -dot(basis[1], p);
        const z = dot(basis[2], p);
        const k = usePerspective ? FOC / Math.max(FOC - z * invR, 0.35) : 1;
        const out: P2 = [x * k, y * k];
        for (const j of [0, 1] as const) {
          low[j] = Math.min(low[j], out[j]);
          high[j] = Math.max(high[j], out[j]);
        }
        return out;
      });
      pts.push(pts[0]!);
      return [pts];
    }),
  }));

  const ribbons = scene.ribbons.map((ribbon) => {
    const screen: P2[] = ribbon.samples.map((sample) => {
      const p = sample.point.map((v, i) => v - scene.center[i]!);
      return [dot(basis[0], p), -dot(basis[1], p)];
    });
    let tangent: P2 = [0, 1],
      previous: P2 = [0, 0];
    const left: P2[] = [],
      right: P2[] = [];
    for (const [i, sample] of ribbon.samples.entries()) {
      const before = screen[Math.max(0, i - 1)]!,
        after = screen[Math.min(i + 1, screen.length - 1)]!;
      const dx = after[0] - before[0],
        dy = after[1] - before[1];
      const n = Math.hypot(dx, dy);
      if (n > 1e-12) tangent = [dx / n, dy / n];
      const side: P2 = [-tangent[1], tangent[0]];
      const projected: P2 = [
        dot(basis[0], sample.side) * sample.width,
        -dot(basis[1], sample.side) * sample.width,
      ];
      const sign = dot(projected, side) >= 0 ? 1 : -1;
      let offset: P2 = projected.map(
        (v, j) => v * (1 - sample.alpha) + side[j]! * sign * sample.width * sample.alpha,
      ) as P2;
      if (i > 0 && dot(offset, previous) < 0) offset = [-offset[0], -offset[1]];
      previous = offset;
      const p = screen[i]!;
      const l: P2 = [p[0] - offset[0], p[1] - offset[1]],
        r: P2 = [p[0] + offset[0], p[1] + offset[1]];
      left.push(l);
      right.push(r);
      for (const point of [l, r])
        for (const j of [0, 1] as const) {
          low[j] = Math.min(low[j], point[j]);
          high[j] = Math.max(high[j], point[j]);
        }
    }
    return { chainId: ribbon.chainId, paths: [left, right] };
  });

  for (const molecule of scene.molecules ?? []) {
    const points: P2[] = molecule.atoms.map((atom) => {
      const p = atom.map((v, i) => v - scene.center[i]!);
      return [dot(basis[0], p), -dot(basis[1], p)];
    });
    const width = o.moleculeStyles[molecule.category].bondWidthAngstrom;
    const molecularPaths = molecularPreviewContours(
      points,
      molecule.bonds,
      width,
      molecule.terminalAtoms,
    );
    for (const path of molecularPaths)
      for (const p of path)
        for (const j of [0, 1] as const) {
          low[j] = Math.min(low[j], p[j]);
          high[j] = Math.max(high[j], p[j]);
        }
    ribbons.push({ chainId: molecule.chainId, paths: molecularPaths });
  }

  const span: P2 = [high[0] - low[0], high[1] - low[1]];
  const maxOutward = hasSlices
    ? 1.45 * Math.floor(o.sliceMaxParallel / 2) * o.sliceParallelSpacingMm
    : 0;
  const pad = Math.max(drawingPenWidth(o, scene), o.penWidthMm + 2 * maxOutward);
  const scale = Math.min(
    Math.max(o.widthMm - 2 * o.marginMm - pad, o.penWidthMm) / Math.max(span[0], 1e-12),
    Math.max(o.heightMm - 2 * o.marginMm - pad, o.penWidthMm) / Math.max(span[1], 1e-12),
  );

  let strokeCount = 0;
  let lengthMm = 0;
  const fitPath = (path: P2[]): string | null => {
    const fitted = path.map(
      (p): P2 => [
        (p[0] - (low[0] + high[0]) * 0.5) * scale + o.widthMm * 0.5,
        (p[1] - (low[1] + high[1]) * 0.5) * scale + o.heightMm * 0.5,
      ],
    );
    if (!fitted.length || fitted.some((p) => !p.every(Number.isFinite))) return null;
    for (let i = 1; i < fitted.length; i++)
      lengthMm += Math.hypot(fitted[i]![0] - fitted[i - 1]![0], fitted[i]![1] - fitted[i - 1]![1]);
    strokeCount++;
    return fitted.map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(4)},${p[1].toFixed(4)}`).join("");
  };

  const opacity = Number(o.sliceInkOpacity.toFixed(3));
  const sliceSvgGroups = projectedSlices
    .map((group, idx) => {
      const groupPaths = group.loops
        .map(fitPath)
        .filter((d): d is string => d !== null)
        .map((d) => `<path d="${d}"/>`)
        .join("");
      return groupPaths
        ? `<g id="slice-${idx + 1}" stroke="${group.color}" stroke-opacity="${opacity}">${groupPaths}</g>`
        : "";
    })
    .join("");

  const ribbonPaths: string[] = [];
  for (const ribbon of ribbons)
    for (const path of ribbon.paths) {
      const d = fitPath(path);
      if (!d) continue;
      const color = o.chainColors[ribbon.chainId] ?? "black";
      const lipid = scene.molecules?.some(
        (m) => m.chainId === ribbon.chainId && m.category === "lipid",
      );
      ribbonPaths.push(
        `<path stroke="${color}"${lipid ? ` stroke-width="${o.lipidWidthMm}"` : ""} d="${d}"/>`,
      );
    }

  const desc = pureSlice
    ? "Approximate 3D slice contours. Overdraw and parallel tracks are omitted. Not for export."
    : hasSlices
      ? "Approximate 3D hybrid outlines and slice contours. Hidden strokes are NOT removed. Not for export."
      : "Approximate 3D ribbon outlines. Hidden strokes are NOT removed. Not for export.";

  return {
    svg: `<svg xmlns="http://www.w3.org/2000/svg" width="${o.widthMm}mm" height="${o.heightMm}mm" viewBox="0 0 ${o.widthMm} ${o.heightMm}"><title>Interaction preview</title><desc>${desc}</desc><g fill="none" stroke-width="${o.penWidthMm}" stroke-linecap="round" stroke-linejoin="round">${sliceSvgGroups}${ribbonPaths.join("")}</g></svg>`,
    widthMm: o.widthMm,
    heightMm: o.heightMm,
    strokeCount,
    lengthMm,
    residueCount: 0,
    elapsedMs: performance.now() - started,
    quality: "preview",
    warnings: [
      pureSlice
        ? "Interaction preview only; refine for exact parallel tracks, overdraw passes and physical statistics."
        : "Interaction preview only; refine for exact hidden lines, tracks and physical statistics.",
    ],
  };
}
