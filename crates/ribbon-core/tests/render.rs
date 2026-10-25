use ribbon_core::*;
use serde_json::{json, Value};

type Mutation<T> = Box<dyn Fn(&mut T)>;

fn segment(chain: &str, offset: f64, n: usize) -> BackboneSegment {
    BackboneSegment {
        chain_id: chain.into(),
        residues: (0..n)
            .map(|i| {
                let t = i as f64 * 0.55;
                BackboneResidue {
                    ca: [offset + 2.0 * t.cos(), 2.0 * t.sin(), i as f64 * 2.7],
                    carbonyl: [t.cos(), t.sin(), 0.3],
                    kind: match i % 18 {
                        0..=5 => SecondaryStructure::H,
                        6..=11 => SecondaryStructure::E,
                        _ => SecondaryStructure::C,
                    },
                }
            })
            .collect(),
    }
}
fn request() -> RenderRequest {
    RenderRequest {
        structure: StructureData {
            name: "synthetic & test <ribbon>".into(),
            molecules: vec![],
            segments: vec![
                segment("auth-A", 0.0, 24),
                segment("long_chain_B", 12.0, 24),
            ],
            chains: vec![
                Chain {
                    id: "auth-A".into(),
                    residue_count: 24,
                },
                Chain {
                    id: "long_chain_B".into(),
                    residue_count: 24,
                },
            ],
            residue_count: 48,
            warnings: vec!["Secondary structure supplied by test fixture.".into()],
        },
        options: RenderOptions {
            selected_chains: vec!["auth-A".into(), "long_chain_B".into()],
            ..RenderOptions::default()
        },
    }
}

fn paths(svg: &str) -> Vec<Vec<[f64; 2]>> {
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
fn stateful_cache_matches_stateless_for_every_option_and_selection_key() {
    let mut req = request();
    req.options.samples = 4;
    let mut session = RenderSession::new(req.structure.clone()).unwrap();
    let variants: Vec<Mutation<RenderOptions>> = vec![
        Box::new(|_| {}),
        Box::new(|o| o.rotation = [33.0, -29.0, 81.0]),
        Box::new(|o| o.lines = 0),
        Box::new(|o| o.lines = 20),
        Box::new(|o| o.samples = 8),
        Box::new(|o| o.selected_chains.clear()),
        Box::new(|o| o.selected_chains = vec!["auth-A".into()]),
        Box::new(|o| o.selected_chains = vec!["long_chain_B".into()]),
        Box::new(|o| o.selected_chains.reverse()),
        Box::new(|o| {
            o.paper = Paper::Bierdeckel;
            o.width_mm = 93.14;
            o.height_mm = 93.14;
            o.margin_mm = 5.0;
        }),
        Box::new(|o| {
            o.paper = Paper::Custom;
            o.width_mm = 140.0;
            o.height_mm = 100.0;
            o.landscape = true;
        }),
        Box::new(|o| o.margin_mm = 0.0),
        Box::new(|o| o.pen_width_mm = 2.0),
        Box::new(|o| o.fill_loops = false),
        Box::new(|o| o.loop_spacing_mm = 0.3),
        Box::new(|o| o.border_passes = 1),
        Box::new(|o| o.border_spacing_mm = 0.2),
        Box::new(|o| o.stroke_direction = StrokeDirection::Travel),
    ];
    let base = req.options.clone();
    for mutate in variants {
        req.options = base.clone();
        mutate(&mut req.options);
        let expected = serde_json::to_value(render(&req).unwrap()).unwrap();
        let (actual, _) = session.calculate(req.options.clone(), false).unwrap();
        assert_eq!(serde_json::to_value(actual).unwrap(), expected);
        // Repeat via joined cache, then group split/merge colors. Direction and
        // order must match stateless even when several segments share a color.
        for colors in [
            vec![],
            vec!["red", "black"],
            vec!["red", "red"],
            vec!["#123aBC", "red"],
        ] {
            req.options.chain_colors = req
                .structure
                .chains
                .iter()
                .zip(colors)
                .map(|(c, color)| (c.id.clone(), color.into()))
                .collect();
            let (actual, timing) = session.calculate(req.options.clone(), false).unwrap();
            assert!(timing.path_cache_hit);
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(render(&req).unwrap()).unwrap()
            );
        }
    }
}

#[test]
fn prepared_cache_is_bounded_and_preview_never_poisoned_the_final_paths() {
    let mut req = request();
    req.options.samples = 2;
    let mut session = RenderSession::new(req.structure.clone()).unwrap();
    let (_, first) = session.calculate(req.options.clone(), false).unwrap();
    assert!(!first.geometry_cache_hit && !first.path_cache_hit);
    req.options.rotation[0] = 20.0;
    let (preview, timing) = session.calculate(req.options.clone(), true).unwrap();
    assert!(timing.geometry_cache_hit && !timing.path_cache_hit);
    assert_eq!(timing.clip_ms, 0.0);
    assert!(preview.svg.contains("Hidden strokes are NOT removed"));
    let (exact, timing) = session.calculate(req.options.clone(), false).unwrap();
    assert!(timing.geometry_cache_hit && !timing.path_cache_hit);
    assert_ne!(preview.svg, exact.svg);
    assert_eq!(exact.svg, render(&req).unwrap().svg);
    for samples in [3, 4, 2] {
        req.options.samples = samples;
        let (_, timing) = session.calculate(req.options.clone(), false).unwrap();
        assert!(!timing.geometry_cache_hit); // 2 has been evicted by 3 + 4.
    }
    // Same chain IDs, samples and options, DIFFERENT coordinates and labels.
    let mut other = req.clone();
    other.structure.segments[0].residues[2].ca[0] += 0.25;
    other.structure.segments[0].residues[2].kind = SecondaryStructure::C;
    let mut replacement = RenderSession::new(other.structure.clone()).unwrap();
    let (result, timing) = replacement.calculate(other.options.clone(), false).unwrap();
    assert!(!timing.geometry_cache_hit && !timing.path_cache_hit);
    assert_eq!(result.svg, render(&other).unwrap().svg);
    assert_ne!(result.svg, render(&req).unwrap().svg);
}

#[test]
fn session_json_validates_options_on_cache_hits_and_recovers_from_errors() {
    let req = request();
    let mut session =
        RenderSession::from_json(&serde_json::to_string(&req.structure).unwrap()).unwrap();
    let options = serde_json::to_string(&req.options).unwrap();
    let first: Value =
        serde_json::from_str(&session.render_options_json(&options, false).unwrap()).unwrap();
    assert!(session.render_options_json("{}", false).is_err());
    let mut invalid = req.options.clone();
    invalid.pen_width_mm = 0.0;
    assert!(session
        .render_options_json(&serde_json::to_string(&invalid).unwrap(), false)
        .is_err());
    let next: Value =
        serde_json::from_str(&session.render_options_json(&options, false).unwrap()).unwrap();
    assert_eq!(first["result"], next["result"]);
    assert_eq!(next["timings"]["pathCacheHit"], true);
    assert!(RenderSession::from_json("{}").is_err());
}

#[test]
fn actual_render_is_finite_line_only_svg_with_physical_statistics() {
    let r = render(&request()).unwrap();
    assert!(r.stroke_count > 5 && r.length_mm > 100.0);
    assert_eq!(r.residue_count, 48);
    assert_eq!(r.elapsed_ms, 0.0);
    assert!(r.svg.contains("width=\"210mm\" height=\"297mm\""));
    assert!(r.svg.contains("stroke-width=\"0.25\""));
    assert!(r.svg.contains("synthetic &amp; test &lt;ribbon&gt;"));
    for forbidden in [
        "<polygon",
        "<rect",
        "<circle",
        "<mask",
        "<clipPath",
        "<image",
        "<text",
        "fill=\"black",
        "NaN",
        "inf",
    ] {
        assert!(!r.svg.contains(forbidden), "{forbidden}");
    }
    let paths = paths(&r.svg);
    assert_eq!(paths.len(), r.stroke_count);
    let length: f64 = paths
        .iter()
        .flat_map(|p| p.windows(2))
        .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
        .sum();
    assert!((length - r.length_mm).abs() < 0.1);
    assert!(r.warnings.iter().any(|w| w.contains("average panel depth")));
    assert!(r.warnings.iter().any(|w| w.contains("overlap")));
}

#[test]
fn resolved_pages_never_swap_landscape_twice_and_ink_stays_inside_margin() {
    for (paper, w, h, landscape, margin) in [
        (Paper::A4, 210.0, 297.0, false, 15.0),
        (Paper::A4, 297.0, 210.0, true, 15.0),
        (Paper::A3, 420.0, 297.0, true, 15.0),
        (Paper::Bierdeckel, 93.14, 93.14, false, 5.0),
        (Paper::Custom, 57.0, 81.0, true, 0.0),
    ] {
        for pen in [0.1, 0.25, 2.0] {
            let mut req = request();
            req.options.paper = paper;
            req.options.width_mm = w;
            req.options.height_mm = h;
            req.options.landscape = landscape;
            req.options.margin_mm = margin;
            req.options.pen_width_mm = pen;
            let r = render(&req).unwrap();
            assert_eq!((r.width_mm, r.height_mm), (w, h));
            assert!(r.svg.contains(&format!("width=\"{w}mm\" height=\"{h}mm\"")));
            for p in paths(&r.svg).into_iter().flatten() {
                assert!(
                    p[0] >= margin + pen * 0.5 - 0.0001 && p[0] <= w - margin - pen * 0.5 + 0.0001
                );
                assert!(
                    p[1] >= margin + pen * 0.5 - 0.0001 && p[1] <= h - margin - pen * 0.5 + 0.0001
                );
            }
        }
    }
}

#[test]
fn explicit_chain_filtering_and_colors_preserve_original_ids() {
    let mut req = request();
    req.options
        .chain_colors
        .insert("auth-A".into(), "#123aBC".into());
    req.options
        .chain_colors
        .insert("long_chain_B".into(), "red".into());
    let all = render(&req).unwrap();
    assert!(all.svg.contains("stroke=\"#123aBC\"") && all.svg.contains("stroke=\"red\""));
    req.options.selected_chains = vec!["long_chain_B".into()];
    let one = render(&req).unwrap();
    assert_eq!(one.residue_count, 24);
    assert!(one.svg.contains("stroke=\"red\"") && !one.svg.contains("#123aBC"));
    assert_ne!(one.svg, all.svg);
    req.options.selected_chains.clear();
    let none = render(&req).unwrap();
    assert_eq!(none.residue_count, 0);
    assert_eq!(none.stroke_count, 0);
    assert_eq!(none.length_mm, 0.0);
    assert!(!none.svg.contains("<path"));
    assert!(none
        .warnings
        .iter()
        .any(|w| w.contains("No chains selected")));
}

#[test]
fn blank_author_chain_ids_and_duplicate_selections_do_not_rename_or_repeat_geometry() {
    let mut req = request();
    req.structure.chains[0].id.clear();
    req.structure.segments[0].chain_id.clear();
    req.options.selected_chains = vec![String::new()];
    req.options.chain_colors.insert(String::new(), "RED".into());
    let first = render(&req).unwrap();
    assert_eq!(first.residue_count, 24);
    assert!(first.svg.contains("stroke=\"RED\""));
    req.options.selected_chains.push(String::new());
    assert_eq!(render(&req).unwrap().svg, first.svg);
}

#[test]
fn view_style_and_travel_controls_change_real_geometry() {
    let req = request();
    let base = render(&req).unwrap();
    let changes: Vec<Mutation<RenderOptions>> = vec![
        Box::new(|o| o.rotation = [25.0, 65.0, 15.0]),
        Box::new(|o| o.lines = 0),
        Box::new(|o| o.lines = 20),
        Box::new(|o| o.samples = 2),
        Box::new(|o| o.fill_loops = false),
        Box::new(|o| o.border_passes = 1),
        Box::new(|o| o.loop_spacing_mm = 0.5),
        Box::new(|o| o.border_spacing_mm = 0.4),
        Box::new(|o| o.stroke_direction = StrokeDirection::Travel),
    ];
    for change in changes {
        let mut changed = req.clone();
        change(&mut changed.options);
        let output = render(&changed).unwrap();
        assert!(output.stroke_count > 0);
        assert_ne!(paths(&output.svg), paths(&base.svg));
    }
}

#[test]
fn json_contract_has_only_camel_case_response_fields_and_strict_requests() {
    let req = request();
    let encoded = serde_json::to_string(&req).unwrap();
    assert!(encoded.contains("\"chainId\"") && encoded.contains("\"selectedChains\""));
    let result: Value = serde_json::from_str(&render_json_native(&encoded).unwrap()).unwrap();
    let mut fields: Vec<_> = result
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort();
    assert_eq!(
        fields,
        [
            "elapsedMs",
            "heightMm",
            "lengthMm",
            "residueCount",
            "strokeCount",
            "svg",
            "warnings",
            "widthMm"
        ]
    );
    let mut bad: Value = serde_json::to_value(&req).unwrap();
    bad["options"]["pen_width_mm"] = json!(0.25);
    assert!(render_json_native(&bad.to_string()).is_err());
    for text in [
        "",
        "null",
        "{}",
        "{broken",
        "{\"structure\":null}",
        "[[[[[[[[[[",
        "{\"options\":{\"widthMm\":1e999}}}",
    ] {
        assert!(render_json_native(text).is_err(), "{text}");
    }
    let mut bad: Value = serde_json::to_value(&req).unwrap();
    bad["structure"]["segments"][0]["residues"][0]["kind"] = json!("helix");
    assert!(render_json_native(&bad.to_string()).is_err());
}

#[test]
fn invalid_options_and_coordinates_return_errors_not_panics() {
    let mutations: Vec<Mutation<RenderRequest>> = vec![
        Box::new(|r| r.options.width_mm = f64::NAN),
        Box::new(|r| r.options.height_mm = f64::INFINITY),
        Box::new(|r| r.options.margin_mm = -1.0),
        Box::new(|r| r.options.margin_mm = 105.0),
        Box::new(|r| r.options.pen_width_mm = 0.09),
        Box::new(|r| r.options.pen_width_mm = 2.01),
        Box::new(|r| r.options.lines = 21),
        Box::new(|r| r.options.samples = 0),
        Box::new(|r| r.options.samples = 65),
        Box::new(|r| r.options.border_passes = 0),
        Box::new(|r| r.options.border_passes = 4),
        Box::new(|r| r.options.loop_spacing_mm = 0.0),
        Box::new(|r| r.options.border_spacing_mm = f64::NAN),
        Box::new(|r| r.options.rotation[2] = f64::NEG_INFINITY),
        Box::new(|r| r.options.selected_chains.push("missing".into())),
        Box::new(|r| {
            r.options
                .chain_colors
                .insert("auth-A".into(), "url(javascript:evil)".into());
        }),
        Box::new(|r| {
            r.options
                .chain_colors
                .insert("missing".into(), "red".into());
        }),
        Box::new(|r| r.structure.segments[0].residues[0].ca[0] = f64::NAN),
        Box::new(|r| r.structure.segments[0].residues[0].ca[0] = 1e20),
        Box::new(|r| r.structure.segments[0].residues[0].carbonyl[0] = f64::INFINITY),
        Box::new(|r| r.structure.segments[0].residues[0].carbonyl = [0.0; 3]),
        Box::new(|r| {
            r.structure.segments[0].residues[1].ca = r.structure.segments[0].residues[0].ca
        }),
        Box::new(|r| r.structure.segments[0].residues[1].ca = [100.0; 3]),
        Box::new(|r| r.structure.segments[0].residues.truncate(3)),
        Box::new(|r| r.structure.segments[0].chain_id = "missing".into()),
        Box::new(|r| r.structure.chains.push(r.structure.chains[0].clone())),
    ];
    for (i, mutate) in mutations.into_iter().enumerate() {
        let mut req = request();
        mutate(&mut req);
        let result = std::panic::catch_unwind(|| render(&req));
        assert!(result.is_ok(), "invalid case {i} panicked");
        assert!(result.unwrap().is_err(), "invalid case {i} was accepted");
    }
}

#[test]
fn workload_limits_reject_before_large_track_allocation() {
    let mut req = request();
    req.options.samples = 64;
    req.structure.segments = vec![segment("auth-A", 0.0, 4000)];
    assert!(render(&req)
        .unwrap_err()
        .to_string()
        .contains("200000 spline samples"));

    let mut req = request();
    req.structure.segments = vec![segment("auth-A", 0.0, 50_001)];
    assert!(render(&req).is_err());

    let mut req = request();
    req.options.selected_chains = vec!["auth-A".into()];
    req.options.width_mm = 2000.0;
    req.options.height_mm = 2000.0;
    req.options.loop_spacing_mm = 0.02;
    req.structure.segments = vec![segment("auth-A", 0.0, 4)];
    req.structure.segments[0]
        .residues
        .iter_mut()
        .for_each(|r| r.kind = SecondaryStructure::C);
    assert!(render(&req).unwrap_err().to_string().contains("256 tracks"));
}

#[test]
fn deterministic_native_output_and_xml_control_sanitizing() {
    let mut req = request();
    req.structure.name = "a\0b\u{fffe}<unsafe>&".into();
    let first = render(&req).unwrap();
    for _ in 0..3 {
        assert_eq!(render(&req).unwrap().svg, first.svg);
    }
    assert!(!first.svg.contains('\0') && !first.svg.contains('\u{fffe}'));
    assert!(first.svg.contains("ab&lt;unsafe&gt;&amp;"));
}
