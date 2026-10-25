use ribbon_core::*;

fn request() -> RenderRequest {
    let molecules: Vec<_> = ["lipid", "molecule"]
        .into_iter()
        .enumerate()
        .map(|(i, category)| Molecule {
            chain_id: category.into(),
            label: category.into(),
            category: category.into(),
            atoms: vec![[0.0, i as f64 * 4.0, 0.0], [2.0, i as f64 * 4.0, 0.0]],
            bonds: vec![[0, 1]],
        })
        .collect();
    RenderRequest {
        structure: StructureData {
            name: "separate bond bands".into(),
            segments: vec![],
            residue_count: 0,
            warnings: vec![],
            chains: molecules
                .iter()
                .map(|m| Chain {
                    id: m.chain_id.clone(),
                    residue_count: 0,
                })
                .collect(),
            molecules,
        },
        options: RenderOptions {
            selected_chains: vec!["lipid".into(), "molecule".into()],
            ..RenderOptions::default()
        },
    }
}

#[test]
fn each_group_has_real_distinct_tracks_independent_of_protein_settings() {
    let mut req = request();
    for lines in [0, 1, 3, 8, 20] {
        req.options.molecule_styles.get_mut("lipid").unwrap().lines = lines;
        let result = render(&req).unwrap();
        // A U-turn connects each symmetric pair into one nested contour;
        // an unpaired middle track remains one open centerline.
        assert_eq!(result.stroke_count, (lines + 2).div_ceil(2) + 2);
        let paths: Vec<_> = result
            .svg
            .lines()
            .filter(|s| s.starts_with("<path"))
            .collect();
        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(
            unique.len(),
            paths.len(),
            "tracks must not repeat coincident geometry"
        );
        for protein_lines in [0, 20] {
            req.options.lines = protein_lines;
            req.options.border_passes = 1;
            req.options.fill_loops = false;
            assert_eq!(render(&req).unwrap().svg, result.svg);
        }
    }
    req.options
        .molecule_styles
        .get_mut("molecule")
        .unwrap()
        .lines = 5;
    assert_eq!(render(&req).unwrap().stroke_count, 11 + 4);
}

#[test]
fn group_width_changes_geometry_and_zero_width_restores_single_wires_with_cache_parity() {
    let mut req = request();
    let mut session = RenderSession::new(req.structure.clone()).unwrap();
    // Exercise non-default density explicitly: outlines, one nested pair and
    // an unpaired centerline, irrespective of the lipid defaults.
    req.options.molecule_styles.get_mut("lipid").unwrap().lines = 3;
    let mut previous = String::new();
    for width in [0.0, 0.5, 1.2, 4.0] {
        req.options
            .molecule_styles
            .get_mut("lipid")
            .unwrap()
            .bond_width_angstrom = width;
        let (actual, timing) = session.calculate(req.options.clone(), false).unwrap();
        assert_eq!(actual.svg, render(&req).unwrap().svg);
        assert_eq!(actual.stroke_count, if width == 0.0 { 1 } else { 3 } + 2);
        assert_ne!(actual.svg, previous);
        assert!(!timing.path_cache_hit);
        if !previous.is_empty() {
            assert!(timing.geometry_cache_hit);
        }
        let (_, cached) = session.calculate(req.options.clone(), false).unwrap();
        assert!(cached.path_cache_hit);
        previous = actual.svg;
        for path in previous.split("d=\"").skip(1) {
            if !path.starts_with('M') {
                continue;
            }
            for p in path
                .split('"')
                .next()
                .unwrap()
                .split(['M', 'L'])
                .filter(|v| !v.is_empty())
            {
                let xy: Vec<f64> = p.split(',').map(|v| v.parse().unwrap()).collect();
                let clearance = req.options.margin_mm + req.options.lipid_width_mm * 0.5;
                assert!(
                    xy[0] >= clearance - 0.0001
                        && xy[0] <= req.options.width_mm - clearance + 0.0001
                );
                assert!(
                    xy[1] >= clearance - 0.0001
                        && xy[1] <= req.options.height_mm - clearance + 0.0001
                );
            }
        }
    }
}

#[test]
fn invalid_group_settings_are_rejected_even_with_no_selection_and_on_cache_hits() {
    let req = request();
    let mut session = RenderSession::new(req.structure.clone()).unwrap();
    session.calculate(req.options.clone(), false).unwrap();
    for mutation in 0..5 {
        let mut o = req.options.clone();
        match mutation {
            0 => o.molecule_styles.get_mut("lipid").unwrap().lines = 21,
            1 => {
                o.molecule_styles
                    .get_mut("water")
                    .unwrap()
                    .bond_width_angstrom = -0.1
            }
            2 => {
                o.molecule_styles
                    .get_mut("water")
                    .unwrap()
                    .bond_width_angstrom = 4.1
            }
            3 => {
                o.molecule_styles
                    .get_mut("water")
                    .unwrap()
                    .bond_width_angstrom = f64::NAN
            }
            _ => {
                o.molecule_styles.insert(
                    "unknown".into(),
                    MoleculeStyle {
                        lines: 1,
                        bond_width_angstrom: 0.5,
                    },
                );
            }
        }
        assert!(session.calculate(o.clone(), false).is_err());
        o.selected_chains.clear();
        assert!(session.calculate(o, false).is_err());
    }
    let mut old = serde_json::to_value(&req).unwrap();
    old["options"]
        .as_object_mut()
        .unwrap()
        .remove("moleculeStyles");
    let restored: RenderRequest = serde_json::from_value(old).unwrap();
    assert_eq!(render(&restored).unwrap().svg, render(&req).unwrap().svg);
}
