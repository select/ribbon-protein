//! A local, pure-Rust protein ribbon renderer, shared by native tests and WASM.
//!
//! The parser owns secondary-structure assignment and backbone segmentation.
//! This engine consumes normalized coordinates only. It never accesses files,
//! the network, native clocks, or browser state.
mod clip;
mod math;
mod model;
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

/// Native pure rendering entry point. All geometry is computed as finite f64;
/// options are strictly validated before any spline or track allocations.
pub fn render(request: &RenderRequest) -> Result<RenderResult, RenderError> {
    request.validate()?;
    let o = &request.options;
    let selected: HashSet<&str> = o.selected_chains.iter().map(String::as_str).collect();
    let segments: Vec<_> = request
        .structure
        .segments
        .iter()
        .filter(|s| selected.contains(s.chain_id.as_str()))
        .collect();
    let mut warnings = request.structure.warnings.clone();
    if selected.is_empty() {
        warnings.push("No chains selected; the drawing is empty.".into());
        return make_result(request, BTreeMap::new(), 0, warnings);
    }
    if segments.is_empty() {
        return model::fail(
            "Selected chains have no continuous backbone segments with at least four residues",
        );
    }
    let residue_count = segments.iter().map(|s| s.residues.len()).sum();
    let mut ribbons = Vec::with_capacity(segments.len());
    let mut fallback_used = false;
    for segment in &segments {
        let (samples, fallback) = ribbon::make_ribbon(segment, o.samples);
        ribbons.push(samples);
        fallback_used |= fallback;
    }
    if fallback_used {
        warnings.push("Some carbonyl directions are parallel to the backbone; stable transported fallback frames were used.".into());
    }
    let all_points: Vec<_> = ribbons.iter().flatten().map(|s| s.point).collect();
    let camera = math::Camera::new(&all_points, o.rotation);
    let mut sections: Vec<_> = ribbons
        .iter()
        .map(|s| ribbon::project_ribbon(s, &camera))
        .collect();
    drop(ribbons);
    drop(all_points);
    fit_page(&mut sections, o)?;
    let tracks = ribbon::track_count(&sections, o)?;
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
    let visible = clip::visible_edges(&mut panels, segments.len(), tracks, o)?;
    drop(panels);
    let mut path_budget = paths::Budget::default();
    let directed = o.stroke_direction == StrokeDirection::Backbone;
    let mut colors: BTreeMap<String, Vec<paths::Path>> = BTreeMap::new();
    for (i, edges) in visible.into_iter().enumerate() {
        // Always keep segments separate while joining, including separate
        // segments with identical chain IDs and colors. A physical gap is not
        // an invitation to draw an extra chord.
        let paths = paths::merge(&edges, directed, &mut path_budget)?;
        let color = o
            .chain_colors
            .get(&segments[i].chain_id)
            .map(String::as_str)
            .unwrap_or("black");
        colors.entry(color.to_owned()).or_default().extend(paths);
    }
    for paths in colors.values_mut() {
        *paths = paths::order(std::mem::take(paths), !directed);
    }
    if colors.values().all(Vec::is_empty) {
        warnings.push(
            "No strokes survived visibility clipping and the 0.08 mm minimum stroke length.".into(),
        );
    }
    make_result(request, colors, residue_count, warnings)
}

/// Final dimensions are already resolved by the UI. Never inspect paper or
/// landscape to rotate/swap them a second time. Fit actual blended outlines,
/// reserving half the physical stroke at each edge.
fn fit_page(sections: &mut [Vec<ribbon::Section>], o: &RenderOptions) -> Result<(), RenderError> {
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
        o.width_mm - 2.0 * o.margin_mm - o.pen_width_mm,
        o.height_mm - 2.0 * o.margin_mm - o.pen_width_mm,
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
    colors: BTreeMap<String, Vec<paths::Path>>,
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
    for (i, (color, paths)) in colors.iter().enumerate() {
        let _ = writeln!(svg, "<g id=\"pen-{}\" stroke=\"{}\">", i + 1, color);
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
