type P2 = [number, number];
type Section = { left: P2; right: P2 };
const PI = Math.PI;
const STEPS = 32;
const CIRCLE_STEPS = 16;
const add = (a: P2, b: P2): P2 => [a[0] + b[0], a[1] + b[1]];
const sub = (a: P2, b: P2): P2 => [a[0] - b[0], a[1] - b[1]];
const mul = (a: P2, n: number): P2 => [a[0] * n, a[1] * n];
const norm = (a: P2) => Math.hypot(...a);
const unit = (a: P2) => mul(a, 1 / norm(a));
const normal = (a: P2): P2 => [-a[1], a[0]];
const cross = (a: P2, b: P2) => a[0] * b[1] - a[1] * b[0];
const dot = (a: P2, b: P2) => a[0] * b[0] + a[1] * b[1];
const rotate = (a: P2, t: number): P2 => [
  a[0] * Math.cos(t) - a[1] * Math.sin(t),
  a[0] * Math.sin(t) + a[1] * Math.cos(t),
];
const band = (p: P2, d: P2, r: number): Section => ({
  left: sub(p, mul(normal(d), r)),
  right: add(p, mul(normal(d), r)),
});
const circle = (p: P2, r: number): P2[] => {
  const path = Array.from({ length: CIRCLE_STEPS }, (_, i) =>
    add(p, rotate([r, 0], (i * PI * 2) / CIRCLE_STEPS)),
  );
  path.push(path[0]!);
  return path;
};
function fillet(before: P2, p: P2, after: P2, radius: number): Section[] {
  const a = sub(p, before),
    b = sub(after, p);
  const la = norm(a),
    lb = norm(b),
    u = unit(a),
    v = unit(b);
  const turn = Math.atan2(cross(u, v), dot(u, v));
  if (Math.abs(turn) < 1e-6) return [band(p, u, radius)];
  if (Math.abs(turn) > PI - 1e-6) return [band(p, u, 0)];
  const tangent = Math.tan(Math.abs(turn) / 2);
  const trim = Math.min(0.35 * Math.min(la, lb), Math.max(radius * 2, 0.15) * tangent);
  const curveRadius = trim / tangent;
  const r = Math.min(radius, curveRadius * 0.8);
  const entry = sub(p, mul(u, trim));
  const center = add(entry, mul(normal(u), Math.sign(turn) * curveRadius));
  const steps = Math.max(2, Math.ceil((Math.abs(turn) / PI) * STEPS));
  return Array.from({ length: steps + 1 }, (_, i) => {
    const d = rotate(u, (turn * i) / steps);
    return band(sub(center, mul(normal(d), Math.sign(turn) * curveRadius)), d, r);
  });
}
function cap(p: P2, outward: P2, r: number): P2[] {
  const n = normal(outward);
  const path = Array.from({ length: STEPS + 1 }, (_, i) => {
    const a = (i * PI) / STEPS;
    return add(add(p, mul(n, r * Math.cos(a))), mul(outward, r * Math.sin(a)));
  });
  path[0] = add(p, mul(n, r));
  path[STEPS] = sub(p, mul(n, r));
  return path.reverse();
}

/** Outlines only, mirroring Rust's circular chain fillets and bounded branch
 * joins. Input is the bounded prepared scene, not user structure data. Source
 * terminal metadata prevents rounded caps on sampled-preview truncations.
 * There is deliberately no visibility or interior-track work on the UI thread.
 */
export function molecularPreviewContours(
  points: P2[],
  bonds: [number, number][],
  width: number,
  terminalAtoms?: number[],
): P2[][] {
  // Defense in depth for malformed/unbounded display scenes. Exact rendering
  // has its own error-reporting budget and never silently reduces quality.
  if (points.length + bonds.length > 12000) return [];
  const adjacent: [number, number][][] = points.map(() => []);
  for (const [e, [a, b]] of bonds.entries()) {
    adjacent[a]!.push([b, e]);
    adjacent[b]!.push([a, e]);
  }
  const paths: P2[][] = points.flatMap((p, i) => (adjacent[i]!.length ? [] : [circle(p, 0.35)]));
  if (!width) return [...paths, ...bonds.map(([a, b]) => [points[a]!, points[b]!])];
  const radius = width * 0.5;
  const terminals =
    terminalAtoms === undefined
      ? new Set(adjacent.flatMap((n, i) => (n.length === 1 ? [i] : [])))
      : new Set(terminalAtoms);
  const joins = points.map(() => new Map<number, Section>());
  for (const [node, neighbors] of adjacent.entries()) {
    if (neighbors.length <= 2) continue;
    const p = points[node]!;
    const rays = neighbors
      .flatMap(([n]) => {
        const d = sub(points[n]!, p);
        return norm(d) > 1e-10 ? [{ n, d: unit(d) }] : [];
      })
      .sort((a, b) => Math.atan2(a.d[1], a.d[0]) - Math.atan2(b.d[1], b.d[0]) || a.n - b.n);
    const sides = rays.map(({ d }) => band(p, d, radius));
    for (let i = 0; i < rays.length; i++) {
      const j = (i + 1) % rays.length,
        u = rays[i]!.d,
        v = rays[j]!.d;
      const angle = Math.atan2(cross(u, v), dot(u, v));
      const gap = (angle + PI * 2) % (PI * 2);
      if (gap <= PI) {
        const distance = Math.min(radius / Math.max(Math.sin(gap / 2), 1e-12), radius * 2);
        const join = add(p, mul(rotate(u, gap / 2), distance));
        sides[i]!.right = join;
        sides[j]!.left = join;
      } else {
        const a = gap - PI,
          steps = Math.max(2, Math.ceil((a / PI) * STEPS));
        paths.push(
          Array.from({ length: steps + 1 }, (_, k) =>
            add(p, mul(rotate(normal(u), (a * k) / steps), radius)),
          ),
        );
      }
    }
    rays.forEach(({ n }, i) => joins[node]!.set(n, sides[i]!));
  }
  const used = new Set<number>();
  const starts: [number, number][] = adjacent.flatMap((n, i) =>
    n.length !== 2 ? n.map(([, e]): [number, number] => [i, e]) : [],
  );
  starts.push(...bonds.map(([a], e): [number, number] => [a, e]));
  for (const [start, first] of starts) {
    if (used.has(first)) continue;
    const nodes = [start];
    let node = start,
      edge = first;
    while (!used.has(edge)) {
      used.add(edge);
      const [a, b] = bonds[edge]!;
      node = a === node ? b : a;
      nodes.push(node);
      if (adjacent[node]!.length !== 2) break;
      const n = adjacent[node]!;
      edge = n[0]![1] === edge ? n[1]![1] : n[0]![1];
    }
    const closed = nodes[0] === nodes.at(-1) && adjacent[start]!.length === 2;
    const centers: P2[] = [];
    for (const n of nodes) {
      const p = points[n]!;
      if (!centers.length || norm(sub(centers.at(-1)!, p)) >= 1e-10) centers.push(p);
    }
    if (centers.length < 2) {
      paths.push(circle(centers[0]!, radius));
      continue;
    }
    if (closed && norm(sub(centers[0]!, centers.at(-1)!)) < 1e-10) centers.pop();
    const sections: Section[] = [];
    for (const [i, p] of centers.entries()) {
      if (closed || (i > 0 && i + 1 < centers.length)) {
        sections.push(
          ...fillet(
            centers[(i + centers.length - 1) % centers.length]!,
            p,
            centers[(i + 1) % centers.length]!,
            radius,
          ),
        );
      } else {
        const d = i === 0 ? sub(centers[1]!, p) : sub(p, centers[i - 1]!);
        const n = i === 0 ? nodes[0]! : nodes.at(-1)!;
        const other = i === 0 ? nodes[1]! : nodes.at(-2)!;
        const join = joins[n]!.get(other);
        sections.push(
          join
            ? i === 0
              ? join
              : { left: join.right, right: join.left }
            : band(p, unit(d), radius),
        );
      }
    }
    if (closed) sections.push(sections[0]!);
    const left = sections.map((s) => s.left),
      right = sections.map((s) => s.right).reverse();
    if (closed) {
      paths.push(left, right);
      continue;
    }
    const endCap = terminals.has(nodes.at(-1)!);
    const startCap = terminals.has(nodes[0]!);
    if (endCap)
      left.push(
        ...cap(centers.at(-1)!, unit(sub(centers.at(-1)!, centers.at(-2)!)), radius).slice(1),
        ...right.slice(1),
      );
    else if (startCap) {
      right.push(
        ...cap(centers[0]!, unit(sub(centers[0]!, centers[1]!)), radius).slice(1),
        ...left.slice(1),
      );
      paths.push(right);
      continue;
    }
    if (startCap)
      left.push(...cap(centers[0]!, unit(sub(centers[0]!, centers[1]!)), radius).slice(1));
    paths.push(left);
    if (!endCap) paths.push(right);
  }
  return paths;
}
