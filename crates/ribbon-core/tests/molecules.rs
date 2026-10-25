use ribbon_core::*;
use serde_json::{json, Value};

fn request() -> RenderRequest {
    RenderRequest {
        structure: StructureData {
            name: "molecules only".into(),
            segments: vec![],
            molecules: vec![
                Molecule {
                    chain_id: "ligand".into(),
                    label: "test ligand".into(),
                    category: "molecule".into(),
                    atoms: vec![
                        [0.0, 0.0, 0.0],
                        [1.5, 0.0, 0.0],
                        [1.5, 1.5, 0.5],
                        [5.0, 0.0, 0.0],
                    ],
                    bonds: vec![[0, 1], [1, 2]],
                },
                Molecule {
                    chain_id: "ion".into(),
                    label: "sodium".into(),
                    category: "ion".into(),
                    atoms: vec![[0.0, 4.0, 1.0]],
                    bonds: vec![],
                },
            ],
            chains: vec![
                Chain {
                    id: "ligand".into(),
                    residue_count: 0,
                },
                Chain {
                    id: "ion".into(),
                    residue_count: 0,
                },
            ],
            residue_count: 0,
            warnings: vec![],
        },
        options: RenderOptions {
            selected_chains: vec!["ligand".into(), "ion".into()],
            ..RenderOptions::default()
        },
    }
}

#[test]
fn molecular_only_bonds_and_isolated_atoms_are_finite_line_only_and_fit_the_page() {
    let req = request();
    let result = render(&req).unwrap();
    assert_eq!(result.residue_count, 0);
    // One connected outline, one unpaired centerline, two isolated circles.
    assert_eq!(result.stroke_count, 4);
    assert!(result.length_mm > 0.0);
    assert!(!result.svg.contains("NaN") && !result.svg.contains("Infinity"));
    assert!(!result.svg.contains("<circle") && !result.svg.contains("<mask"));
    assert!(result.svg.contains("fill=\"none\""));
    let mut only_ion = req.clone();
    only_ion.options.selected_chains = vec!["ion".into()];
    assert_eq!(render(&only_ion).unwrap().stroke_count, 1);
    only_ion.options.selected_chains.clear();
    assert_eq!(render(&only_ion).unwrap().stroke_count, 0);
}

#[test]
fn molecular_selection_rotation_and_colors_have_exact_session_cache_parity() {
    let mut req = request();
    let mut session = RenderSession::new(req.structure.clone()).unwrap();
    for selection in [vec!["ion"], vec!["ligand"], vec!["ion", "ligand"], vec![]] {
        req.options.selected_chains = selection.into_iter().map(String::from).collect();
        for rotation in [[0.0; 3], [45.0, 90.0, 10.0]] {
            req.options.rotation = rotation;
            for color in ["red", "#123aBc"] {
                req.options
                    .chain_colors
                    .insert("ligand".into(), color.into());
                let (result, _) = session.calculate(req.options.clone(), false).unwrap();
                assert_eq!(result.svg, render(&req).unwrap().svg);
                let scene: Value =
                    serde_json::from_str(&session.preview_scene_json().unwrap()).unwrap();
                assert_eq!(scene["ribbons"], json!([]));
                assert_eq!(
                    scene["molecules"].as_array().unwrap().len(),
                    req.options.selected_chains.len()
                );
            }
        }
    }
}

#[test]
fn lipids_have_independent_width_even_when_sharing_a_color_with_other_molecules() {
    let mut req = request();
    req.structure.molecules[0].category = "lipid".into();
    req.options
        .chain_colors
        .insert("ligand".into(), "red".into());
    req.options.chain_colors.insert("ion".into(), "red".into());
    let mut session = RenderSession::new(req.structure.clone()).unwrap();
    for width in [0.8, 1.2, 2.0] {
        req.options.lipid_width_mm = width;
        let (actual, timing) = session.calculate(req.options.clone(), false).unwrap();
        assert_eq!(actual.svg, render(&req).unwrap().svg);
        assert!(!timing.path_cache_hit);
        assert!(actual
            .svg
            .contains(&format!("stroke=\"red\" stroke-width=\"{width}\"")));
        assert!(actual.svg.contains("stroke=\"red\">"));
        let (_, cached) = session.calculate(req.options.clone(), false).unwrap();
        assert!(cached.path_cache_hit);
    }
    for width in [0.0, 2.1, f64::NAN] {
        req.options.lipid_width_mm = width;
        assert!(render(&req).is_err());
    }
}

#[test]
fn rounded_rings_branches_and_tiny_bonds_fit_and_keep_exact_session_parity() {
    for (atoms, bonds) in [
        (
            vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [2.0, 2.0, 0.0],
                [0.0, 2.0, 0.0],
            ],
            vec![[0, 1], [1, 2], [2, 3], [3, 0]],
        ),
        (
            vec![
                [0.0; 3],
                [3.0, 0.0, 0.0],
                [-2.0, 2.0, 0.0],
                [-2.0, -2.0, 1.0],
            ],
            vec![[0, 1], [0, 2], [0, 3]],
        ),
        (vec![[0.0; 3], [0.0, 0.0, 2.0]], vec![[0, 1]]),
        (
            vec![[0.0; 3], [1e-8, 0.0, 1.0], [0.0, 1e-9, 2.0]],
            vec![[0, 1], [1, 2]],
        ),
    ] {
        let mut req = request();
        req.structure.molecules.truncate(1);
        req.structure.molecules[0].atoms = atoms;
        req.structure.molecules[0].bonds = bonds;
        req.structure.molecules[0].category = "lipid".into();
        req.options.selected_chains = vec!["ligand".into()];
        let mut session = RenderSession::new(req.structure.clone()).unwrap();
        for width in [0.0, 0.4, 4.0] {
            for lines in [0, 3] {
                req.options
                    .molecule_styles
                    .get_mut("lipid")
                    .unwrap()
                    .bond_width_angstrom = width;
                req.options.molecule_styles.get_mut("lipid").unwrap().lines = lines;
                for rotation in [[0.0; 3], [90.0, 13.0, 27.0]] {
                    req.options.rotation = rotation;
                    let exact = render(&req).unwrap();
                    assert!(!exact.svg.contains("NaN") && !exact.svg.contains("Infinity"));
                    assert_eq!(
                        session.calculate(req.options.clone(), false).unwrap().0.svg,
                        exact.svg
                    );
                    assert!(
                        session
                            .calculate(req.options.clone(), false)
                            .unwrap()
                            .1
                            .path_cache_hit
                    );
                    for point in exact
                        .svg
                        .split("d=\"")
                        .skip(1)
                        .filter(|p| p.starts_with('M'))
                    {
                        for xy in point
                            .split('"')
                            .next()
                            .unwrap()
                            .split(['M', 'L'])
                            .filter(|v| !v.is_empty())
                        {
                            let p: Vec<f64> = xy.split(',').map(|v| v.parse().unwrap()).collect();
                            let clearance = req.options.margin_mm
                                + req.options.lipid_width_mm.max(req.options.pen_width_mm) * 0.5;
                            assert!(
                                p[0] >= clearance - 0.0001
                                    && p[0] <= req.options.width_mm - clearance + 0.0001
                            );
                            assert!(
                                p[1] >= clearance - 0.0001
                                    && p[1] <= req.options.height_mm - clearance + 0.0001
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn malformed_molecular_geometry_is_rejected_even_when_unselected() {
    let req = request();
    for mutation in 0..7 {
        let mut bad = req.clone();
        bad.options.selected_chains.clear();
        let molecule = &mut bad.structure.molecules[0];
        match mutation {
            0 => molecule.atoms[0][0] = f64::NAN,
            1 => molecule.bonds.push([0, 999]),
            2 => molecule.bonds.push([0, 0]),
            3 => molecule.bonds.push([1, 0]),
            4 => molecule.category = "<script>".into(),
            5 => molecule.chain_id = "unknown".into(),
            _ => molecule.atoms.clear(),
        }
        assert!(render(&bad).is_err(), "mutation {mutation}");
    }
}
