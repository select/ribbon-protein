//! A local, pure-Rust protein ribbon renderer, shared by native tests and WASM.
//!
//! The parser owns secondary-structure assignment and backbone segmentation.
//! This engine consumes normalized coordinates only; no files or network.
//! Optional session stage timings use a monotonic clock, never geometry inputs.
mod clip;
mod math;
mod model;
mod molecule;
mod paths;
mod ribbon;
mod slicer;

pub use model::*;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Browser boundary. The worker owns timing and cancellation (terminate the
/// worker to cancel synchronous WASM). Native callers use render_json_native.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn render_json(request: &str) -> Result<String, JsValue> {
    render_json_native(request).map_err(|error| JsValue::from_str(&error.to_string()))
}

/// Same JSON conversion as the WASM entry point, without JS-only error values.
pub fn render_json_native(request: &str) -> Result<String, RenderError> {
    if request.len() > model::MAX_JSON_BYTES {
        return model::fail("Render request exceeds 32 MiB");
    }
    let request: RenderRequest = serde_json::from_str(request)
        .map_err(|e| RenderError(format!("Invalid render request JSON: {e}")))?;
    let result = render(&request)?;
    serde_json::to_string(&result)
        .map_err(|e| RenderError(format!("Cannot encode render result: {e}")))
}

/// Native stateless oracle. The session uses the very same f64 pipeline, but
/// retains prepared geometry and the last view's per-segment joined paths.
pub fn render(request: &RenderRequest) -> Result<RenderResult, RenderError> {
    request.validate()?;
    let has_slice = request.options.has_selected_mode(RenderMode::Slice);
    let has_ribbon = request.options.has_selected_mode(RenderMode::Ribbon);
    if has_slice && !has_ribbon {
        let prepared = slicer::prepare_slices(request, false, None)?;
        return slicer::render_from_prepared(request, &prepared, false);
    }
    if !has_slice
        && request.options.selected_chains.is_empty()
        && request.options.render_mode == RenderMode::Slice
    {
        let prepared = slicer::prepare_slices(request, false, None)?;
        return slicer::render_from_prepared(request, &prepared, false);
    }
    let prepared = prepare(request);
    if has_slice {
        let slice_prepared = slicer::prepare_slices(request, false, Some(&prepared.camera))?;
        return render_hybrid(request, &prepared, &slice_prepared, false).map(|(r, _)| r);
    }
    let (joined, _) = draw(request, &prepared, false)?;
    finish(request, &joined)
}

#[derive(Clone, Debug, PartialEq)]
struct PreparedKey {
    samples: usize,
    ribbon_indices: Vec<usize>,
    slice_chains: Vec<String>,
}

struct Prepared {
    key: PreparedKey,
    indices: Vec<usize>,
    ribbons: Vec<Vec<ribbon::Sample>>,
    camera: math::Camera,
    residue_count: usize,
    warnings: Vec<String>,
}
struct Joined {
    paths: Vec<Vec<paths::Path>>,
    indices: Vec<usize>,
    residue_count: usize,
    warnings: Vec<String>,
}

fn is_lipid(structure: &StructureData, index: usize) -> bool {
    index
        .checked_sub(structure.segments.len())
        .is_some_and(|i| structure.molecules[i].category == "lipid")
}
fn component_id(structure: &StructureData, index: usize) -> &str {
    if index < structure.segments.len() {
        &structure.segments[index].chain_id
    } else {
        &structure.molecules[index - structure.segments.len()].chain_id
    }
}
fn selected_ribbon_indices(request: &RenderRequest) -> Vec<usize> {
    let selected: HashSet<&str> = request
        .options
        .selected_chains
        .iter()
        .filter(|id| request.options.mode_of(id) == RenderMode::Ribbon)
        .map(String::as_str)
        .collect();
    (0..request.structure.segments.len() + request.structure.molecules.len())
        .filter(|&i| selected.contains(component_id(&request.structure, i)))
        .collect()
}
fn selected_slice_chains(request: &RenderRequest) -> Vec<String> {
    let wanted: HashSet<&str> = request
        .options
        .selected_chains
        .iter()
        .filter(|id| request.options.mode_of(id) == RenderMode::Slice)
        .map(String::as_str)
        .collect();
    request
        .structure
        .chains
        .iter()
        .filter(|c| wanted.contains(c.id.as_str()))
        .map(|c| c.id.clone())
        .collect()
}
fn prepared_key(request: &RenderRequest) -> PreparedKey {
    PreparedKey {
        samples: request.options.samples,
        ribbon_indices: selected_ribbon_indices(request),
        slice_chains: selected_slice_chains(request),
    }
}
fn prepare(request: &RenderRequest) -> Prepared {
    let key = prepared_key(request);
    let indices = key.ribbon_indices.clone();
    let mut warnings = request.structure.warnings.clone();
    let mut fallback_used = false;
    let mut residue_count = 0;
    let ribbons: Vec<_> = indices
        .iter()
        .filter_map(|&i| {
            let segment = request.structure.segments.get(i)?;
            residue_count += segment.residues.len();
            let (samples, fallback) = ribbon::make_ribbon(segment, request.options.samples);
            fallback_used |= fallback;
            Some(samples)
        })
        .collect();
    if fallback_used {
        warnings.push("Some carbonyl directions are parallel to the backbone; stable transported fallback frames were used.".into());
    }
    let mut all_points: Vec<_> = ribbons.iter().flatten().map(|s| s.point).collect();
    for &i in &indices {
        if i >= request.structure.segments.len() {
            all_points
                .extend(&request.structure.molecules[i - request.structure.segments.len()].atoms);
        }
    }
    if !key.slice_chains.is_empty() {
        let (by_color, _) = slicer::collect_atoms_by_color(&request.structure, &request.options);
        for atoms in by_color.values() {
            all_points.extend(atoms.iter().map(|&(p, _)| p));
        }
    }
    let camera = if all_points.is_empty() {
        math::Camera {
            center: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    } else {
        math::Camera::new(&all_points, [0.0; 3])
    };
    Prepared {
        key,
        indices,
        ribbons,
        camera,
        residue_count,
        warnings,
    }
}

#[derive(Default, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageTimings {
    pub prepare_ms: f64,
    pub project_ms: f64,
    pub clip_ms: f64,
    pub paths_ms: f64,
    pub finish_ms: f64,
    pub geometry_cache_hit: bool,
    pub path_cache_hit: bool,
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function ribbon_now() { return performance.now(); }")]
extern "C" {
    fn ribbon_now() -> f64;
}
fn now() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        ribbon_now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
    }
}

type PageFit = (math::P2, f64, math::P2);

fn draw_with_bounds(
    request: &RenderRequest,
    prepared: &Prepared,
    preview: bool,
    extra_bounds: Option<([math::P2; 2], f64)>,
) -> Result<(Joined, StageTimings, Option<PageFit>), RenderError> {
    let o = &request.options;
    let mut timings = StageTimings::default();
    let mut warnings = prepared.warnings.clone();
    let empty = || Joined {
        paths: vec![],
        indices: vec![],
        residue_count: 0,
        warnings: vec![],
    };
    if o.selected_chains.is_empty() {
        warnings.push("No chains selected; the drawing is empty.".into());
        return Ok((
            Joined {
                warnings,
                ..empty()
            },
            timings,
            None,
        ));
    }
    if prepared.indices.is_empty() {
        return model::fail(
            "Selected chains have no continuous backbone segments with at least four residues",
        );
    }
    let started = now();
    let camera = prepared.camera.rotated(o.rotation);
    let stride = if preview {
        prepared
            .ribbons
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            .div_ceil(800)
            .max(1)
    } else {
        1
    };
    let mut sections: Vec<_> = prepared
        .ribbons
        .iter()
        .map(|samples| {
            if stride == 1 {
                ribbon::project_ribbon(samples, &camera)
            } else {
                let mut sparse: Vec<_> = samples.iter().step_by(stride).cloned().collect();
                if (samples.len() - 1) % stride != 0 {
                    sparse.push(samples.last().unwrap().clone());
                }
                ribbon::project_ribbon(&sparse, &camera)
            }
        })
        .collect();
    let mut section_indices: Vec<_> = prepared
        .indices
        .iter()
        .copied()
        .filter(|&i| i < request.structure.segments.len())
        .collect();
    let mut molecular_lines = vec![None; sections.len()];
    let mut molecular_budget = molecule::Budget::default();
    for &index in &prepared.indices {
        if index < request.structure.segments.len() {
            continue;
        }
        let m = &request.structure.molecules[index - request.structure.segments.len()];
        let style = o.molecule_style(&m.category);
        let paths = molecule::project(
            m,
            &camera,
            style.bond_width_angstrom,
            if preview { 0 } else { style.lines },
            &mut molecular_budget,
        )?;
        for path in paths {
            molecular_lines.push(Some(path.profile));
            section_indices.push(index);
            sections.push(path.sections);
        }
    }
    let stroke_pad = extra_bounds
        .map(|(_, pad)| o.max_pen_width(&request.structure).max(pad))
        .unwrap_or_else(|| o.max_pen_width(&request.structure));
    let fit_transform = fit_page(&mut sections, o, stroke_pad, extra_bounds.map(|(b, _)| b))?;
    timings.project_ms = now() - started;
    if preview {
        warnings.push("Interaction preview: ribbon outlines and molecular bonds, without hidden-line removal. Refine before exporting.".into());
        let paths = sections
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if matches!(molecular_lines[i], Some(molecule::Profile::Surface)) {
                    return vec![];
                }
                let left = s.iter().map(|v| v.left).collect();
                if s.iter().all(|v| v.left == v.right) {
                    vec![left]
                } else {
                    vec![left, s.iter().map(|v| v.right).collect()]
                }
            })
            .collect();
        return Ok((
            Joined {
                paths,
                indices: section_indices.clone(),
                residue_count: prepared.residue_count,
                warnings,
            },
            timings,
            Some(fit_transform),
        ));
    }
    let tracks = ribbon::track_count(&sections, o, &molecular_lines)?;
    if sections
        .iter()
        .zip(&section_indices)
        .zip(&molecular_lines)
        .any(|((path, &index), lines)| {
            lines.is_some_and(|profile| {
                let molecule::Profile::Band(lines) = profile else {
                    return false;
                };
                lines > 0
                    && path.iter().any(|s| {
                        s.width() > 1e-12
                            && s.width() / ((lines + 1) as f64)
                                < if is_lipid(&request.structure, index) {
                                    o.lipid_width_mm
                                } else {
                                    o.pen_width_mm
                                }
                    })
            })
        })
    {
        warnings.push("Molecular tracks are closer than the pen stroke width and may merge in ink; increase group bond width, reduce interior lines or use a finer stroke.".into());
    }
    if o.fill_loops && o.pen_width_mm > o.loop_spacing_mm {
        warnings
            .push("Pen width exceeds loop spacing; dense loop tracks may overlap in ink.".into());
    }
    if o.border_passes > 1 && o.pen_width_mm > o.border_spacing_mm {
        warnings.push(
            "Pen width exceeds border spacing; inset border passes may overlap in ink.".into(),
        );
    }
    warnings.push("Hidden-line removal uses average panel depth; intersecting or strongly twisted surfaces may have local visibility approximations.".into());
    let started = now();
    let mut panels = Vec::with_capacity(sections.iter().map(|s| s.len() - 1).sum());
    for (segment, sections) in sections.into_iter().enumerate() {
        for (step, pair) in sections.windows(2).enumerate() {
            panels.push(clip::Panel {
                start: pair[0].clone(),
                end: pair[1].clone(),
                segment,
                step,
            });
        }
    }
    let visible = clip::visible_edges(
        &mut panels,
        section_indices.len(),
        tracks,
        o,
        &molecular_lines,
    )?;
    timings.clip_ms = now() - started;
    let started = now();
    let mut budget = paths::Budget::default();
    let directed = o.stroke_direction == StrokeDirection::Backbone;
    let mut joined_indices = Vec::new();
    let mut groups: Vec<Vec<clip::Edge>> = Vec::new();
    for (index, edges) in section_indices.into_iter().zip(visible) {
        if joined_indices.last() == Some(&index) {
            groups.last_mut().unwrap().extend(edges);
        } else {
            joined_indices.push(index);
            groups.push(edges);
        }
    }
    let paths = groups
        .into_iter()
        .map(|edges| paths::merge(&edges, directed, &mut budget))
        .collect::<Result<_, _>>()?;
    timings.paths_ms = now() - started;
    Ok((
        Joined {
            paths,
            indices: joined_indices,
            residue_count: prepared.residue_count,
            warnings,
        },
        timings,
        Some(fit_transform),
    ))
}

fn draw(
    request: &RenderRequest,
    prepared: &Prepared,
    preview: bool,
) -> Result<(Joined, StageTimings), RenderError> {
    draw_with_bounds(request, prepared, preview, None).map(|(j, t, _)| (j, t))
}

fn finish(request: &RenderRequest, joined: &Joined) -> Result<RenderResult, RenderError> {
    let o = &request.options;
    let mut colors: BTreeMap<(String, bool), Vec<paths::Path>> = BTreeMap::new();
    for (&index, paths) in joined.indices.iter().zip(&joined.paths) {
        let color = o
            .chain_colors
            .get(component_id(&request.structure, index))
            .map(String::as_str)
            .unwrap_or("black");
        colors
            .entry((color.to_owned(), is_lipid(&request.structure, index)))
            .or_default()
            .extend(paths.iter().cloned());
    }
    for paths in colors.values_mut() {
        *paths = paths::order(
            std::mem::take(paths),
            o.stroke_direction != StrokeDirection::Backbone,
        );
    }
    let mut warnings = joined.warnings.clone();
    if !joined.indices.is_empty() && colors.values().all(Vec::is_empty) {
        warnings.push(
            "No strokes survived visibility clipping and the 0.08 mm minimum stroke length.".into(),
        );
    }
    make_result(request, colors, joined.residue_count, warnings)
}

fn render_hybrid(
    request: &RenderRequest,
    prepared: &Prepared,
    slice_prepared: &slicer::PreparedSlices,
    preview: bool,
) -> Result<(RenderResult, StageTimings), RenderError> {
    let o = &request.options;
    let rotated = prepared.camera.rotated(o.rotation);
    let (projected_slices, slice_bounds, z_range) =
        slicer::project_slice_groups(slice_prepared, &rotated, false);
    let max_outward = 1.45 * (o.slice_max_parallel / 2) as f64 * o.slice_parallel_spacing_mm;
    let slice_pad = o.pen_width_mm + 2.0 * max_outward;

    let (joined, mut timings, fit_opt) =
        draw_with_bounds(request, prepared, preview, Some((slice_bounds, slice_pad)))?;
    let finish_started = now();
    let (mid, scale, page_mid) =
        fit_opt.unwrap_or(([0.0, 0.0], 1.0, [o.width_mm * 0.5, o.height_mm * 0.5]));
    let mut warnings = joined.warnings.clone();
    for w in &slice_prepared.warnings {
        if !warnings.contains(w) {
            warnings.push(w.clone());
        }
    }
    let slice_groups = slicer::build_weighted_slice_groups(
        projected_slices,
        z_range,
        mid,
        scale,
        page_mid,
        o,
        preview,
        &mut warnings,
    );

    let mut colors: BTreeMap<(String, bool), Vec<paths::Path>> = BTreeMap::new();
    for (&index, paths) in joined.indices.iter().zip(&joined.paths) {
        let color = o
            .chain_colors
            .get(component_id(&request.structure, index))
            .map(String::as_str)
            .unwrap_or("black");
        colors
            .entry((color.to_owned(), is_lipid(&request.structure, index)))
            .or_default()
            .extend(paths.iter().cloned());
    }
    if !preview {
        for paths in colors.values_mut() {
            *paths = paths::order(
                std::mem::take(paths),
                o.stroke_direction != StrokeDirection::Backbone,
            );
        }
    }

    let (slice_strokes, slice_points, slice_len) = slicer::slice_groups_stats(&slice_groups);
    let ribbon_strokes: usize = colors.values().map(Vec::len).sum();
    let ribbon_points: usize = colors.values().flatten().map(Vec::len).sum();
    let ribbon_len: f64 = colors.values().flatten().map(|p| paths::length(p)).sum();

    if slice_points.saturating_add(ribbon_points) > model::MAX_PIECES {
        return model::fail("SVG exceeds 2000000 output points; reduce drawing complexity");
    }

    let opacity = slicer::format_opacity(o.slice_ink_opacity);
    let desc = if preview {
        "Approximate hybrid interaction preview. Hidden strokes and slice depth tracks are omitted until release."
    } else {
        "Hybrid ribbon and sliced-surface pen plot. Slice groups use stroke-opacity to simulate overdraw buildup; ribbon strokes are opaque centerlines."
    };
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">\n<title>{} / hybrid plot</title>\n<desc>{}</desc>\n<g fill=\"none\" stroke=\"black\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
        o.width_mm,
        o.height_mm,
        o.width_mm,
        o.height_mm,
        xml(&request.structure.name),
        desc,
        o.pen_width_mm,
    );
    slicer::append_slice_groups_svg(&mut svg, &slice_groups, &opacity);
    for (i, ((color, lipid), paths)) in colors.iter().enumerate() {
        if *lipid {
            let _ = writeln!(
                svg,
                "<g id=\"pen-{}\" stroke=\"{}\" stroke-width=\"{}\">",
                i + 1,
                color,
                o.lipid_width_mm
            );
        } else {
            let _ = writeln!(svg, "<g id=\"pen-{}\" stroke=\"{}\">", i + 1, color);
        }
        for path in paths {
            if let Some(first) = path.first() {
                let _ = write!(svg, "<path d=\"M{:.4},{:.4}", first[0], first[1]);
                for point in &path[1..] {
                    let _ = write!(svg, "L{:.4},{:.4}", point[0], point[1]);
                }
                svg.push_str("\"/>\n");
            }
        }
        svg.push_str("</g>\n");
    }
    svg.push_str("</g>\n</svg>\n");
    if svg.len() > 64 * 1024 * 1024 {
        return model::fail("SVG exceeds 64 MiB");
    }
    timings.finish_ms = now() - finish_started;
    Ok((
        RenderResult {
            svg,
            width_mm: o.width_mm,
            height_mm: o.height_mm,
            stroke_count: slice_strokes + ribbon_strokes,
            length_mm: slice_len + ribbon_len,
            residue_count: prepared.residue_count + slice_prepared.residue_count,
            elapsed_ms: 0.0,
            warnings,
        },
        timings,
    ))
}

/// Owns exactly one structure. Replacement drops every structure-dependent cache.
/// At most two prepared sample/selection variants and one joined view are kept.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct RenderSession {
    request: RenderRequest,
    prepared: std::collections::VecDeque<Prepared>,
    slice_prepared: std::collections::VecDeque<slicer::PreparedSlices>,
    joined: Option<(RenderOptions, Joined)>,
}
impl RenderSession {
    pub fn new(structure: StructureData) -> Result<Self, RenderError> {
        let request = RenderRequest {
            structure,
            options: RenderOptions::default(),
        };
        request.validate()?;
        Ok(Self {
            request,
            prepared: Default::default(),
            slice_prepared: Default::default(),
            joined: None,
        })
    }
    fn ensure_prepared(&mut self) -> bool {
        let pkey = prepared_key(&self.request);
        if let Some(index) = self.prepared.iter().position(|p| p.key == pkey) {
            let entry = self.prepared.remove(index).unwrap();
            self.prepared.push_front(entry);
            true
        } else {
            if self.prepared.len() == 2 {
                self.prepared.pop_back();
            }
            self.prepared.push_front(prepare(&self.request));
            false
        }
    }
    pub fn calculate(
        &mut self,
        options: RenderOptions,
        preview: bool,
    ) -> Result<(RenderResult, StageTimings), RenderError> {
        self.request.options = options;
        self.request.validate()?;
        let has_slice = self.request.options.has_selected_mode(RenderMode::Slice);
        let has_ribbon = self.request.options.has_selected_mode(RenderMode::Ribbon);
        let pure_slice = (has_slice && !has_ribbon)
            || (!has_slice
                && self.request.options.selected_chains.is_empty()
                && self.request.options.render_mode == RenderMode::Slice);

        if pure_slice {
            let slice_key =
                slicer::SliceKey::new(&self.request.structure, &self.request.options, None);
            let started = now();
            let index = self.slice_prepared.iter().position(|p| p.key == slice_key);
            if preview && index.is_none() {
                let temp = slicer::prepare_slices(&self.request, true, None)?;
                let prepare_ms = now() - started;
                let finish_started = now();
                let result = slicer::render_from_prepared(&self.request, &temp, true)?;
                return Ok((
                    result,
                    StageTimings {
                        prepare_ms,
                        finish_ms: now() - finish_started,
                        ..Default::default()
                    },
                ));
            }
            let geometry_hit = index.is_some();
            if let Some(index) = index {
                let entry = self.slice_prepared.remove(index).unwrap();
                self.slice_prepared.push_front(entry);
            } else {
                if self.slice_prepared.len() == 2 {
                    self.slice_prepared.pop_back();
                }
                self.slice_prepared
                    .push_front(slicer::prepare_slices(&self.request, false, None)?);
            }
            let prepare_ms = now() - started;
            let finish_started = now();
            let result =
                slicer::render_from_prepared(&self.request, &self.slice_prepared[0], preview)?;
            return Ok((
                result,
                StageTimings {
                    prepare_ms,
                    finish_ms: now() - finish_started,
                    geometry_cache_hit: geometry_hit,
                    ..Default::default()
                },
            ));
        }

        if has_slice && has_ribbon {
            let started = now();
            let rib_hit = self.ensure_prepared();
            let cam = self.prepared[0].camera.clone();
            let slice_key =
                slicer::SliceKey::new(&self.request.structure, &self.request.options, Some(&cam));
            let slc_idx = self.slice_prepared.iter().position(|p| p.key == slice_key);
            if preview && slc_idx.is_none() {
                let temp = slicer::prepare_slices(&self.request, true, Some(&cam))?;
                let prepare_ms = now() - started;
                let (result, mut timings) =
                    render_hybrid(&self.request, &self.prepared[0], &temp, true)?;
                timings.prepare_ms = prepare_ms;
                return Ok((result, timings));
            }
            let slc_hit = slc_idx.is_some();
            if let Some(index) = slc_idx {
                let entry = self.slice_prepared.remove(index).unwrap();
                self.slice_prepared.push_front(entry);
            } else {
                if self.slice_prepared.len() == 2 {
                    self.slice_prepared.pop_back();
                }
                self.slice_prepared.push_front(slicer::prepare_slices(
                    &self.request,
                    false,
                    Some(&cam),
                )?);
            }
            let prepare_ms = now() - started;
            let (result, mut timings) = render_hybrid(
                &self.request,
                &self.prepared[0],
                &self.slice_prepared[0],
                preview,
            )?;
            timings.prepare_ms = prepare_ms;
            timings.geometry_cache_hit = rib_hit && slc_hit;
            return Ok((result, timings));
        }

        let mut key = self.request.options.clone();
        key.chain_colors.clear();
        let hit = !preview && self.joined.as_ref().is_some_and(|(k, _)| *k == key);
        let mut timings = StageTimings {
            path_cache_hit: hit,
            ..Default::default()
        };
        if !hit {
            let started = now();
            let geometry_hit = self.ensure_prepared();
            let prepare_ms = now() - started;
            let (joined, stages) = draw(&self.request, &self.prepared[0], preview)?;
            timings = StageTimings {
                prepare_ms,
                geometry_cache_hit: geometry_hit,
                ..stages
            };
            if preview {
                let started = now();
                let mut colors = BTreeMap::new();
                for (&index, paths) in joined.indices.iter().zip(&joined.paths) {
                    let color = self
                        .request
                        .options
                        .chain_colors
                        .get(component_id(&self.request.structure, index))
                        .cloned()
                        .unwrap_or_else(|| "black".into());
                    colors
                        .entry((color, is_lipid(&self.request.structure, index)))
                        .or_insert_with(Vec::new)
                        .extend(paths.iter().cloned());
                }
                let mut result =
                    make_result(&self.request, colors, joined.residue_count, joined.warnings)?;
                result.svg = result.svg.replace(
                    "Visible vector centerlines only. Hidden strokes geometrically removed.",
                    "Approximate interaction preview. Hidden strokes are NOT removed.",
                );
                timings.finish_ms = now() - started;
                return Ok((result, timings));
            }
            self.joined = Some((key, joined));
        }
        let started = now();
        let result = finish(&self.request, &self.joined.as_ref().unwrap().1)?;
        timings.finish_ms = now() - started;
        Ok((result, timings))
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl RenderSession {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn from_json(structure: &str) -> Result<RenderSession, String> {
        if structure.len() > model::MAX_JSON_BYTES {
            return Err("Structure exceeds 32 MiB".into());
        }
        let structure =
            serde_json::from_str(structure).map_err(|e| format!("Invalid structure JSON: {e}"))?;
        Self::new(structure).map_err(|e| e.to_string())
    }
    /// Small actual 3D ribbon/slice scene for responsive main-thread interaction while
    /// synchronous full-quality WASM is busy. Full-sample PCA is retained.
    pub fn preview_scene_json(&mut self) -> Result<String, String> {
        self.request.validate().map_err(|e| e.to_string())?;
        let has_slice = self.request.options.has_selected_mode(RenderMode::Slice);
        let has_ribbon = self.request.options.has_selected_mode(RenderMode::Ribbon);
        let pure_slice = (has_slice && !has_ribbon)
            || (!has_slice
                && self.request.options.selected_chains.is_empty()
                && self.request.options.render_mode == RenderMode::Slice);

        if pure_slice {
            let slice_key =
                slicer::SliceKey::new(&self.request.structure, &self.request.options, None);
            let index =
                if let Some(index) = self.slice_prepared.iter().position(|p| p.key == slice_key) {
                    index
                } else {
                    if self.slice_prepared.len() == 2 {
                        self.slice_prepared.pop_back();
                    }
                    let prepared = slicer::prepare_slices(&self.request, false, None)
                        .map_err(|e| e.to_string())?;
                    self.slice_prepared.push_front(prepared);
                    0
                };
            let prepared = &self.slice_prepared[index];
            let slice_groups = slicer::sample_preview_groups(&prepared.groups, 4800);
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct SliceScene {
                center: math::V3,
                basis: [math::V3; 3],
                ribbons: [(); 0],
                molecules: [(); 0],
                slice_groups: Vec<slicer::PreparedSliceGroup>,
                slice_radius: f64,
            }
            return serde_json::to_string(&SliceScene {
                center: prepared.camera.center,
                basis: prepared.camera.basis,
                ribbons: [],
                molecules: [],
                slice_groups,
                slice_radius: prepared.radius,
            })
            .map_err(|e| e.to_string());
        }

        self.ensure_prepared();
        let mut slice_groups = Vec::new();
        let mut slice_radius = None;
        if has_slice {
            let cam = self.prepared[0].camera.clone();
            let slice_key =
                slicer::SliceKey::new(&self.request.structure, &self.request.options, Some(&cam));
            let index =
                if let Some(index) = self.slice_prepared.iter().position(|p| p.key == slice_key) {
                    index
                } else {
                    if self.slice_prepared.len() == 2 {
                        self.slice_prepared.pop_back();
                    }
                    let prepared = slicer::prepare_slices(&self.request, false, Some(&cam))
                        .map_err(|e| e.to_string())?;
                    self.slice_prepared.push_front(prepared);
                    0
                };
            slice_groups = slicer::sample_preview_groups(&self.slice_prepared[index].groups, 3600);
            slice_radius = Some(self.slice_prepared[index].radius);
        }

        let prepared = &self.prepared[0];
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SceneRibbon<'a> {
            chain_id: &'a str,
            samples: Vec<&'a ribbon::Sample>,
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Scene<'a> {
            center: math::V3,
            basis: [math::V3; 3],
            ribbons: Vec<SceneRibbon<'a>>,
            molecules: Vec<molecule::PreviewMolecule>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            slice_groups: Vec<slicer::PreparedSliceGroup>,
            #[serde(skip_serializing_if = "Option::is_none")]
            slice_radius: Option<f64>,
        }
        let stride = prepared
            .ribbons
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            .div_ceil(800)
            .max(1);
        let ribbons = prepared
            .indices
            .iter()
            .filter(|&&index| index < self.request.structure.segments.len())
            .zip(&prepared.ribbons)
            .map(|(&index, samples)| {
                let mut sparse: Vec<_> = samples.iter().step_by(stride).collect();
                if (samples.len() - 1) % stride != 0 {
                    sparse.push(samples.last().unwrap());
                }
                SceneRibbon {
                    chain_id: &self.request.structure.segments[index].chain_id,
                    samples: sparse,
                }
            })
            .collect();
        serde_json::to_string(&Scene {
            center: prepared.camera.center,
            basis: prepared.camera.basis,
            ribbons,
            molecules: molecule::preview(
                &prepared
                    .indices
                    .iter()
                    .filter_map(|&index| {
                        index
                            .checked_sub(self.request.structure.segments.len())
                            .map(|i| &self.request.structure.molecules[i])
                    })
                    .collect::<Vec<_>>(),
            ),
            slice_groups,
            slice_radius,
        })
        .map_err(|e| e.to_string())
    }
    pub fn render_options_json(&mut self, options: &str, preview: bool) -> Result<String, String> {
        if options.len() > 1024 * 1024 {
            return Err("Options exceed 1 MiB".into());
        }
        let options =
            serde_json::from_str(options).map_err(|e| format!("Invalid options JSON: {e}"))?;
        let (result, timings) = self
            .calculate(options, preview)
            .map_err(|e| e.to_string())?;
        #[derive(serde::Serialize)]
        struct Output {
            result: RenderResult,
            timings: StageTimings,
        }
        serde_json::to_string(&Output { result, timings }).map_err(|e| e.to_string())
    }
}

/// Final dimensions are already resolved by the UI. Never inspect paper or
/// landscape to rotate/swap them a second time. Fit actual blended outlines,
/// reserving half the physical stroke at each edge.
fn fit_page(
    sections: &mut [Vec<ribbon::Section>],
    o: &RenderOptions,
    stroke_width: f64,
    extra_bounds: Option<[math::P2; 2]>,
) -> Result<(math::P2, f64, math::P2), RenderError> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    if let Some([elo, ehi]) = extra_bounds {
        for i in 0..2 {
            low[i] = low[i].min(elo[i]);
            high[i] = high[i].max(ehi[i]);
        }
    }
    for section in sections.iter().flatten() {
        for p in [section.left, section.right] {
            for i in 0..2 {
                if !p[i].is_finite() {
                    return model::fail("Nonfinite projected geometry");
                }
                low[i] = low[i].min(p[i]);
                high[i] = high[i].max(p[i]);
            }
        }
    }
    let span = math::sub(high, low);
    if !span.iter().all(|v| v.is_finite()) || span[0].max(span[1]) < 1e-8 {
        return model::fail("Backbone projection is degenerate; cannot fit artwork to the page");
    }
    let available = [
        (o.width_mm - 2.0 * o.margin_mm - stroke_width).max(o.pen_width_mm),
        (o.height_mm - 2.0 * o.margin_mm - stroke_width).max(o.pen_width_mm),
    ];
    let scale = (available[0] / span[0].max(1e-12)).min(available[1] / span[1].max(1e-12));
    let middle = math::mul(math::add(low, high), 0.5);
    let page_middle = [o.width_mm * 0.5, o.height_mm * 0.5];
    for section in sections.iter_mut().flatten() {
        section.left = math::add(
            math::mul(math::sub(section.left, middle), scale),
            page_middle,
        );
        section.right = math::add(
            math::mul(math::sub(section.right, middle), scale),
            page_middle,
        );
    }
    Ok((middle, scale, page_middle))
}

pub(crate) fn xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\t' | '\n' | '\r' => escaped.push(ch),
            c if c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}' => escaped.push(c),
            _ => {}
        }
    }
    escaped
}

fn make_result(
    request: &RenderRequest,
    colors: BTreeMap<(String, bool), Vec<paths::Path>>,
    residue_count: usize,
    warnings: Vec<String>,
) -> Result<RenderResult, RenderError> {
    let o = &request.options;
    let stroke_count = colors.values().map(Vec::len).sum();
    let length_mm = colors.values().flatten().map(|p| paths::length(p)).sum();
    let point_count: usize = colors.values().flatten().map(Vec::len).sum();
    if point_count > model::MAX_PIECES {
        return model::fail("SVG exceeds 2000000 output points; reduce drawing complexity");
    }
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">\n<title>{} / pen plot</title>\n<desc>Visible vector centerlines only. Hidden strokes geometrically removed. No filled surfaces, masks, fonts, or raster data. {}.</desc>\n<g fill=\"none\" stroke=\"black\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
        o.width_mm, o.height_mm, o.width_mm, o.height_mm, xml(&request.structure.name),
        if o.stroke_direction == StrokeDirection::Backbone { "Backbone-directed strokes, without travel-driven reversals" }
        else { "Stroke directions optimized for pen-up travel" },
        o.pen_width_mm,
    );
    for (i, ((color, lipid), paths)) in colors.iter().enumerate() {
        if *lipid {
            let _ = writeln!(
                svg,
                "<g id=\"pen-{}\" stroke=\"{}\" stroke-width=\"{}\">",
                i + 1,
                color,
                o.lipid_width_mm
            );
        } else {
            let _ = writeln!(svg, "<g id=\"pen-{}\" stroke=\"{}\">", i + 1, color);
        }
        for path in paths {
            if let Some(first) = path.first() {
                let _ = write!(svg, "<path d=\"M{:.4},{:.4}", first[0], first[1]);
                for point in &path[1..] {
                    let _ = write!(svg, "L{:.4},{:.4}", point[0], point[1]);
                }
                svg.push_str("\"/>\n");
            }
        }
        svg.push_str("</g>\n");
    }
    svg.push_str("</g>\n</svg>\n");
    if svg.len() > 64 * 1024 * 1024 {
        return model::fail("SVG exceeds 64 MiB");
    }
    Ok(RenderResult {
        svg,
        width_mm: o.width_mm,
        height_mm: o.height_mm,
        stroke_count,
        length_mm,
        residue_count,
        elapsed_ms: 0.0,
        warnings,
    })
}
