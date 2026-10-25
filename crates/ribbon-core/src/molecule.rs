//! Molecular wire geometry. Zero-width sections reuse the ribbon clipping
//! pipeline: bonds can be hidden by ribbons, but never act as opaque surfaces.
use crate::{math::Camera, ribbon::Section, Molecule};

pub(crate) fn project(molecule: &Molecule, camera: &Camera) -> Vec<Vec<Section>> {
    let points: Vec<_> = molecule.atoms.iter().map(|&p| camera.point(p)).collect();
    let section = |p: [f64; 3]| Section {
        left: [p[0], -p[1]],
        right: [p[0], -p[1]],
        z: p[2],
        alpha: 0.0,
    };
    let mut connected = vec![false; points.len()];
    let mut paths = Vec::with_capacity(molecule.bonds.len());
    for &[a, b] in &molecule.bonds {
        connected[a] = true;
        connected[b] = true;
        paths.push(vec![section(points[a]), section(points[b])]);
    }
    for (i, &point) in points.iter().enumerate() {
        if connected[i] {
            continue;
        }
        // Fixed molecular-space radius, billboarded toward the camera. Includes
        // isolated fragments of incomplete ligands, not just named metal ions.
        paths.push(
            (0..=16)
                .map(|step| {
                    let angle = step as f64 * std::f64::consts::TAU / 16.0;
                    section([
                        point[0] + 0.35 * angle.cos(),
                        point[1] + 0.35 * angle.sin(),
                        point[2],
                    ])
                })
                .collect(),
        );
    }
    paths
}

/// Bounded geometry for the browser's display-only drag preview. Missing
/// sampled bonds do not change the full-quality drawing or prepared camera.
pub(crate) fn preview(molecules: &[&Molecule]) -> Vec<Molecule> {
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
            let mut connected = vec![false; m.atoms.len()];
            for &[a, b] in &m.bonds {
                connected[a] = true;
                connected[b] = true;
                if cursor % stride == 0 {
                    let start = atoms.len();
                    atoms.extend([m.atoms[a], m.atoms[b]]);
                    bonds.push([start, start + 1]);
                }
                cursor += 1;
            }
            for (i, point) in m.atoms.iter().enumerate() {
                if connected[i] {
                    continue;
                }
                if cursor % stride == 0 {
                    atoms.push(*point);
                }
                cursor += 1;
            }
            (!atoms.is_empty()).then(|| Molecule {
                chain_id: m.chain_id.clone(),
                label: m.label.clone(),
                category: m.category.clone(),
                atoms,
                bonds,
            })
        })
        .collect()
}
