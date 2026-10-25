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
    assert_eq!(result.stroke_count, 4); // two bonds, two isolated atom outlines
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
