//! Sliced molecular surface contours for pen plotting.
//!
//! Builds a 3D Gaussian occupancy field from heavy atoms, extracts section
//! contours (parallel slices, steerable cross-hatch lattices, or continuous
//! helical surface spirals) via edge-indexed marching squares, and encodes 3D
//! depth for single-pen plotters through parallel contour tracks and multi-pass
//! overdraw.

use crate::math::{self, add, dot, lerp, mul, norm, sub, Camera, P2, V3};
use crate::model::{
    fail, RenderError, RenderMode, RenderOptions, RenderRequest, RenderResult, SliceAxis,
    SlicePattern, StrokeDirection, StructureData, MAX_PIECES,
};
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use std::collections::{BTreeMap, HashSet};
use std::f64::consts::PI;
use std::fmt::Write;

const BLOB: f64 = 0.55;
const ISO: f32 = 0.19;
const FOC: f64 = 3.4;
const MAX_VOXELS: usize = 6_000_000;
const PREVIEW_MAX_VOXELS: usize = 1_200_000;
const MIN_LENGTH_MM: f64 = 0.08;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SliceKey {
    colored_chains: Vec<(String, String)>,
    camera_sig: Option<[u64; 12]>,
    axis: SliceAxis,
    slice_rotation: [u64; 3],
    view_rotation: Option<[u64; 3]>,
    pattern: SlicePattern,
    slice_count: usize,
    hatch_angle_bits: u64,
    spiral_density_bits: u64,
    coarseness: usize,
    probe_bits: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedSliceGroup {
    pub(crate) color: String,
    pub(crate) loops: Vec<Vec<V3>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) polylines: Vec<Vec<V3>>,
}

#[derive(Clone)]
pub(crate) struct PreparedSlices {
    pub(crate) key: SliceKey,
    pub(crate) camera: Camera,
    pub(crate) groups: Vec<PreparedSliceGroup>,
    pub(crate) radius: f64,
    pub(crate) residue_count: usize,
    pub(crate) warnings: Vec<String>,
}

fn camera_signature(camera: &Camera) -> [u64; 12] {
    let mut out = [0u64; 12];
    for i in 0..3 {
        out[i] = (camera.center[i] + 0.0).to_bits();
        for j in 0..3 {
            out[3 + i * 3 + j] = (camera.basis[i][j] + 0.0).to_bits();
        }
    }
    out
}

impl SliceKey {
    pub(crate) fn new(
        structure: &StructureData,
        options: &RenderOptions,
        override_camera: Option<&Camera>,
    ) -> Self {
        let wanted: HashSet<&str> = options
            .selected_chains
            .iter()
            .filter(|id| options.mode_of(id) == RenderMode::Slice)
            .map(String::as_str)
            .collect();
        let colored_chains = structure
            .chains
            .iter()
            .filter(|c| wanted.contains(c.id.as_str()))
            .map(|c| {
                let color = options
                    .chain_colors
                    .get(&c.id)
                    .cloned()
                    .unwrap_or_else(|| "black".into());
                (c.id.clone(), color)
            })
            .collect();
        let view_rotation = (options.slice_axis == SliceAxis::View)
            .then(|| options.rotation.map(|angle| (angle + 0.0).to_bits()));
        let slice_rotation = options.slice_rotation.map(|angle| (angle + 0.0).to_bits());
        let (slice_count, hatch_angle_bits, spiral_density_bits) = match options.slice_pattern {
            SlicePattern::Slices => (options.slice_count, 0, 0),
            SlicePattern::Crosshatch => (
                options.slice_count,
                (options.slice_hatch_angle + 0.0).to_bits(),
                0,
            ),
            SlicePattern::Spiral => (0, 0, (options.slice_spiral_density + 0.0).to_bits()),
        };
        Self {
            colored_chains,
            camera_sig: override_camera.map(camera_signature),
            axis: options.slice_axis,
            slice_rotation,
            view_rotation,
            pattern: options.slice_pattern,
            slice_count,
            hatch_angle_bits,
            spiral_density_bits,
            coarseness: options.slice_coarseness,
            probe_bits: (options.slice_probe_angstrom + 0.0).to_bits(),
        }
    }
}

pub(crate) fn collect_atoms_by_color(
    structure: &StructureData,
    options: &RenderOptions,
) -> (BTreeMap<String, Vec<(V3, f64)>>, usize) {
    let selected: HashSet<&str> = options
        .selected_chains
        .iter()
        .filter(|id| options.mode_of(id) == RenderMode::Slice)
        .map(String::as_str)
        .collect();
    let color_of = |id: &str| {
        options
            .chain_colors
            .get(id)
            .cloned()
            .unwrap_or_else(|| "black".into())
    };
    let mut residue_count = 0usize;
    for segment in &structure.segments {
        if selected.contains(segment.chain_id.as_str()) {
            residue_count += segment.residues.len();
        }
    }
    let mut by_color: BTreeMap<String, Vec<(V3, f64)>> = BTreeMap::new();
    let mut covered_chains = HashSet::new();
    for entry in &structure.chain_atoms {
        if !selected.contains(entry.chain_id.as_str()) {
            continue;
        }
        covered_chains.insert(entry.chain_id.as_str());
        let bucket = by_color.entry(color_of(&entry.chain_id)).or_default();
        for (i, &pos) in entry.atoms.iter().enumerate() {
            let r = entry.radii.get(i).copied().unwrap_or(1.70);
            bucket.push((pos, r));
        }
    }
    for segment in &structure.segments {
        if !selected.contains(segment.chain_id.as_str())
            || covered_chains.contains(segment.chain_id.as_str())
        {
            continue;
        }
        let bucket = by_color.entry(color_of(&segment.chain_id)).or_default();
        for (i, residue) in segment.residues.iter().enumerate() {
            bucket.push((residue.ca, 1.85));
            let o_dir = math::unit(residue.carbonyl, [0.0, 1.0, 0.0]);
            bucket.push((add(residue.ca, mul(o_dir, 1.50)), 1.52));
            if i > 0 {
                bucket.push((lerp(segment.residues[i - 1].ca, residue.ca, 0.5), 1.65));
            }
        }
    }
    for molecule in &structure.molecules {
        if !selected.contains(molecule.chain_id.as_str()) {
            continue;
        }
        let bucket = by_color.entry(color_of(&molecule.chain_id)).or_default();
        for (i, &pos) in molecule.atoms.iter().enumerate() {
            let r = molecule.radii.get(i).copied().unwrap_or(1.70);
            bucket.push((pos, r));
        }
    }
    (by_color, residue_count)
}

fn section_basis(camera: &Camera, options: &RenderOptions) -> [V3; 3] {
    let b = camera.basis;
    let base = match options.slice_axis {
        SliceAxis::Z => b,
        SliceAxis::Y => [b[0], mul(b[2], -1.0), b[1]],
        SliceAxis::X => [b[1], b[2], b[0]],
        SliceAxis::View => camera.rotated(options.rotation).basis,
    };
    Camera {
        center: camera.center,
        basis: base,
    }
    .rotated(options.slice_rotation)
    .basis
}

struct ScalarField {
    grid: Vec<f32>,
    nx: usize,
    ny: usize,
    nz: usize,
    h: f64,
    ox: f64,
    oy: f64,
    oz: f64,
    low: [f64; 3],
    high: [f64; 3],
    coarsened: bool,
}

fn build_field(
    atoms: &[(V3, f64)],
    center: V3,
    basis: [V3; 3],
    coarseness: usize,
    probe: f64,
    voxel_cap: usize,
) -> ScalarField {
    let mut local = Vec::with_capacity(atoms.len());
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    let mut max_cut = 0.0f64;

    for &(pos, radius) in atoms {
        let rel = sub(pos, center);
        let p = [dot(basis[0], rel), dot(basis[1], rel), dot(basis[2], rel)];
        let sigma = (BLOB * (radius + probe)).max(0.30);
        let cut = 3.0 * sigma;
        max_cut = max_cut.max(cut);
        for axis in 0..3 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
        local.push((p, sigma));
    }

    let c = coarseness as f64 / 100.0;
    let initial_h = 0.55 + 1.75 * c * c;
    let mut h = initial_h;
    let (nx, ny, nz) = loop {
        let pad = max_cut + h;
        let nx = (((high[0] - low[0] + 2.0 * pad) / h).ceil() as usize).saturating_add(1);
        let ny = (((high[1] - low[1] + 2.0 * pad) / h).ceil() as usize).saturating_add(1);
        let nz = (((high[2] - low[2] + 2.0 * pad) / h).ceil() as usize).saturating_add(1);
        if nx.saturating_mul(ny).saturating_mul(nz) <= voxel_cap {
            break (nx.max(2), ny.max(2), nz.max(2));
        }
        h *= 1.15;
    };

    let pad = max_cut + h;
    let (ox, oy, oz) = (low[0] - pad, low[1] - pad, low[2] - pad);
    let mut grid = vec![0.0f32; nx * ny * nz];
    let plane_stride = nx * ny;

    for &(p, sigma) in &local {
        let inv_2s2 = 1.0 / (2.0 * sigma * sigma);
        let cut = 3.0 * sigma;
        let cut2 = cut * cut;
        let i0 = ((p[0] - cut - ox) / h).floor().max(0.0) as usize;
        let i1 = (((p[0] + cut - ox) / h).ceil().max(0.0) as usize).min(nx - 1);
        let j0 = ((p[1] - cut - oy) / h).floor().max(0.0) as usize;
        let j1 = (((p[1] + cut - oy) / h).ceil().max(0.0) as usize).min(ny - 1);
        let k0 = ((p[2] - cut - oz) / h).floor().max(0.0) as usize;
        let k1 = (((p[2] + cut - oz) / h).ceil().max(0.0) as usize).min(nz - 1);

        for k in k0..=k1 {
            let dz = oz + k as f64 * h - p[2];
            let dz2 = dz * dz;
            if dz2 > cut2 {
                continue;
            }
            let kb = k * plane_stride;
            for j in j0..=j1 {
                let dy = oy + j as f64 * h - p[1];
                let dzy = dz2 + dy * dy;
                if dzy > cut2 {
                    continue;
                }
                let jb = kb + j * nx;
                for i in i0..=i1 {
                    let dx = ox + i as f64 * h - p[0];
                    let d2 = dzy + dx * dx;
                    if d2 <= cut2 {
                        grid[jb + i] += (-d2 * inv_2s2).exp() as f32;
                    }
                }
            }
        }
    }

    let blur_passes = (1.0 + 3.0 * c).round() as usize;
    let mut line = vec![0.0f32; nx.max(ny).max(nz)];
    for _ in 0..blur_passes {
        box_blur_3d(&mut grid, nx, ny, nz, &mut line);
    }

    ScalarField {
        grid,
        nx,
        ny,
        nz,
        h,
        ox,
        oy,
        oz,
        low,
        high,
        coarsened: h > initial_h * 1.01,
    }
}

fn box_blur_3d(grid: &mut [f32], nx: usize, ny: usize, nz: usize, line: &mut [f32]) {
    let plane = nx * ny;
    for k in 0..nz {
        for j in 0..ny {
            let base = k * plane + j * nx;
            line[..nx].copy_from_slice(&grid[base..base + nx]);
            for i in 0..nx {
                let left = line[i.saturating_sub(1)];
                let right = line[(i + 1).min(nx - 1)];
                grid[base + i] = (left + line[i] + right) * (1.0 / 3.0);
            }
        }
    }
    for k in 0..nz {
        let kb = k * plane;
        for i in 0..nx {
            for j in 0..ny {
                line[j] = grid[kb + j * nx + i];
            }
            for j in 0..ny {
                let down = line[j.saturating_sub(1)];
                let up = line[(j + 1).min(ny - 1)];
                grid[kb + j * nx + i] = (down + line[j] + up) * (1.0 / 3.0);
            }
        }
    }
    for j in 0..ny {
        let jb = j * nx;
        for i in 0..nx {
            for k in 0..nz {
                line[k] = grid[k * plane + jb + i];
            }
            for k in 0..nz {
                let back = line[k.saturating_sub(1)];
                let front = line[(k + 1).min(nz - 1)];
                grid[k * plane + jb + i] = (back + line[k] + front) * (1.0 / 3.0);
            }
        }
    }
}

fn plane_at(field: &ScalarField, z: f64, out: &mut [f32]) {
    let plane = field.nx * field.ny;
    let fk = (z - field.oz) / field.h;
    let (k, t) = if fk <= 0.0 {
        (0, 0.0f32)
    } else if fk >= (field.nz - 1) as f64 {
        (field.nz - 2, 1.0f32)
    } else {
        let k = fk.floor() as usize;
        (k, (fk - k as f64) as f32)
    };
    let a = k * plane;
    let b = a + plane;
    let one_minus_t = 1.0 - t;
    for (i, slot) in out.iter_mut().enumerate().take(plane) {
        *slot = field.grid[a + i] * one_minus_t + field.grid[b + i] * t;
    }
}

fn sample_trilinear(field: &ScalarField, u: f64, v: f64, w: f64) -> f32 {
    let fx = (u - field.ox) / field.h;
    let fy = (v - field.oy) / field.h;
    let fz = (w - field.oz) / field.h;
    if fx < 0.0
        || fy < 0.0
        || fz < 0.0
        || fx >= (field.nx - 1) as f64
        || fy >= (field.ny - 1) as f64
        || fz >= (field.nz - 1) as f64
    {
        return 0.0;
    }
    let i = (fx.floor() as usize).min(field.nx - 2);
    let j = (fy.floor() as usize).min(field.ny - 2);
    let k = (fz.floor() as usize).min(field.nz - 2);
    let tx = (fx - i as f64) as f32;
    let ty = (fy - j as f64) as f32;
    let tz = (fz - k as f64) as f32;
    let plane = field.nx * field.ny;
    let idx00 = k * plane + j * field.nx + i;
    let idx10 = idx00 + field.nx;
    let idx01 = idx00 + plane;
    let idx11 = idx01 + field.nx;
    let c00 = field.grid[idx00] * (1.0 - tx) + field.grid[idx00 + 1] * tx;
    let c10 = field.grid[idx10] * (1.0 - tx) + field.grid[idx10 + 1] * tx;
    let c01 = field.grid[idx01] * (1.0 - tx) + field.grid[idx01 + 1] * tx;
    let c11 = field.grid[idx11] * (1.0 - tx) + field.grid[idx11 + 1] * tx;
    let c0 = c00 * (1.0 - ty) + c10 * ty;
    let c1 = c01 * (1.0 - ty) + c11 * ty;
    c0 * (1.0 - tz) + c1 * tz
}

/// Exact grid-edge marching squares returning (closed_loops, open_arcs).
/// Every crossed grid edge receives a unique integer ID and consistent CCW
/// orientation around v > ISO. Boundary-clipped chains without incoming edges
/// are traced first as unbroken open arcs; remaining cycles form closed loops.
fn marching_squares_paths(
    plane: &[f32],
    nx: usize,
    ny: usize,
    ox: f64,
    oy: f64,
    h: f64,
) -> (Vec<Vec<P2>>, Vec<Vec<P2>>) {
    if nx < 2 || ny < 2 {
        return (Vec::new(), Vec::new());
    }
    let h_count = ny * nx;
    let total_edges = h_count + (ny - 1) * (nx + 1);
    let mut edge_point = vec![[0.0; 2]; total_edges];
    let mut next_edge = vec![u32::MAX; total_edges];
    let mut has_prev = vec![false; total_edges];
    let mut active_starts = Vec::new();

    let interp = |va: f32, vb: f32| -> f64 {
        let denom = (vb - va) as f64;
        if denom.abs() > 1e-12 {
            ((ISO - va) as f64 / denom).clamp(0.0, 1.0)
        } else {
            0.5
        }
    };

    let mut add_seg = |from: usize, fp: P2, to: usize, tp: P2| {
        edge_point[from] = fp;
        edge_point[to] = tp;
        next_edge[from] = to as u32;
        has_prev[to] = true;
        active_starts.push(from as u32);
    };

    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let o = j * nx + i;
            let (v0, v1, v2, v3) = (plane[o], plane[o + 1], plane[o + nx + 1], plane[o + nx]);
            let mask = u8::from(v0 > ISO)
                | (u8::from(v1 > ISO) << 1)
                | (u8::from(v2 > ISO) << 2)
                | (u8::from(v3 > ISO) << 3);
            if mask == 0 || mask == 15 {
                continue;
            }

            let ea = j * nx + i;
            let eb = h_count + j * (nx + 1) + i + 1;
            let ec = (j + 1) * nx + i;
            let ed = h_count + j * (nx + 1) + i;

            let pa = [ox + (i as f64 + interp(v0, v1)) * h, oy + j as f64 * h];
            let pb = [
                ox + (i + 1) as f64 * h,
                oy + (j as f64 + interp(v1, v2)) * h,
            ];
            let pc = [
                ox + (i as f64 + interp(v3, v2)) * h,
                oy + (j + 1) as f64 * h,
            ];
            let pd = [ox + i as f64 * h, oy + (j as f64 + interp(v0, v3)) * h];

            match mask {
                1 => add_seg(ea, pa, ed, pd),
                2 => add_seg(eb, pb, ea, pa),
                3 => add_seg(eb, pb, ed, pd),
                4 => add_seg(ec, pc, eb, pb),
                5 => {
                    if (v0 + v1 + v2 + v3) * 0.25 > ISO {
                        add_seg(ea, pa, eb, pb);
                        add_seg(ec, pc, ed, pd);
                    } else {
                        add_seg(ea, pa, ed, pd);
                        add_seg(ec, pc, eb, pb);
                    }
                }
                6 => add_seg(ec, pc, ea, pa),
                7 => add_seg(ec, pc, ed, pd),
                8 => add_seg(ed, pd, ec, pc),
                9 => add_seg(ea, pa, ec, pc),
                10 => {
                    if (v0 + v1 + v2 + v3) * 0.25 > ISO {
                        add_seg(ed, pd, ea, pa);
                        add_seg(eb, pb, ec, pc);
                    } else {
                        add_seg(eb, pb, ea, pa);
                        add_seg(ed, pd, ec, pc);
                    }
                }
                11 => add_seg(eb, pb, ec, pc),
                12 => add_seg(ed, pd, eb, pb),
                13 => add_seg(ea, pa, eb, pb),
                14 => add_seg(ed, pd, ea, pa),
                _ => {}
            }
        }
    }

    let mut visited = vec![false; total_edges];
    let mut open_arcs = Vec::new();
    for &start_id in &active_starts {
        let start = start_id as usize;
        if visited[start] || has_prev[start] {
            continue;
        }
        let mut pts = Vec::new();
        let mut cur = start;
        while cur != u32::MAX as usize && !visited[cur] {
            visited[cur] = true;
            pts.push(edge_point[cur]);
            cur = next_edge[cur] as usize;
        }
        if cur != u32::MAX as usize && pts.last() != Some(&edge_point[cur]) {
            pts.push(edge_point[cur]);
        }
        if pts.len() >= 2 {
            open_arcs.push(pts);
        }
    }

    let mut loops = Vec::new();
    for start_id in active_starts {
        let start = start_id as usize;
        if visited[start] {
            continue;
        }
        let mut pts = Vec::new();
        let mut cur = start;
        while cur != u32::MAX as usize && !visited[cur] {
            visited[cur] = true;
            pts.push(edge_point[cur]);
            cur = next_edge[cur] as usize;
        }
        if pts.len() >= 3 {
            loops.push(pts);
        }
    }
    (loops, open_arcs)
}

fn rdp_open(pts: &[P2], tol: f64) -> Vec<P2> {
    let n = pts.len();
    if n < 3 || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let tol2 = tol * tol;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let v = sub(pts[b], pts[a]);
        let len2 = dot(v, v);
        let mut best = None;
        let mut best_d2 = tol2;
        for (i, &point) in pts.iter().enumerate().take(b).skip(a + 1) {
            let p = sub(point, pts[a]);
            let d2 = if len2 <= 1e-24 {
                dot(p, p)
            } else {
                let cr = p[0] * v[1] - p[1] * v[0];
                cr * cr / len2
            };
            if d2 > best_d2 {
                best_d2 = d2;
                best = Some(i);
            }
        }
        if let Some(mid) = best {
            keep[mid] = true;
            stack.push((a, mid));
            stack.push((mid, b));
        }
    }
    pts.iter()
        .zip(keep)
        .filter_map(|(&p, k)| k.then_some(p))
        .collect()
}

fn rdp_open_3d(pts: &[V3], tol: f64) -> Vec<V3> {
    let n = pts.len();
    if n < 3 || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let tol2 = tol * tol;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let v = sub(pts[b], pts[a]);
        let len2 = dot(v, v);
        let mut best = None;
        let mut best_d2 = tol2;
        for (i, &point) in pts.iter().enumerate().take(b).skip(a + 1) {
            let p = sub(point, pts[a]);
            let d2 = if len2 <= 1e-24 {
                dot(p, p)
            } else {
                let cr = math::cross(p, v);
                dot(cr, cr) / len2
            };
            if d2 > best_d2 {
                best_d2 = d2;
                best = Some(i);
            }
        }
        if let Some(mid) = best {
            keep[mid] = true;
            stack.push((a, mid));
            stack.push((mid, b));
        }
    }
    let mut dedup: Vec<V3> = Vec::new();
    for (&p, k) in pts.iter().zip(keep) {
        if k && dedup.last().is_none_or(|&prev| norm(sub(p, prev)) > 1e-6) {
            dedup.push(p);
        }
    }
    dedup
}

fn simplify_closed_loop(loop_pts: &[P2], tol: f64) -> Option<Vec<P2>> {
    if loop_pts.len() < 3 {
        return None;
    }
    let start = (0..loop_pts.len())
        .min_by(|&a, &b| {
            loop_pts[a][0]
                .total_cmp(&loop_pts[b][0])
                .then(loop_pts[a][1].total_cmp(&loop_pts[b][1]))
        })
        .unwrap_or(0);
    let n = loop_pts.len();
    let rotated: Vec<P2> = (0..n).map(|k| loop_pts[(start + k) % n]).collect();
    let opposite = (1..n)
        .max_by(|&a, &b| {
            let da = sub(rotated[a], rotated[0]);
            let db = sub(rotated[b], rotated[0]);
            dot(da, da).total_cmp(&dot(db, db))
        })
        .unwrap_or(n / 2);
    if norm(sub(rotated[opposite], rotated[0])) <= tol {
        return None;
    }
    let first_half = rdp_open(&rotated[..=opposite], tol);
    let mut second_input = rotated[opposite..].to_vec();
    second_input.push(rotated[0]);
    let second_half = rdp_open(&second_input, tol);

    let mut out = first_half;
    if second_half.len() > 2 {
        out.extend_from_slice(&second_half[1..second_half.len() - 1]);
    }
    let mut dedup: Vec<P2> = Vec::with_capacity(out.len());
    for p in out {
        if dedup.last().is_none_or(|&prev| norm(sub(p, prev)) > 1e-6) {
            dedup.push(p);
        }
    }
    if dedup.len() > 1 && norm(sub(*dedup.last().unwrap(), dedup[0])) <= 1e-6 {
        dedup.pop();
    }
    (dedup.len() >= 3).then_some(dedup)
}

fn box_corners(low: [f64; 3], high: [f64; 3]) -> [V3; 8] {
    [
        [low[0], low[1], low[2]],
        [high[0], low[1], low[2]],
        [low[0], high[1], low[2]],
        [high[0], high[1], low[2]],
        [low[0], low[1], high[2]],
        [high[0], low[1], high[2]],
        [low[0], high[1], high[2]],
        [high[0], high[1], high[2]],
    ]
}

#[allow(clippy::too_many_arguments)]
fn extract_parallel_slices(
    field: &ScalarField,
    center: V3,
    basis: [V3; 3],
    z_lo: f64,
    z_hi: f64,
    slice_count: usize,
    simplify_tol: f64,
    max_r2: &mut f64,
) -> Vec<Vec<V3>> {
    let mut plane = vec![0.0f32; field.nx * field.ny];
    let mut loops = Vec::new();
    for i in 0..slice_count {
        let w = z_lo + (i as f64 + 0.5) * (z_hi - z_lo) / slice_count as f64;
        if w < field.oz - field.h || w > field.oz + field.nz as f64 * field.h {
            continue;
        }
        plane_at(field, w, &mut plane);
        let (raw_loops, _) =
            marching_squares_paths(&plane, field.nx, field.ny, field.ox, field.oy, field.h);
        for raw_loop in raw_loops {
            let Some(simplified) = simplify_closed_loop(&raw_loop, simplify_tol) else {
                continue;
            };
            let mut loop_3d = Vec::with_capacity(simplified.len());
            for [u, v] in simplified {
                let rel = add(add(mul(basis[0], u), mul(basis[1], v)), mul(basis[2], w));
                *max_r2 = max_r2.max(dot(rel, rel));
                loop_3d.push(add(center, rel));
            }
            loops.push(loop_3d);
        }
    }
    loops
}

#[allow(clippy::too_many_arguments)]
fn extract_crosshatch_slices(
    field: &ScalarField,
    center: V3,
    basis: [V3; 3],
    global_low: [f64; 3],
    global_high: [f64; 3],
    slice_count: usize,
    hatch_angle_deg: f64,
    simplify_tol: f64,
    max_r2: &mut f64,
) -> Vec<Vec<V3>> {
    let half = 0.5 * hatch_angle_deg.to_radians();
    let global_corners = box_corners(global_low, global_high);
    let field_corners = box_corners(
        [field.ox, field.oy, field.oz],
        [
            field.ox + (field.nx - 1) as f64 * field.h,
            field.oy + (field.ny - 1) as f64 * field.h,
            field.oz + (field.nz - 1) as f64 * field.h,
        ],
    );
    let h = field.h;
    let mut loops = Vec::new();
    let mut plane = Vec::new();

    for theta in [-half, half] {
        let (st, ct) = theta.sin_cos();
        let e0: V3 = [ct, 0.0, -st];
        let e2: V3 = [st, 0.0, ct];

        let mut s2_lo = f64::INFINITY;
        let mut s2_hi = f64::NEG_INFINITY;
        for c in &global_corners {
            let p = dot(e2, *c);
            s2_lo = s2_lo.min(p);
            s2_hi = s2_hi.max(p);
        }

        let mut f0_lo = f64::INFINITY;
        let mut f0_hi = f64::NEG_INFINITY;
        let mut f2_lo = f64::INFINITY;
        let mut f2_hi = f64::NEG_INFINITY;
        for c in &field_corners {
            let p0 = dot(e0, *c);
            let p2 = dot(e2, *c);
            f0_lo = f0_lo.min(p0);
            f0_hi = f0_hi.max(p0);
            f2_lo = f2_lo.min(p2);
            f2_hi = f2_hi.max(p2);
        }

        let s0_origin = f0_lo - h;
        let ns0 = (((f0_hi - f0_lo + 2.0 * h) / h).ceil() as usize)
            .saturating_add(1)
            .max(3);
        let ns1 = field.ny;
        let s1_origin = field.oy;
        plane.resize(ns0 * ns1, 0.0);

        for i in 0..slice_count {
            let s2 = s2_lo + (i as f64 + 0.5) * (s2_hi - s2_lo) / slice_count as f64;
            if s2 < f2_lo || s2 > f2_hi {
                continue;
            }
            for j in 0..ns1 {
                let v = s1_origin + j as f64 * h;
                let row = j * ns0;
                for col in 0..ns0 {
                    let s0 = s0_origin + col as f64 * h;
                    let u = s0 * ct + s2 * st;
                    let w = -s0 * st + s2 * ct;
                    plane[row + col] = sample_trilinear(field, u, v, w);
                }
            }
            let (raw_loops, _) = marching_squares_paths(&plane, ns0, ns1, s0_origin, s1_origin, h);
            for raw_loop in raw_loops {
                let Some(simplified) = simplify_closed_loop(&raw_loop, simplify_tol) else {
                    continue;
                };
                let mut loop_3d = Vec::with_capacity(simplified.len());
                for [s0, v] in simplified {
                    let u = s0 * ct + s2 * st;
                    let w = -s0 * st + s2 * ct;
                    let rel = add(add(mul(basis[0], u), mul(basis[1], v)), mul(basis[2], w));
                    *max_r2 = max_r2.max(dot(rel, rel));
                    loop_3d.push(add(center, rel));
                }
                loops.push(loop_3d);
            }
        }
    }
    loops
}

const N_RING: usize = 128;

fn point_in_poly(pt: P2, poly: &[P2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let pi = poly[i];
        let pj = poly[j];
        if ((pi[1] > pt[1]) != (pj[1] > pt[1]))
            && (pt[0] < (pj[0] - pi[0]) * (pt[1] - pi[1]) / (pj[1] - pi[1]) + pi[0])
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn closest_point_on_closed_poly(poly: &[P2], target: P2) -> (usize, f64, P2) {
    let n = poly.len();
    let mut best_i = 0usize;
    let mut best_t = 0.0f64;
    let mut best_p = poly[0];
    let mut best_d2 = f64::INFINITY;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let ab = sub(b, a);
        let len2 = dot(ab, ab);
        let t = if len2 > 1e-24 {
            (dot(sub(target, a), ab) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let p = lerp(a, b, t);
        let d = sub(target, p);
        let d2 = dot(d, d);
        if d2 < best_d2 {
            best_d2 = d2;
            best_i = i;
            best_t = t;
            best_p = p;
        }
    }
    (best_i, best_t, best_p)
}

fn resample_ccw_ring(raw: &[P2], start_hint: Option<P2>) -> [P2; N_RING] {
    let mut pts = raw.to_vec();
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    let n = pts.len();
    let mut ordered = Vec::with_capacity(n + 2);
    if let Some(hint) = start_hint {
        let (edge_i, edge_t, start_pt) = closest_point_on_closed_poly(&pts, hint);
        ordered.push(start_pt);
        for k in 1..=n {
            let v = pts[(edge_i + k) % n];
            if norm(sub(v, *ordered.last().unwrap())) > 1e-9 {
                ordered.push(v);
            }
        }
        if edge_t > 1e-6 && norm(sub(start_pt, *ordered.last().unwrap())) > 1e-9 {
            ordered.push(start_pt);
        } else if norm(sub(ordered[0], *ordered.last().unwrap())) > 1e-9 {
            ordered.push(ordered[0]);
        }
    } else {
        let start = (0..n)
            .max_by(|&a, &b| pts[a][0].total_cmp(&pts[b][0]))
            .unwrap_or(0);
        for k in 0..=n {
            ordered.push(pts[(start + k) % n]);
        }
    }

    let seg_count = ordered.len().saturating_sub(1);
    if seg_count == 0 {
        return [pts[0]; N_RING];
    }
    let mut cum = Vec::with_capacity(ordered.len());
    cum.push(0.0f64);
    for w in ordered.windows(2) {
        cum.push(cum.last().unwrap() + norm(sub(w[1], w[0])));
    }
    let total = *cum.last().unwrap();
    if total <= 1e-12 {
        return [ordered[0]; N_RING];
    }

    let mut out = [[0.0; 2]; N_RING];
    let mut seg = 0usize;
    for (k, slot) in out.iter_mut().enumerate() {
        let target_s = total * (k as f64 / N_RING as f64);
        while seg + 1 < seg_count && cum[seg + 1] < target_s {
            seg += 1;
        }
        let seg_len = cum[seg + 1] - cum[seg];
        let t = if seg_len > 1e-12 {
            ((target_s - cum[seg]) / seg_len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        *slot = lerp(ordered[seg], ordered[seg + 1], t);
    }
    out
}

fn sample_ring(ring: &[P2; N_RING], frac: f64) -> P2 {
    let f = frac.rem_euclid(1.0) * N_RING as f64;
    let i = (f.floor() as usize) % N_RING;
    let t = f - f.floor();
    lerp(ring[i], ring[(i + 1) % N_RING], t)
}

fn closest_frac_on_ring(ring: &[P2; N_RING], target: P2) -> f64 {
    let (i, t, _) = closest_point_on_closed_poly(ring, target);
    ((i as f64 + t) / N_RING as f64).rem_euclid(1.0)
}

fn find_extremum_tip(
    field: &ScalarField,
    poly: &[P2],
    low: P2,
    high: P2,
    w_base: f64,
    dw_signed: f64,
) -> (P2, f64) {
    let mut best_uv = mul(add(low, high), 0.5);
    let mut best_val = sample_trilinear(field, best_uv[0], best_uv[1], w_base);
    for &v in poly {
        let val = sample_trilinear(field, v[0], v[1], w_base);
        if val > best_val {
            best_val = val;
            best_uv = v;
        }
    }
    let nx_s = (((high[0] - low[0]) / (0.35 * field.h)).ceil() as usize).clamp(4, 28);
    let ny_s = (((high[1] - low[1]) / (0.35 * field.h)).ceil() as usize).clamp(4, 28);
    for jy in 0..=ny_s {
        let v = low[1] + (high[1] - low[1]) * (jy as f64 / ny_s as f64);
        for ix in 0..=nx_s {
            let u = low[0] + (high[0] - low[0]) * (ix as f64 / nx_s as f64);
            if point_in_poly([u, v], poly) {
                let val = sample_trilinear(field, u, v, w_base);
                if val > best_val {
                    best_val = val;
                    best_uv = [u, v];
                }
            }
        }
    }
    let mut lo = 0.0f64;
    let mut hi = 1.25f64;
    for _ in 0..14 {
        let mid = 0.5 * (lo + hi);
        let w = w_base + mid * dw_signed;
        if sample_trilinear(field, best_uv[0], best_uv[1], w) > ISO {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (best_uv, w_base + 0.5 * (lo + hi) * dw_signed)
}

struct ContourNode {
    w: f64,
    raw: Vec<P2>,
    low: P2,
    high: P2,
    probes: [P2; 8],
    parents: Vec<usize>,
    children: Vec<usize>,
}

struct SpiralBranch {
    nodes: Vec<usize>,
    parents: Vec<usize>,
    children: Vec<usize>,
    rings: Vec<[P2; N_RING]>,
    ws: Vec<f64>,
    s_cum: Vec<f64>,
    has_peak_cap: bool,
    has_nadir_cap: bool,
}

impl SpiralBranch {
    fn total_s(&self) -> f64 {
        self.s_cum.last().copied().unwrap_or(0.0)
    }

    fn eval_at(&self, s: f64, frac: f64) -> (P2, f64) {
        let m = self.rings.len();
        if m == 0 {
            return ([0.0, 0.0], 0.0);
        }
        if m == 1 || s <= 0.0 {
            return (sample_ring(&self.rings[0], frac), self.ws[0]);
        }
        let s_clamped = s.min(self.total_s());
        let idx = match self.s_cum.binary_search_by(|v| v.total_cmp(&s_clamped)) {
            Ok(i) => i.min(m - 2),
            Err(i) => i.saturating_sub(1).min(m - 2),
        };
        let ds = self.s_cum[idx + 1] - self.s_cum[idx];
        let alpha = if ds > 1e-12 {
            ((s_clamped - self.s_cum[idx]) / ds).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let p0 = sample_ring(&self.rings[idx], frac);
        let p1 = sample_ring(&self.rings[idx + 1], frac);
        let uv = lerp(p0, p1, alpha);
        // Smooth dome profile right at summit and nadir caps so poles are rounded, not conical.
        let w_alpha = if idx == 0 && self.has_peak_cap {
            alpha * alpha
        } else if idx + 2 == m && self.has_nadir_cap {
            1.0 - (1.0 - alpha) * (1.0 - alpha)
        } else {
            alpha
        };
        let w = self.ws[idx] + w_alpha * (self.ws[idx + 1] - self.ws[idx]);
        (uv, w)
    }
}

fn contours_overlap(a: &ContourNode, b: &ContourNode, pad: f64) -> bool {
    if a.high[0] + pad < b.low[0]
        || b.high[0] + pad < a.low[0]
        || a.high[1] + pad < b.low[1]
        || b.high[1] + pad < a.low[1]
    {
        return false;
    }
    if a.probes.iter().any(|&p| point_in_poly(p, &b.raw))
        || b.probes.iter().any(|&p| point_in_poly(p, &a.raw))
    {
        return true;
    }
    let pad2 = pad * pad;
    for &pa in &a.probes {
        for &pb in &b.probes {
            let d = sub(pa, pb);
            if dot(d, d) <= pad2 {
                return true;
            }
        }
    }
    false
}

/// Multi-peak Reeb-tree geodesic surface spiral:
/// starts at each 3D surface peak, winds down at constant 3D surface spacing,
/// and merges secondary peak spirals tangentially into a single spiral on
/// shared trunk regions.
#[allow(clippy::too_many_arguments)]
fn extract_spiral_surface(
    field: &ScalarField,
    center: V3,
    basis: [V3; 3],
    _axis_uv: P2,
    z_lo: f64,
    z_hi: f64,
    spiral_turns: f64,
    simplify_tol: f64,
    max_r2: &mut f64,
) -> (Vec<Vec<V3>>, Vec<Vec<V3>>) {
    let h = field.h;
    let turns = spiral_turns.max(1.0);
    let span_z = (z_hi - z_lo).max(1e-6);
    let n_slices = ((turns * 10.0).ceil() as usize)
        .max((span_z / (0.28 * h)).ceil() as usize)
        .clamp(48, 720);
    let dw = span_z / n_slices as f64;
    let fz_min = field.oz - h;
    let fz_max = field.oz + field.nz as f64 * h;

    let mut plane = vec![0.0f32; field.nx * field.ny];
    let mut nodes: Vec<ContourNode> = Vec::new();
    let mut prev_slice_nodes: Vec<usize> = Vec::new();

    for s in 0..=n_slices {
        let w = z_hi - s as f64 * dw;
        if w < fz_min || w > fz_max {
            prev_slice_nodes.clear();
            continue;
        }
        plane_at(field, w, &mut plane);
        let (raw_loops, _) =
            marching_squares_paths(&plane, field.nx, field.ny, field.ox, field.oy, h);

        let mut curr_slice_nodes = Vec::new();
        for mut raw in raw_loops {
            let area = signed_area(&raw);
            // Outer surface contours are CCW (area > 0); skip tiny sub-voxel specks and internal cavities.
            if area <= 0.0 || closed_perimeter(&raw) < 0.45 * h {
                continue;
            }
            if raw.len() >= 6 {
                let n_raw = raw.len();
                let prev_raw = raw.clone();
                for k in 0..n_raw {
                    let a = prev_raw[(k + n_raw - 1) % n_raw];
                    let b = prev_raw[k];
                    let c = prev_raw[(k + 1) % n_raw];
                    raw[k] = [
                        0.2 * a[0] + 0.6 * b[0] + 0.2 * c[0],
                        0.2 * a[1] + 0.6 * b[1] + 0.2 * c[1],
                    ];
                }
            }
            let mut low = [f64::INFINITY; 2];
            let mut high = [f64::NEG_INFINITY; 2];
            for &p in &raw {
                low[0] = low[0].min(p[0]);
                low[1] = low[1].min(p[1]);
                high[0] = high[0].max(p[0]);
                high[1] = high[1].max(p[1]);
            }
            let ring_probe = resample_ccw_ring(&raw, None);
            let probes = std::array::from_fn(|k| ring_probe[k * (N_RING / 8)]);
            let id = nodes.len();
            nodes.push(ContourNode {
                w,
                raw,
                low,
                high,
                probes,
                parents: Vec::new(),
                children: Vec::new(),
            });
            curr_slice_nodes.push(id);
        }

        let overlap_pad = 0.85 * h;
        for &p_id in &prev_slice_nodes {
            for &c_id in &curr_slice_nodes {
                if contours_overlap(&nodes[p_id], &nodes[c_id], overlap_pad) {
                    nodes[p_id].children.push(c_id);
                    nodes[c_id].parents.push(p_id);
                }
            }
        }
        prev_slice_nodes = curr_slice_nodes;
    }

    if nodes.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // Group 1-to-1 chains into monotonic Reeb-graph branches (ordered top-to-bottom).
    let mut node_branch = vec![usize::MAX; nodes.len()];
    let mut branches: Vec<SpiralBranch> = Vec::new();
    for start_id in 0..nodes.len() {
        if node_branch[start_id] != usize::MAX {
            continue;
        }
        let is_start = nodes[start_id].parents.len() != 1
            || nodes[nodes[start_id].parents[0]].children.len() != 1;
        if !is_start {
            continue;
        }
        let b_id = branches.len();
        let mut chain = vec![start_id];
        node_branch[start_id] = b_id;
        let mut cur = start_id;
        while nodes[cur].children.len() == 1 {
            let next = nodes[cur].children[0];
            if nodes[next].parents.len() == 1 && node_branch[next] == usize::MAX {
                node_branch[next] = b_id;
                chain.push(next);
                cur = next;
            } else {
                break;
            }
        }
        branches.push(SpiralBranch {
            nodes: chain,
            parents: Vec::new(),
            children: Vec::new(),
            rings: Vec::new(),
            ws: Vec::new(),
            s_cum: Vec::new(),
            has_peak_cap: false,
            has_nadir_cap: false,
        });
    }

    for branch in &mut branches {
        let first_node = branch.nodes[0];
        let mut p_branches = Vec::new();
        for &p_node in &nodes[first_node].parents {
            let pb = node_branch[p_node];
            if pb != usize::MAX && !p_branches.contains(&pb) {
                p_branches.push(pb);
            }
        }
        let last_node = *branch.nodes.last().unwrap();
        let mut c_branches = Vec::new();
        for &c_node in &nodes[last_node].children {
            let cb = node_branch[c_node];
            if cb != usize::MAX && !c_branches.contains(&cb) {
                c_branches.push(cb);
            }
        }
        branch.parents = p_branches;
        branch.children = c_branches;
    }

    // Preliminary surface length estimate per branch (bottom-up + top-down) to pick global pitch & main trunks.
    let mut prelim_len = vec![0.0f64; branches.len()];
    for (b_id, branch) in branches.iter().enumerate() {
        let mut s = 0.0f64;
        let mut prev_r = resample_ccw_ring(&nodes[branch.nodes[0]].raw, None);
        let mut prev_w = nodes[branch.nodes[0]].w;
        if branch.parents.is_empty() {
            let n0 = &nodes[branch.nodes[0]];
            let (tip_uv, tip_w) = find_extremum_tip(field, &n0.raw, n0.low, n0.high, n0.w, dw);
            let mean_r2: f64 = prev_r
                .iter()
                .map(|&p| {
                    let d = sub(p, tip_uv);
                    dot(d, d) + (tip_w - prev_w).powi(2)
                })
                .sum::<f64>()
                / N_RING as f64;
            s += mean_r2.sqrt();
        }
        for &nid in &branch.nodes[1..] {
            let next_r = resample_ccw_ring(&nodes[nid].raw, Some(prev_r[0]));
            let dz = prev_w - nodes[nid].w;
            let mean_d2: f64 = (0..N_RING)
                .map(|i| {
                    let d = sub(next_r[i], prev_r[i]);
                    dot(d, d) + dz * dz
                })
                .sum::<f64>()
                / N_RING as f64;
            s += mean_d2.sqrt();
            prev_r = next_r;
            prev_w = nodes[nid].w;
        }
        if branch.children.is_empty() {
            let nl = &nodes[*branch.nodes.last().unwrap()];
            let (tip_uv, tip_w) = find_extremum_tip(field, &nl.raw, nl.low, nl.high, nl.w, -dw);
            let mean_r2: f64 = prev_r
                .iter()
                .map(|&p| {
                    let d = sub(p, tip_uv);
                    dot(d, d) + (prev_w - tip_w).powi(2)
                })
                .sum::<f64>()
                / N_RING as f64;
            s += mean_r2.sqrt();
        }
        prelim_len[b_id] = s;
    }

    // Topological sort (branches are naturally created in increasing start_id / decreasing w order,
    // verify Kahn's order to guarantee every parent precedes every child).
    let mut in_deg: Vec<usize> = branches.iter().map(|b| b.parents.len()).collect();
    let mut topo = Vec::with_capacity(branches.len());
    let mut ready: Vec<usize> = (0..branches.len()).filter(|&b| in_deg[b] == 0).collect();
    while let Some(b) = ready.pop() {
        topo.push(b);
        for &c in &branches[b].children {
            in_deg[c] -= 1;
            if in_deg[c] == 0 {
                ready.push(c);
            }
        }
    }

    let mut downstream = vec![0.0f64; branches.len()];
    for &b in topo.iter().rev() {
        let best_child = branches[b]
            .children
            .iter()
            .map(|&c| downstream[c])
            .fold(0.0f64, f64::max);
        downstream[b] = prelim_len[b] + best_child;
    }
    let mut upstream = vec![0.0f64; branches.len()];
    for &b in &topo {
        let best_parent = branches[b]
            .parents
            .iter()
            .map(|&p| upstream[p])
            .fold(0.0f64, f64::max);
        upstream[b] = best_parent + prelim_len[b];
    }

    let max_surface_span = downstream
        .iter()
        .copied()
        .fold(0.0f64, f64::max)
        .max(span_z);
    let step_pitch = (max_surface_span / turns).max(0.25 * h);

    let primary_child: Vec<Option<usize>> = (0..branches.len())
        .map(|b| {
            branches[b]
                .children
                .iter()
                .copied()
                .max_by(|&a, &c| downstream[a].total_cmp(&downstream[c]))
        })
        .collect();

    let to_world = |uv: P2, w: f64, max_r2: &mut f64| -> V3 {
        let rel = add(
            add(mul(basis[0], uv[0]), mul(basis[1], uv[1])),
            mul(basis[2], w),
        );
        *max_r2 = max_r2.max(dot(rel, rel));
        add(center, rel)
    };

    let mut polylines: Vec<Vec<V3>> = Vec::new();
    let mut branch_poly: Vec<Option<usize>> = vec![None; branches.len()];
    let mut branch_end_uv: Vec<P2> = vec![[0.0, 0.0]; branches.len()];
    let mut branch_end_phase: Vec<f64> = vec![0.0; branches.len()];

    for &b_id in &topo {
        // Filter microscopic atomic-ripple bumps (< 14% of a turn) that immediately merge.
        if branches[b_id].parents.is_empty()
            && !branches[b_id].children.is_empty()
            && prelim_len[b_id] < 0.14 * step_pitch
        {
            continue;
        }

        let active_parents: Vec<usize> = branches[b_id]
            .parents
            .iter()
            .copied()
            .filter(|&p| branch_poly[p].is_some())
            .collect();

        // At a multi-peak saddle merge, trim any parent tail that curved into the inner saddle
        // valley back (< 1 turn) to where it hugs the shared outer flank with CCW alignment.
        if active_parents.len() >= 2 {
            let mut merged_ccw = nodes[branches[b_id].nodes[0]].raw.clone();
            if signed_area(&merged_ccw) < 0.0 {
                merged_ccw.reverse();
            }
            for &p in &active_parents {
                let Some(pid) = branch_poly[p] else {
                    continue;
                };
                let poly = &mut polylines[pid];
                if poly.len() < 6 {
                    continue;
                }
                let (_, _, near_tail) = closest_point_on_closed_poly(&merged_ccw, branch_end_uv[p]);
                if norm(sub(branch_end_uv[p], near_tail)) <= 0.9 * h {
                    continue;
                }
                let lookback = poly.len().saturating_sub(64).max(2);
                let mut best_idx = poly.len() - 1;
                let mut best_d2 = f64::INFINITY;
                for idx in (lookback..poly.len()).rev() {
                    let rel = sub(poly[idx], center);
                    let uv = [dot(basis[0], rel), dot(basis[1], rel)];
                    let prev_rel = sub(poly[idx - 1], center);
                    let prev_uv = [dot(basis[0], prev_rel), dot(basis[1], prev_rel)];
                    let dir_uv = sub(uv, prev_uv);
                    let (ei, _, proj_p) = closest_point_on_closed_poly(&merged_ccw, uv);
                    let edge_dir = sub(merged_ccw[(ei + 1) % merged_ccw.len()], merged_ccw[ei]);
                    if dot(dir_uv, edge_dir) <= 0.0 {
                        continue;
                    }
                    let d = sub(uv, proj_p);
                    let d2 = dot(d, d);
                    if d2 < best_d2 {
                        best_d2 = d2;
                        best_idx = idx;
                        if d2 <= (0.65 * h) * (0.65 * h) {
                            break;
                        }
                    }
                }
                if best_idx + 1 < poly.len() {
                    poly.truncate(best_idx + 1);
                    let rel = sub(*poly.last().unwrap(), center);
                    branch_end_uv[p] = [dot(basis[0], rel), dot(basis[1], rel)];
                }
            }
        }

        let continuing_parent = active_parents
            .iter()
            .copied()
            .filter(|&p| primary_child[p] == Some(b_id))
            .max_by(|&a, &c| upstream[a].total_cmp(&upstream[c]));

        // Build aligned rings + exact 3D surface cumulative arc-length for branch b_id.
        let start_hint = if let Some(p_main) = continuing_parent {
            Some(branch_end_uv[p_main])
        } else if let Some(&p_split) = active_parents.first() {
            // Secondary child of a downward split: align ring[0] closest to parent's last contour.
            let p_last = branches[p_split].rings.last().unwrap();
            let probe = nodes[branches[b_id].nodes[0]].probes[0];
            let frac = closest_frac_on_ring(p_last, probe);
            Some(sample_ring(p_last, frac))
        } else {
            None
        };

        let mut rings = Vec::with_capacity(branches[b_id].nodes.len() + 2);
        let mut ws = Vec::with_capacity(branches[b_id].nodes.len() + 2);
        let has_peak_cap = active_parents.is_empty();
        let has_nadir_cap = branches[b_id].children.is_empty();

        let first_n = &nodes[branches[b_id].nodes[0]];
        let first_ring = resample_ccw_ring(&first_n.raw, start_hint);
        if has_peak_cap {
            let (tip_uv, tip_w) = find_extremum_tip(
                field,
                &first_n.raw,
                first_n.low,
                first_n.high,
                first_n.w,
                dw,
            );
            rings.push([tip_uv; N_RING]);
            ws.push(tip_w);
        }
        rings.push(first_ring);
        ws.push(first_n.w);

        for &nid in &branches[b_id].nodes[1..] {
            let prev0 = rings.last().unwrap()[0];
            rings.push(resample_ccw_ring(&nodes[nid].raw, Some(prev0)));
            ws.push(nodes[nid].w);
        }

        if has_nadir_cap {
            let last_n = &nodes[*branches[b_id].nodes.last().unwrap()];
            let (tip_uv, tip_w) =
                find_extremum_tip(field, &last_n.raw, last_n.low, last_n.high, last_n.w, -dw);
            rings.push([tip_uv; N_RING]);
            ws.push(tip_w);
        }

        // Bridge vertical step from continuing parent so S includes the saddle transition.
        let mut s_cum = Vec::with_capacity(rings.len());
        s_cum.push(0.0);
        for k in 1..rings.len() {
            let dz = ws[k - 1] - ws[k];
            let mean_d2: f64 = (0..N_RING)
                .map(|i| {
                    let d = sub(rings[k][i], rings[k - 1][i]);
                    dot(d, d) + dz * dz
                })
                .sum::<f64>()
                / N_RING as f64;
            s_cum.push(s_cum.last().unwrap() + mean_d2.sqrt());
        }

        branches[b_id].rings = rings;
        branches[b_id].ws = ws;
        branches[b_id].s_cum = s_cum;
        branches[b_id].has_peak_cap = has_peak_cap;
        branches[b_id].has_nadir_cap = has_nadir_cap;

        let s_total = branches[b_id].total_s();
        let ds_sample = (step_pitch / 96.0).clamp(0.02 * h, 0.30 * h);
        let n_steps = ((s_total / ds_sample).ceil() as usize).max(2);

        // Determine polyline ownership & initial phase.
        let (poly_idx, phase0) = if let Some(p_main) = continuing_parent {
            let pid = branch_poly[p_main].unwrap();
            (pid, 0.0)
        } else if let Some(&p_split) = active_parents.first() {
            // Secondary child leg at a downward split: detach tangentially from p_split's last turn!
            let pid = polylines.len();
            let mut split_pts = Vec::new();
            let target_uv = branches[b_id].rings[0][0];
            let p_branch = &branches[p_split];
            let p_s_end = p_branch.total_s();
            let frac_end = branch_end_phase[p_split].rem_euclid(1.0);
            let frac_target = closest_frac_on_ring(p_branch.rings.last().unwrap(), target_uv);
            // Walk back up to < 1 turn on p_split to find where p_split's spiral passed frac_target - 0.25.
            let d_frac = 0.28f64;
            let detach_frac = (frac_target - d_frac).rem_euclid(1.0);
            let back_turns = (frac_end - detach_frac).rem_euclid(1.0) + d_frac;
            let s_start = (p_s_end - back_turns * step_pitch).max(0.0);
            if p_s_end > s_start + 1e-6 {
                let m_slope = d_frac * step_pitch;
                let detach_steps = 22usize;
                for k in 0..=detach_steps {
                    let u = k as f64 / detach_steps as f64;
                    let u2 = u * u;
                    let u3 = u2 * u;
                    let h_s = (2.0 * u3 - 3.0 * u2 + 1.0) * s_start
                        + (u3 - 2.0 * u2 + u) * m_slope
                        + (-2.0 * u3 + 3.0 * u2) * p_s_end
                        + (u3 - u2) * m_slope;
                    let f = detach_frac + u * d_frac;
                    let (uv, w) = p_branch.eval_at(h_s, f);
                    split_pts.push(to_world(uv, w, max_r2));
                }
            }
            polylines.push(split_pts);
            (pid, 0.0)
        } else {
            let pid = polylines.len();
            polylines.push(Vec::new());
            (pid, 0.0)
        };

        // Trace the constant-step spiral along branch b_id.
        let skip_first = !polylines[poly_idx].is_empty() && continuing_parent.is_none();
        for step in 0..=n_steps {
            if step == 0 && skip_first {
                continue;
            }
            let s = s_total * (step as f64 / n_steps as f64);
            let frac = phase0 + s / step_pitch;
            let (uv, w) = branches[b_id].eval_at(s, frac);
            let pt = to_world(uv, w, max_r2);
            if polylines[poly_idx]
                .last()
                .is_none_or(|&prev| norm(sub(pt, prev)) > 1e-6)
            {
                polylines[poly_idx].push(pt);
            }
            if step == n_steps {
                branch_end_uv[b_id] = uv;
                branch_end_phase[b_id] = frac;
            }
        }
        branch_poly[b_id] = Some(poly_idx);

        // Connect all secondary merging peak spirals smoothly & tangentially onto b_id's shared spiral!
        for &p_sec in &active_parents {
            if Some(p_sec) == continuing_parent {
                continue;
            }
            let Some(sec_poly_idx) = branch_poly[p_sec] else {
                continue;
            };
            let sec_end_uv = branch_end_uv[p_sec];
            let frac_sec = closest_frac_on_ring(&branches[b_id].rings[0], sec_end_uv);
            // Target join on b_id's shared spiral: frac(s) = phase0 + s / step_pitch.
            // Choose target_frac >= frac_sec + d_frac so s_target > 0 and Hermite S'(u) > 0 everywhere.
            let d_frac = 0.32f64;
            let mut target_frac = frac_sec + d_frac;
            if target_frac <= phase0 + 0.15 {
                target_frac += 1.0;
            }
            let mut s_target = (target_frac - phase0) * step_pitch;
            if s_target > s_total && s_total > 1e-6 {
                s_target = s_total;
                target_frac = phase0 + s_total / step_pitch;
            }
            let total_dfrac = (target_frac - frac_sec).max(0.08);
            let m_slope = (total_dfrac * step_pitch).min(s_target);
            let blend_steps = 24usize;
            for k in 1..=blend_steps {
                let u = k as f64 / blend_steps as f64;
                let u2 = u * u;
                let u3 = u2 * u;
                let h_s = (u3 - 2.0 * u2 + u) * m_slope
                    + (-2.0 * u3 + 3.0 * u2) * s_target
                    + (u3 - u2) * m_slope;
                let f = frac_sec + u * total_dfrac;
                let (uv, w) = branches[b_id].eval_at(h_s.clamp(0.0, s_total), f);
                let pt = to_world(uv, w, max_r2);
                if polylines[sec_poly_idx]
                    .last()
                    .is_none_or(|&prev| norm(sub(pt, prev)) > 1e-6)
                {
                    polylines[sec_poly_idx].push(pt);
                }
            }
        }
    }

    let open_tol = (simplify_tol * 0.18).min(0.012 * step_pitch).max(0.004);
    let polylines_3d: Vec<Vec<V3>> = polylines
        .into_iter()
        .map(|poly| rdp_open_3d(&poly, open_tol))
        .filter(|poly| poly.len() >= 2)
        .collect();

    (Vec::new(), polylines_3d)
}

pub(crate) fn prepare_slices(
    request: &RenderRequest,
    preview: bool,
    override_camera: Option<&Camera>,
) -> Result<PreparedSlices, RenderError> {
    let o = &request.options;
    let key = SliceKey::new(&request.structure, o, override_camera);
    let mut warnings = request
        .structure
        .warnings
        .iter()
        .filter(|w| !w.contains("DSSP") && !w.contains("bond bands"))
        .cloned()
        .collect::<Vec<_>>();

    if !o.has_selected_mode(RenderMode::Slice) {
        if o.selected_chains.is_empty() {
            warnings.push("No chains selected; the drawing is empty.".into());
        }
        return Ok(PreparedSlices {
            key,
            camera: override_camera.cloned().unwrap_or(Camera {
                center: [0.0; 3],
                basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            }),
            groups: Vec::new(),
            radius: 1.0,
            residue_count: 0,
            warnings,
        });
    }

    let (by_color, residue_count) = collect_atoms_by_color(&request.structure, o);
    let all_positions: Vec<V3> = by_color
        .values()
        .flat_map(|atoms| atoms.iter().map(|&(p, _)| p))
        .collect();
    if all_positions.is_empty() {
        return fail("Selected components contain no atomic coordinates to slice");
    }

    let camera = match override_camera {
        Some(cam) => cam.clone(),
        None => Camera::new(&all_positions, [0.0; 3]),
    };
    let basis = section_basis(&camera, o);
    let group_count = by_color.len().max(1);
    let voxel_cap = if preview {
        (PREVIEW_MAX_VOXELS / group_count).max(150_000)
    } else {
        (MAX_VOXELS / group_count).max(600_000)
    };

    let c = o.slice_coarseness as f64 / 100.0;
    let simplify_tol = 0.02 + 0.88 * c * c;
    let mut coarsened_any = false;
    let mut groups = Vec::with_capacity(by_color.len());
    let mut max_r2 = 0.0f64;

    let mut global_low = [f64::INFINITY; 3];
    let mut global_high = [f64::NEG_INFINITY; 3];
    let mut fields = Vec::with_capacity(by_color.len());
    for (color, atoms) in by_color {
        let field = build_field(
            &atoms,
            camera.center,
            basis,
            o.slice_coarseness,
            o.slice_probe_angstrom,
            voxel_cap,
        );
        coarsened_any |= field.coarsened;
        for axis in 0..3 {
            global_low[axis] = global_low[axis].min(field.low[axis]);
            global_high[axis] = global_high[axis].max(field.high[axis]);
        }
        fields.push((color, field));
    }

    if coarsened_any && !preview {
        warnings.push(
            "Envelope grid resolution was coarsened automatically to stay within local memory limits."
                .into(),
        );
    }

    let pad_probe = o.slice_probe_angstrom;
    let padded_low = [
        global_low[0] - pad_probe,
        global_low[1] - pad_probe,
        global_low[2] - pad_probe,
    ];
    let padded_high = [
        global_high[0] + pad_probe,
        global_high[1] + pad_probe,
        global_high[2] + pad_probe,
    ];
    let axis_uv = [
        0.5 * (global_low[0] + global_high[0]),
        0.5 * (global_low[1] + global_high[1]),
    ];

    for (color, field) in fields {
        let (loops, polylines) = match o.slice_pattern {
            SlicePattern::Slices => (
                extract_parallel_slices(
                    &field,
                    camera.center,
                    basis,
                    padded_low[2],
                    padded_high[2],
                    o.slice_count,
                    simplify_tol,
                    &mut max_r2,
                ),
                Vec::new(),
            ),
            SlicePattern::Crosshatch => (
                extract_crosshatch_slices(
                    &field,
                    camera.center,
                    basis,
                    padded_low,
                    padded_high,
                    o.slice_count,
                    o.slice_hatch_angle,
                    simplify_tol,
                    &mut max_r2,
                ),
                Vec::new(),
            ),
            SlicePattern::Spiral => extract_spiral_surface(
                &field,
                camera.center,
                basis,
                axis_uv,
                padded_low[2],
                padded_high[2],
                o.slice_spiral_density,
                simplify_tol,
                &mut max_r2,
            ),
        };
        if !loops.is_empty() || !polylines.is_empty() {
            groups.push(PreparedSliceGroup {
                color,
                loops,
                polylines,
            });
        }
    }

    if groups.is_empty() {
        return fail(
            "Slicer envelope came out empty; reduce coarseness or increase envelope probe",
        );
    }

    Ok(PreparedSlices {
        key,
        camera,
        groups,
        radius: max_r2.sqrt().max(1.0),
        residue_count,
        warnings,
    })
}

fn signed_area(pts: &[P2]) -> f64 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * sum
}

fn closed_perimeter(pts: &[P2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| norm(sub(pts[(i + 1) % n], pts[i]))).sum()
}

fn path_length(pts: &[P2]) -> f64 {
    pts.windows(2).map(|w| norm(sub(w[1], w[0]))).sum()
}

fn outward_normals(pts: &[P2], area: f64) -> Vec<P2> {
    let n = pts.len();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let edge_normals: Vec<P2> = (0..n)
        .map(|i| {
            let d = sub(pts[(i + 1) % n], pts[i]);
            let len = norm(d);
            if len > 1e-12 {
                [sign * d[1] / len, -sign * d[0] / len]
            } else {
                [0.0, 1.0]
            }
        })
        .collect();
    (0..n)
        .map(|i| {
            let prev = edge_normals[(i + n - 1) % n];
            let curr = edge_normals[i];
            let bisector = math::unit(add(prev, curr), curr);
            let proj = dot(bisector, curr).max(0.69);
            mul(bisector, (1.0 / proj).clamp(1.0, 1.45))
        })
        .collect()
}

fn open_normals(pts: &[P2]) -> Vec<P2> {
    let n = pts.len();
    if n < 2 {
        return vec![[0.0, 1.0]; n];
    }
    let edge_normals: Vec<P2> = (0..n - 1)
        .map(|i| {
            let d = sub(pts[i + 1], pts[i]);
            let len = norm(d);
            if len > 1e-12 {
                [d[1] / len, -d[0] / len]
            } else {
                [0.0, 1.0]
            }
        })
        .collect();
    (0..n)
        .map(|i| {
            if i == 0 {
                edge_normals[0]
            } else if i == n - 1 {
                edge_normals[n - 2]
            } else {
                let prev = edge_normals[i - 1];
                let curr = edge_normals[i];
                let bisector = math::unit(add(prev, curr), curr);
                let proj = dot(bisector, curr).max(0.69);
                mul(bisector, (1.0 / proj).clamp(1.0, 1.45))
            }
        })
        .collect()
}

fn cyclic_runs(mask: &[bool]) -> Vec<(usize, usize)> {
    let n = mask.len();
    if n == 0 {
        return Vec::new();
    }
    if mask.iter().all(|&v| v) {
        return vec![(0, n)];
    }
    let Some(first_false) = mask.iter().position(|&v| !v) else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut k = 1usize;
    while k <= n {
        let idx = (first_false + k) % n;
        if mask[idx] {
            let start = idx;
            let mut len = 1usize;
            while k + len <= n && mask[(first_false + k + len) % n] {
                len += 1;
            }
            runs.push((start, len));
            k += len;
        } else {
            k += 1;
        }
    }
    runs
}

fn linear_runs(mask: &[bool]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut i = 0;
    while i < mask.len() {
        if mask[i] {
            let start = i;
            while i < mask.len() && mask[i] {
                i += 1;
            }
            runs.push((start, i - start));
        } else {
            i += 1;
        }
    }
    runs
}

#[derive(Clone)]
pub(crate) struct WeightedPath {
    pub(crate) points: Vec<P2>,
    pub(crate) passes: usize,
}

pub(crate) struct ProjectedSlicePath {
    pub(crate) points: Vec<P2>,
    pub(crate) zs: Vec<f64>,
    pub(crate) closed: bool,
}

pub(crate) struct ProjectedSliceGroup {
    pub(crate) color: String,
    pub(crate) paths: Vec<ProjectedSlicePath>,
}

#[derive(Clone, PartialEq)]
struct Endpoint {
    point: P2,
    path: usize,
    reverse: bool,
}
impl RTreeObject for Endpoint {
    type Envelope = AABB<P2>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}
impl PointDistance for Endpoint {
    fn distance_2(&self, point: &P2) -> f64 {
        let d = sub(self.point, *point);
        dot(d, d)
    }
}

fn order_weighted(items: Vec<WeightedPath>, allow_reverse: bool) -> Vec<WeightedPath> {
    let mut slots: Vec<Option<WeightedPath>> = items
        .into_iter()
        .filter(|w| w.points.len() >= 2 && w.passes >= 1)
        .map(Some)
        .collect();
    let endpoints: Vec<Endpoint> = slots
        .iter()
        .enumerate()
        .flat_map(|(id, item)| {
            let Some(w) = item.as_ref() else {
                return Vec::new();
            };
            let mut res = vec![Endpoint {
                point: w.points[0],
                path: id,
                reverse: false,
            }];
            if allow_reverse {
                res.push(Endpoint {
                    point: *w.points.last().unwrap(),
                    path: id,
                    reverse: true,
                });
            }
            res
        })
        .collect();
    let mut tree = RTree::bulk_load(endpoints);
    let mut ordered = Vec::with_capacity(slots.len());
    let mut cursor = [0.0; 2];

    while let Some(nearest) = tree.nearest_neighbor(&cursor).cloned() {
        if let Some(mut item) = slots[nearest.path].take() {
            let start = Endpoint {
                point: item.points[0],
                path: nearest.path,
                reverse: false,
            };
            let end = Endpoint {
                point: *item.points.last().unwrap(),
                path: nearest.path,
                reverse: true,
            };
            tree.remove(&start);
            if allow_reverse {
                tree.remove(&end);
            }
            if nearest.reverse {
                item.points.reverse();
            }
            let first = item.points[0];
            let last = *item.points.last().unwrap();
            let closed = norm(sub(last, first)) < 1e-6;
            cursor = if closed || item.passes % 2 == 1 {
                last
            } else {
                first
            };
            ordered.push(item);
        } else {
            tree.remove(&nearest);
        }
    }
    ordered
}

fn emit_track_passes(
    track_pts: &[P2],
    track_u: &[f64],
    is_closed: bool,
    max_passes: usize,
    out: &mut Vec<WeightedPath>,
) {
    let n = track_pts.len();
    if n < 2 {
        return;
    }
    let passes_at: Vec<usize> = track_u
        .iter()
        .map(|&u| 1 + ((u * max_passes as f64).floor() as usize).min(max_passes.saturating_sub(1)))
        .collect();
    let min_p = passes_at.iter().copied().min().unwrap_or(1);
    let max_p = passes_at.iter().copied().max().unwrap_or(1);

    let mut full = track_pts.to_vec();
    if is_closed {
        full.push(track_pts[0]);
    }
    if path_length(&full) >= MIN_LENGTH_MM {
        out.push(WeightedPath {
            points: full,
            passes: min_p,
        });
    }

    if max_p <= min_p {
        return;
    }

    let mut prev_runs: Vec<(usize, usize)> = Vec::new();
    let mut prev_start_index = 0usize;
    for level in (min_p + 1)..=max_p {
        let mask: Vec<bool> = passes_at.iter().map(|&p| p >= level).collect();
        let runs = if is_closed {
            cyclic_runs(&mask)
        } else {
            linear_runs(&mask)
        };
        if !prev_runs.is_empty() && runs == prev_runs {
            for item in &mut out[prev_start_index..] {
                item.passes += 1;
            }
            continue;
        }
        prev_start_index = out.len();
        let mut emitted_any = false;
        for &(start, len) in &runs {
            if len < 2 && !(is_closed && len == 1 && n >= 2) {
                continue;
            }
            let arc: Vec<P2> = if is_closed {
                if len == n {
                    let mut v = track_pts.to_vec();
                    v.push(track_pts[0]);
                    v
                } else {
                    let count = (len + 1).min(n);
                    (0..count).map(|k| track_pts[(start + k) % n]).collect()
                }
            } else {
                let end = (start + len).min(n - 1);
                track_pts[start..=end].to_vec()
            };
            if path_length(&arc) >= MIN_LENGTH_MM {
                out.push(WeightedPath {
                    points: arc,
                    passes: 1,
                });
                emitted_any = true;
            }
        }
        prev_runs = if emitted_any { runs } else { Vec::new() };
    }
}

pub(crate) fn project_slice_groups(
    prepared: &PreparedSlices,
    rotated_camera: &Camera,
    perspective: bool,
) -> (Vec<ProjectedSliceGroup>, [P2; 2], [f64; 2]) {
    let inv_r = 1.0 / prepared.radius.max(1.0);
    let mut z_min = f64::INFINITY;
    let mut z_max = f64::NEG_INFINITY;
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];

    let mut project_pts = |pts_3d: &[V3], closed: bool| -> ProjectedSlicePath {
        let mut points = Vec::with_capacity(pts_3d.len());
        let mut zs = Vec::with_capacity(pts_3d.len());
        for &world in pts_3d {
            let c = rotated_camera.point(world);
            z_min = z_min.min(c[2]);
            z_max = z_max.max(c[2]);
            let p = if perspective {
                let nz = (c[2] * inv_r).clamp(-1.0, 1.0);
                let f = FOC / (FOC - nz);
                [c[0] * f, -c[1] * f]
            } else {
                [c[0], -c[1]]
            };
            for axis in 0..2 {
                low[axis] = low[axis].min(p[axis]);
                high[axis] = high[axis].max(p[axis]);
            }
            points.push(p);
            zs.push(c[2]);
        }
        ProjectedSlicePath { points, zs, closed }
    };

    let groups = prepared
        .groups
        .iter()
        .map(|group| {
            let mut paths = Vec::with_capacity(group.loops.len() + group.polylines.len());
            for loop_3d in &group.loops {
                paths.push(project_pts(loop_3d, true));
            }
            for poly_3d in &group.polylines {
                paths.push(project_pts(poly_3d, false));
            }
            ProjectedSliceGroup {
                color: group.color.clone(),
                paths,
            }
        })
        .collect();

    (groups, [low, high], [z_min, z_max])
}

fn depth_weights(zs: Vec<f64>, z_min: f64, z_span: f64, gamma: f64, closed: bool) -> Vec<f64> {
    let n = zs.len();
    let mut u: Vec<f64> = zs
        .into_iter()
        .map(|z| {
            let t = if z_span > 1e-9 {
                ((z - z_min) / z_span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            t.powf(gamma)
        })
        .collect();
    if n >= 3 {
        for _ in 0..2 {
            let prev = u.clone();
            if closed {
                for i in 0..n {
                    u[i] = 0.25 * prev[(i + n - 1) % n] + 0.5 * prev[i] + 0.25 * prev[(i + 1) % n];
                }
            } else {
                for i in 0..n {
                    u[i] = 0.25 * prev[i.saturating_sub(1)]
                        + 0.5 * prev[i]
                        + 0.25 * prev[(i + 1).min(n - 1)];
                }
            }
        }
    }
    u
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_weighted_slice_groups(
    projected_groups: Vec<ProjectedSliceGroup>,
    z_range: [f64; 2],
    mid: P2,
    scale: f64,
    page_mid: P2,
    o: &RenderOptions,
    preview: bool,
    warnings: &mut Vec<String>,
) -> Vec<(String, Vec<WeightedPath>)> {
    if preview {
        warnings.push(
            "Interaction preview: base slice contours only. Release to compute parallel depth tracks and overdraw passes."
                .into(),
        );
        return projected_groups
            .into_iter()
            .filter_map(|group| {
                let weighted: Vec<WeightedPath> = group
                    .paths
                    .into_iter()
                    .filter_map(|path| {
                        let mut mapped: Vec<P2> = path
                            .points
                            .into_iter()
                            .map(|p| add(mul(sub(p, mid), scale), page_mid))
                            .collect();
                        if path.closed {
                            let first = *mapped.first()?;
                            mapped.push(first);
                        }
                        (mapped.len() >= 2 && path_length(&mapped) >= MIN_LENGTH_MM).then_some(
                            WeightedPath {
                                points: mapped,
                                passes: 1,
                            },
                        )
                    })
                    .collect();
                (!weighted.is_empty()).then_some((group.color, weighted))
            })
            .collect();
    }

    if o.slice_max_parallel > 1 && o.pen_width_mm > o.slice_parallel_spacing_mm {
        warnings.push(
            "Parallel slice tracks are closer than the pen stroke width and will partially overlap in ink."
                .into(),
        );
    }

    let z_span = z_range[1] - z_range[0];
    let allow_reverse = o.stroke_direction != StrokeDirection::Backbone;
    let mut out = Vec::with_capacity(projected_groups.len());

    for group in projected_groups {
        let mut weighted = Vec::new();
        for path in group.paths {
            let pts: Vec<P2> = path
                .points
                .into_iter()
                .map(|p| add(mul(sub(p, mid), scale), page_mid))
                .collect();
            let n = pts.len();

            if path.closed {
                if n < 3 {
                    continue;
                }
                let perim = closed_perimeter(&pts);
                if perim < MIN_LENGTH_MM {
                    continue;
                }
                let area = signed_area(&pts);
                let inradius = 2.0 * area.abs() / perim.max(1e-9);
                let normals = outward_normals(&pts, area);
                let u = depth_weights(path.zs, z_range[0], z_span, o.slice_depth_gamma, true);

                emit_track_passes(&pts, &u, true, o.slice_max_passes, &mut weighted);

                for track in 1..o.slice_max_parallel {
                    let pair = track.div_ceil(2);
                    let inward = track % 2 == 1;
                    let dist = pair as f64 * o.slice_parallel_spacing_mm;
                    if inward && (n <= 5 || dist >= 0.50 * inradius) {
                        continue;
                    }
                    let signed_offset = if inward { -dist } else { dist };
                    let threshold = track as f64 / o.slice_max_parallel as f64 - 1e-9;
                    let mask: Vec<bool> = u.iter().map(|&val| val >= threshold).collect();
                    let runs = cyclic_runs(&mask);

                    if runs.len() == 1 && runs[0].1 == n {
                        let offset_pts: Vec<P2> = (0..n)
                            .map(|i| add(pts[i], mul(normals[i], signed_offset)))
                            .collect();
                        if inward && signed_area(&offset_pts) * area <= 0.0 {
                            continue;
                        }
                        emit_track_passes(&offset_pts, &u, true, o.slice_max_passes, &mut weighted);
                    } else {
                        let max_unassigned = runs
                            .iter()
                            .map(|(_, len)| n.saturating_sub(*len) / 2)
                            .min()
                            .unwrap_or(0);
                        let taper = 3.min(max_unassigned);
                        for (start, len) in runs {
                            let total = len + 2 * taper;
                            if total < 2 {
                                continue;
                            }
                            let begin = (start + n - taper) % n;
                            let mut arc_pts = Vec::with_capacity(total);
                            let mut arc_u = Vec::with_capacity(total);
                            for k in 0..total {
                                let idx = (begin + k) % n;
                                let weight = if taper > 0 && k < taper {
                                    let f = (k + 1) as f64 / (taper + 1) as f64;
                                    0.5 - 0.5 * (PI * f).cos()
                                } else if taper > 0 && k >= taper + len {
                                    let rem = total - k;
                                    let f = rem as f64 / (taper + 1) as f64;
                                    0.5 - 0.5 * (PI * f).cos()
                                } else {
                                    1.0
                                };
                                arc_pts
                                    .push(add(pts[idx], mul(normals[idx], signed_offset * weight)));
                                arc_u.push(u[idx]);
                            }
                            emit_track_passes(
                                &arc_pts,
                                &arc_u,
                                false,
                                o.slice_max_passes,
                                &mut weighted,
                            );
                        }
                    }
                }
            } else {
                if n < 2 || path_length(&pts) < MIN_LENGTH_MM {
                    continue;
                }
                let normals = open_normals(&pts);
                let u = depth_weights(path.zs, z_range[0], z_span, o.slice_depth_gamma, false);

                emit_track_passes(&pts, &u, false, o.slice_max_passes, &mut weighted);

                for track in 1..o.slice_max_parallel {
                    let pair = track.div_ceil(2);
                    let side = if track % 2 == 1 { -1.0 } else { 1.0 };
                    let signed_offset = side * pair as f64 * o.slice_parallel_spacing_mm;
                    let threshold = track as f64 / o.slice_max_parallel as f64 - 1e-9;
                    let mask: Vec<bool> = u.iter().map(|&val| val >= threshold).collect();
                    for (start, len) in linear_runs(&mask) {
                        let left_taper = start.min(3);
                        let right_taper = (n - (start + len)).min(3);
                        let begin = start - left_taper;
                        let end_excl = start + len + right_taper;
                        if end_excl - begin < 2 {
                            continue;
                        }
                        let mut arc_pts = Vec::with_capacity(end_excl - begin);
                        let mut arc_u = Vec::with_capacity(end_excl - begin);
                        for idx in begin..end_excl {
                            let weight = if idx < start {
                                let f = (idx - begin + 1) as f64 / (left_taper + 1) as f64;
                                0.5 - 0.5 * (PI * f).cos()
                            } else if idx >= start + len {
                                let rem = end_excl - idx;
                                let f = rem as f64 / (right_taper + 1) as f64;
                                0.5 - 0.5 * (PI * f).cos()
                            } else {
                                1.0
                            };
                            arc_pts.push(add(pts[idx], mul(normals[idx], signed_offset * weight)));
                            arc_u.push(u[idx]);
                        }
                        emit_track_passes(
                            &arc_pts,
                            &arc_u,
                            false,
                            o.slice_max_passes,
                            &mut weighted,
                        );
                    }
                }
            }
        }
        if !weighted.is_empty() {
            out.push((group.color, order_weighted(weighted, allow_reverse)));
        }
    }
    out
}

fn fit_weighted_groups(
    groups: &mut [(String, Vec<WeightedPath>)],
    options: &RenderOptions,
) -> Result<(), RenderError> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for (_, paths) in groups.iter() {
        for item in paths {
            for p in &item.points {
                for axis in 0..2 {
                    if !p[axis].is_finite() {
                        return fail("Nonfinite projected slice geometry");
                    }
                    low[axis] = low[axis].min(p[axis]);
                    high[axis] = high[axis].max(p[axis]);
                }
            }
        }
    }
    let span = sub(high, low);
    if !span.iter().all(|v| v.is_finite()) || span[0].max(span[1]) < 1e-8 {
        return fail("Slice projection is degenerate; cannot fit artwork to the page");
    }
    let min_x = options.margin_mm + options.pen_width_mm * 0.5;
    let max_x = options.width_mm - options.margin_mm - options.pen_width_mm * 0.5;
    let min_y = options.margin_mm + options.pen_width_mm * 0.5;
    let max_y = options.height_mm - options.margin_mm - options.pen_width_mm * 0.5;
    let avail = [max_x - min_x, max_y - min_y];
    let scale = (avail[0] / span[0].max(1e-12)).min(avail[1] / span[1].max(1e-12));
    let mid = mul(add(low, high), 0.5);
    let page_mid = [options.width_mm * 0.5, options.height_mm * 0.5];
    for (_, paths) in groups.iter_mut() {
        for item in paths {
            for p in &mut item.points {
                *p = add(mul(sub(*p, mid), scale), page_mid);
            }
        }
    }
    Ok(())
}

pub(crate) fn format_opacity(opacity: f64) -> String {
    format!("{opacity:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

pub(crate) fn slice_groups_stats(groups: &[(String, Vec<WeightedPath>)]) -> (usize, usize, f64) {
    let mut stroke_count = 0usize;
    let mut point_count = 0usize;
    let mut length_mm = 0.0f64;
    for (_, paths) in groups {
        for item in paths {
            stroke_count = stroke_count.saturating_add(item.passes);
            point_count = point_count.saturating_add(item.points.len().saturating_mul(item.passes));
            length_mm += path_length(&item.points) * item.passes as f64;
        }
    }
    (stroke_count, point_count, length_mm)
}

pub(crate) fn append_slice_groups_svg(
    svg: &mut String,
    groups: &[(String, Vec<WeightedPath>)],
    opacity: &str,
) {
    for (idx, (color, weighted)) in groups.iter().enumerate() {
        if weighted.is_empty() {
            continue;
        }
        let _ = writeln!(
            svg,
            "<g id=\"slice-{}\" stroke=\"{}\" stroke-opacity=\"{}\">",
            idx + 1,
            color,
            opacity
        );
        for item in weighted {
            let Some(&first) = item.points.first() else {
                continue;
            };
            let last = *item.points.last().unwrap();
            let closed = norm(sub(last, first)) < 1e-6;
            for pass in 0..item.passes {
                if !closed && pass % 2 == 1 {
                    let _ = write!(svg, "<path d=\"M{:.4},{:.4}", last[0], last[1]);
                    for point in item.points.iter().rev().skip(1) {
                        let _ = write!(svg, "L{:.4},{:.4}", point[0], point[1]);
                    }
                    svg.push_str("\"/>\n");
                } else {
                    let _ = write!(svg, "<path d=\"M{:.4},{:.4}", first[0], first[1]);
                    for point in &item.points[1..] {
                        let _ = write!(svg, "L{:.4},{:.4}", point[0], point[1]);
                    }
                    svg.push_str("\"/>\n");
                }
            }
        }
        svg.push_str("</g>\n");
    }
}

pub(crate) fn sample_preview_groups(
    groups: &[PreparedSliceGroup],
    budget: usize,
) -> Vec<PreparedSliceGroup> {
    let total_pts: usize = groups
        .iter()
        .map(|g| {
            g.loops.iter().map(Vec::len).sum::<usize>()
                + g.polylines.iter().map(Vec::len).sum::<usize>()
        })
        .sum();
    let stride = total_pts.div_ceil(budget).max(1);
    groups
        .iter()
        .filter_map(|group| {
            let loops: Vec<Vec<V3>> = group
                .loops
                .iter()
                .filter_map(|loop_pts| {
                    if stride == 1 || loop_pts.len() <= 6 {
                        return Some(loop_pts.clone());
                    }
                    let sampled: Vec<V3> = loop_pts.iter().step_by(stride).copied().collect();
                    (sampled.len() >= 3).then_some(sampled)
                })
                .collect();
            let polylines: Vec<Vec<V3>> = group
                .polylines
                .iter()
                .filter_map(|poly_pts| {
                    if stride == 1 || poly_pts.len() <= 4 {
                        return Some(poly_pts.clone());
                    }
                    let mut sampled: Vec<V3> = poly_pts.iter().step_by(stride).copied().collect();
                    if let Some(&last) = poly_pts.last() {
                        if sampled.last() != Some(&last) {
                            sampled.push(last);
                        }
                    }
                    (sampled.len() >= 2).then_some(sampled)
                })
                .collect();
            (!loops.is_empty() || !polylines.is_empty()).then_some(PreparedSliceGroup {
                color: group.color.clone(),
                loops,
                polylines,
            })
        })
        .collect()
}

pub(crate) fn render_from_prepared(
    request: &RenderRequest,
    prepared: &PreparedSlices,
    preview: bool,
) -> Result<RenderResult, RenderError> {
    let o = &request.options;
    let mut warnings = prepared.warnings.clone();

    if prepared.groups.is_empty() {
        return make_slice_svg(request, Vec::new(), 0, warnings, preview);
    }

    let camera = prepared.camera.rotated(o.rotation);
    let (projected, [low, high], z_range) =
        project_slice_groups(prepared, &camera, o.slice_perspective);

    let span = sub(high, low);
    if !span.iter().all(|v| v.is_finite()) || span[0].max(span[1]) < 1e-8 {
        return fail("Slice projection is degenerate; cannot fit artwork to the page");
    }

    let max_outward = 1.45 * (o.slice_max_parallel / 2) as f64 * o.slice_parallel_spacing_mm;
    let avail = [
        (o.width_mm - 2.0 * o.margin_mm - o.pen_width_mm - 2.0 * max_outward).max(o.pen_width_mm),
        (o.height_mm - 2.0 * o.margin_mm - o.pen_width_mm - 2.0 * max_outward).max(o.pen_width_mm),
    ];
    let pre_scale = (avail[0] / span[0].max(1e-12)).min(avail[1] / span[1].max(1e-12));
    let mid = mul(add(low, high), 0.5);
    let page_mid = [o.width_mm * 0.5, o.height_mm * 0.5];

    let mut groups = build_weighted_slice_groups(
        projected,
        z_range,
        mid,
        pre_scale,
        page_mid,
        o,
        preview,
        &mut warnings,
    );

    if groups.is_empty() {
        warnings.push("No slice strokes survived minimum stroke length filtering.".into());
        return make_slice_svg(
            request,
            Vec::new(),
            prepared.residue_count,
            warnings,
            preview,
        );
    }

    fit_weighted_groups(&mut groups, o)?;
    make_slice_svg(request, groups, prepared.residue_count, warnings, preview)
}

fn make_slice_svg(
    request: &RenderRequest,
    groups: Vec<(String, Vec<WeightedPath>)>,
    residue_count: usize,
    warnings: Vec<String>,
    preview: bool,
) -> Result<RenderResult, RenderError> {
    let o = &request.options;
    let (stroke_count, point_count, length_mm) = slice_groups_stats(&groups);
    if point_count > MAX_PIECES {
        return fail("SVG exceeds 2000000 output points; reduce slice count, parallel lines, or overdraw passes");
    }

    let desc = if preview {
        "Approximate slice interaction preview. Parallel tracks and overdraw passes are omitted until release."
    } else {
        "Sliced molecular surface contours for pen plotting. Depth encoded by parallel tracks and multi-pass overdraw; stroke-opacity simulates ink buildup. No filled surfaces, masks, fonts, or raster data."
    };
    let opacity = format_opacity(o.slice_ink_opacity);

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">\n<title>{} / slice plot</title>\n<desc>{}</desc>\n<g fill=\"none\" stroke=\"black\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
        o.width_mm,
        o.height_mm,
        o.width_mm,
        o.height_mm,
        crate::xml(&request.structure.name),
        desc,
        o.pen_width_mm,
    );

    append_slice_groups_svg(&mut svg, &groups, &opacity);

    svg.push_str("</g>\n</svg>\n");
    if svg.len() > 64 * 1024 * 1024 {
        return fail("SVG exceeds 64 MiB");
    }

    Ok(RenderResult {
        svg,
        width_mm: o.width_mm,
        height_mm: o.height_mm,
        stroke_count,
        length_mm,
        residue_count,
        elapsed_ms: 0.0,
        warnings,
    })
}
