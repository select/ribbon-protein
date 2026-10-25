use super::*;

fn camera() -> Camera {
    Camera {
        center: [0.0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    }
}
fn molecule(atoms: Vec<V3>, bonds: Vec<[usize; 2]>) -> Molecule {
    Molecule {
        chain_id: "lipid".into(),
        label: "lipid".into(),
        category: "lipid".into(),
        atoms,
        bonds,
    }
}
fn project(m: &Molecule, width: f64, lines: usize) -> Vec<ProjectedPath> {
    super::project(m, &camera(), width, lines, &mut Budget::default()).unwrap()
}
fn bands(paths: &[ProjectedPath]) -> Vec<&[Section]> {
    paths
        .iter()
        .filter(|p| matches!(p.profile, Profile::Band(_)))
        .map(|p| p.sections.as_slice())
        .collect()
}
fn assert_finite(paths: &[ProjectedPath]) {
    for p in paths {
        for s in &p.sections {
            assert!(s
                .left
                .into_iter()
                .chain(s.right)
                .chain([s.z])
                .all(f64::is_finite));
        }
    }
}
#[test]
fn smooth_parallel_tracks_and_nested_terminal_u_turns_are_real_geometry() {
    let m = molecule(
        vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 1.0],
            [4.0, 2.0, 1.0],
        ],
        vec![[1, 0], [1, 2], [3, 2]],
    );
    let paths = project(&m, 0.4, 3);
    let b = bands(&paths);
    assert_eq!(
        b.len(),
        1,
        "bond input orientation cannot break degree-two chains"
    );
    assert!(b[0].len() > 10);
    for s in b[0] {
        assert!((s.width() - 0.4).abs() < 1e-10);
        let tracks = profile(s, 3);
        for pair in tracks.windows(2) {
            assert!((norm(sub(pair[1], pair[0])) - 0.1).abs() < 1e-10);
        }
    }
    for track in 0..5 {
        let directions: Vec<_> = b[0]
            .windows(2)
            .filter_map(|pair| {
                let d = sub(profile(&pair[1], 3)[track], profile(&pair[0], 3)[track]);
                (norm(d) > EPS).then(|| unit(d, [1.0, 0.0]))
            })
            .collect();
        assert!(
            directions.windows(2).all(|d| dot(d[0], d[1]) > 0.97),
            "no sharp degree-two corner in any parallel track"
        );
    }
    let arcs: Vec<_> = paths
        .iter()
        .filter(|p| matches!(p.profile, Profile::Wire))
        .collect();
    assert_eq!(
        arcs.len(),
        4,
        "two nested semicircles at each true terminal"
    );
    for arc in arcs {
        assert_eq!(arc.sections.len(), ARC_STEPS + 1);
        assert_ne!(arc.sections[ARC_STEPS / 2].left[0], arc.sections[0].left[0]);
    }
    assert_finite(&paths);
}
#[test]
fn caps_weld_both_outlines_and_paired_tracks_into_closed_contours() {
    let m = molecule(vec![[0.0; 3], [2.0, 0.0, 0.0]], vec![[0, 1]]);
    for lines in [0, 1, 3, 4, 20] {
        let projected = project(&m, 0.4, lines);
        let mut edges = Vec::new();
        for p in projected {
            match p.profile {
                Profile::Surface => {}
                Profile::Wire => {
                    edges.extend(p.sections.windows(2).map(|s| [s[0].left, s[1].left]))
                }
                Profile::Band(n) => {
                    for s in p.sections.windows(2) {
                        for (i, (a, b)) in profile(&s[0], n)
                            .into_iter()
                            .zip(profile(&s[1], n))
                            .enumerate()
                        {
                            edges.push(if i * 2 > n + 1 { [b, a] } else { [a, b] });
                        }
                    }
                }
            }
        }
        let joined =
            crate::paths::merge(&edges, true, &mut crate::paths::Budget::default()).unwrap();
        assert_eq!(joined.len(), (lines + 2).div_ceil(2));
        assert_eq!(
            joined.iter().filter(|p| p.first() == p.last()).count(),
            (lines + 2) / 2
        );
    }
}
#[test]
fn rings_close_without_caps_and_branches_never_add_cross_arm_tracks() {
    let ring = molecule(
        vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
        ],
        vec![[0, 1], [2, 1], [2, 3], [0, 3]],
    );
    let p = project(&ring, 0.4, 3);
    assert_eq!(p.len(), 1);
    assert_eq!(
        p[0].sections.first().unwrap().left,
        p[0].sections.last().unwrap().left
    );
    assert_eq!(
        p[0].sections.first().unwrap().right,
        p[0].sections.last().unwrap().right
    );
    let branch = molecule(
        vec![
            [0.0; 3],
            [3.0, 0.0, 0.0],
            [-2.0, 2.0, 0.0],
            [-2.0, -2.0, 0.0],
        ],
        vec![[0, 1], [0, 2], [0, 3]],
    );
    let p = project(&branch, 0.4, 3);
    let b = bands(&p);
    assert_eq!(b.len(), 3);
    for arm in b {
        assert_eq!(arm.len(), 2);
        assert!(norm(arm[0].left) <= 0.4 + EPS);
        assert!(norm(arm[0].right) <= 0.4 + EPS);
    }
    assert_eq!(
        p.iter()
            .filter(|p| matches!(p.profile, Profile::Wire))
            .count(),
        6,
        "three true termini only, two nested caps each"
    );
    assert_finite(&p);
}
#[test]
fn zero_width_is_original_wires_and_orphans_keep_fixed_circles() {
    let m = molecule(
        vec![[0.0; 3], [2.0, 0.0, 1.0], [4.0, 4.0, 0.0]],
        vec![[0, 1]],
    );
    let p = project(&m, 0.0, 20);
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].sections.len(), 17);
    assert_eq!(p[1].sections.len(), 2);
    assert_eq!(p[1].sections[0].left, [0.0, 0.0]);
    assert_eq!(p[1].sections[1].left, [2.0, 0.0]);
    assert_eq!(p[1].sections[1].z, 1.0);
    assert!((p[0].sections[0].left[0] - 4.0 - 0.35).abs() < EPS);
}
#[test]
fn end_on_short_hairpins_and_rotation_remain_finite_and_bounded() {
    for atoms in [
        vec![[0.0; 3], [0.0, 0.0, 2.0]],
        vec![[0.0; 3], [1e-8, 0.0, 1.0], [0.0, 1e-9, 2.0]],
        vec![[0.0; 3], [1.0, 0.0, 1.0], [0.0, 0.0, 2.0]],
    ] {
        let bonds = (1..atoms.len()).map(|i| [i - 1, i]).collect();
        let m = molecule(atoms, bonds);
        for rotation in [[0.0; 3], [90.0, 0.0, 0.0], [31.0, 72.0, 183.0]] {
            let p = super::project(
                &m,
                &camera().rotated(rotation),
                4.0,
                20,
                &mut Budget::default(),
            )
            .unwrap();
            assert_finite(&p);
            assert!(p.iter().flat_map(|p| &p.sections).all(|s| s
                .left
                .into_iter()
                .chain(s.right)
                .all(|v| v.abs() < 6.0)));
        }
    }
}
#[test]
fn expanded_work_is_bounded_before_a_long_chain_allocation() {
    let n = 8000;
    let m = molecule(
        (0..n).map(|i| [i as f64, (i % 2) as f64, 0.0]).collect(),
        (1..n).map(|i| [i - 1, i]).collect(),
    );
    assert!(
        super::project(&m, &camera(), 0.4, 20, &mut Budget::default())
            .unwrap_err()
            .to_string()
            .contains("Molecular geometry")
    );
}
#[test]
fn clipping_never_adds_a_cap_at_an_occlusion_cut() {
    let m = molecule(vec![[0.0; 3], [4.0, 0.0, 0.0]], vec![[0, 1]]);
    let projected = project(&m, 0.4, 0);
    let mut profiles = Vec::new();
    let mut panels = Vec::new();
    for (segment, p) in projected.into_iter().enumerate() {
        profiles.push(Some(p.profile));
        for (step, pair) in p.sections.windows(2).enumerate() {
            panels.push(crate::clip::Panel {
                start: pair[0].clone(),
                end: pair[1].clone(),
                segment,
                step,
            });
        }
    }
    let count = profiles.len();
    profiles.push(None);
    panels.push(crate::clip::Panel {
        start: Section {
            left: [1.0, -1.0],
            right: [1.0, 1.0],
            z: 10.0,
            alpha: 0.0,
        },
        end: Section {
            left: [3.0, -1.0],
            right: [3.0, 1.0],
            z: 10.0,
            alpha: 0.0,
        },
        segment: count,
        step: 0,
    });
    let visible = crate::clip::visible_edges(
        &mut panels,
        count + 1,
        2,
        &crate::RenderOptions::default(),
        &profiles,
    )
    .unwrap();
    let edges: Vec<_> = visible.into_iter().take(count).flatten().collect();
    let paths = crate::paths::merge(&edges, true, &mut crate::paths::Budget::default()).unwrap();
    assert_eq!(
        paths.len(),
        2,
        "one open U-contour on either side of the mask"
    );
    for path in paths {
        assert_ne!(path.first(), path.last());
        for end in [path[0], *path.last().unwrap()] {
            assert!((end[0] - 1.0).abs() < EPS || (end[0] - 3.0).abs() < EPS);
            assert!((end[1].abs() - 0.2).abs() < EPS);
        }
    }
}
#[test]
fn sampled_preview_preserves_shared_atoms_and_never_marks_cuts_as_terminals() {
    let m = molecule(
        (0..4000).map(|i| [i as f64, 0.0, 0.0]).collect(),
        (1..4000).map(|i| [i - 1, i]).collect(),
    );
    let sampled = preview(&[&m]);
    assert!(sampled[0].molecule.atoms.len() + sampled[0].molecule.bonds.len() <= 3000);
    assert_eq!(sampled[0].terminal_atoms, vec![0]);
    let m = molecule(
        vec![[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
        vec![[0, 1], [1, 2]],
    );
    let sampled = preview(&[&m]);
    assert_eq!(sampled[0].molecule.atoms.len(), 3);
    assert_eq!(sampled[0].molecule.bonds, vec![[0, 1], [1, 2]]);
    assert_eq!(sampled[0].terminal_atoms, vec![0, 2]);
}
