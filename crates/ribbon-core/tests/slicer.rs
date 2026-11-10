use ribbon_core::*;
use serde_json::Value;

fn helix_segment(chain: &str, offset_x: f64, n: usize) -> BackboneSegment {
    BackboneSegment {
        chain_id: chain.into(),
        residues: (0..n)
            .map(|i| {
                let t = i as f64 * 0.65;
                BackboneResidue {
                    ca: [offset_x + 2.3 * t.cos(), 2.3 * t.sin(), i as f64 * 1.5],
                    carbonyl: [t.cos(), t.sin(), 0.4],
                    kind: SecondaryStructure::H,
                }
            })
            .collect(),
    }
}

fn slice_request() -> RenderRequest {
    RenderRequest {
        structure: StructureData {
            name: "slicer & test".into(),
            segments: vec![helix_segment("A", 0.0, 20), helix_segment("B", 11.0, 20)],
            molecules: vec![Molecule {
                chain_id: "ligand:1".into(),
                label: "HEM".into(),
                category: "molecule".into(),
                atoms: vec![[5.5, 0.0, 12.0], [7.0, 0.5, 12.0], [6.2, 1.4, 12.5]],
                bonds: vec![[0, 1], [1, 2], [0, 2]],
                radii: vec![1.70, 1.55, 1.80],
            }],
            chain_atoms: vec![ChainAtoms {
                chain_id: "A".into(),
                atoms: (0..20)
                    .flat_map(|i| {
                        let t = i as f64 * 0.65;
                        let ca = [2.3 * t.cos(), 2.3 * t.sin(), i as f64 * 1.5];
                        let cb = [3.8 * t.cos(), 3.8 * t.sin(), i as f64 * 1.5 + 0.3];
                        [ca, cb]
                    })
                    .collect(),
                radii: (0..40)
                    .map(|k| if k % 2 == 0 { 1.70 } else { 1.80 })
                    .collect(),
            }],
            chains: vec![
                Chain {
                    id: "A".into(),
                    residue_count: 20,
                },
                Chain {
                    id: "B".into(),
                    residue_count: 20,
                },
                Chain {
                    id: "ligand:1".into(),
                    residue_count: 0,
                },
            ],
            residue_count: 40,
            warnings: vec![],
        },
        options: RenderOptions {
            render_mode: RenderMode::Slice,
            slice_count: 16,
            slice_coarseness: 45,
            slice_probe_angstrom: 1.4,
            slice_max_passes: 4,
            slice_max_parallel: 3,
            slice_parallel_spacing_mm: 0.25,
            slice_depth_gamma: 1.4,
            slice_perspective: true,
            slice_ink_opacity: 0.35,
            selected_chains: vec!["A".into(), "B".into(), "ligand:1".into()],
            ..RenderOptions::default()
        },
    }
}

fn parse_paths(svg: &str) -> Vec<Vec<[f64; 2]>> {
    svg.split("<path d=\"")
        .skip(1)
        .map(|part| {
            part.split('"')
                .next()
                .unwrap()
                .split(['M', 'L'])
                .filter(|p| !p.is_empty())
                .map(|pair| {
                    let mut xy = pair.split(',').map(|v| v.parse::<f64>().unwrap());
                    [xy.next().unwrap(), xy.next().unwrap()]
                })
                .collect()
        })
        .collect()
}

#[test]
fn slice_mode_uses_translucent_black_only_in_slice_mode_and_preserves_line_only_svg() {
    let slice_req = slice_request();
    let sliced = render(&slice_req).unwrap();
    assert!(sliced.stroke_count > 20 && sliced.length_mm > 100.0);
    assert_eq!(sliced.residue_count, 40);
    assert!(sliced
        .svg
        .contains("stroke=\"black\" stroke-opacity=\"0.35\""));
    assert!(sliced.svg.contains("slicer &amp; test / slice plot"));
    for forbidden in [
        "<polygon",
        "<rect",
        "<circle",
        "<mask",
        "<clipPath",
        "<image",
        "<text",
        "style=",
        "fill=\"black",
        "NaN",
        "inf",
    ] {
        assert!(!sliced.svg.contains(forbidden), "unexpected {forbidden}");
    }

    let parsed = parse_paths(&sliced.svg);
    assert_eq!(parsed.len(), sliced.stroke_count);
    let actual_len: f64 = parsed
        .iter()
        .flat_map(|p| p.windows(2))
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum();
    assert!((actual_len - sliced.length_mm).abs() < 0.1);

    let mut ribbon_req = slice_req;
    ribbon_req.options.render_mode = RenderMode::Ribbon;
    let ribbon = render(&ribbon_req).unwrap();
    assert!(!ribbon.svg.contains("stroke-opacity"));
}

#[test]
fn overdraw_passes_and_parallel_tracks_independently_build_plotter_depth() {
    let mut base = slice_request();
    base.options.rotation = [28.0, -35.0, 10.0];
    base.options.slice_max_passes = 1;
    base.options.slice_max_parallel = 1;
    let single = render(&base).unwrap();
    let single_paths = parse_paths(&single.svg);

    let mut overdrawn = base.clone();
    overdrawn.options.slice_max_passes = 4;
    let overdrawn_res = render(&overdrawn).unwrap();
    assert!(overdrawn_res.stroke_count > single.stroke_count);
    assert!(overdrawn_res.length_mm > single.length_mm * 1.4);

    // Parallel tracks add distinct geometry, not merely identical retraced paths.
    let mut parallel = base.clone();
    parallel.options.slice_max_parallel = 4;
    let parallel_res = render(&parallel).unwrap();
    let parallel_paths = parse_paths(&parallel_res.svg);
    assert!(parallel_res.stroke_count > single.stroke_count);
    assert!(parallel_paths.len() > single_paths.len());

    let mut combined = overdrawn;
    combined.options.slice_max_parallel = 4;
    let combined_res = render(&combined).unwrap();
    assert!(combined_res.stroke_count > overdrawn_res.stroke_count);
    assert!(combined_res.stroke_count > parallel_res.stroke_count);
}

#[test]
fn slice_axes_perspective_and_envelope_controls_change_geometry_and_fit_margins() {
    let base = slice_request();
    let base_paths = parse_paths(&render(&base).unwrap().svg);

    for axis in [SliceAxis::X, SliceAxis::Y, SliceAxis::View] {
        let mut req = base.clone();
        req.options.rotation = [22.0, 38.0, -15.0];
        req.options.slice_axis = axis;
        let out = render(&req).unwrap();
        assert!(out.stroke_count > 0);
        assert_ne!(parse_paths(&out.svg), base_paths);
    }

    let mut ortho = base.clone();
    ortho.options.slice_perspective = false;
    assert_ne!(parse_paths(&render(&ortho).unwrap().svg), base_paths);

    for (w, h, margin, pen, spacing) in [
        (210.0, 297.0, 15.0, 0.4, 0.22),
        (297.0, 210.0, 15.0, 0.25, 0.5),
        (93.14, 93.14, 5.0, 0.8, 0.35),
        (120.0, 90.0, 0.0, 1.5, 0.8),
    ] {
        let mut req = base.clone();
        req.options.width_mm = w;
        req.options.height_mm = h;
        req.options.margin_mm = margin;
        req.options.pen_width_mm = pen;
        req.options.slice_parallel_spacing_mm = spacing;
        req.options.slice_max_parallel = 5;
        let out = render(&req).unwrap();
        for p in parse_paths(&out.svg).into_iter().flatten() {
            assert!(
                p[0] >= margin + pen * 0.5 - 0.0002 && p[0] <= w - margin - pen * 0.5 + 0.0002,
                "x={} outside margin on width {}",
                p[0],
                w
            );
            assert!(
                p[1] >= margin + pen * 0.5 - 0.0002 && p[1] <= h - margin - pen * 0.5 + 0.0002,
                "y={} outside margin on height {}",
                p[1],
                h
            );
        }
    }
}

#[test]
fn slice_session_cache_matches_stateless_and_preview_never_poisons_exact_slices() {
    let mut req = slice_request();
    let mut session = RenderSession::new(req.structure.clone()).unwrap();

    // Preview on a cold view-axis key must not poison the subsequent exact calculation.
    req.options.slice_axis = SliceAxis::View;
    req.options.rotation = [18.0, -24.0, 5.0];
    let (preview, preview_timing) = session.calculate(req.options.clone(), true).unwrap();
    assert!(!preview_timing.geometry_cache_hit);
    assert!(preview
        .svg
        .contains("Approximate slice interaction preview"));

    let (exact, exact_timing) = session.calculate(req.options.clone(), false).unwrap();
    assert!(!exact_timing.geometry_cache_hit);
    assert_eq!(exact.svg, render(&req).unwrap().svg);
    assert_ne!(preview.svg, exact.svg);

    // Fixed-axis slices hit geometry cache across rotations, pass counts, and travel modes.
    req.options.slice_axis = SliceAxis::Z;
    let (first_z, t0) = session.calculate(req.options.clone(), false).unwrap();
    assert!(!t0.geometry_cache_hit);
    assert_eq!(first_z.svg, render(&req).unwrap().svg);

    for rot in [[0.0, 0.0, 0.0], [40.0, 25.0, -10.0], [-60.0, 110.0, 45.0]] {
        req.options.rotation = rot;
        req.options.slice_max_passes = 3;
        req.options.stroke_direction = StrokeDirection::Travel;
        let (actual, timing) = session.calculate(req.options.clone(), false).unwrap();
        assert!(timing.geometry_cache_hit);
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(render(&req).unwrap()).unwrap()
        );
    }

    let scene: Value = serde_json::from_str(&session.preview_scene_json().unwrap()).unwrap();
    assert!(scene["slices"].as_array().unwrap().len() > 4);
    assert!(scene["sliceRadius"].as_f64().unwrap() > 1.0);

    // Empty selection produces an empty slice sheet without error.
    req.options.selected_chains.clear();
    let empty = render(&req).unwrap();
    assert_eq!(empty.stroke_count, 0);
    assert!(empty
        .warnings
        .iter()
        .any(|w| w.contains("No chains selected")));
}

#[test]
fn invalid_slicer_options_and_atom_radii_fail_cleanly() {
    type Mut = Box<dyn Fn(&mut RenderRequest)>;
    let cases: Vec<Mut> = vec![
        Box::new(|r| r.options.slice_count = 1),
        Box::new(|r| r.options.slice_count = 121),
        Box::new(|r| r.options.slice_coarseness = 101),
        Box::new(|r| r.options.slice_probe_angstrom = -0.1),
        Box::new(|r| r.options.slice_probe_angstrom = 3.6),
        Box::new(|r| r.options.slice_max_passes = 0),
        Box::new(|r| r.options.slice_max_passes = 9),
        Box::new(|r| r.options.slice_max_parallel = 0),
        Box::new(|r| r.options.slice_max_parallel = 8),
        Box::new(|r| r.options.slice_parallel_spacing_mm = 0.01),
        Box::new(|r| r.options.slice_depth_gamma = 0.1),
        Box::new(|r| r.options.slice_ink_opacity = 0.05),
        Box::new(|r| r.options.slice_ink_opacity = 1.1),
        Box::new(|r| r.structure.chain_atoms[0].radii.pop().map(|_| ()).unwrap()),
        Box::new(|r| r.structure.chain_atoms[0].radii[0] = 9.0),
        Box::new(|r| r.structure.molecules[0].radii[0] = 0.1),
    ];
    for (i, mutate) in cases.into_iter().enumerate() {
        let mut req = slice_request();
        mutate(&mut req);
        assert!(render(&req).is_err(), "case {i} should fail validation");
    }
}
