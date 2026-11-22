//! Write the plan first, then build it in locked passes (`docs/wiki/scene-spec.md`).
//!
//! The agent writes a SceneSpec per video; Rust validates it, keeps the review ledger and says,
//! in numbers, which pass is open, what evidence is missing and what to call next. The agent
//! judges one contact sheet and chooses one action. Nothing here looks at a picture.
//! The spec lives in `<project>/scene_specs/<name>.json`, inside the project folder.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{CallCtx, Output, ToolError, node_id, parse, tool};

#[cfg(test)]
mod beat_tests;
pub mod beats;
pub mod calibration;
pub mod features;
pub mod loop_policy;
pub mod routing;
pub mod schema;
pub mod status;
#[cfg(test)]
mod tests;
pub mod validate;

use schema::{Action, Evidence, FeatureReview, FeatureTarget, Layer, Review, SceneSpec};

pub const NAMES: [&str; 5] = [
    "scene_spec_new",
    "scene_spec_write",
    "scene_spec_validate",
    "scene_status",
    "record_review",
];

pub const FOLDER: &str = "scene_specs";
pub const MAX_SPEC_BYTES: usize = 256 * 1024;
const MAX_NAME_LEN: usize = 64;
const DEFAULT_NAME: &str = "main";

fn name_prop() -> Value {
    json!({"type": "string", "description": "Plain spec name: lowercase letters, digits, . _ - (default \"main\"). Stored as scene_specs/<name>.json in the project."})
}

pub fn definitions() -> Vec<Value> {
    vec![
        tool(
            "scene_spec_new",
            "Start the SceneSpec for a video: a starter with the minimum number of details for its \
             complexity tier and placeholders that FAIL strict validation until you replace them \
             with the real layers, details, features, palette and motion. Write the plan before \
             you build anything; see get_playbook build.",
            json!({
                "name": name_prop(),
                "subject": {"type": "string", "description": "What the video is about, one sentence."},
                "intent": {"type": "string", "description": "explainer, promo, tutorial, story, ..."},
                "audience": {"type": "string"},
                "video": {"type": "string", "description": "Video id; takes its size, frame rate and length."},
                "tier": {"type": "string", "enum": ["simple", "moderate", "complex", "ultra"], "description": "Complexity tier (default moderate): needs at least 3, 6, 10, 16 inventoried details."},
                "silent": {"type": "boolean", "description": "No audio planned."},
                "max_per_pass": {"type": "integer", "minimum": 1, "description": "Refine actions allowed per pass (default 3)."},
                "max_total": {"type": "integer", "minimum": 1, "description": "Refine actions allowed in total (default 6); not below max_per_pass."},
                "overwrite": {"type": "boolean", "description": "Replace an existing spec of this name, review history included."},
            }),
            &["subject"],
            false,
        ),
        tool(
            "scene_spec_write",
            "Replace the SceneSpec with a full, edited one (a refine-spec, or filling the starter in). \
             The review history is kept as recorded: you cannot rewrite it. Returns the strict \
             validation.",
            json!({
                "name": name_prop(),
                "spec": {"type": "object", "description": "The whole SceneSpec (schema opendesigner.scene_spec/1)."},
            }),
            &["spec"],
            false,
        ),
        tool(
            "scene_spec_validate",
            "Check the SceneSpec. Errors always fail; quality findings (routing under 0.82, too few \
             details for the tier, details that map to nothing, more than 5 critical or 3 \
             important features per pass, ambient motion without loop, gait without stance, no \
             audio) fail only with strict, which a build needs.",
            json!({
                "name": name_prop(),
                "strict": {"type": "boolean"},
                "include_spec": {"type": "boolean", "description": "Also return the spec."},
            }),
            &[],
            true,
        ),
        tool(
            "scene_status",
            "Where the build is, from the review history only: current pass, passes completed, \
             refine counters, active or stopped with the reason, missing evidence and the exact \
             next tool call. Call it before every action; never continue from memory.",
            json!({"name": name_prop()}),
            &[],
            true,
        ),
        tool(
            "record_review",
            "Record your review of a pass and choose ONE action: continue, refine-spec (a planned \
             thing is missing or wrong in the plan), refine-code (the plan is right, the build is \
             not), request-input, or stop. A continue for a visual pass needs the contact sheet and \
             frames in exports/, ai_score >= 0.7, every layer score and every critical feature at \
             its minimum; a pass cannot be continued before the one before it. Refused when the loop \
             limit was reached.",
            json!({
                "name": name_prop(),
                "pass_id": {"type": "string", "enum": schema::PASSES},
                "action": {"type": "string", "enum": ["continue", "refine-spec", "refine-code", "request-input", "stop"]},
                "fidelity": {"type": "number", "minimum": 0, "maximum": 1, "description": "0.2 placeholder, 0.4 silhouette recognizable, 0.6 macro and meso right, 0.75 reads correctly, 0.85 strong, 0.95 near-reference."},
                "ai_score": {"type": "number", "minimum": 0, "maximum": 1, "description": "Your score of the contact sheet as a whole."},
                "layer_scores": {"type": "object", "description": "0 to 1 each: silhouette_proportion, component_structure, form_detail, palette_fidelity, motion_quality.", "additionalProperties": {"type": "number"}},
                "feature_reviews": {"type": "array", "items": {"type": "object", "properties": {
                    "id": {"type": "string"}, "score": {"type": "number"}, "visible": {"type": "boolean"}, "notes": {"type": "string"}},
                    "required": ["id"]}},
                "summary": {"type": "string", "description": "What changed, what still does not match, and what the numeric gate does not measure."},
                "evidence": {"type": "object", "properties": {
                    "contact_sheet": {"type": "string", "description": "exports/<name>.png"},
                    "frames": {"type": "array", "items": {"type": "string"}}}},
                "defect_tags": {"type": "array", "items": {"type": "string"}, "description": "Short names of the defects you still see; the same tag twice running means refine-spec."},
                "reverted": {"type": "boolean", "description": "This iteration undid the previous one."},
                "hard_gate_failures": {"type": "array", "items": {"type": "string"}, "description": "Numeric gate findings with severity fail."},
            }),
            &["pass_id", "action", "fidelity", "summary"],
            false,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    let dir = ctx
        .ws
        .dir()
        .map(Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder, so no scene_specs/".into()))?;
    match name {
        "scene_spec_new" => spec_new(ctx, &dir, parse(args)?),
        "scene_spec_write" => spec_write(&dir, parse(args)?),
        "scene_spec_validate" => spec_validate(&dir, parse(args)?),
        "scene_status" => spec_status(&dir, parse(args)?),
        "record_review" => record_review(&dir, parse(args)?),
        _ => Err(ToolError(format!("unknown tool {name}"))),
    }
}

fn fail<T>(message: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(message.into()))
}

/// Plain names only: `[a-z0-9._-]+`, at most 64 characters, no leading dot, no `..`.
pub fn valid_name(name: &str) -> Result<(), ToolError> {
    let plain = !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && !name.starts_with('.')
        && !name.contains("..")
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        });
    if plain {
        Ok(())
    } else {
        fail(format!(
            "{name:?} is not a valid spec name: lowercase letters, digits, . _ - only, up to {MAX_NAME_LEN} characters"
        ))
    }
}

fn refuse_symlink(path: &Path) -> Result<(), ToolError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fail(format!(
            "{} is a symbolic link; scene specs never follow links",
            path.display()
        )),
        _ => Ok(()),
    }
}

fn spec_path(dir: &Path, name: &str) -> Result<PathBuf, ToolError> {
    valid_name(name)?;
    let folder = dir.join(FOLDER);
    refuse_symlink(&folder)?;
    Ok(folder.join(format!("{name}.json")))
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub fn load(dir: &Path, name: &str) -> Result<SceneSpec, ToolError> {
    let path = spec_path(dir, name)?;
    let meta = fs::symlink_metadata(&path).map_err(|_| {
        ToolError(format!(
            "no scene spec called {name}: scene_spec_new {{\"subject\": ...}} starts one"
        ))
    })?;
    if !meta.is_file() {
        return fail(format!("{} is not a plain file", path.display()));
    }
    if meta.len() as usize > MAX_SPEC_BYTES {
        return fail(format!(
            "{name} is larger than {} KB",
            MAX_SPEC_BYTES / 1024
        ));
    }
    let text = fs::read_to_string(&path)?;
    serde_json::from_str(&text)
        .map_err(|e| ToolError(format!("{name} is not a valid scene spec: {e}")))
}

/// Writes atomically: a temporary file in the same folder, then a rename.
pub fn save(dir: &Path, name: &str, spec: &SceneSpec) -> Result<PathBuf, ToolError> {
    let path = spec_path(dir, name)?;
    refuse_symlink(&path)?;
    let text = serde_json::to_string_pretty(spec).map_err(|e| ToolError(e.to_string()))?;
    if text.len() > MAX_SPEC_BYTES {
        return fail(format!(
            "the spec is {} KB, over the {} KB limit: trim details or history",
            text.len() / 1024,
            MAX_SPEC_BYTES / 1024
        ));
    }
    let folder = dir.join(FOLDER);
    fs::create_dir_all(&folder)?;
    let tmp = folder.join(format!(".{name}.json.tmp"));
    refuse_symlink(&tmp)?;
    let written = fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    if let Err(e) = written.and_then(|()| fs::rename(&tmp, &path)) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(path)
}

fn name_of(name: Option<String>) -> String {
    name.unwrap_or_else(|| DEFAULT_NAME.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewArgs {
    #[serde(default)]
    name: Option<String>,
    subject: String,
    #[serde(default)]
    intent: Option<String>,
    #[serde(default)]
    audience: Option<String>,
    #[serde(default)]
    video: Option<String>,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    silent: bool,
    #[serde(default)]
    max_per_pass: Option<u32>,
    #[serde(default)]
    max_total: Option<u32>,
    #[serde(default)]
    overwrite: bool,
}

/// The starter: the tier's minimum of detail slots and two layers and two features, all marked as
/// placeholders so strict validation fails until the agent replaces them.
pub fn starter(name: &str, subject: &str, tier: &str) -> SceneSpec {
    let floor = features::tier_floor(tier).unwrap_or(0);
    let mut spec = SceneSpec {
        id: name.to_string(),
        ..SceneSpec::default()
    };
    spec.brief.subject = subject.to_string();
    spec.complexity.tier = tier.to_string();
    spec.routing.classification = json!({
        "kind": "unknown", "confidence": 0.0,
        "evidence_refs": ["brief:subject"], "provider": "agent", "version": "1",
    });
    spec.layer_plan = vec![
        Layer {
            id: "hero".into(),
            level: "macro".into(),
            role: "hero".into(),
            depth: Some(1.0),
            placeholder: true,
            ..Layer::default()
        },
        Layer {
            id: "hero-part".into(),
            level: "meso".into(),
            role: "prop".into(),
            parent: Some("hero".into()),
            placeholder: true,
            ..Layer::default()
        },
    ];
    spec.detail_inventory.details = (1..=floor)
        .map(|i| schema::Detail {
            id: format!("detail-{i}"),
            placeholder: true,
            ..schema::Detail::default()
        })
        .collect();
    spec.feature_targets = vec![
        FeatureTarget {
            id: "overall-silhouette".into(),
            tier: "critical".into(),
            pass_ids: vec!["blockout".into(), "shapes".into()],
            minimum_score: Some(features::CRITICAL_MIN),
            must_pass: true,
            placeholder: true,
            ..FeatureTarget::default()
        },
        FeatureTarget {
            id: "primary-structure".into(),
            tier: "critical".into(),
            pass_ids: vec!["shapes".into()],
            minimum_score: Some(features::CRITICAL_MIN),
            must_pass: true,
            placeholder: true,
            ..FeatureTarget::default()
        },
    ];
    spec.freeze.after_pass = Some("shapes".into());
    spec.calibration.uncalibrated = ["attach.gap", "light.tolerance", "budgets"]
        .map(String::from)
        .to_vec();
    spec
}

fn spec_new(ctx: &mut CallCtx, dir: &Path, a: NewArgs) -> Result<Output, ToolError> {
    let name = name_of(a.name);
    let path = spec_path(dir, &name)?;
    if exists(&path) && !a.overwrite {
        return fail(format!(
            "{name} already exists; scene_spec_write edits it, overwrite: true replaces it and its review history"
        ));
    }
    let tier = a.tier.unwrap_or_else(|| "moderate".into());
    if features::tier_floor(&tier).is_none() {
        return fail(format!(
            "tier {tier:?} is not simple, moderate, complex or ultra"
        ));
    }
    let limits = loop_policy::LoopLimits::new(
        a.max_per_pass.unwrap_or(loop_policy::DEFAULT_MAX_PER_PASS),
        a.max_total.unwrap_or(loop_policy::DEFAULT_MAX_TOTAL),
    )
    .map_err(|e| ToolError(e.to_string()))?;
    let mut spec = starter(&name, a.subject.trim(), &tier);
    if spec.brief.subject.is_empty() {
        return fail("subject must say what the video is about");
    }
    spec.limits.max_per_pass = limits.max_per_pass;
    spec.limits.max_total = limits.max_total;
    spec.brief.intent = a.intent.unwrap_or_default();
    spec.brief.audience = a.audience.unwrap_or_default();
    spec.brief.silent = a.silent;
    if let Some(video) = &a.video {
        let id = node_id(video)?;
        let doc = ctx.ws.session.document();
        let v = doc.video(&id).ok_or_else(|| {
            ToolError(format!(
                "video {video} does not exist; list_videos shows them"
            ))
        })?;
        spec.video_id = Some(video.clone());
        spec.brief.format = schema::Format {
            width: v.width,
            height: v.height,
            fps: v.fps,
        };
        spec.brief.duration_ms = v.duration_ms();
    }
    save(dir, &name, &spec)?;
    let report = validate::validate(&spec);
    Ok(Output::json(&json!({
        "name": name,
        "path": format!("{FOLDER}/{name}.json"),
        "spec": spec,
        "validation": report.to_json(true),
        "next": "Replace every placeholder (routing, layers, details, features), add palette, motion and audio, write it with scene_spec_write, then scene_spec_validate with strict.",
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArgs {
    #[serde(default)]
    name: Option<String>,
    spec: Value,
}

fn spec_write(dir: &Path, a: WriteArgs) -> Result<Output, ToolError> {
    let name = name_of(a.name);
    let mut spec: SceneSpec = serde_json::from_value(a.spec)
        .map_err(|e| ToolError(format!("the spec does not fit the schema: {e}")))?;
    let (previous, kept) = match load(dir, &name) {
        Ok(old) => (old.review_history, true),
        Err(_) => (Vec::new(), false),
    };
    spec.review_history = previous;
    save(dir, &name, &spec)?;
    Ok(Output::json(&json!({
        "name": name,
        "written": format!("{FOLDER}/{name}.json"),
        "review_history": if kept { "kept" } else { "empty: there was no readable spec to keep it from" },
        "validation": validate::validate(&spec).to_json(true),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateArgs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    include_spec: bool,
}

fn spec_validate(dir: &Path, a: ValidateArgs) -> Result<Output, ToolError> {
    let name = name_of(a.name);
    let spec = load(dir, &name)?;
    let mut out = validate::validate(&spec).to_json(a.strict);
    let resolution = routing::resolve(
        &spec.routing.classification,
        spec.routing.track.as_deref(),
        routing::TRACK_BY_KIND,
    );
    out["name"] = json!(name);
    out["routing"] = json!({
        "status": resolution.status.as_str(),
        "kind": resolution.classification.kind,
        "confidence": resolution.classification.confidence,
        "track": resolution.track,
        "conflicts": resolution.conflicts,
        "threshold": routing::CONFIDENCE_THRESHOLD,
    });
    out["min_details"] = json!(features::effective_min_details(
        &spec.complexity.tier,
        spec.detail_inventory.target_min_details
    ));
    if a.include_spec {
        out["spec"] = json!(spec);
    }
    Ok(Output::json(&out))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusArgs {
    #[serde(default)]
    name: Option<String>,
}

fn spec_status(dir: &Path, a: StatusArgs) -> Result<Output, ToolError> {
    let name = name_of(a.name);
    let spec = load(dir, &name)?;
    Ok(Output::json(&status::status(&spec, dir, &name)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordArgs {
    #[serde(default)]
    name: Option<String>,
    pass_id: String,
    action: Action,
    fidelity: Option<f64>,
    #[serde(default)]
    ai_score: Option<f64>,
    #[serde(default)]
    layer_scores: std::collections::BTreeMap<String, f64>,
    #[serde(default)]
    feature_reviews: Vec<FeatureReview>,
    summary: String,
    #[serde(default)]
    evidence: Evidence,
    #[serde(default)]
    defect_tags: Vec<String>,
    #[serde(default)]
    reverted: bool,
    #[serde(default)]
    hard_gate_failures: Vec<String>,
}

fn record_review(dir: &Path, a: RecordArgs) -> Result<Output, ToolError> {
    let name = name_of(a.name);
    let mut spec = load(dir, &name)?;
    let review = Review {
        pass_id: a.pass_id,
        action: a.action,
        fidelity: a.fidelity,
        ai_score: a.ai_score,
        layer_scores: a.layer_scores,
        feature_reviews: a.feature_reviews,
        summary: a.summary,
        evidence: a.evidence,
        defect_tags: a.defect_tags,
        reverted: a.reverted,
        hard_gate_failures: a.hard_gate_failures,
    };
    let recorded = status::record(&mut spec, review, dir).map_err(|e| ToolError(e.to_string()))?;
    save(dir, &name, &spec)?;
    Ok(Output::json(&json!({
        "recorded": recorded.index,
        "completes_pass": recorded.completes_pass,
        "status": status::status(&spec, dir, &name),
    })))
}
