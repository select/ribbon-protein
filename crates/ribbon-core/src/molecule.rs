//! Artistic bond bands with independent interior tracks. Their silhouettes
//! participate in the same approximate hidden-line removal as protein ribbons.
use crate::{
    math::{lerp, Camera},
    ribbon::Section,
    Molecule,
};

pub(crate) fn profile(section: &Section, lines: usize) -> Vec<[f64; 2]> {
    (0..lines + 2)
        .map(|i| lerp(section.left, section.right, i as f64 / (lines + 1) as f64))
        .collect()
}

pub(crate) fn project(molecule: &Molecule, camera: &Camera, width: f64) -> Vec<Vec<Section>> {
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
        let mut start = section(points[a]);
        let mut end = section(points[b]);
        let dx = end.left[0] - start.left[0];
        let dy = end.left[1] - start.left[1];
        let length = dx.hypot(dy);
        if width > 0.0 && length < 1e-10 {
            // A bond viewed exactly end-on becomes an outline cap, not NaNs or
            // a fabricated bond direction. No repeated coincident tracks.
            paths.push(
                (0..=16)
                    .map(|i| {
                        let angle = i as f64 * std::f64::consts::TAU / 16.0;
                        section([
                            points[a][0] + width * 0.5 * angle.cos(),
                            points[a][1] + width * 0.5 * angle.sin(),
                            points[a][2].max(points[b][2]),
                        ])
                    })
                    .collect(),
            );
            continue;
        }
        if width > 0.0 {
            let offset = [-dy / length * width * 0.5, dx / length * width * 0.5];
            for s in [&mut start, &mut end] {
                s.right = [s.left[0] + offset[0], s.left[1] + offset[1]];
                s.left = [s.left[0] - offset[0], s.left[1] - offset[1]];
            }
        }
        paths.push(vec![start, end]);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bond_bands_have_exact_width_separate_tracks_and_finite_end_on_caps() {
        let camera = Camera {
            center: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let mut molecule = Molecule {
            chain_id: "test".into(),
            label: "test".into(),
            category: "lipid".into(),
            atoms: vec![[0.0; 3], [2.0, 0.0, 1.0]],
            bonds: vec![[0, 1]],
        };
        let band = project(&molecule, &camera, 1.2);
        assert_eq!(band.len(), 1);
        for s in &band[0] {
            assert!((s.width() - 1.2).abs() < 1e-12);
        }
        assert_eq!(band[0][0].z, 0.0);
        assert_eq!(band[0][1].z, 1.0);
        let tracks = profile(&band[0][0], 3);
        assert_eq!(tracks.len(), 5);
        assert!(tracks
            .windows(2)
            .all(|w| (w[1][1] - w[0][1] - 0.3).abs() < 1e-12));
        molecule.atoms[1] = [0.0, 0.0, 2.0];
        let cap = project(&molecule, &camera, 1.2);
        assert_eq!(cap[0].len(), 17);
        assert!(cap[0]
            .iter()
            .all(|s| s.left == s.right && s.left.into_iter().all(f64::is_finite)));
        assert_eq!(cap[0][0].z, 2.0);
    }
}
