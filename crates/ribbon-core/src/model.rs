//! The camelCase JSON boundary mirrors app/types/render.ts.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt;

pub type Vec3 = [f64; 3];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum SecondaryStructure {
    H,
    E,
    C,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackboneResidue {
    pub ca: Vec3,
    /// Carbonyl direction O - C, NOT the absolute oxygen coordinate.
    pub carbonyl: Vec3,
    pub kind: SecondaryStructure,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackboneSegment {
    pub chain_id: String,
    pub residues: Vec<BackboneResidue>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chain {
    pub id: String,
    pub residue_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Molecule {
    pub chain_id: String,
    pub label: String,
    pub category: String,
    pub atoms: Vec<Vec3>,
    pub bonds: Vec<[usize; 2]>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructureData {
    pub name: String,
    pub segments: Vec<BackboneSegment>,
    #[serde(default)]
    pub molecules: Vec<Molecule>,
    pub chains: Vec<Chain>,
    pub residue_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Paper {
    A4,
    A3,
    Bierdeckel,
    Custom,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StrokeDirection {
    Backbone,
    Travel,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderOptions {
    pub paper: Paper,
    pub landscape: bool,
    pub width_mm: f64,
    pub height_mm: f64,
    pub margin_mm: f64,
    pub rotation: Vec3,
    pub lines: usize,
    pub pen_width_mm: f64,
    pub samples: usize,
    pub fill_loops: bool,
    pub loop_spacing_mm: f64,
    pub border_passes: usize,
    pub border_spacing_mm: f64,
    pub stroke_direction: StrokeDirection,
    pub selected_chains: Vec<String>,
    pub chain_colors: BTreeMap<String, String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            paper: Paper::A4,
            landscape: false,
            width_mm: 210.0,
            height_mm: 297.0,
            margin_mm: 15.0,
            rotation: [0.0; 3],
            lines: 7,
            pen_width_mm: 0.25,
            samples: 16,
            fill_loops: true,
            loop_spacing_mm: 0.15,
            border_passes: 3,
            border_spacing_mm: 0.1,
            stroke_direction: StrokeDirection::Backbone,
            selected_chains: Vec::new(),
            chain_colors: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderRequest {
    pub structure: StructureData,
    pub options: RenderOptions,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderResult {
    pub svg: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub stroke_count: usize,
    pub length_mm: f64,
    pub residue_count: usize,
    /// Deliberately zero: the browser worker measures wall time. No native-time
    /// APIs are used in the pure engine (Instant is not portable to WASM).
    pub elapsed_ms: f64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError(pub String);

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RenderError {}

pub(crate) fn fail<T>(message: impl Into<String>) -> Result<T, RenderError> {
    Err(RenderError(message.into()))
}

pub(crate) const MAX_RESIDUES: usize = 50_000;
pub(crate) const MAX_SAMPLES: usize = 200_000;
pub(crate) const MAX_TRACKS: usize = 256;
pub(crate) const MAX_EDGES: usize = 1_000_000;
pub(crate) const MAX_PIECES: usize = 2_000_000;
pub(crate) const MAX_CLIP_TESTS: usize = 50_000_000;
pub(crate) const MAX_JSON_BYTES: usize = 32 * 1024 * 1024;

fn bounded(value: f64, low: f64, high: f64) -> bool {
    value.is_finite() && value >= low && value <= high
}

fn valid_id(id: &str) -> bool {
    // Empty author chain IDs are legal in PDB; do not rename or reject them.
    id.len() <= 128 && !id.chars().any(char::is_control)
}

pub(crate) fn valid_color(color: &str) -> bool {
    color.eq_ignore_ascii_case("black")
        || color.eq_ignore_ascii_case("red")
        || (color.len() == 7
            && color.starts_with('#')
            && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
}

impl RenderRequest {
    /// Validate before any spline allocation; caps apply even to unselected data.
    pub fn validate(&self) -> Result<(), RenderError> {
        let o = &self.options;
        if !bounded(o.width_mm, 10.0, 2000.0) || !bounded(o.height_mm, 10.0, 2000.0) {
            return fail("widthMm and heightMm must be finite and between 10 and 2000 mm");
        }
        if !bounded(o.pen_width_mm, 0.1, 2.0) {
            return fail("penWidthMm must be between 0.1 and 2 mm");
        }
        if !bounded(o.margin_mm, 0.0, 1000.0)
            || 2.0 * o.margin_mm + o.pen_width_mm >= o.width_mm.min(o.height_mm)
        {
            return fail("Page dimensions must exceed twice marginMm plus penWidthMm");
        }
        if o.rotation
            .iter()
            .any(|v| !bounded(*v, -360_000.0, 360_000.0))
        {
            return fail("rotation must contain finite angles in [-360000, 360000] degrees");
        }
        if o.lines > 20 || !(2..=64).contains(&o.samples) || !(1..=3).contains(&o.border_passes) {
            return fail("lines must be 0..20, samples 2..64, and borderPasses 1..3");
        }
        if !bounded(o.loop_spacing_mm, 0.02, 10.0) || !bounded(o.border_spacing_mm, 0.01, 10.0) {
            return fail("loopSpacingMm must be 0.02..10 and borderSpacingMm 0.01..10");
        }
        let s = &self.structure;
        if s.name.len() > 1024
            || s.warnings.len() > 100
            || s.warnings.iter().any(|w| w.len() > 2048)
        {
            return fail("Structure name or warnings exceed text limits");
        }
        if s.segments.len() > MAX_RESIDUES / 4
            || s.chains.len() > 4096
            || o.selected_chains.len() > 4096
            || o.chain_colors.len() > 4096
        {
            return fail("Too many backbone segments or chain entries");
        }
        let mut ids = HashSet::new();
        for chain in &s.chains {
            if !valid_id(&chain.id)
                || !ids.insert(chain.id.as_str())
                || chain.residue_count > MAX_RESIDUES
            {
                return fail("chains must contain unique IDs of at most 128 bytes and bounded residue counts");
            }
        }
        if o.selected_chains
            .iter()
            .any(|id| !ids.contains(id.as_str()))
        {
            return fail("selectedChains contains an unknown chain ID");
        }
        if o.chain_colors
            .iter()
            .any(|(id, color)| !ids.contains(id.as_str()) || !valid_color(color))
        {
            return fail("chainColors must map known IDs to black, red, or #RRGGBB");
        }
        let mut count = 0usize;
        let mut samples = 0usize;
        for (index, segment) in s.segments.iter().enumerate() {
            let n = segment.residues.len();
            if !(4..=MAX_RESIDUES).contains(&n) || !ids.contains(segment.chain_id.as_str()) {
                return fail(format!(
                    "Segment {index} needs a known chain ID and 4..50000 residues"
                ));
            }
            count += n;
            samples += (n - 1) * o.samples + 1;
            if count > MAX_RESIDUES || samples > MAX_SAMPLES {
                return fail("Structure exceeds 50000 residues or 200000 spline samples; reduce samples or input size");
            }
            for (i, residue) in segment.residues.iter().enumerate() {
                if residue.ca.iter().any(|v| !bounded(*v, -1e6, 1e6))
                    || residue.carbonyl.iter().any(|v| !bounded(*v, -1e4, 1e4))
                    || crate::math::norm(residue.carbonyl) < 1e-8
                {
                    return fail(format!("Segment {index}, residue {i}: nonfinite, excessive, or zero backbone coordinates"));
                }
                if i > 0 {
                    let gap =
                        crate::math::norm(crate::math::sub(residue.ca, segment.residues[i - 1].ca));
                    if !(1e-6..=4.500001).contains(&gap) {
                        return fail(format!("Segment {index}, residue {i}: duplicate CA or gap >4.5 Å; split incomplete backbones before rendering"));
                    }
                }
            }
        }
        let mut atoms = 0usize;
        let mut bonds = 0usize;
        let mut molecular_ids = HashSet::new();
        let mut wire_edges = 0usize;
        for molecule in &s.molecules {
            if !ids.contains(molecule.chain_id.as_str())
                || !molecular_ids.insert(molecule.chain_id.as_str())
                || s.segments.iter().any(|s| s.chain_id == molecule.chain_id)
                || molecule.label.len() > 512
                || !["molecule", "nucleic", "water", "ion"].contains(&molecule.category.as_str())
                || molecule.atoms.is_empty()
            {
                return fail("Molecules need distinct known component IDs, bounded labels, categories and atoms");
            }
            atoms = atoms.saturating_add(molecule.atoms.len());
            bonds = bonds.saturating_add(molecule.bonds.len());
            if atoms > 250_000 || bonds > 250_000 {
                return fail("Molecular geometry exceeds 250000 atoms or bonds");
            }
            if molecule
                .atoms
                .iter()
                .flatten()
                .any(|v| !bounded(*v, -1e6, 1e6))
            {
                return fail(
                    "Molecular coordinates must be finite and within +/-1000000 angstroms",
                );
            }
            let mut pairs = HashSet::new();
            let mut connected = vec![false; molecule.atoms.len()];
            for &[a, b] in &molecule.bonds {
                if a >= molecule.atoms.len()
                    || b >= molecule.atoms.len()
                    || a == b
                    || !pairs.insert((a.min(b), a.max(b)))
                {
                    return fail("Molecular bonds need unique, distinct, in-range atom indices");
                }
                connected[a] = true;
                connected[b] = true;
            }
            wire_edges += molecule.bonds.len() + 16 * connected.iter().filter(|&&v| !v).count();
            if wire_edges > MAX_EDGES {
                return fail("Molecular outlines exceed 1000000 edges; export fewer components");
            }
        }
        if s.residue_count > MAX_RESIDUES {
            return fail("residueCount exceeds 50000");
        }
        Ok(())
    }
}
