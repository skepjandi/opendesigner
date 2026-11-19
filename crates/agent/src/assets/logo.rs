//! Logo work: an agent writes a mark as SVG, the editor draws it crisp for the design and exports
//! the sizes a brand needs (`docs/wiki/agent-integration.md#logo-design`). The craft lives in the
//! `logo` playbook; this is the part only the editor can do: rasterize, check, export.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::project::{self, Sidecar};
use super::raster;
use crate::tools::{Content, Output, ToolError};

pub const NAMES: &[&str] = &["import_svg", "export_icon_set"];
/// The sizes of an icon set: App Store, iPhone/iPad, Android, web, favicon.
const SIZES: &[u32] = &[1024, 512, 256, 180, 167, 152, 120, 64, 48, 32, 16];

fn fail(e: impl ToString) -> ToolError {
    ToolError(e.to_string())
}

fn out(v: &Value) -> Output {
    Output {
        content: vec![Content::Text(v.to_string())],
    }
}

pub fn definitions() -> Vec<Value> {
    let tool = |name: &str, description: &str, props: Value, required: &[&str]| {
        json!({
            "name": name, "description": description,
            "inputSchema": {"type": "object", "properties": props, "required": required, "additionalProperties": false},
            "annotations": {"readOnlyHint": false, "openWorldHint": false},
        })
    };
    vec![
        tool(
            "import_svg",
            "Put a logo or any vector drawing you wrote as SVG into the project: it is checked \
             (live text, embedded pictures, filters, masks, many colors, gradients, no viewBox are \
             reported), drawn crisp as a PNG in media/ for an image node, and the SVG itself is kept \
             as media/<name>.svg for export_icon_set. Read get_playbook logo first: construct the \
             mark from simple shapes, no <text>, black first, then color. Look at the result with \
             render_preview afterwards, also very small.",
            json!({
                "svg": {"type": "string", "description": "The whole SVG document, with a viewBox."},
                "name": {"type": "string", "description": "File name without ending."},
                "width": {"type": "integer", "minimum": 1, "maximum": 4096},
                "height": {"type": "integer", "minimum": 1, "maximum": 4096},
                "color": {"type": "string", "description": "Hex color to replace currentColor and the fills/strokes with (a one-color version)."},
                "background": {"type": "string", "description": "Hex color drawn behind the mark (the PNG is then opaque)."}
            }),
            &["svg"],
        ),
        tool(
            "export_icon_set",
            "Write the PNG sizes a brand needs from a mark saved by import_svg (media/<name>.svg) \
             into exports/<name>-icons/: 1024, 512, 256, 180, 167, 152, 120, 64, 48, 32 and 16 px. \
             With `background` the icons are opaque (the App Store rejects transparency in the \
             1024 icon); without it they keep their transparency. For a tiny size that needs a \
             simpler drawing, import that drawing as its own svg and export it with `only_sizes`.",
            json!({
                "svg_file": {"type": "string", "description": "media/<name>.svg from import_svg."},
                "name": {"type": "string", "description": "Folder name stem; default the file's name."},
                "background": {"type": "string", "description": "Hex color behind the mark."},
                "padding_percent": {"type": "integer", "minimum": 0, "maximum": 40, "description": "Empty margin around the mark, as a percent of the icon (default 0)."},
                "only_sizes": {"type": "array", "items": {"type": "integer"}, "maxItems": 12, "description": "Only these sizes (from the list above)."}
            }),
            &["svg_file"],
        ),
    ]
}

/// What is wrong with an SVG as a logo master, in plain words. Cheap string checks.
pub fn audit(svg: &str) -> Vec<String> {
    let lower = svg.to_ascii_lowercase();
    let mut notes = Vec::new();
    if !lower.contains("viewbox") {
        notes.push("no viewBox: give it one (0 0 256 256 for a symbol) so it scales".to_string());
    }
    if lower.contains("<text") {
        notes.push(
            "live <text>: draw the letters as paths so the mark does not depend on a font".into(),
        );
    }
    if lower.contains("<image") {
        notes.push("an embedded picture: a logo is vector, draw it".into());
    }
    if lower.contains("<filter") || lower.contains("<mask") || lower.contains("<clippath") {
        notes.push("a filter, mask or clip: build the shape with real geometry instead".into());
    }
    let gradients =
        lower.matches("<lineargradient").count() + lower.matches("<radialgradient").count();
    if gradients > 0 {
        notes.push(format!(
            "{gradients} gradient(s): the mark must also work in one flat color"
        ));
    }
    let colors = colors_in(&lower);
    if colors.len() > 3 {
        notes.push(format!(
            "{} colors: more than three needs a reason",
            colors.len()
        ));
    }
    notes
}

fn colors_in(lower: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let hex: String = lower[i + 1..]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if matches!(hex.len(), 3 | 6) {
                let full = if hex.len() == 3 {
                    hex.chars().flat_map(|c| [c, c]).collect()
                } else {
                    hex
                };
                if !found.contains(&full) {
                    found.push(full);
                }
            }
        }
        i += 1;
    }
    found
}

/// Inserts a filled rectangle behind everything, using the viewBox.
fn with_background(svg: &str, color: &str) -> Result<String, ToolError> {
    let lower = svg.to_ascii_lowercase();
    let vb = lower
        .find("viewbox=")
        .and_then(|at| {
            let rest = &svg[at + 8..];
            let quote = rest.chars().next()?;
            let end = rest[1..].find(quote)?;
            Some(rest[1..1 + end].to_string())
        })
        .ok_or_else(|| fail("a background needs a viewBox on the SVG"))?;
    let n: Vec<f64> = vb
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let [x, y, w, h] = n[..] else {
        return Err(fail("the viewBox must be four numbers"));
    };
    let open = svg.find("<svg").ok_or_else(|| fail("not an SVG"))?;
    let close = svg[open..].find('>').ok_or_else(|| fail("not an SVG"))? + open + 1;
    Ok(format!(
        "{}<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{color}\"/>{}",
        &svg[..close],
        &svg[close..]
    ))
}

/// Shrinks the drawing into the middle by wrapping it in a group (needs a viewBox).
fn with_padding(svg: &str, percent: u32) -> Result<String, ToolError> {
    if percent == 0 {
        return Ok(svg.to_string());
    }
    let lower = svg.to_ascii_lowercase();
    let at = lower
        .find("viewbox=")
        .ok_or_else(|| fail("padding needs a viewBox"))?;
    let rest = &svg[at + 8..];
    let quote = rest.chars().next().ok_or_else(|| fail("bad viewBox"))?;
    let end = rest[1..].find(quote).ok_or_else(|| fail("bad viewBox"))?;
    let n: Vec<f64> = rest[1..1 + end]
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let [x, y, w, h] = n[..] else {
        return Err(fail("the viewBox must be four numbers"));
    };
    let k = 1.0 - 2.0 * f64::from(percent) / 100.0;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let open = svg.find("<svg").ok_or_else(|| fail("not an SVG"))?;
    let tag_end = svg[open..].find('>').ok_or_else(|| fail("not an SVG"))? + open + 1;
    let close = svg.rfind("</svg>").ok_or_else(|| fail("not an SVG"))?;
    Ok(format!(
        "{}<g transform=\"translate({cx} {cy}) scale({k}) translate({} {})\">{}</g>{}",
        &svg[..tag_end],
        -cx,
        -cy,
        &svg[tag_end..close],
        &svg[close..]
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportArgs {
    svg: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    background: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportArgs {
    svg_file: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    padding_percent: Option<u32>,
    #[serde(default)]
    only_sizes: Option<Vec<u32>>,
}

fn color_ok(c: &str) -> Result<(), ToolError> {
    if raster::is_hex_color(c) {
        Ok(())
    } else {
        Err(fail(format!("{c:?} is not a hex color like #1a2b3c")))
    }
}

pub fn call(project: Option<&Path>, name: &str, a: Value) -> Result<Output, ToolError> {
    let dir = project.ok_or_else(|| fail("the project has no folder"))?;
    match name {
        "import_svg" => import(dir, serde_json::from_value(a).map_err(fail)?),
        "export_icon_set" => export(dir, serde_json::from_value(a).map_err(fail)?),
        other => Err(fail(format!("no tool {other}"))),
    }
}

fn import(dir: &Path, a: ImportArgs) -> Result<Output, ToolError> {
    let notes = audit(&a.svg);
    let mut svg = a.svg.clone();
    if let Some(c) = &a.color {
        color_ok(c)?;
        svg = raster::recolor(&svg, c);
    }
    if let Some(b) = &a.background {
        color_ok(b)?;
        svg = with_background(&svg, b)?;
    }
    let (png, w, h) = raster::rasterize(&svg, a.width, a.height).map_err(fail)?;
    let stem = a.name.unwrap_or_else(|| "logo".into());
    let sidecar = Sidecar {
        id: "authored:svg".into(),
        source: "written by the agent".into(),
        licence: "the user's own work".into(),
        licence_url: String::new(),
        attribution_required: false,
        attribution: String::new(),
        recipe: None,
    };
    let placed = project::place(dir, &stem, "png", &png, &sidecar, Some((w, h))).map_err(fail)?;
    // keep the master next to the picture, same name
    let name = placed
        .file
        .trim_start_matches("media/")
        .trim_end_matches(".png")
        .to_string();
    let svg_path = dir.join("media").join(format!("{name}.svg"));
    std::fs::write(&svg_path, svg.as_bytes()).map_err(fail)?;
    Ok(out(&json!({
        "file": placed.file, "svg_file": format!("media/{name}.svg"),
        "width": w, "height": h, "checks": notes,
        "next": "Put the PNG in an image node, render_preview it at 16 to 64 px and in one color, fix what fails, then export_icon_set.",
    })))
}

fn export(dir: &Path, a: ExportArgs) -> Result<Output, ToolError> {
    let file = a.svg_file.strip_prefix("media/").unwrap_or(&a.svg_file);
    if file.contains(['/', '\\']) || file.starts_with('.') || !file.ends_with(".svg") {
        return Err(fail("svg_file is media/<name>.svg from import_svg"));
    }
    let src = dir.join("media").join(file);
    let meta = std::fs::symlink_metadata(&src).map_err(fail)?;
    if !meta.is_file() {
        return Err(fail("that is not a file in media/"));
    }
    let mut svg = std::fs::read_to_string(&src).map_err(fail)?;
    svg = with_padding(&svg, a.padding_percent.unwrap_or(0).min(40))?;
    if let Some(b) = &a.background {
        color_ok(b)?;
        svg = with_background(&svg, b)?;
    }
    let stem = project::clean_stem(a.name.as_deref().unwrap_or(file.trim_end_matches(".svg")));
    if stem.is_empty() {
        return Err(fail("the name needs letters or digits"));
    }
    let target = dir.join("exports").join(format!("{stem}-icons"));
    std::fs::create_dir_all(&target).map_err(fail)?;
    let sizes: Vec<u32> = match &a.only_sizes {
        Some(list) => list.iter().copied().filter(|s| SIZES.contains(s)).collect(),
        None => SIZES.to_vec(),
    };
    if sizes.is_empty() {
        return Err(fail(format!("sizes are among {SIZES:?}")));
    }
    let mut written = Vec::new();
    for s in sizes {
        let (png, w, h) = raster::rasterize(&svg, Some(s), Some(s)).map_err(fail)?;
        let path = target.join(format!("icon-{s}.png"));
        std::fs::write(&path, png).map_err(fail)?;
        written.push(
            json!({"path": format!("exports/{stem}-icons/icon-{s}.png"), "width": w, "height": h}),
        );
    }
    Ok(out(&json!({
        "files": written,
        "note": if a.background.is_some() { "opaque icons" } else { "transparent icons: the App Store 1024 icon must be opaque, pass `background`" },
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><circle cx="128" cy="128" r="96" fill="#0f7c80"/><rect x="108" y="48" width="40" height="160" fill="#ffffff"/></svg>"##;

    #[test]
    fn the_audit_names_what_a_logo_master_should_not_have() {
        assert!(audit(MARK).is_empty(), "{:?}", audit(MARK));
        let bad = r##"<svg xmlns="http://www.w3.org/2000/svg"><text>Hi</text><image href="x"/><filter id="f"/><linearGradient id="g"/><rect fill="#111"/><rect fill="#222"/><rect fill="#333"/><rect fill="#444"/></svg>"##;
        let n = audit(bad).join("\n");
        for needle in [
            "no viewBox",
            "live <text>",
            "embedded picture",
            "filter",
            "gradient",
            "4 colors",
        ] {
            assert!(n.contains(needle), "{needle} in {n}");
        }
    }

    #[test]
    fn a_mark_is_imported_checked_and_exported_as_an_icon_set() {
        let d = tempfile::tempdir().unwrap();
        let r = import(
            d.path(),
            ImportArgs {
                svg: MARK.into(),
                name: Some("Harbor".into()),
                width: Some(256),
                height: None,
                color: None,
                background: None,
            },
        )
        .unwrap();
        let Content::Text(t) = &r.content[0] else {
            panic!()
        };
        let v: Value = serde_json::from_str(t).unwrap();
        assert_eq!(v["file"], "media/harbor.png");
        assert_eq!(v["svg_file"], "media/harbor.svg");
        assert!(d.path().join("media/harbor.svg").is_file());

        let r = export(
            d.path(),
            ExportArgs {
                svg_file: "media/harbor.svg".into(),
                name: None,
                background: Some("#102030".into()),
                padding_percent: Some(10),
                only_sizes: Some(vec![1024, 16, 999]),
            },
        )
        .unwrap();
        let Content::Text(t) = &r.content[0] else {
            panic!()
        };
        let v: Value = serde_json::from_str(t).unwrap();
        assert_eq!(
            v["files"].as_array().unwrap().len(),
            2,
            "999 is not an icon size"
        );
        let big = std::fs::read(d.path().join("exports/harbor-icons/icon-1024.png")).unwrap();
        assert!(big.starts_with(&[0x89, b'P', b'N', b'G']));
        // refusals
        assert!(
            export(
                d.path(),
                ExportArgs {
                    svg_file: "media/../x.svg".into(),
                    name: None,
                    background: None,
                    padding_percent: None,
                    only_sizes: None
                }
            )
            .is_err()
        );
        assert!(
            import(
                d.path(),
                ImportArgs {
                    svg: MARK.into(),
                    name: None,
                    width: None,
                    height: None,
                    color: Some("red".into()),
                    background: None
                }
            )
            .is_err()
        );
    }
}
