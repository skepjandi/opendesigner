//! The SceneSpec: what the agent decides before it builds (`docs/wiki/scene-spec.md`).
//!
//! Plain serde structs. Unknown fields are accepted (forward compatible) and kept on the top
//! level; the checks live in `validate`.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

pub const SCHEMA: &str = "opendesigner.scene_spec/1";

pub const PASSES: [&str; 6] = ["blockout", "shapes", "palette", "motion", "polish", "audio"];
pub const LAYER_SCORES: [&str; 5] = [
    "silhouette_proportion",
    "component_structure",
    "form_detail",
    "palette_fidelity",
    "motion_quality",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Continue,
    RefineSpec,
    RefineCode,
    RequestInput,
    Stop,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Continue => "continue",
            Action::RefineSpec => "refine-spec",
            Action::RefineCode => "refine-code",
            Action::RequestInput => "request-input",
            Action::Stop => "stop",
        }
    }

    pub fn is_refine(self) -> bool {
        matches!(self, Action::RefineSpec | Action::RefineCode)
    }
}

/// A pass is visual when a contact sheet can show whether it worked; `audio` cannot be seen.
pub fn is_visual_pass(pass: &str) -> bool {
    pass != "audio"
}

fn number_or_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(Value::deserialize(d)?.as_f64())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneSpec {
    pub schema: String,
    pub id: String,
    pub video_id: Option<String>,
    pub brief: Brief,
    pub routing: Routing,
    pub complexity: Complexity,
    pub palette: Palette,
    pub layer_plan: Vec<Layer>,
    pub detail_inventory: DetailInventory,
    pub feature_targets: Vec<FeatureTarget>,
    pub motion_plan: Vec<Motion>,
    pub audio_plan: AudioPlan,
    pub beat_map: Vec<Beat>,
    /// The seed this spec came from (`apply_seed`), when it did.
    pub seed: Option<String>,
    pub budgets: Budgets,
    pub build_passes: Vec<String>,
    pub limits: Limits,
    pub freeze: Freeze,
    pub review_history: Vec<Review>,
    pub calibration: CalibrationLists,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for SceneSpec {
    fn default() -> Self {
        SceneSpec {
            schema: SCHEMA.into(),
            id: String::new(),
            video_id: None,
            brief: Brief::default(),
            routing: Routing::default(),
            complexity: Complexity::default(),
            palette: Palette::default(),
            layer_plan: Vec::new(),
            detail_inventory: DetailInventory::default(),
            feature_targets: Vec::new(),
            motion_plan: Vec::new(),
            audio_plan: AudioPlan::default(),
            beat_map: Vec::new(),
            seed: None,
            budgets: Budgets::default(),
            build_passes: PASSES.iter().map(|p| (*p).to_string()).collect(),
            limits: Limits::default(),
            freeze: Freeze::default(),
            review_history: Vec::new(),
            calibration: CalibrationLists::default(),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Brief {
    pub subject: String,
    pub intent: String,
    pub format: Format,
    pub duration_ms: u64,
    pub audience: String,
    pub style: Value,
    pub silent: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Format {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Routing {
    pub track: Option<String>,
    /// Kept raw so a malformed record fails closed in `routing::resolve` instead of failing to load.
    pub classification: Value,
    pub status: String,
    pub conflicts: Vec<String>,
}

impl Default for Routing {
    fn default() -> Self {
        Routing {
            track: None,
            classification: Value::Null,
            status: "request-input".into(),
            conflicts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Complexity {
    pub tier: String,
    pub scores: BTreeMap<String, u8>,
}

impl Default for Complexity {
    fn default() -> Self {
        Complexity {
            tier: "moderate".into(),
            scores: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    pub tokens: Vec<Token>,
    pub max_tokens: usize,
    pub min_pair_delta_e: f64,
    pub min_text_contrast: f64,
    pub light: Light,
}

impl Default for Palette {
    fn default() -> Self {
        Palette {
            tokens: Vec::new(),
            max_tokens: 8,
            min_pair_delta_e: 8.0,
            min_text_contrast: 4.5,
            light: Light::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Token {
    pub id: String,
    pub hex: String,
    pub role: String,
    pub derived_from: Option<String>,
    /// `sampled` (taken from a reference) or `declared`.
    pub source: Option<String>,
    pub sample_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
    pub angle_deg: f64,
    pub tolerance_deg: f64,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            angle_deg: -45.0,
            tolerance_deg: 20.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub id: String,
    pub level: String,
    pub role: String,
    pub node_ref: Option<String>,
    pub depth: Option<f64>,
    pub bleed: bool,
    pub parent: Option<String>,
    pub attach: Option<Attach>,
    pub palette_refs: Vec<String>,
    pub side: Option<String>,
    pub facing: Option<String>,
    pub mirror_of: Option<String>,
    pub expects_motion: Option<String>,
    pub min_px: Option<f64>,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attach {
    /// The layer it hangs from, when that is not its `parent`.
    pub to: Option<String>,
    pub socket: Option<[f64; 2]>,
    pub gap_tolerance_px: Option<f64>,
    pub contact: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DetailInventory {
    pub scan_method: String,
    pub target_min_details: Option<u32>,
    pub details: Vec<Detail>,
}

impl Default for DetailInventory {
    fn default() -> Self {
        DetailInventory {
            scan_method: "component-zones".into(),
            target_min_details: None,
            details: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Detail {
    pub id: String,
    pub kind: String,
    pub region: Option<Region>,
    pub scale: Option<String>,
    pub affects: Option<String>,
    pub maps_to: Option<MapsTo>,
    pub evidence_ref: Option<String>,
    pub confidence: f64,
    pub min_px: Option<f64>,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub units: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MapsTo {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureTarget {
    pub id: String,
    pub tier: String,
    pub pass_ids: Vec<String>,
    #[serde(deserialize_with = "number_or_none")]
    pub minimum_score: Option<f64>,
    pub must_pass: bool,
    pub node_refs: Vec<String>,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    pub id: String,
    pub target: String,
    pub kind: String,
    pub preset: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    /// Three states: `true` measured to loop, `false` measured not to, `null` nobody measured.
    #[serde(rename = "loop")]
    pub looping: Option<bool>,
    pub stance: Vec<Stance>,
    pub expect: Value,
    pub intent_inferred: Option<bool>,
    pub camera_x_max_px: Option<f64>,
    pub zoom_max: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stance {
    pub from_ms: u64,
    pub to_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioPlan {
    pub bpm: Option<f64>,
    pub tracks: Vec<AudioTrack>,
    pub cues: Vec<AudioCue>,
    pub loudness_lufs: Option<f64>,
    pub true_peak_db: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioTrack {
    pub id: String,
    pub kind: String,
    pub from_ms: u64,
    pub to_ms: u64,
    pub duck_under: Option<String>,
    pub duck_db: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioCue {
    pub at_ms: u64,
    pub sfx: String,
    pub sync_to: Option<String>,
    pub tolerance_ms: Option<u64>,
}

/// One moment the viewer must take something from: time, takeaway, focal object, the change that
/// shows it, the words and sound with it, and when it leaves.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Beat {
    /// The beat's name; sound cues are placed by it (`beat_times`). Defaults to `beat-<n>`.
    pub id: String,
    pub at_ms: u64,
    pub takeaway: String,
    /// A `layer_plan` id (or its node_ref) the eye is led to.
    pub focal_node: Option<String>,
    pub transformation: String,
    pub copy: Option<String>,
    pub audio_cue: Option<String>,
    pub exit_ms: Option<u64>,
    /// Set once the beat is on the timeline; `scene_status` points at the first that is not.
    pub built: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Budgets {
    pub max_nodes: Option<u32>,
    pub max_path_commands: Option<u32>,
    pub max_blur_nodes: Option<u32>,
    pub max_shadow_nodes: Option<u32>,
    pub max_frame_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub max_per_pass: u32,
    pub max_total: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_per_pass: crate::scene_spec::loop_policy::DEFAULT_MAX_PER_PASS,
            max_total: crate::scene_spec::loop_policy::DEFAULT_MAX_TOTAL,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Freeze {
    pub after_pass: Option<String>,
    pub hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrationLists {
    pub calibrated: Vec<String>,
    pub uncalibrated: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub pass_id: String,
    pub action: Action,
    #[serde(default, deserialize_with = "number_or_none")]
    pub fidelity: Option<f64>,
    #[serde(default, deserialize_with = "number_or_none")]
    pub ai_score: Option<f64>,
    #[serde(default)]
    pub layer_scores: BTreeMap<String, f64>,
    #[serde(default)]
    pub feature_reviews: Vec<FeatureReview>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub evidence: Evidence,
    #[serde(default)]
    pub defect_tags: Vec<String>,
    #[serde(default)]
    pub reverted: bool,
    #[serde(default)]
    pub hard_gate_failures: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureReview {
    pub id: String,
    #[serde(deserialize_with = "number_or_none")]
    pub score: Option<f64>,
    pub visible: Option<bool>,
    pub notes: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Evidence {
    pub contact_sheet: Option<String>,
    pub frames: Vec<String>,
}
