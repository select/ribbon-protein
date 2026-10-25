import type { PreviewScene, RenderOptions, RenderResult, Vec3 } from "../types/render";

type P2 = [number, number];
export function previewGeometryKey(options: RenderOptions): string {
  return JSON.stringify([options.samples, [...new Set(options.selectedChains)].sort()]);
}
const dot = (a: number[], b: number[]) => a.reduce((s, v, i) => s + v * b[i]!, 0);
const combine = (a: Vec3, x: number, b: Vec3, y: number): Vec3 =>
  a.map((v, i) => v * x + b[i]! * y) as Vec3;

/** Display-only 3D ribbon contours. This bounded scene comes from Rust's exact
 * smoothing/frames/widths and full-sample PCA. No 2D image rotation, occlusion,
 * track construction, joining or plotter ordering runs on the UI thread.
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
    const connected = new Set(molecule.bonds.flat());
    const molecularPaths: P2[][] = molecule.bonds.map(([a, b]) => [points[a]!, points[b]!]);
    points.forEach((p, i) => {
      if (!connected.has(i))
        molecularPaths.push(
          Array.from({ length: 17 }, (_, step): P2 => {
            const angle = (step * Math.PI * 2) / 16;
            return [p[0] + 0.35 * Math.cos(angle), p[1] + 0.35 * Math.sin(angle)];
          }),
        );
    });
    for (const path of molecularPaths)
      for (const p of path)
        for (const j of [0, 1] as const) {
          low[j] = Math.min(low[j], p[j]);
          high[j] = Math.max(high[j], p[j]);
        }
    ribbons.push({ chainId: molecule.chainId, paths: molecularPaths });
  }
  const span: P2 = [high[0] - low[0], high[1] - low[1]];
  const scale = Math.min(
    (o.widthMm - 2 * o.marginMm - o.penWidthMm) / Math.max(span[0], 1e-12),
    (o.heightMm - 2 * o.marginMm - o.penWidthMm) / Math.max(span[1], 1e-12),
  );
  const paths: string[] = [];
  let lengthMm = 0;
  for (const ribbon of ribbons)
    for (const path of ribbon.paths) {
      const fitted = path.map(
        (p): P2 => [
          (p[0] - (low[0] + high[0]) * 0.5) * scale + o.widthMm * 0.5,
          (p[1] - (low[1] + high[1]) * 0.5) * scale + o.heightMm * 0.5,
        ],
      );
      if (!fitted.length || fitted.some((p) => !p.every(Number.isFinite))) continue;
      for (let i = 1; i < fitted.length; i++)
        lengthMm += Math.hypot(
          fitted[i]![0] - fitted[i - 1]![0],
          fitted[i]![1] - fitted[i - 1]![1],
        );
      const color = o.chainColors[ribbon.chainId] ?? "black";
      paths.push(
        `<path stroke="${color}" d="${fitted.map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(4)},${p[1].toFixed(4)}`).join("")}"/>`,
      );
    }
  return {
    svg: `<svg xmlns="http://www.w3.org/2000/svg" width="${o.widthMm}mm" height="${o.heightMm}mm" viewBox="0 0 ${o.widthMm} ${o.heightMm}"><title>Interaction preview</title><desc>Approximate 3D ribbon outlines. Hidden strokes are NOT removed. Not for export.</desc><g fill="none" stroke-width="${o.penWidthMm}" stroke-linecap="round" stroke-linejoin="round">${paths.join("")}</g></svg>`,
    widthMm: o.widthMm,
    heightMm: o.heightMm,
    strokeCount: paths.length,
    lengthMm,
    residueCount: 0,
    elapsedMs: performance.now() - started,
    quality: "preview",
    warnings: [
      "Interaction preview only; refine for exact hidden lines, tracks and physical statistics.",
    ],
  };
}
