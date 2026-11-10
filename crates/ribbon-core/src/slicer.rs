//! Sliced molecular surface contours for pen plotting.
//!
//! Builds a 3D Gaussian occupancy field from heavy atoms, extracts section
//! contours via edge-indexed marching squares, and encodes 3D depth for
//! single-pen plotters through parallel contour tracks and multi-pass overdraw.

use crate::math::{self, add, dot, lerp, mul, norm, sub, Camera, P2, V3};
use crate::model::{
    fail, RenderError, RenderMode, RenderOptions, RenderRequest, RenderResult, SliceAxis,
    StrokeDirection, StructureData, MAX_PIECES,
};
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use std::collections::{BTreeMap, HashSet};
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
    view_rotation: Option<[u64; 3]>,
    slice_count: usize,
    coarseness: usize,
    probe_bits: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedSliceGroup {
    pub(crate) color: String,
    pub(crate) loops: Vec<Vec<V3>>,
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
        Self {
            colored_chains,
            camera_sig: override_camera.map(camera_signature),
            axis: options.slice_axis,
            view_rotation,
            slice_count: options.slice_count,
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
    match options.slice_axis {
        SliceAxis::Z => b,
        SliceAxis::Y => [b[2], b[0], b[1]],
        SliceAxis::X => [b[1], b[2], b[0]],
        SliceAxis::View => camera.rotated(options.rotation).basis,
    }
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
    z_min: f64,
    z_max: f64,
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

    let blur_passes = if coarseness > 88 {
        3
    } else if coarseness > 65 {
        2
    } else if coarseness > 35 {
        1
    } else {
        0
    };
    if blur_passes > 0 {
        let mut line = vec![0.0f32; nx.max(ny).max(nz)];
        for _ in 0..blur_passes {
            box_blur_3d(&mut grid, nx, ny, nz, &mut line);
        }
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
        z_min: low[2],
        z_max: high[2],
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

/// Exact grid-edge marching squares. Every crossed grid edge receives a unique
/// integer ID and consistent CCW orientation around `v > ISO`, so segments
/// stitch deterministically into closed loops without float-hash collisions.
fn marching_squares(plane: &[f32], nx: usize, ny: usize, ox: f64, oy: f64, h: f64) -> Vec<Vec<P2>> {
    let h_count = ny * nx;
    let total_edges = h_count + (ny - 1) * (nx + 1);
    let mut edge_point = vec![[0.0; 2]; total_edges];
    let mut next_edge = vec![u32::MAX; total_edges];
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
    let mut loops = Vec::new();
    for start in active_starts {
        let start = start as usize;
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
    loops
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
    // Remove accidental near-coincident consecutive vertices.
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

    let mut global_z_min = f64::INFINITY;
    let mut global_z_max = f64::NEG_INFINITY;
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
        global_z_min = global_z_min.min(field.z_min);
        global_z_max = global_z_max.max(field.z_max);
        fields.push((color, field));
    }

    if coarsened_any && !preview {
        warnings.push(
            "Envelope grid resolution was coarsened automatically to stay within local memory limits."
                .into(),
        );
    }

    let z_lo = global_z_min - o.slice_probe_angstrom;
    let z_hi = global_z_max + o.slice_probe_angstrom;

    for (color, field) in fields {
        let mut plane = vec![0.0f32; field.nx * field.ny];
        let mut loops = Vec::new();
        for i in 0..o.slice_count {
            let w = z_lo + (i as f64 + 0.5) * (z_hi - z_lo) / o.slice_count as f64;
            if w < field.oz - field.h || w > field.oz + field.nz as f64 * field.h {
                continue;
            }
            plane_at(&field, w, &mut plane);
            for raw_loop in
                marching_squares(&plane, field.nx, field.ny, field.ox, field.oy, field.h)
            {
                let Some(simplified) = simplify_closed_loop(&raw_loop, simplify_tol) else {
                    continue;
                };
                let mut loop_3d = Vec::with_capacity(simplified.len());
                for [u, v] in simplified {
                    let rel = add(add(mul(basis[0], u), mul(basis[1], v)), mul(basis[2], w));
                    max_r2 = max_r2.max(dot(rel, rel));
                    loop_3d.push(add(camera.center, rel));
                }
                loops.push(loop_3d);
            }
        }
        if !loops.is_empty() {
            groups.push(PreparedSliceGroup { color, loops });
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

/// Outward vertex offset vectors (bounded miter `[1.0, 1.45]`).
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

/// Contiguous `true` runs `(start, len)` around a cyclic ring of length `n`.
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

pub(crate) struct ProjectedSliceGroup {
    pub(crate) color: String,
    pub(crate) loops: Vec<(Vec<P2>, Vec<f64>)>,
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
                    // Include one bridging vertex so adjacent depth tiers connect cleanly.
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

    let groups = prepared
        .groups
        .iter()
        .map(|group| {
            let loops = group
                .loops
                .iter()
                .map(|loop_3d| {
                    let mut pts = Vec::with_capacity(loop_3d.len());
                    let mut zs = Vec::with_capacity(loop_3d.len());
                    for &world in loop_3d {
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
                        pts.push(p);
                        zs.push(c[2]);
                    }
                    (pts, zs)
                })
                .collect();
            ProjectedSliceGroup {
                color: group.color.clone(),
                loops,
            }
        })
        .collect();

    (groups, [low, high], [z_min, z_max])
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
                    .loops
                    .into_iter()
                    .filter_map(|(pts, _)| {
                        let mut closed: Vec<P2> = pts
                            .into_iter()
                            .map(|p| add(mul(sub(p, mid), scale), page_mid))
                            .collect();
                        let first = *closed.first()?;
                        closed.push(first);
                        (path_length(&closed) >= MIN_LENGTH_MM).then_some(WeightedPath {
                            points: closed,
                            passes: 1,
                        })
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
        for (raw_pts, zs) in group.loops {
            let pts: Vec<P2> = raw_pts
                .into_iter()
                .map(|p| add(mul(sub(p, mid), scale), page_mid))
                .collect();
            let n = pts.len();
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

            let mut u: Vec<f64> = zs
                .into_iter()
                .map(|z| {
                    let t = if z_span > 1e-9 {
                        ((z - z_range[0]) / z_span).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    t.powf(o.slice_depth_gamma)
                })
                .collect();
            for _ in 0..2 {
                let prev = u.clone();
                for i in 0..n {
                    u[i] = 0.25 * prev[(i + n - 1) % n] + 0.5 * prev[i] + 0.25 * prev[(i + 1) % n];
                }
            }

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
                                0.5 - 0.5 * (std::f64::consts::PI * f).cos()
                            } else if taper > 0 && k >= taper + len {
                                let rem = total - k;
                                let f = rem as f64 / (taper + 1) as f64;
                                0.5 - 0.5 * (std::f64::consts::PI * f).cos()
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
    let total_pts: usize = groups.iter().flat_map(|g| &g.loops).map(Vec::len).sum();
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
            (!loops.is_empty()).then_some(PreparedSliceGroup {
                color: group.color.clone(),
                loops,
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
