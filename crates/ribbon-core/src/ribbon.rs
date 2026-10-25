//! Custom style port of legacy/python/render.py and continuous_tracks in plotter.py.
use crate::math::*;
use crate::model::{
    fail, BackboneSegment, RenderError, RenderOptions, SecondaryStructure, MAX_EDGES, MAX_TRACKS,
};

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Sample {
    pub point: V3,
    pub side: V3,
    pub width: f64,
    pub alpha: f64,
}

pub(crate) fn make_ribbon(segment: &BackboneSegment, samples: usize) -> (Vec<Sample>, bool) {
    let n = segment.residues.len();
    let ca: Vec<_> = segment.residues.iter().map(|r| r.ca).collect();
    let centers = gaussian(&ca, 0.55);
    let mut sides = Vec::with_capacity(n);
    let mut fallback_used = false;
    for (i, residue) in segment.residues.iter().enumerate() {
        let tangent = unit(
            gradient(&centers, i),
            unit(gradient(&ca, i), [0.0, 1.0, 0.0]),
        );
        let projected = sub(
            residue.carbonyl,
            mul(tangent, dot(residue.carbonyl, tangent)),
        );
        let previous = sides
            .last()
            .copied()
            .unwrap_or_else(|| perpendicular(tangent));
        let fallback = unit(
            sub(previous, mul(tangent, dot(previous, tangent))),
            perpendicular(tangent),
        );
        if norm(projected) < 1e-8 {
            fallback_used = true;
        }
        let mut side = unit(projected, fallback);
        if i > 0 && dot(sides[i - 1], side) < 0.0 {
            side = mul(side, -1.0);
        }
        sides.push(side);
    }
    let sides = gaussian(&sides, 0.8);
    let kinds: Vec<_> = segment.residues.iter().map(|r| r.kind).collect();
    let mut widths: Vec<_> = kinds
        .iter()
        .map(|kind| {
            [match kind {
                SecondaryStructure::H => 1.15,
                SecondaryStructure::E => 1.35,
                SecondaryStructure::C => 0.20,
            }]
        })
        .collect();
    for i in 0..n {
        if kinds[i] == SecondaryStructure::E
            && (i + 1 == n || kinds[i + 1] != SecondaryStructure::E)
        {
            widths[i] = [0.06];
            if i > 0 && kinds[i - 1] == SecondaryStructure::E {
                widths[i - 1] = [2.0];
            }
        }
    }
    let mut widths = gaussian(&widths, 0.75);
    widths[0] = [0.08];
    widths[n - 1] = [0.08];
    let loop_values: Vec<_> = kinds
        .iter()
        .map(|k| [f64::from(*k == SecondaryStructure::C)])
        .collect();
    let loop_values = gaussian(&loop_values, 1.15);
    let center_spline = Cubic::new(centers);
    let side_spline = Cubic::new(sides);
    let width_spline = Pchip::new(widths.into_iter().map(|v| v[0]).collect());
    let loop_spline = Pchip::new(loop_values.into_iter().map(|v| v[0]).collect());
    let count = (n - 1) * samples + 1;
    let mut result: Vec<Sample> = Vec::with_capacity(count);
    let mut previous_tangent = unit(sub(ca[1], ca[0]), [0.0, 1.0, 0.0]);
    for i in 0..count {
        let t = i as f64 / samples as f64;
        let (point, derivative) = center_spline.at(t);
        let tangent = unit(derivative, previous_tangent);
        previous_tangent = tangent;
        let (side, _) = side_spline.at(t);
        let projected = sub(side, mul(tangent, dot(side, tangent)));
        let previous = result
            .last()
            .map(|s| s.side)
            .unwrap_or_else(|| perpendicular(tangent));
        let fallback = unit(
            sub(previous, mul(tangent, dot(previous, tangent))),
            perpendicular(tangent),
        );
        let mut side = unit(projected, fallback);
        if dot(side, previous) < 0.0 {
            side = mul(side, -1.0);
        }
        let alpha = loop_spline.at(t).clamp(0.0, 1.0);
        result.push(Sample {
            point,
            side,
            width: width_spline.at(t).max(0.04),
            alpha: alpha * alpha * (3.0 - 2.0 * alpha),
        });
    }
    (result, fallback_used)
}

#[derive(Clone, Debug)]
pub(crate) struct Section {
    pub left: P2,
    pub right: P2,
    pub z: f64,
    pub alpha: f64,
}
impl Section {
    pub(crate) fn width(&self) -> f64 {
        norm(sub(self.right, self.left))
    }
}

/// Project and blend tube/ribbon frames BEFORE fitting, so tube extents cannot
/// escape the frame as with a fit that considers only projected ribbon sides.
pub(crate) fn project_ribbon(samples: &[Sample], camera: &Camera) -> Vec<Section> {
    let points: Vec<_> = samples.iter().map(|s| camera.point(s.point)).collect();
    let screen: Vec<P2> = points.iter().map(|p| [p[0], -p[1]]).collect();
    let mut result: Vec<Section> = Vec::with_capacity(samples.len());
    let mut previous_offset = [0.0; 2];
    let mut previous_tangent = [0.0, 1.0];
    for (i, sample) in samples.iter().enumerate() {
        let tangent = unit(gradient(&screen, i), previous_tangent);
        previous_tangent = tangent;
        let screen_side = [-tangent[1], tangent[0]];
        let side = camera.vector(sample.side);
        let ribbon = mul([side[0], -side[1]], sample.width);
        let sign = if dot(ribbon, screen_side) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let tube = mul(screen_side, sign * sample.width);
        let mut offset = lerp(ribbon, tube, sample.alpha);
        if i > 0 && dot(offset, previous_offset) < 0.0 {
            offset = mul(offset, -1.0);
        }
        previous_offset = offset;
        result.push(Section {
            left: sub(screen[i], offset),
            right: add(screen[i], offset),
            z: points[i][2],
            alpha: sample.alpha,
        });
    }
    result
}

/// One shared protein count, including duplicate ribbon tracks that merge
/// into dense loops. Molecular bands have independent, nonduplicated counts.
/// Bound the actual combined workload without silently reducing density.
pub(crate) fn track_count(
    sections: &[Vec<Section>],
    options: &RenderOptions,
    molecular_lines: &[Option<crate::molecule::Profile>],
) -> Result<usize, RenderError> {
    let max_loop_width = sections
        .iter()
        .flatten()
        .filter(|s| s.alpha > 0.95)
        .map(Section::width)
        .fold(0.0, f64::max);
    let intervals = if options.fill_loops {
        (max_loop_width / options.loop_spacing_mm).ceil() as usize
    } else {
        1
    };
    let count = (options.lines + 2 * options.border_passes).max(intervals + 1);
    if count > MAX_TRACKS {
        return fail(
            "Loop fill exceeds 256 tracks; increase loopSpacingMm or reduce page dimensions",
        );
    }
    let panels: usize = sections
        .iter()
        .enumerate()
        .map(|(i, s)| {
            (s.len() - 1)
                * if s.iter().all(|v| v.left == v.right) {
                    1
                } else {
                    molecular_lines
                        .get(i)
                        .copied()
                        .flatten()
                        .map_or(count, |profile| profile.tracks().max(1))
                }
        })
        .sum();
    if panels > MAX_EDGES {
        return fail(
            "Drawing exceeds 1000000 track segments; reduce samples, loop density, or input size",
        );
    }
    Ok(count)
}

/// Identical section inputs produce bit-identical endpoints at every seam.
/// No transverse caps or connecting chords are ever generated.
pub(crate) fn profile(section: &Section, count: usize, o: &RenderOptions) -> Vec<P2> {
    let width = section.width().max(1e-12);
    let border: Vec<_> = (0..o.border_passes)
        .map(|j| {
            (j as f64 * o.border_spacing_mm / width)
                .min(0.35 * j as f64 / (o.border_passes - 1).max(1) as f64)
        })
        .collect();
    let mut ribbon = border.clone();
    for j in 0..o.lines {
        ribbon.push(if o.lines == 1 {
            0.5
        } else {
            0.12 + j as f64 * 0.76 / (o.lines - 1) as f64
        });
    }
    ribbon.extend(border.iter().map(|t| 1.0 - t));
    ribbon.sort_by(f64::total_cmp);
    (0..count)
        .map(|j| {
            // Python's round uses ties-to-even. Match it when remapping duplicates.
            let index = (j as f64 * (ribbon.len() - 1) as f64 / (count - 1) as f64)
                .round_ties_even() as usize;
            let tube = if o.fill_loops {
                j as f64 / (count - 1) as f64
            } else {
                f64::from(j >= count / 2)
            };
            let t = (1.0 - section.alpha) * ribbon[index] + section.alpha * tube;
            lerp(section.left, section.right, t)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BackboneResidue;
    fn section(x: f64, width: f64, alpha: f64) -> Section {
        Section {
            left: [x, -width / 2.0],
            right: [x, width / 2.0],
            z: 0.0,
            alpha,
        }
    }
    #[test]
    fn molecular_density_is_included_in_the_track_work_budget() {
        let o = RenderOptions {
            lines: 0,
            border_passes: 1,
            fill_loops: false,
            ..RenderOptions::default()
        };
        let sections = vec![vec![section(0.0, 1.0, 0.0), section(1.0, 1.0, 0.0)]; 45_455];
        assert!(track_count(
            &sections,
            &o,
            &vec![Some(crate::molecule::Profile::Band(20)); sections.len()]
        )
        .is_err());
        assert_eq!(
            track_count(
                &sections,
                &o,
                &vec![Some(crate::molecule::Profile::Band(0)); sections.len()]
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn transition_profiles_share_seams_and_never_make_caps() {
        let o = RenderOptions::default();
        let sections = vec![vec![
            section(0.0, 2.0, 0.0),
            section(1.0, 2.0, 0.5),
            section(2.0, 0.4, 1.0),
        ]];
        let count = track_count(&sections, &o, &[]).unwrap();
        assert_eq!(count, 13);
        let a = profile(&sections[0][0], count, &o);
        let b = profile(&sections[0][1], count, &o);
        let c = profile(&sections[0][2], count, &o);
        for j in 0..count {
            assert!(a[j][0] < b[j][0] && b[j][0] < c[j][0]);
            assert_eq!(b[j], profile(&sections[0][1], count, &o)[j]);
        }
    }
    #[test]
    fn borders_are_inset_and_loop_fill_respects_spacing() {
        let o = RenderOptions {
            lines: 0,
            ..RenderOptions::default()
        };
        let ribbon = section(0.0, 1.0, 0.0);
        let p = profile(&ribbon, 6, &o);
        let expected = [-0.5, -0.4, -0.3, 0.3, 0.4, 0.5];
        for (p, y) in p.iter().zip(expected) {
            assert!((p[1] - y).abs() < 1e-12);
        }
        let loop_section = section(1.0, 1.0, 1.0);
        let count = track_count(&[vec![ribbon, loop_section.clone()]], &o, &[]).unwrap();
        let p = profile(&loop_section, count, &o);
        assert!(p
            .windows(2)
            .all(|p| norm(sub(p[1], p[0])) <= o.loop_spacing_mm));
        let no_fill = RenderOptions {
            fill_loops: false,
            ..o
        };
        assert!(profile(&loop_section, count, &no_fill)
            .iter()
            .all(|p| p[1].abs() == 0.5));
    }
    #[test]
    fn stable_frames_for_parallel_and_flipped_carbonyls() {
        let segment = BackboneSegment {
            chain_id: "A".into(),
            residues: (0..8)
                .map(|i| BackboneResidue {
                    ca: [0.0, i as f64 * 3.8, 0.0],
                    carbonyl: if i % 2 == 0 {
                        [0.0, 1.0, 0.0]
                    } else {
                        [-1.0, 0.0, 0.0]
                    },
                    kind: SecondaryStructure::H,
                })
                .collect(),
        };
        let (samples, fallback) = make_ribbon(&segment, 16);
        assert!(fallback);
        for s in &samples {
            assert!((norm(s.side) - 1.0).abs() < 1e-10);
            assert!(s.width >= 0.04 && s.width.is_finite());
        }
        assert!(samples.windows(2).all(|w| dot(w[0].side, w[1].side) > 0.99));
    }
    #[test]
    fn numerical_samples_match_the_python_scipy_reference() {
        #[derive(serde::Deserialize)]
        struct ExpectedSample {
            index: usize,
            point: V3,
            side: V3,
            width: f64,
            alpha: f64,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            segment: BackboneSegment,
            samples: Vec<ExpectedSample>,
        }
        let reference: Fixture =
            serde_json::from_str(include_str!("../tests/fixtures/python-ribbon.json")).unwrap();
        let (samples, fallback) = make_ribbon(&reference.segment, 8);
        assert!(!fallback);
        for expected in reference.samples {
            let actual = &samples[expected.index];
            assert!(norm(sub(actual.point, expected.point)) < 1e-10);
            assert!(norm(sub(actual.side, expected.side)) < 1e-10);
            assert!((actual.width - expected.width).abs() < 1e-10);
            assert!((actual.alpha - expected.alpha).abs() < 1e-10);
        }
    }

    #[test]
    fn beta_tip_is_narrow_with_arrow_shoulder() {
        let segment = BackboneSegment {
            chain_id: "A".into(),
            residues: (0..14)
                .map(|i| BackboneResidue {
                    ca: [0.0, i as f64 * 3.8, 0.0],
                    carbonyl: [1.0, 0.0, 0.0],
                    kind: if i < 10 {
                        SecondaryStructure::E
                    } else {
                        SecondaryStructure::C
                    },
                })
                .collect(),
        };
        let (s, _) = make_ribbon(&segment, 16);
        assert!(s[8 * 16].width > s[6 * 16].width);
        assert!(s[9 * 16].width < s[8 * 16].width);
        assert_eq!(s[0].width, 0.08);
        assert!((s.last().unwrap().width - 0.08).abs() < 1e-12);
    }
}
