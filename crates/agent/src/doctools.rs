//! MCP tools for the document itself: the linter (`validate_document`) and template batch export
//! (`batch_export`) (`docs/wiki/agent-integration.md#document-tools`).
//!
//! `validate_document` only reads. `batch_export` reads the project and writes only into
//! `exports/` (see [`crate::batch`]).

use od_core::lint::{
    self, ExportTarget, Finding, LintOptions, Severity, TextMeasure, TextOverflow,
};
use od_core::{Node, OpError, Operation, Workspace};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::batch::{self, BatchRequest, Row, Target};
use crate::tools::{
    ApplyArgs, CallCtx, Output, ToolError, apply_ops, check_brief, node_id, parse, tool,
};

pub const NAMES: [&str; 3] = ["validate_document", "list_variables", "batch_export"];

/// Text layout comes from the renderer, so the linter sees what an export would draw.
pub struct RenderedText;

impl TextMeasure for RenderedText {
    fn overflow(&self, text: &Node) -> Option<TextOverflow> {
        od_render::check_text(text)
            .into_iter()
            .find_map(|w| match w {
                od_render::Warning::TextOverflow {
                    needed, available, ..
                } => Some(TextOverflow { needed, available }),
                od_render::Warning::ImageNotRendered { .. } => None,
            })
    }
}

/// The size rule of a store preset as a lint export target.
pub fn preset_target(preset: &'static od_store::Preset) -> ExportTarget<'static> {
    ExportTarget {
        id: preset.id.to_string(),
        accepts: Box::new(move |width, height| {
            let facts = od_store::ImageFacts {
                width,
                height,
                format: od_store::Format::Png,
                has_alpha: false,
            };
            !preset
                .validate_image(&facts)
                .iter()
                .any(|v| matches!(v, od_store::Violation::Size { .. }))
        }),
    }
}

/// The format the user chose for the workspace, when it is an image format: a frame matches it at
/// its size or at whole multiples of its width (a panorama of slides).
fn brief_target(ws: &Workspace) -> Option<ExportTarget<'static>> {
    let brief = crate::formats::load_brief(ws.dir()?);
    let format = brief
        .format
        .filter(|f| brief.chosen_by_user && f.kind == "image")?;
    Some(ExportTarget {
        id: format.label,
        accepts: Box::new(move |w, h| h == format.height && w % format.width == 0 && w > 0),
    })
}

/// Lints the open document with the renderer's text layout, the project's files, the workspace's
/// chosen format and any extra `targets`.
pub fn lint_workspace(ws: &Workspace, mut targets: Vec<ExportTarget<'static>>) -> Vec<Finding> {
    targets.extend(brief_target(ws));
    let opts = LintOptions {
        project_dir: ws.dir(),
        text: Some(&RenderedText),
        export_targets: targets,
    };
    lint::lint(ws.session.document(), &opts)
}

/// The report the tool and the CLI print.
pub fn report_json(findings: &[Finding], at_least: Severity) -> Value {
    let (errors, warnings, infos) = lint::counts(findings);
    let shown: Vec<&Finding> = findings.iter().filter(|f| f.severity >= at_least).collect();
    json!({
        "ok": errors == 0,
        "errors": errors,
        "warnings": warnings,
        "infos": infos,
        "findings": shown,
        "rules": "https://github.com/skepjandi/opendesigner/wiki/lint-rules",
    })
}

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    vec![
        tool(
            "validate_document",
            "Lint the whole document and its videos and get findings with stable rule codes (OD001 ...): \
             nodes outside their frame, clipped or overflowing or tiny or empty text, low contrast, \
             missing images or footage or sound, undefined tokens, hidden nodes a video animates, \
             overlapping siblings, frames that match no export target, video timing problems, and broken \
             template variable bindings. Each finding says what is wrong, where (node, frame or video) and \
             how to fix it. Read-only. Call it before you finish a turn and fix every error and warning, \
             or say why you left one.",
            json!({
                "min_severity": {"type": "string", "enum": ["info", "warn", "error"],
                    "description": "Only list findings at or above this severity (default info). Counts always cover everything."}
            }),
            &[],
            true,
        ),
        tool(
            "list_variables",
            "The document's template variables: id, kind (string, number, color, bool, enum, image, font), \
             label, description, what it portrays (subject_name, subject_domain, subject_tagline, \
             subject_logo, host_identity: slots that carry the identity of what the piece is about, which \
             you must take from the user and never invent), default, current value, enum_values and the \
             node properties it is bound to. Declare and change them with apply_ops \
             declare_variable, set_variable, set_variable_value and remove_variable.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "batch_export",
            "Render the project once per row of template-variable values (see list_variables), for personalized sets such as one card per customer or one \
             video per language. Each row is {name, variables:{<variable id>: value}, locale?}; the project \
             is never changed (values are applied to a scratch copy through set_variable_value). Pick \
             exactly one of frame (a PNG per row) or video (an MP4 per row, needs ffmpeg). A row either \
             exports completely and validates against preset, or fails with its reason and writes nothing; \
             other rows still run. Files go to exports/ only: out_template (default exports/{name}.png or \
             .mp4) may use {name} and {index}. Unknown or mistyped variables are skipped with a warning \
             unless strict_variables is true, which fails the row. At most 100 rows (20 for video).",
            json!({
                "rows": {"type": "array", "minItems": 1, "maxItems": 100, "items": {"type": "object"},
                    "description": "Rows {name: letters/digits/_/-, variables: {id: string|number|boolean}, locale?}."},
                "frame": id("Id of the frame to render for each row (a PNG)."),
                "video": id("Id of the video to render for each row (an MP4)."),
                "preset": id("A store preset id (list_export_presets or list_video_presets) every output must meet."),
                "out_template": id("exports/{name}.png (or .mp4 for a video); {name} and {index} are replaced."),
                "strict_variables": {"type": "boolean", "description": "Fail a row that names an unknown variable or gives a value of the wrong type."},
                "scale": {"type": "number", "minimum": 0.1, "maximum": 4, "description": "Pixels per document unit for frames (default 1)."}
            }),
            &["rows"],
            false,
        ),
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateArgs {
    #[serde(default)]
    min_severity: Option<Severity>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchArgs {
    rows: Vec<Row>,
    #[serde(default)]
    frame: Option<String>,
    #[serde(default)]
    video: Option<String>,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    out_template: Option<String>,
    #[serde(default)]
    strict_variables: bool,
    #[serde(default)]
    scale: Option<f32>,
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "validate_document" => {
            let a: ValidateArgs = parse(args)?;
            let findings = lint_workspace(ctx.ws, Vec::new());
            let at_least = a.min_severity.unwrap_or(Severity::Info);
            Ok(Output::json(&report_json(&findings, at_least)))
        }
        "list_variables" => Ok(Output::json(&json!({
            "variables": ctx.ws.session.document().variables,
        }))),
        "batch_export" => {
            let request = batch_request(parse(args)?)?;
            let report = batch::run(ctx.ws, &request)?;
            Ok(Output::json(&report.to_json()))
        }
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

fn batch_request(a: BatchArgs) -> Result<BatchRequest, ToolError> {
    let target = match (&a.frame, &a.video) {
        (Some(f), None) => Target::Frame(node_id(f)?),
        (None, Some(v)) => Target::Video(node_id(v)?),
        _ => {
            return Err(ToolError(
                "give exactly one of frame (a PNG per row) or video (an MP4 per row)".into(),
            ));
        }
    };
    Ok(BatchRequest {
        rows: a.rows,
        target,
        preset: a.preset,
        out_template: a.out_template,
        strict: a.strict_variables,
        scale: a.scale,
    })
}

/// An operation rejection as the tools report it: the stable code, the message, the hint.
pub fn failure_text(e: &OpError) -> String {
    format!("{}: {e}\nhint: {}", e.code(), e.hint())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyCall {
    ops: Vec<Operation>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

/// `apply_ops`: with `dry_run` the batch is checked on a scratch copy and nothing else happens (no
/// checkpoint, no history entry, no save); otherwise it is applied for real.
pub(crate) fn apply_ops_call(ctx: &mut CallCtx, args: Value) -> Result<Output, ToolError> {
    let call: ApplyCall = parse(args)?;
    if !call.dry_run {
        return apply_ops(
            ctx,
            ApplyArgs {
                ops: call.ops,
                turn: call.turn,
                summary: call.summary,
            },
        );
    }
    check_brief(ctx, &call.ops)?;
    Ok(Output::json(&dry_run_json(&ctx.ws.check_ops(&call.ops))))
}

/// What a dry run reports, for the tool and the CLI alike.
pub fn dry_run_json(check: &od_core::OpsCheck) -> Value {
    json!({
        "stage": "dry_run",
        "ok": check.ok,
        "would_change": check.would_change,
        "would_create": check.would_create,
        "ops": check.ops,
    })
}
