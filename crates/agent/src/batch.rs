//! Batch export: render the project once per row of template-variable values
//! (`docs/wiki/agent-integration.md#batch-export`).
//!
//! Each row is applied to a scratch copy of the document through the ordinary
//! `set_variable_value` operation, so the project, its history and its files are never touched
//! except for the exported files themselves. A row either exports completely or reports why not;
//! rows are independent. Row files and `batch_export` arguments are untrusted: sizes, names,
//! types and output paths are checked before anything is rendered.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use od_core::{Actor, Document, NodeId, Operation, VarValue, Workspace};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::export::{Plan, export_assets};
use crate::tools::ToolError;

pub const MAX_ROWS: usize = 100;
/// Video rows each encode a whole video, so fewer are allowed in one call.
pub const MAX_VIDEO_ROWS: usize = 20;
pub const MAX_ROWS_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_VARIABLES_PER_ROW: usize = 100;
const MAX_NAME_LEN: usize = 64;
const MAX_TEMPLATE_LEN: usize = 200;
const EXPORTS: &str = "exports";

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

/// One export: a name for the output file and the variable values to render with.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    /// Letters, digits, `_` and `-`; becomes `{name}` in the output template.
    pub name: String,
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
    /// Render this language's translations of text nodes (they win over variable-written text).
    #[serde(default)]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Frame(NodeId),
    Video(NodeId),
}

#[derive(Debug, Clone)]
pub struct BatchRequest {
    pub rows: Vec<Row>,
    pub target: Target,
    pub preset: Option<String>,
    /// `exports/{name}.png` or `.mp4`; `{name}` and `{index}` (1-based) are replaced.
    pub out_template: Option<String>,
    /// Unknown or mistyped variables fail the row instead of being skipped with a warning.
    pub strict: bool,
    pub scale: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowReport {
    pub name: String,
    /// Files written, relative to the project folder.
    pub files: Vec<String>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

impl RowReport {
    pub fn ok(&self) -> bool {
        self.error.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchReport {
    pub rows: Vec<RowReport>,
}

impl BatchReport {
    pub fn ok(&self) -> bool {
        self.rows.iter().all(RowReport::ok)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "ok": self.ok(),
            "exported": self.rows.iter().filter(|r| r.ok()).count(),
            "failed": self.rows.iter().filter(|r| !r.ok()).count(),
            "rows": self.rows.iter().map(|r| json!({
                "name": r.name,
                "ok": r.ok(),
                "files": r.files,
                "warnings": r.warnings,
                "error": r.error,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Rows from a rows file: a JSON array of rows, or `{"rows": [...]}`.
pub fn parse_rows(text: &str) -> Result<Vec<Row>, ToolError> {
    if text.len() as u64 > MAX_ROWS_FILE_BYTES {
        return fail(format!("the rows file is over {MAX_ROWS_FILE_BYTES} bytes"));
    }
    let value: Value = serde_json::from_str(text)
        .map_err(|e| ToolError(format!("the rows file is not valid JSON: {e}")))?;
    let rows = match value {
        Value::Array(_) => value,
        Value::Object(mut o) if o.len() == 1 => o
            .remove("rows")
            .ok_or_else(|| ToolError("the rows file object must hold only \"rows\"".into()))?,
        _ => return fail("the rows file must be an array of rows or {\"rows\": [...]}"),
    };
    serde_json::from_value(rows).map_err(|e| ToolError(format!("invalid rows: {e}")))
}

/// Reads and parses a rows file, refusing one over [`MAX_ROWS_FILE_BYTES`] before reading it.
pub fn read_rows_file(path: &Path) -> Result<Vec<Row>, ToolError> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_ROWS_FILE_BYTES {
        return fail(format!(
            "{} is {size} bytes, over the {MAX_ROWS_FILE_BYTES} byte limit",
            path.display()
        ));
    }
    parse_rows(&std::fs::read_to_string(path)?)
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn extension(target: &Target) -> &'static str {
    match target {
        Target::Frame(_) => "png",
        Target::Video(_) => "mp4",
    }
}

/// The file name inside `exports/` that `template` gives row `name` (`index` counts from 1).
/// Only the `exports/` folder is allowed: no `..`, no absolute path, no separators in the name.
pub fn output_name(
    template: &str,
    name: &str,
    index: usize,
    extension: &str,
) -> Result<String, ToolError> {
    if template.len() > MAX_TEMPLATE_LEN {
        return fail(format!(
            "the output template is over {MAX_TEMPLATE_LEN} characters"
        ));
    }
    let rest = template
        .strip_prefix("exports/")
        .ok_or_else(|| ToolError(format!(
            "the output template must start with exports/ (got {template:?}): batch files are written only there"
        )))?;
    let file = rest
        .replace("{name}", name)
        .replace("{index}", &index.to_string());
    let plain = !file.is_empty()
        && !file.contains("..")
        && file
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !plain {
        return fail(format!(
            "the output template gives {file:?}; it must be a plain file name (letters, digits . _ -) directly in exports/"
        ));
    }
    if !file.ends_with(&format!(".{extension}")) {
        return fail(format!("the output file {file:?} must end in .{extension}"));
    }
    Ok(file)
}

/// Refuses an `exports/` folder or output file that is a symlink, so a link cannot redirect a
/// write outside the project.
fn ensure_not_linked(project: &Path, file: &str) -> Result<(), ToolError> {
    for path in [project.join(EXPORTS), project.join(EXPORTS).join(file)] {
        if let Ok(meta) = std::fs::symlink_metadata(&path)
            && meta.file_type().is_symlink()
        {
            return fail(format!(
                "{} is a symbolic link; batch export will not write through it",
                path.display()
            ));
        }
    }
    Ok(())
}

fn to_var_value(v: &Value) -> Option<VarValue> {
    match v {
        Value::Bool(b) => Some(VarValue::Bool(*b)),
        Value::Number(n) => n.as_f64().map(VarValue::Number),
        Value::String(s) => Some(VarValue::Text(s.clone())),
        _ => None,
    }
}

/// The document as row `row` shows it: its variable values written through
/// `set_variable_value` on a scratch session. Returns the document and warnings for what was
/// skipped (unknown or invalid values) when not `strict`.
fn row_document(
    doc: &Document,
    row: &Row,
    strict: bool,
) -> Result<(Document, Vec<String>), ToolError> {
    if row.variables.len() > MAX_VARIABLES_PER_ROW {
        return fail(format!(
            "row {} sets {} variables; at most {MAX_VARIABLES_PER_ROW}",
            row.name,
            row.variables.len()
        ));
    }
    let mut session = od_core::Session::new(doc.clone());
    let mut warnings = Vec::new();
    for (key, raw) in &row.variables {
        let problem = match apply_value(&mut session, key, raw) {
            Ok(()) => continue,
            Err(p) => p,
        };
        if strict {
            return fail(format!("variable {key}: {problem}"));
        }
        warnings.push(format!("variable {key} skipped: {problem}"));
    }
    Ok((session.document().clone(), warnings))
}

fn apply_value(session: &mut od_core::Session, key: &str, raw: &Value) -> Result<(), String> {
    let id = NodeId::parse(key).map_err(|_| "no such variable".to_string())?;
    if session.document().variable(&id).is_none() {
        return Err("no such variable".into());
    }
    let value = to_var_value(raw).ok_or("a variable takes a string, a number or a boolean")?;
    session
        .apply(
            Actor::User,
            None,
            &[Operation::SetVariableValue { id, value }],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn check_request(req: &BatchRequest) -> Result<Vec<String>, ToolError> {
    let limit = match req.target {
        Target::Frame(_) => MAX_ROWS,
        Target::Video(_) => MAX_VIDEO_ROWS,
    };
    if req.rows.is_empty() || req.rows.len() > limit {
        return fail(format!("give 1 to {limit} rows, not {}", req.rows.len()));
    }
    let ext = extension(&req.target);
    let template = req
        .out_template
        .clone()
        .unwrap_or_else(|| format!("exports/{{name}}.{ext}"));
    let mut seen = HashSet::new();
    let mut names = HashSet::new();
    let mut files = Vec::with_capacity(req.rows.len());
    for (i, row) in req.rows.iter().enumerate() {
        if !name_ok(&row.name) {
            return fail(format!(
                "row {} has the name {:?}; use 1 to {MAX_NAME_LEN} letters, digits, _ or -",
                i + 1,
                row.name
            ));
        }
        if let Some(l) = &row.locale {
            od_core::limits::locale_tag(l)?;
        }
        if !names.insert(row.name.as_str()) {
            return fail(format!("two rows are named {:?}", row.name));
        }
        let file = output_name(&template, &row.name, i + 1, ext)?;
        if !seen.insert(file.clone()) {
            return fail(format!(
                "rows would overwrite each other's file {file}: put {{name}} or {{index}} in the output template"
            ));
        }
        files.push(file);
    }
    Ok(files)
}

/// Renders every row. The project is only read: variables are applied to scratch copies, and the
/// only files written are the exports.
pub fn run(ws: &Workspace, req: &BatchRequest) -> Result<BatchReport, ToolError> {
    let files = check_request(req)?;
    let project = match ws.dir() {
        Some(d) => d.to_path_buf(),
        None => return fail("the project has no folder on disk, so there is nowhere to export to"),
    };
    let doc = ws.session.document();
    let mut reports = Vec::with_capacity(req.rows.len());
    for (row, file) in req.rows.iter().zip(&files) {
        let outcome = export_row(ws, doc, &project, req, row, file);
        reports.push(match outcome {
            Ok((files, warnings)) => RowReport {
                name: row.name.clone(),
                files,
                warnings,
                error: None,
            },
            Err(e) => RowReport {
                name: row.name.clone(),
                files: Vec::new(),
                warnings: Vec::new(),
                error: Some(e.0),
            },
        });
    }
    Ok(BatchReport { rows: reports })
}

type RowOutput = (Vec<String>, Vec<String>);

fn export_row(
    ws: &Workspace,
    doc: &Document,
    project: &Path,
    req: &BatchRequest,
    row: &Row,
    file: &str,
) -> Result<RowOutput, ToolError> {
    ensure_not_linked(project, file)?;
    let (varied, mut warnings) = row_document(doc, row, req.strict)?;
    let locale = row.locale.as_deref();
    match &req.target {
        Target::Frame(frame) => {
            let scratch = ws.scratch(varied);
            let out = export_assets(
                &scratch,
                &Plan::Frame { frame, file },
                req.preset.as_deref(),
                req.scale,
                false,
                locale,
            )?;
            warnings.extend(out.warnings);
            Ok((out.files.into_iter().map(|f| f.path).collect(), warnings))
        }
        Target::Video(video) => {
            let ffmpeg = od_media::Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))?;
            let src = crate::video_export::Source {
                doc: varied,
                dir: Some(project.to_path_buf()),
            };
            let out = crate::video_export::export_video(
                &src,
                video,
                file,
                locale,
                req.preset.as_deref(),
                &ffmpeg,
                &mut |_, _| true,
            )?;
            warnings.extend(out.warnings);
            Ok((vec![out.path], warnings))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn row(name: &str, vars: Value) -> Row {
        serde_json::from_value(json!({"name": name, "variables": vars})).unwrap()
    }

    #[test]
    fn output_templates_are_confined_to_the_exports_folder() {
        assert_eq!(
            output_name("exports/{name}.png", "acme", 1, "png").unwrap(),
            "acme.png"
        );
        assert_eq!(
            output_name("exports/card-{index}-{name}.png", "a", 3, "png").unwrap(),
            "card-3-a.png"
        );
        for bad in [
            "../exports/{name}.png",
            "exports/../{name}.png",
            "/etc/{name}.png",
            "{name}.png",
            "exports/sub/{name}.png",
            "exports\\{name}.png",
            "exports/{name}.jpg",
            "exports/.{name}.png",
            "media/{name}.png",
            "C:/exports/{name}.png",
        ] {
            assert!(output_name(bad, "acme", 1, "png").is_err(), "{bad}");
        }
        assert!(output_name("exports/{name}.png", "a/b", 1, "png").is_err());
        assert!(output_name("exports/{name}.png", "..", 1, "png").is_err());
    }

    #[test]
    fn rows_files_are_size_capped_and_type_checked() {
        let rows = parse_rows(r#"[{"name": "a", "variables": {"title": "Hi", "n": 3}}]"#).unwrap();
        assert_eq!(rows[0].name, "a");
        assert_eq!(parse_rows(r#"{"rows": [{"name": "a"}]}"#).unwrap().len(), 1);
        for bad in [
            "not json",
            r#"{"rows": [], "extra": 1}"#,
            r#"[{"name": "a", "surprise": 1}]"#,
            r#"[{"variables": {}}]"#,
            r#"[{"name": 3}]"#,
            r#""rows""#,
        ] {
            assert!(parse_rows(bad).is_err(), "{bad}");
        }
        let huge = format!(r#"[{{"name": "{}"}}]"#, "a".repeat(2 * 1024 * 1024));
        assert!(parse_rows(&huge).is_err());
    }

    #[test]
    fn requests_with_bad_names_duplicates_or_colliding_outputs_are_refused_up_front() {
        let request = |rows: Vec<Row>, template: Option<&str>| BatchRequest {
            rows,
            target: Target::Frame(NodeId::parse("n_1").unwrap()),
            preset: None,
            out_template: template.map(str::to_string),
            strict: false,
            scale: None,
        };
        let a = row("a", json!({}));
        let b = row("b", json!({}));
        assert_eq!(
            check_request(&request(vec![a.clone(), b.clone()], None)).unwrap(),
            ["a.png", "b.png"]
        );
        assert!(check_request(&request(vec![], None)).is_err());
        assert!(check_request(&request(vec![a.clone(), a.clone()], None)).is_err());
        assert!(check_request(&request(vec![row("../x", json!({}))], None)).is_err());
        assert!(
            check_request(&request(
                vec![a.clone(), b.clone()],
                Some("exports/fixed.png")
            ))
            .is_err()
        );
        assert!(check_request(&request(vec![a.clone(), b], Some("exports/{index}.png"))).is_ok());
        assert!(check_request(&request(vec![a], Some("../out/{name}.png"))).is_err());
    }

    #[test]
    fn symlinked_exports_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(elsewhere.path(), tmp.path().join("exports")).unwrap();
            assert!(ensure_not_linked(tmp.path(), "a.png").is_err());
        }
        let clean = tempfile::tempdir().unwrap();
        std::fs::create_dir(clean.path().join("exports")).unwrap();
        assert!(ensure_not_linked(clean.path(), "a.png").is_ok());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                elsewhere.path().join("target.png"),
                clean.path().join("exports/a.png"),
            )
            .unwrap();
            assert!(ensure_not_linked(clean.path(), "a.png").is_err());
        }
    }
}
