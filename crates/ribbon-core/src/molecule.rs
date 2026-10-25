//! Topology-traced molecular bands. Fillets, outlines and terminal U-turns are
//! actual vector geometry, built before visibility clipping (never at a cut).
use crate::{
    math::*,
    model::{fail, RenderError, MAX_EDGES},
    ribbon::Section,
    Molecule,
};
use std::f64::consts::{PI, TAU};

const ARC_STEPS: usize = 32;
// Preserve the established isolated-atom/end-on circle representation.
const CIRCLE_STEPS: usize = 16;
const EPS: f64 = 1e-10;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Profile {
    Band(usize),
    Wire,
    Surface,
}
impl Profile {
    pub(crate) fn tracks(self) -> usize {
        match self {
            Self::Band(lines) => lines + 2,
            Self::Wire => 1,
            Self::Surface => 0,
        }
    }
}
#[derive(Debug)]
pub(crate) struct ProjectedPath {
    pub sections: Vec<Section>,
    pub profile: Profile,
}

/// Shared across molecules, checked before allocating expanded chain geometry.
#[derive(Default)]
pub(crate) struct Budget(usize);
impl Budget {
    fn check(&self, segments: usize) -> Result<(), RenderError> {
        if self.0.saturating_add(segments) > MAX_EDGES {
            fail("Molecular geometry exceeds 1000000 segments; reduce bond width, interior lines or input size")
        } else {
            Ok(())
        }
    }
    fn push(
        &mut self,
        paths: &mut Vec<ProjectedPath>,
        sections: Vec<Section>,
        profile: Profile,
    ) -> Result<(), RenderError> {
        // Surface panels count as work too, even though they draw no tracks.
        let work = (sections.len() - 1) * profile.tracks().max(1);
        self.check(work)?;
        self.0 += work;
        paths.push(ProjectedPath { sections, profile });
        Ok(())
    }
}

pub(crate) fn profile(section: &Section, lines: usize) -> Vec<P2> {
    (0..lines + 2)
        .map(|i| lerp(section.left, section.right, i as f64 / (lines + 1) as f64))
        .collect()
}
fn wire(p: V3) -> Section {
    Section {
        left: [p[0], p[1]],
        right: [p[0], p[1]],
        z: p[2],
        alpha: 0.0,
    }
}
fn normal(d: P2) -> P2 {
    [-d[1], d[0]]
}
fn rotate(p: P2, a: f64) -> P2 {
    [
        p[0] * a.cos() - p[1] * a.sin(),
        p[0] * a.sin() + p[1] * a.cos(),
    ]
}
fn band(p: V3, d: P2, radius: f64) -> Section {
    let offset = mul(normal(d), radius);
    Section {
        left: sub([p[0], p[1]], offset),
        right: add([p[0], p[1]], offset),
        z: p[2],
        alpha: 0.0,
    }
}
fn circle(p: V3, radius: f64) -> Vec<Section> {
    let mut result: Vec<_> = (0..CIRCLE_STEPS)
        .map(|i| {
            wire([
                p[0] + radius * (i as f64 * TAU / CIRCLE_STEPS as f64).cos(),
                p[1] - radius * (i as f64 * TAU / CIRCLE_STEPS as f64).sin(),
                p[2],
            ])
        })
        .collect();
    result.push(result[0].clone());
    result
}

/// A cap has a surface for occlusion and separate nested semicircular tracks.
/// The middle track (odd interior count) simply ends at the terminal atom.
fn cap(
    p: V3,
    outward: P2,
    radius: f64,
    lines: usize,
    paths: &mut Vec<ProjectedPath>,
    budget: &mut Budget,
) -> Result<(), RenderError> {
    let n = normal(outward);
    let mut surface = Vec::with_capacity(ARC_STEPS / 2 + 1);
    for i in 0..=ARC_STEPS / 2 {
        let a = i as f64 * PI / ARC_STEPS as f64;
        let center = add([p[0], p[1]], mul(outward, radius * a.sin()));
        let offset = mul(n, radius * a.cos());
        surface.push(Section {
            left: sub(center, offset),
            right: add(center, offset),
            z: p[2],
            alpha: 0.0,
        });
    }
    budget.push(paths, surface, Profile::Surface)?;
    for j in 0..(lines + 2) / 2 {
        let r = radius * (1.0 - 2.0 * j as f64 / (lines + 1) as f64);
        let mut arc: Vec<_> = (0..=ARC_STEPS)
            .map(|i| {
                let a = i as f64 * PI / ARC_STEPS as f64;
                let q = add(
                    add([p[0], p[1]], mul(n, r * a.cos())),
                    mul(outward, r * a.sin()),
                );
                wire([q[0], q[1], p[2]])
            })
            .collect();
        // Use exact seam coordinates rather than sin(PI)'s tiny residual.
        arc[0] = wire([p[0] + n[0] * r, p[1] + n[1] * r, p[2]]);
        arc[ARC_STEPS] = wire([p[0] - n[0] * r, p[1] - n[1] * r, p[2]]);
        arc.reverse();
        budget.push(paths, arc, Profile::Wire)?;
    }
    Ok(())
}

/// Circular centerline fillets keep all offset tracks parallel. At a projected
/// hairpin too tight for the requested width, locally narrow the band rather
/// than generate inverted offsets, unbounded miters or invented directions.
fn fillet_size(prev: V3, p: V3, next: V3) -> usize {
    let u = unit(sub([p[0], p[1]], [prev[0], prev[1]]), [1.0, 0.0]);
    let v = unit(sub([next[0], next[1]], [p[0], p[1]]), [1.0, 0.0]);
    let turn = cross2(u, v).atan2(dot(u, v)).abs();
    if !(1e-6..=PI - 1e-6).contains(&turn) {
        1
    } else {
        ((turn / PI * ARC_STEPS as f64).ceil() as usize).max(2) + 1
    }
}
fn fillet(prev: V3, p: V3, next: V3, radius: f64) -> Vec<Section> {
    let a = sub([p[0], p[1]], [prev[0], prev[1]]);
    let b = sub([next[0], next[1]], [p[0], p[1]]);
    let (la, lb) = (norm(a), norm(b));
    let (u, v) = (mul(a, 1.0 / la), mul(b, 1.0 / lb));
    let turn = cross2(u, v).atan2(dot(u, v));
    if turn.abs() < 1e-6 {
        return vec![band(p, u, radius)];
    }
    if turn.abs() > PI - 1e-6 {
        return vec![band(p, u, 0.0)];
    }
    let tangent = (turn.abs() * 0.5).tan();
    let trim = (0.35 * la.min(lb)).min((radius * 2.0).max(0.15) * tangent);
    let curve_radius = trim / tangent;
    let local_radius = radius.min(curve_radius * 0.8);
    let entry = sub([p[0], p[1]], mul(u, trim));
    let center = add(entry, mul(normal(u), turn.signum() * curve_radius));
    let za = p[2] + (prev[2] - p[2]) * trim / la;
    let zb = p[2] + (next[2] - p[2]) * trim / lb;
    let steps = ((turn.abs() / PI * ARC_STEPS as f64).ceil() as usize).max(2);
    (0..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let d = rotate(u, turn * t);
            let q = sub(center, mul(normal(d), turn.signum() * curve_radius));
            band([q[0], q[1], za * (1.0 - t) + zb * t], d, local_radius)
        })
        .collect()
}

pub(crate) fn project(
    molecule: &Molecule,
    camera: &Camera,
    width: f64,
    lines: usize,
    budget: &mut Budget,
) -> Result<Vec<ProjectedPath>, RenderError> {
    // Input validation bounds atoms and bonds. This adjacency is linear, never
    // an all-pairs neighbor search, and walking is iterative for long polymers.
    let points: Vec<_> = molecule
        .atoms
        .iter()
        .map(|&p| {
            let p = camera.point(p);
            [p[0], -p[1], p[2]]
        })
        .collect();
    let mut adjacent = vec![Vec::new(); points.len()];
    for (e, &[a, b]) in molecule.bonds.iter().enumerate() {
        adjacent[a].push((b, e));
        adjacent[b].push((a, e));
    }
    let mut paths = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        if adjacent[i].is_empty() {
            budget.check(CIRCLE_STEPS)?;
            budget.push(&mut paths, circle(p, 0.35), Profile::Wire)?;
        }
    }
    if width == 0.0 {
        budget.check(molecule.bonds.len())?;
        for &[a, b] in &molecule.bonds {
            budget.push(
                &mut paths,
                vec![wire(points[a]), wire(points[b])],
                Profile::Wire,
            )?;
        }
        return Ok(paths);
    }
    let radius = width * 0.5;
    // Branch outline joins use angular neighbors, not a guessed chemical
    // continuation. Bounded bevel points live within two radii of the atom;
    // no centerline or interior-track chord is added between separate arms.
    let mut joins: Vec<Vec<(usize, Section)>> = vec![Vec::new(); points.len()];
    for (node, neighbors) in adjacent.iter().enumerate().filter(|(_, n)| n.len() > 2) {
        let p = points[node];
        let origin = [p[0], p[1]];
        let mut rays: Vec<_> = neighbors
            .iter()
            .filter_map(|&(n, _)| {
                let d = sub([points[n][0], points[n][1]], origin);
                (norm(d) > EPS).then(|| (n, unit(d, [1.0, 0.0])))
            })
            .collect();
        rays.sort_by(|a, b| {
            a.1[1]
                .atan2(a.1[0])
                .total_cmp(&b.1[1].atan2(b.1[0]))
                .then(a.0.cmp(&b.0))
        });
        budget.check(rays.len().saturating_mul(2 * ARC_STEPS))?;
        let mut sides: Vec<_> = rays.iter().map(|&(_, d)| band(p, d, radius)).collect();
        for i in 0..rays.len() {
            let j = (i + 1) % rays.len();
            let (u, v) = (rays[i].1, rays[j].1);
            let gap = cross2(u, v).atan2(dot(u, v)).rem_euclid(TAU);
            if gap <= PI {
                let bisector = rotate(u, gap * 0.5);
                let distance = (radius / (gap * 0.5).sin().max(1e-12)).min(2.0 * radius);
                let join = add(origin, mul(bisector, distance));
                sides[i].right = join;
                sides[j].left = join;
            } else {
                let angle = gap - PI;
                let steps = ((angle / PI * ARC_STEPS as f64).ceil() as usize).max(2);
                let arc: Vec<_> = (0..=steps)
                    .map(|k| {
                        let q = add(
                            origin,
                            mul(rotate(normal(u), angle * k as f64 / steps as f64), radius),
                        );
                        wire([q[0], q[1], p[2]])
                    })
                    .collect();
                let surface = arc
                    .iter()
                    .map(|s| Section {
                        left: origin,
                        right: s.left,
                        z: p[2],
                        alpha: 0.0,
                    })
                    .collect();
                budget.push(&mut paths, surface, Profile::Surface)?;
                budget.push(&mut paths, arc, Profile::Wire)?;
            }
        }
        joins[node] = rays.iter().zip(sides).map(|(&(n, _), s)| (n, s)).collect();
    }
    let mut used = vec![false; molecule.bonds.len()];
    // First trace maximal branch/terminal-to-branch/terminal chains, then rings.
    let starts = adjacent
        .iter()
        .enumerate()
        .filter(|(_, n)| n.len() != 2)
        .flat_map(|(a, n)| n.iter().map(move |&(_, e)| (a, e)))
        .chain(molecule.bonds.iter().enumerate().map(|(e, &[a, _])| (a, e)));
    for (start, first) in starts {
        if used[first] {
            continue;
        }
        let mut nodes = vec![start];
        let (mut node, mut edge) = (start, first);
        loop {
            if used[edge] {
                break;
            }
            used[edge] = true;
            let [a, b] = molecule.bonds[edge];
            node = if a == node { b } else { a };
            nodes.push(node);
            if adjacent[node].len() != 2 {
                break;
            }
            let n = &adjacent[node];
            edge = if n[0].1 == edge { n[1].1 } else { n[0].1 };
        }
        // A loop returning to a branch must still use that branch's joins.
        let closed = nodes[0] == *nodes.last().unwrap() && adjacent[start].len() == 2;
        let mut centers: Vec<V3> = Vec::with_capacity(nodes.len());
        for &n in &nodes {
            let p = points[n];
            if let Some(last) = centers.last_mut() {
                if norm(sub([last[0], last[1]], [p[0], p[1]])) < EPS {
                    last[2] = last[2].max(p[2]);
                    continue;
                }
            }
            centers.push(p);
        }
        if centers.len() < 2 {
            budget.check(2 * ARC_STEPS)?;
            let p = centers[0];
            // End-on bands have a disk silhouette, not repeated tracks.
            let disk = (0..=ARC_STEPS)
                .map(|i| {
                    let q = rotate([radius, 0.0], i as f64 * TAU / ARC_STEPS as f64);
                    Section {
                        left: [p[0], p[1]],
                        right: add([p[0], p[1]], q),
                        z: p[2],
                        alpha: 0.0,
                    }
                })
                .collect();
            budget.push(&mut paths, disk, Profile::Surface)?;
            budget.push(&mut paths, circle(p, radius), Profile::Wire)?;
            continue;
        }
        if closed
            && norm(sub(
                [centers[0][0], centers[0][1]],
                [centers.last().unwrap()[0], centers.last().unwrap()[1]],
            )) < EPS
        {
            centers.pop();
        }
        // Count the actual expanded sections without allocating them. Straight
        // long chains are not rejected using a worst-case turn at every atom.
        let section_count: usize = (0..centers.len())
            .map(|i| {
                if closed || (i > 0 && i + 1 < centers.len()) {
                    fillet_size(
                        centers[(i + centers.len() - 1) % centers.len()],
                        centers[i],
                        centers[(i + 1) % centers.len()],
                    )
                } else {
                    1
                }
            })
            .sum::<usize>()
            + usize::from(closed);
        let terminals = if closed {
            0
        } else {
            usize::from(adjacent[nodes[0]].len() == 1)
                + usize::from(adjacent[*nodes.last().unwrap()].len() == 1)
        };
        let cap_work = terminals * (ARC_STEPS / 2 + (lines + 2) / 2 * ARC_STEPS);
        budget.check(
            (section_count - 1)
                .saturating_mul(lines + 2)
                .saturating_add(cap_work),
        )?;
        let mut sections = Vec::with_capacity(section_count);
        for i in 0..centers.len() {
            let p = centers[i];
            if closed || (i > 0 && i + 1 < centers.len()) {
                sections.extend(fillet(
                    centers[(i + centers.len() - 1) % centers.len()],
                    p,
                    centers[(i + 1) % centers.len()],
                    radius,
                ));
            } else {
                let direction = if i == 0 {
                    sub(centers[1], p)
                } else {
                    sub(p, centers[i - 1])
                };
                let mut s = band(p, unit([direction[0], direction[1]], [1.0, 0.0]), radius);
                let (n, other) = if i == 0 {
                    (nodes[0], nodes[1])
                } else {
                    (*nodes.last().unwrap(), nodes[nodes.len() - 2])
                };
                if let Some((_, join)) = joins[n].iter().find(|(neighbor, _)| *neighbor == other) {
                    s = join.clone();
                    if i > 0 {
                        std::mem::swap(&mut s.left, &mut s.right);
                    }
                }
                sections.push(s);
            }
        }
        if closed {
            sections.push(sections[0].clone());
        }
        budget.push(&mut paths, sections, Profile::Band(lines))?;
        if !closed {
            for (i, other, n) in [
                (0, 1, nodes[0]),
                (centers.len() - 1, centers.len() - 2, *nodes.last().unwrap()),
            ] {
                if adjacent[n].len() == 1 {
                    let d = sub(centers[i], centers[other]);
                    cap(
                        centers[i],
                        unit([d[0], d[1]], [1.0, 0.0]),
                        radius,
                        lines,
                        &mut paths,
                        budget,
                    )?;
                }
            }
        }
    }
    Ok(paths)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewMolecule {
    #[serde(flatten)]
    molecule: Molecule,
    /// True input terminals only; a sampled truncation is deliberately absent.
    terminal_atoms: Vec<usize>,
}

/// Preserve shared atom identity while sampling bonds. Terminal metadata comes
/// from the unsampled graph, so a preview cut cannot acquire a false U-turn.
pub(crate) fn preview(molecules: &[&Molecule]) -> Vec<PreviewMolecule> {
    let size: usize = molecules
        .iter()
        .map(|m| m.atoms.len() + m.bonds.len())
        .sum();
    let stride = size.div_ceil(2000).max(1);
    let mut cursor = 0;
    molecules
        .iter()
        .filter_map(|m| {
            let mut atoms = Vec::new();
            let mut bonds = Vec::new();
            let mut terminal_atoms = Vec::new();
            let mut degrees = vec![0; m.atoms.len()];
            for &[a, b] in &m.bonds {
                degrees[a] += 1;
                degrees[b] += 1;
            }
            let mut indices = vec![None; m.atoms.len()];
            for &[a, b] in &m.bonds {
                if cursor % stride == 0 {
                    let mut bond = [0; 2];
                    for (j, n) in [a, b].into_iter().enumerate() {
                        let id = *indices[n].get_or_insert_with(|| {
                            let id = atoms.len();
                            atoms.push(m.atoms[n]);
                            if degrees[n] == 1 {
                                terminal_atoms.push(id);
                            }
                            id
                        });
                        bond[j] = id;
                    }
                    bonds.push(bond);
                }
                cursor += 1;
            }
            for (i, &p) in m.atoms.iter().enumerate() {
                if degrees[i] != 0 {
                    continue;
                }
                if cursor % stride == 0 {
                    atoms.push(p);
                }
                cursor += 1;
            }
            (!atoms.is_empty()).then(|| PreviewMolecule {
                molecule: Molecule {
                    chain_id: m.chain_id.clone(),
                    label: m.label.clone(),
                    category: m.category.clone(),
                    atoms,
                    bonds,
                },
                terminal_atoms,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
