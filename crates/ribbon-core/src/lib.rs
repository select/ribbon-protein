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
    let prepared = prepare(request);
    let (joined, _) = draw(request, &prepared, false)?;
    finish(request, &joined)
}

struct Prepared {
    samples: usize,
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
fn selected_indices(request: &RenderRequest) -> Vec<usize> {
    let selected: HashSet<&str> = request
        .options
        .selected_chains
        .iter()
        .map(String::as_str)
        .collect();
    (0..request.structure.segments.len() + request.structure.molecules.len())
        .filter(|&i| selected.contains(component_id(&request.structure, i)))
        .collect()
}
fn prepare(request: &RenderRequest) -> Prepared {
    let indices = selected_indices(request);
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
    let camera = if all_points.is_empty() {
        math::Camera {
            center: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    } else {
        math::Camera::new(&all_points, [0.0; 3])
    };
    Prepared {
        samples: request.options.samples,
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

fn draw(
    request: &RenderRequest,
    prepared: &Prepared,
    preview: bool,
) -> Result<(Joined, StageTimings), RenderError> {
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
        ));
    }
    if prepared.indices.is_empty() {
        return model::fail(
            "Selected chains have no continuous backbone segments with at least four residues",
        );
    }
    let started = now();
    let camera = prepared.camera.rotated(o.rotation);
    // Preview keeps the exact full-sample PCA convention, but projects a bounded
    // subset of the actual 3D ribbon. It never enters clipping or path ordering.
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
    for &index in &prepared.indices {
        if index < request.structure.segments.len() {
            continue;
        }
        let m = &request.structure.molecules[index - request.structure.segments.len()];
        let style = o.molecule_style(&m.category);
        let paths = molecule::project(m, &camera, style.bond_width_angstrom);
        molecular_lines.extend(std::iter::repeat_n(Some(style.lines), paths.len()));
        section_indices.extend(std::iter::repeat_n(index, paths.len()));
        sections.extend(paths);
    }
    fit_page(&mut sections, o, o.max_pen_width(&request.structure))?;
    timings.project_ms = now() - started;
    if preview {
        warnings.push("Interaction preview: ribbon outlines and molecular bonds, without hidden-line removal. Refine before exporting.".into());
        let paths = sections
            .iter()
            .map(|s| {
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
        ));
    }
    let tracks = ribbon::track_count(&sections, o, &molecular_lines)?;
    if sections
        .iter()
        .zip(&section_indices)
        .zip(&molecular_lines)
        .any(|((path, &index), lines)| {
            lines.is_some_and(|lines| {
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
    let paths = visible
        .into_iter()
        .map(|edges| paths::merge(&edges, directed, &mut budget))
        .collect::<Result<_, _>>()?;
    timings.paths_ms = now() - started;
    Ok((
        Joined {
            paths,
            indices: section_indices,
            residue_count: prepared.residue_count,
            warnings,
        },
        timings,
    ))
}
fn finish(request: &RenderRequest, joined: &Joined) -> Result<RenderResult, RenderError> {
    let o = &request.options;
    let mut colors: BTreeMap<(String, bool), Vec<paths::Path>> = BTreeMap::new();
    // Joining remains per segment, NOT per color. Rebuild color groups and
    // rerun ordering: a color change can merge/split groups and change travel.
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

/// Owns exactly one structure. Replacement drops every structure-dependent cache.
/// At most two prepared sample/selection variants and one joined view are kept.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct RenderSession {
    request: RenderRequest,
    prepared: std::collections::VecDeque<Prepared>,
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
            joined: None,
        })
    }
    pub fn calculate(
        &mut self,
        options: RenderOptions,
        preview: bool,
    ) -> Result<(RenderResult, StageTimings), RenderError> {
        self.request.options = options;
        self.request.validate()?;
        let mut key = self.request.options.clone();
        key.chain_colors.clear();
        // The joined cache is independent only of colors; pen width affects fit.
        let hit = !preview && self.joined.as_ref().is_some_and(|(k, _)| *k == key);
        let mut timings = StageTimings {
            path_cache_hit: hit,
            ..Default::default()
        };
        if !hit {
            let started = now();
            let indices = selected_indices(&self.request);
            let index = self
                .prepared
                .iter()
                .position(|p| p.samples == key.samples && p.indices == indices);
            let geometry_hit = index.is_some();
            if let Some(index) = index {
                let prepared = self.prepared.remove(index).unwrap();
                self.prepared.push_front(prepared);
            } else {
                if self.prepared.len() == 2 {
                    self.prepared.pop_back();
                }
                self.prepared.push_front(prepare(&self.request));
            }
            let prepare_ms = now() - started;
            let (joined, stages) = draw(&self.request, &self.prepared[0], preview)?;
            timings = StageTimings {
                prepare_ms,
                geometry_cache_hit: geometry_hit,
                ..stages
            };
            if preview {
                let started = now();
                // No plotter ordering in interaction preview.
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
    /// Small actual 3D ribbon scene for responsive main-thread interaction while
    /// synchronous full-quality WASM is busy. Full-sample PCA is retained.
    pub fn preview_scene_json(&mut self) -> Result<String, String> {
        self.request.validate().map_err(|e| e.to_string())?;
        let indices = selected_indices(&self.request);
        let index = self
            .prepared
            .iter()
            .position(|p| p.samples == self.request.options.samples && p.indices == indices);
        let index = if let Some(index) = index {
            index
        } else {
            if self.prepared.len() == 2 {
                self.prepared.pop_back();
            }
            self.prepared.push_front(prepare(&self.request));
            0
        };
        let prepared = &self.prepared[index];
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SceneRibbon<'a> {
            chain_id: &'a str,
            samples: Vec<&'a ribbon::Sample>,
        }
        #[derive(serde::Serialize)]
        struct Scene<'a> {
            center: math::V3,
            basis: [math::V3; 3],
            ribbons: Vec<SceneRibbon<'a>>,
            molecules: Vec<Molecule>,
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
) -> Result<(), RenderError> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
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
        o.width_mm - 2.0 * o.margin_mm - stroke_width,
        o.height_mm - 2.0 * o.margin_mm - stroke_width,
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
    Ok(())
}

fn xml(text: &str) -> String {
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
