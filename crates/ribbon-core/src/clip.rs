//! Hidden-line removal by subtracting unions of parameter intervals.
//!
//! R-tree queries bound candidate surfaces. Even-odd quadrilateral membership
//! handles concave and bow-tie panels without exporting masks or native GEOS.
//! Like the Python reference, panels use average center depth, not ray tracing.
use crate::math::*;
use crate::model::{fail, RenderError, RenderOptions, MAX_CLIP_TESTS, MAX_PIECES};
use crate::ribbon::{profile, Section};
use rstar::{RTree, RTreeObject, AABB};

pub(crate) type Edge = [P2; 2];
const EPS: f64 = 1e-9;

#[derive(Clone, Debug)]
pub(crate) struct Panel {
    pub start: Section,
    pub end: Section,
    pub segment: usize,
    pub step: usize,
}
impl Panel {
    pub(crate) fn corners(&self) -> [P2; 4] {
        [
            self.start.left,
            self.end.left,
            self.end.right,
            self.start.right,
        ]
    }
    pub(crate) fn depth(&self) -> f64 {
        (self.start.z + self.end.z) * 0.5
    }
}

#[derive(Clone)]
struct IndexedPanel {
    index: usize,
    envelope: AABB<P2>,
}
impl RTreeObject for IndexedPanel {
    type Envelope = AABB<P2>;
    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}
fn envelope(points: &[P2]) -> AABB<P2> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for p in points {
        for i in 0..2 {
            low[i] = low[i].min(p[i] - EPS);
            high[i] = high[i].max(p[i] + EPS);
        }
    }
    AABB::from_corners(low, high)
}

fn on_edge(p: P2, a: P2, b: P2) -> bool {
    let d = sub(b, a);
    let len = norm(d);
    if len < EPS {
        return norm(sub(p, a)) <= EPS;
    }
    let v = sub(p, a);
    cross2(d, v).abs() <= EPS * len && dot(v, d) >= -EPS * len && dot(v, d) <= dot(d, d) + EPS * len
}

/// Boundaries are covered, matching difference against a closed polygon.
/// A single point contact produces no positive-length occlusion interval.
fn covered(p: P2, polygon: &[P2; 4]) -> bool {
    let mut inside = false;
    for i in 0..4 {
        let (a, b) = (polygon[i], polygon[(i + 1) % 4]);
        if on_edge(p, a, b) {
            return true;
        }
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if x > p[0] {
                inside = !inside;
            }
        }
    }
    inside
}

fn has_area(polygon: &[P2; 4]) -> bool {
    (1..3)
        .any(|i| cross2(sub(polygon[i], polygon[0]), sub(polygon[i + 1], polygon[0])).abs() > 1e-16)
}

/// Return all positive-length covered source-parameter spans, including
/// collinear boundaries. Does not reorder the source's endpoint orientation.
pub(crate) fn occlusion_intervals(source: Edge, polygon: &[P2; 4]) -> Vec<(f64, f64)> {
    if !has_area(polygon) {
        return Vec::new();
    }
    let (a, b) = (source[0], source[1]);
    let d = sub(b, a);
    let d2 = dot(d, d);
    if d2 < 1e-24 {
        return Vec::new();
    }
    let mut cuts = vec![0.0, 1.0];
    for i in 0..4 {
        let (p, q) = (polygon[i], polygon[(i + 1) % 4]);
        let e = sub(q, p);
        let delta = sub(p, a);
        let denominator = cross2(d, e);
        if denominator.abs() > 1e-14 * (d2 * dot(e, e)).sqrt() {
            let t = cross2(delta, e) / denominator;
            let u = cross2(delta, d) / denominator;
            if (-1e-12..=1.0 + 1e-12).contains(&t) && (-1e-12..=1.0 + 1e-12).contains(&u) {
                cuts.push(t.clamp(0.0, 1.0));
            }
        } else if cross2(delta, d).abs() <= EPS * d2.sqrt() {
            cuts.push((dot(delta, d) / d2).clamp(0.0, 1.0));
            cuts.push((dot(sub(q, a), d) / d2).clamp(0.0, 1.0));
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() <= 1e-12);
    cuts.windows(2)
        .filter_map(|w| {
            if w[1] - w[0] > 1e-12 && covered(lerp(a, b, (w[0] + w[1]) * 0.5), polygon) {
                Some((w[0], w[1]))
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn subtract_intervals(source: Edge, intervals: &mut [(f64, f64)]) -> Vec<Edge> {
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut result = Vec::new();
    let mut cursor = 0.0f64;
    for &(low, high) in intervals.iter() {
        if low > cursor + 1e-12 {
            result.push([
                lerp(source[0], source[1], cursor),
                lerp(source[0], source[1], low),
            ]);
        }
        cursor = cursor.max(high);
        if cursor >= 1.0 - 1e-12 {
            break;
        }
    }
    if cursor < 1.0 - 1e-12 {
        result.push([lerp(source[0], source[1], cursor), source[1]]);
    }
    result
}

pub(crate) fn visible_edges(
    panels: &mut [Panel],
    segment_count: usize,
    tracks: usize,
    options: &RenderOptions,
    molecular_lines: &[Option<usize>],
) -> Result<Vec<Vec<Edge>>, RenderError> {
    // Stable tie breaking matches Python's stable panel sort without relying
    // on hash iteration or R-tree insertion order.
    panels.sort_by(|a, b| {
        a.depth()
            .total_cmp(&b.depth())
            .then(a.segment.cmp(&b.segment))
            .then(a.step.cmp(&b.step))
    });
    let tree = RTree::bulk_load(
        panels
            .iter()
            .enumerate()
            // Molecular wires are not surfaces; indexing them would waste
            // candidate tests without ever producing an occlusion interval.
            .filter(|(_, p)| has_area(&p.corners()))
            .map(|(index, p)| IndexedPanel {
                index,
                envelope: envelope(&p.corners()),
            })
            .collect(),
    );
    let mut groups = vec![Vec::new(); segment_count];
    let mut tests = 0usize;
    let mut pieces = 0usize;
    for (index, panel) in panels.iter().enumerate() {
        let wire = panel.start.left == panel.start.right && panel.end.left == panel.end.right;
        let lines = molecular_lines.get(panel.segment).copied().flatten();
        let section_profile = |section: &Section| {
            if wire {
                vec![section.left]
            } else if let Some(lines) = lines {
                crate::molecule::profile(section, lines)
            } else {
                profile(section, tracks, options)
            }
        };
        let start = section_profile(&panel.start);
        let end = section_profile(&panel.end);
        let mut previous = None;
        for (a, b) in start.into_iter().zip(end) {
            let source = [a, b];
            if norm(sub(b, a)) < 1e-12 || previous == Some(source) {
                continue;
            }
            previous = Some(source);
            let mut intervals = Vec::new();
            for candidate in tree.locate_in_envelope_intersecting(&envelope(&source)) {
                tests += 1;
                if tests > MAX_CLIP_TESTS {
                    return fail("Occlusion exceeds 50000000 panel tests; reduce samples or select fewer chains");
                }
                if candidate.index <= index {
                    continue;
                }
                intervals.extend(occlusion_intervals(
                    source,
                    &panels[candidate.index].corners(),
                ));
            }
            let visible = subtract_intervals(source, &mut intervals);
            pieces += visible.len();
            if pieces > MAX_PIECES {
                return fail(
                    "Clipping exceeds 2000000 visible pieces; reduce samples or input size",
                );
            }
            groups[panel.segment].extend(visible);
        }
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> [P2; 4] {
        [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }
    fn clip(source: Edge, polygons: &[[P2; 4]]) -> Vec<Edge> {
        let mut intervals = polygons
            .iter()
            .flat_map(|p| occlusion_intervals(source, p))
            .collect::<Vec<_>>();
        subtract_intervals(source, &mut intervals)
    }
    #[test]
    fn overlapping_masks_are_unioned_and_direction_is_preserved() {
        let source = [[4.0, 0.0], [-4.0, 0.0]];
        let pieces = clip(
            source,
            &[square(-2.0, -1.0, 1.0, 1.0), square(0.0, -1.0, 2.0, 1.0)],
        );
        assert_eq!(
            pieces,
            vec![[[4.0, 0.0], [2.0, 0.0]], [[-2.0, 0.0], [-4.0, 0.0]]]
        );
        assert!(pieces.iter().all(|p| p[0][0] > p[1][0]));
    }
    #[test]
    fn boundaries_hide_spans_but_point_contacts_do_not_add_breaks() {
        let p = square(0.0, 0.0, 1.0, 1.0);
        assert_eq!(clip([[-1.0, 0.0], [2.0, 0.0]], &[p]).len(), 2);
        let tangent = [[-1.0, 1.0], [1.0, -1.0]];
        assert_eq!(clip(tangent, &[p]), vec![tangent]);
        assert!(clip([[0.0, 0.0], [1.0, 0.0]], &[p]).is_empty());
        assert_eq!(clip([[-1.0, -1e-5], [2.0, -1e-5]], &[p]).len(), 1);
    }
    #[test]
    fn concave_bow_tie_and_degenerate_panels_are_safe() {
        let bow = [[-1.0, -1.0], [1.0, 1.0], [-1.0, 1.0], [1.0, -1.0]];
        let pieces = clip([[-2.0, 0.5], [2.0, 0.5]], &[bow]);
        assert_eq!(pieces.len(), 2);
        assert!((norm(sub(pieces[0][1], pieces[0][0])) - 1.5).abs() < 1e-12);
        let degenerate = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        assert!(occlusion_intervals([[0.0, 0.0], [3.0, 0.0]], &degenerate).is_empty());
        let concave = [[0.0, 0.0], [2.0, 0.0], [0.5, 0.5], [0.0, 2.0]];
        assert_eq!(clip([[-1.0, 0.25], [3.0, 0.25]], &[concave]).len(), 2);
    }
    #[test]
    fn interval_clipping_matches_shapely_for_96_crossing_concave_and_overlapping_cases() {
        #[derive(serde::Deserialize)]
        struct Case {
            source: Edge,
            polygons: Vec<[P2; 4]>,
            visible: Vec<Edge>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture =
            serde_json::from_str(include_str!("../tests/fixtures/shapely-clip.json")).unwrap();
        for (index, case) in fixture.cases.into_iter().enumerate() {
            let actual = clip(case.source, &case.polygons);
            assert_eq!(actual.len(), case.visible.len(), "case {index}");
            for (a, b) in actual.iter().zip(&case.visible) {
                assert!(
                    norm(sub(a[0], b[0])) < 1e-8,
                    "case {index}, start: {a:?} vs {b:?}"
                );
                assert!(
                    norm(sub(a[1], b[1])) < 1e-8,
                    "case {index}, end: {a:?} vs {b:?}"
                );
            }
        }
    }

    #[test]
    fn molecular_wires_hide_behind_ribbons_but_do_not_occlude_front_ink() {
        let section = |x, y, z| Section {
            left: [x, y],
            right: [x, y],
            z,
            alpha: 0.0,
        };
        let mut panels = vec![
            Panel {
                start: section(-2.0, 0.0, -1.0),
                end: section(2.0, 0.0, -1.0),
                segment: 0,
                step: 0,
            },
            Panel {
                start: section(-2.0, 0.0, 1.0),
                end: section(2.0, 0.0, 1.0),
                segment: 1,
                step: 0,
            },
            Panel {
                start: Section {
                    left: [-1.0, -1.0],
                    right: [-1.0, 1.0],
                    z: 0.0,
                    alpha: 0.0,
                },
                end: Section {
                    left: [1.0, -1.0],
                    right: [1.0, 1.0],
                    z: 0.0,
                    alpha: 0.0,
                },
                segment: 2,
                step: 0,
            },
        ];
        let groups = visible_edges(
            &mut panels,
            3,
            3,
            &RenderOptions {
                lines: 1,
                border_passes: 1,
                ..RenderOptions::default()
            },
            &[],
        )
        .unwrap();
        assert_eq!(
            groups[0],
            vec![[[-2.0, 0.0], [-1.0, 0.0]], [[1.0, 0.0], [2.0, 0.0]]]
        );
        assert_eq!(groups[1], vec![[[-2.0, 0.0], [2.0, 0.0]]]);
        assert_eq!(groups[2].len(), 3);
    }

    #[test]
    fn rtree_clipping_keeps_front_ink_and_never_clips_own_panel() {
        let section = |x, y0, y1, z| Section {
            left: [x, y0],
            right: [x, y1],
            z,
            alpha: 0.0,
        };
        let mut panels = vec![
            Panel {
                start: section(-2.0, -2.0, 2.0, -1.0),
                end: section(2.0, -2.0, 2.0, -1.0),
                segment: 0,
                step: 0,
            },
            Panel {
                start: section(-1.0, -1.0, 1.0, 1.0),
                end: section(1.0, -1.0, 1.0, 1.0),
                segment: 1,
                step: 0,
            },
        ];
        let o = RenderOptions {
            lines: 1,
            border_passes: 1,
            ..RenderOptions::default()
        };
        let groups = visible_edges(&mut panels, 2, 3, &o, &[]).unwrap();
        assert_eq!(groups[0].len(), 4); // two borders + split center
        assert_eq!(groups[1].len(), 3);
        assert!(
            (groups[0].iter().map(|e| norm(sub(e[1], e[0]))).sum::<f64>() - 10.0).abs() < 1e-12
        );
        assert!((groups[1].iter().map(|e| norm(sub(e[1], e[0]))).sum::<f64>() - 6.0).abs() < 1e-12);
    }
}
