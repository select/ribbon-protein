//! Direction-preserving weld/dedup/join, iterative RDP and indexed travel order.
use crate::clip::Edge;
use crate::math::*;
use crate::model::{fail, RenderError};
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use std::collections::{HashMap, HashSet};

pub(crate) type Path = Vec<P2>;
const JOIN_MM: f64 = 0.01;
const SIMPLIFY_MM: f64 = 0.015;
const MIN_LENGTH_MM: f64 = 0.08;

/// Shared across all backbone segments, so fragmentation cannot multiply the
/// allowed worst-case work. Limits also bound pathological branch allocation.
#[derive(Default)]
pub(crate) struct Budget {
    continuation_pairs: usize,
    simplify_tests: usize,
}

struct Graph {
    vertices: Vec<P2>,
    edges: Vec<[usize; 2]>,
    incoming: Vec<Vec<usize>>,
    outgoing: Vec<Vec<usize>>,
}

fn weld_edges(source: &[Edge]) -> Graph {
    let mut vertices: Vec<P2> = Vec::new();
    let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut unique = HashSet::new();
    let mut edges = Vec::with_capacity(source.len());
    let mut weld = |point: P2| {
        let (bx, by) = (
            (point[0] / JOIN_MM).floor() as i64,
            (point[1] / JOIN_MM).floor() as i64,
        );
        let mut best: Option<(f64, usize)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(bucket) = buckets.get(&(bx + dx, by + dy)) {
                    for &id in bucket {
                        let d = dot(sub(point, vertices[id]), sub(point, vertices[id]));
                        if d <= JOIN_MM * JOIN_MM
                            && best.is_none_or(|(distance, old)| {
                                d < distance || (d == distance && id < old)
                            })
                        {
                            best = Some((d, id));
                        }
                    }
                }
            }
        }
        if let Some((_, id)) = best {
            id
        } else {
            let id = vertices.len();
            vertices.push(point);
            buckets.entry((bx, by)).or_default().push(id);
            id
        }
    };
    for edge in source {
        let (a, b) = (weld(edge[0]), weld(edge[1]));
        if a != b && unique.insert((a.min(b), a.max(b))) {
            // Dedup uses an undirected key but keeps FIRST source orientation.
            edges.push([a, b]);
        }
    }
    let mut incoming = vec![Vec::new(); vertices.len()];
    let mut outgoing = vec![Vec::new(); vertices.len()];
    for (i, &[a, b]) in edges.iter().enumerate() {
        outgoing[a].push(i);
        incoming[b].push(i);
    }
    Graph {
        vertices,
        edges,
        incoming,
        outgoing,
    }
}

fn join_directed(graph: &Graph, budget: &mut Budget) -> Result<Vec<Path>, RenderError> {
    let edges = &graph.edges;
    let mut following = vec![None; edges.len()];
    let mut preceding = vec![None; edges.len()];
    for vertex in 0..graph.vertices.len() {
        let ins = &graph.incoming[vertex];
        let outs = &graph.outgoing[vertex];
        let count = ins.len().saturating_mul(outs.len());
        budget.continuation_pairs = budget.continuation_pairs.saturating_add(count);
        if count > 65_536 || budget.continuation_pairs > 20_000_000 {
            return fail("Welded branches exceed 65536 pairs per vertex or 20000000 total continuation pairs; reduce track density");
        }
        let mut pairs = Vec::with_capacity(count);
        for &i in ins {
            let a = graph.vertices[edges[i][0]];
            let b = graph.vertices[edges[i][1]];
            let d = unit(sub(b, a), [1.0, 0.0]);
            for &j in outs {
                let p = graph.vertices[edges[j][0]];
                let q = graph.vertices[edges[j][1]];
                let e = unit(sub(q, p), [1.0, 0.0]);
                pairs.push((-dot(d, e), i, j));
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        for (_, i, j) in pairs {
            if following[i].is_none() && preceding[j].is_none() {
                following[i] = Some(j);
                preceding[j] = Some(i);
            }
        }
    }
    let mut used = vec![false; edges.len()];
    let mut paths = Vec::new();
    let mut walk = |start: usize| {
        let mut path = vec![graph.vertices[edges[start][0]]];
        let mut i = start;
        while !used[i] {
            used[i] = true;
            path.push(graph.vertices[edges[i][1]]);
            if let Some(j) = following[i] {
                i = j;
            } else {
                break;
            }
        }
        paths.push(path);
    };
    // Avoid borrowing 'used' across walks: walk returns immediately on used.
    for (i, pred) in preceding.iter().enumerate() {
        if pred.is_none() {
            walk(i);
        }
    }
    for i in 0..edges.len() {
        walk(i);
    }
    paths.retain(|p| p.len() >= 2);
    Ok(paths)
}

fn join_undirected(graph: &Graph) -> Vec<Path> {
    let mut incident = vec![Vec::new(); graph.vertices.len()];
    for (i, &[a, b]) in graph.edges.iter().enumerate() {
        incident[a].push(i);
        incident[b].push(i);
    }
    let mut used = vec![false; graph.edges.len()];
    let mut paths = Vec::new();
    let mut walk = |start: usize, start_vertex: usize| {
        if used[start] {
            return;
        }
        let mut vertex = start_vertex;
        let mut edge = start;
        let mut path = vec![graph.vertices[vertex]];
        loop {
            if used[edge] {
                break;
            }
            used[edge] = true;
            let [a, b] = graph.edges[edge];
            vertex = if a == vertex { b } else { a };
            path.push(graph.vertices[vertex]);
            if incident[vertex].len() != 2 {
                break;
            }
            let candidates = &incident[vertex];
            edge = if candidates[0] == edge {
                candidates[1]
            } else {
                candidates[0]
            };
        }
        paths.push(path);
    };
    for (v, neighbors) in incident.iter().enumerate() {
        if neighbors.len() != 2 {
            for &edge in neighbors {
                walk(edge, v);
            }
        }
    }
    for (i, &[v, _]) in graph.edges.iter().enumerate() {
        walk(i, v);
    }
    paths
}

fn distance_squared(p: P2, a: P2, b: P2) -> f64 {
    let d = sub(b, a);
    let length_squared = dot(d, d);
    let t = if length_squared > 1e-24 {
        (dot(sub(p, a), d) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let v = sub(p, lerp(a, b, t));
    dot(v, v)
}

/// Iterative Douglas-Peucker avoids call-stack overflow on huge contours.
/// Endpoints (including exact occlusion cut points) never move.
pub(crate) fn simplify(
    path: &[P2],
    tolerance: f64,
    budget: &mut Budget,
) -> Result<Path, RenderError> {
    if path.len() <= 2 {
        return Ok(path.to_vec());
    }
    let mut keep = vec![false; path.len()];
    keep[0] = true;
    keep[path.len() - 1] = true;
    let mut pending = vec![(0, path.len() - 1)];
    while let Some((a, b)) = pending.pop() {
        let mut best = tolerance * tolerance;
        let mut selected = None;
        budget.simplify_tests = budget.simplify_tests.saturating_add(b - a - 1);
        if budget.simplify_tests > 50_000_000 {
            return fail(
                "Simplification exceeds 50000000 point tests; reduce samples or input size",
            );
        }
        for i in a + 1..b {
            let distance = distance_squared(path[i], path[a], path[b]);
            if distance > best {
                best = distance;
                selected = Some(i);
            }
        }
        if let Some(i) = selected {
            keep[i] = true;
            pending.push((a, i));
            pending.push((i, b));
        }
    }
    Ok(path
        .iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(*p))
        .collect())
}

pub(crate) fn length(path: &[P2]) -> f64 {
    path.windows(2).map(|p| norm(sub(p[1], p[0]))).sum()
}

pub(crate) fn merge(
    source: &[Edge],
    directed: bool,
    budget: &mut Budget,
) -> Result<Vec<Path>, RenderError> {
    let graph = weld_edges(source);
    let joined = if directed {
        join_directed(&graph, budget)?
    } else {
        join_undirected(&graph)
    };
    let mut result = Vec::with_capacity(joined.len());
    for path in joined {
        let path = simplify(&path, SIMPLIFY_MM, budget)?;
        if length(&path) >= MIN_LENGTH_MM {
            result.push(path);
        }
    }
    Ok(result)
}

#[derive(Clone, PartialEq)]
struct Endpoint {
    point: P2,
    path: usize,
    reverse: bool,
}
impl RTreeObject for Endpoint {
    type Envelope = AABB<P2>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}
impl PointDistance for Endpoint {
    fn distance_2(&self, point: &P2) -> f64 {
        dot(sub(self.point, *point), sub(self.point, *point))
    }
}

/// Greedy endpoint ordering using an R-tree instead of quadratic scans.
/// Backbone mode never reverses paths. Travel mode may reverse but never
/// inserts drawing moves between disjoint strokes.
pub(crate) fn order(paths: Vec<Path>, allow_reverse: bool) -> Vec<Path> {
    let mut paths: Vec<Option<Path>> = paths
        .into_iter()
        .filter(|p| p.len() >= 2)
        .map(Some)
        .collect();
    let endpoints = paths
        .iter()
        .enumerate()
        .flat_map(|(id, path)| {
            let Some(path) = path.as_ref() else {
                return Vec::new();
            };
            let mut result = vec![Endpoint {
                point: path[0],
                path: id,
                reverse: false,
            }];
            if allow_reverse {
                result.push(Endpoint {
                    point: path[path.len() - 1],
                    path: id,
                    reverse: true,
                });
            }
            result
        })
        .collect();
    let mut tree = RTree::bulk_load(endpoints);
    let mut ordered = Vec::with_capacity(paths.len());
    let mut cursor = [0.0; 2];
    while let Some(nearest) = tree.nearest_neighbor(&cursor).cloned() {
        // Every endpoint is removed together with its path; no stale entries.
        if let Some(mut path) = paths[nearest.path].take() {
            let start = Endpoint {
                point: path[0],
                path: nearest.path,
                reverse: false,
            };
            let end = Endpoint {
                point: path[path.len() - 1],
                path: nearest.path,
                reverse: true,
            };
            tree.remove(&start);
            if allow_reverse {
                tree.remove(&end);
            }
            if nearest.reverse {
                path.reverse();
            }
            cursor = path[path.len() - 1];
            ordered.push(path);
        } else {
            tree.remove(&nearest);
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge(source: &[Edge], directed: bool) -> Result<Vec<Path>, RenderError> {
        super::merge(source, directed, &mut Budget::default())
    }
    #[test]
    fn joins_small_gaps_and_keeps_first_duplicate_direction() {
        let source = [
            [[0.0, 0.0], [1.0, 0.0]],
            [[1.002, 0.0], [2.0, 0.0]],
            [[1.0, 0.0], [0.0, 0.0]],
        ];
        let result = merge(&source, true).unwrap();
        assert_eq!(result, vec![vec![[0.0, 0.0], [2.0, 0.0]]]);
    }
    #[test]
    fn never_bridges_large_gaps() {
        let source = [[[0.0, 0.0], [1.0, 0.0]], [[1.1, 0.0], [2.0, 0.0]]];
        for directed in [true, false] {
            let result = merge(&source, directed).unwrap();
            assert_eq!(result.len(), 2);
            assert!((result.iter().map(|p| length(p)).sum::<f64>() - 1.9).abs() < 1e-12);
        }
    }
    #[test]
    fn directed_branch_uses_smooth_continuation_without_retracing() {
        let source = [
            [[0.0, 0.0], [1.0, 0.0]],
            [[1.0, -1.0], [1.0, 0.0]],
            [[1.0, 0.0], [2.0, 0.0]],
            [[1.0, 0.0], [1.0, 1.0]],
        ];
        let result = merge(&source, true).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&vec![[0.0, 0.0], [2.0, 0.0]]));
        assert!(result.contains(&vec![[1.0, -1.0], [1.0, 1.0]]));
        assert_eq!(result.iter().map(|p| length(p)).sum::<f64>(), 4.0);
        assert_eq!(merge(&source, false).unwrap().len(), 4);
    }
    #[test]
    fn directed_cycles_traverse_every_edge_once() {
        let source = [
            [[0.0, 0.0], [1.0, 0.0]],
            [[1.0, 0.0], [1.0, 1.0]],
            [[1.0, 1.0], [0.0, 0.0]],
        ];
        let result = merge(&source, true).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].first(), result[0].last());
        assert!((length(&result[0]) - 2.0 - 2.0f64.sqrt()).abs() < 1e-12);
    }
    #[test]
    fn ordering_respects_orientation_and_lengths() {
        let source = vec![vec![[3.0, 0.0], [2.0, 0.0]], vec![[1.0, 0.0], [0.0, 0.0]]];
        let directed = order(source.clone(), false);
        assert_eq!(directed, vec![source[1].clone(), source[0].clone()]);
        let travel = order(source, true);
        assert_eq!(
            travel,
            vec![vec![[0.0, 0.0], [1.0, 0.0]], vec![[2.0, 0.0], [3.0, 0.0]]]
        );
        assert_eq!(travel.iter().map(|p| length(p)).sum::<f64>(), 2.0);
    }
    #[test]
    fn pathological_simplification_and_join_work_returns_errors() {
        let mut budget = Budget {
            simplify_tests: 50_000_000,
            continuation_pairs: 0,
        };
        assert!(simplify(&[[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]], 0.015, &mut budget).is_err());
        let graph = weld_edges(&[[[0.0, 0.0], [1.0, 0.0]], [[1.0, 0.0], [2.0, 0.0]]]);
        budget.continuation_pairs = 20_000_000;
        assert!(join_directed(&graph, &mut budget).is_err());
    }

    #[test]
    fn simplification_preserves_occlusion_endpoints() {
        let path = [[0.1234, 0.0], [0.5, 0.001], [1.2345, 0.0]];
        assert_eq!(
            simplify(&path, 0.015, &mut Budget::default()).unwrap(),
            vec![path[0], path[2]]
        );
    }
}
