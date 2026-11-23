//! Files made to be sent to someone, rather than uploaded to a store: a JPEG, a PDF with a page
//! per frame, or one self-contained HTML page. No store rules apply, so there is no preset;
//! `export` stays the tool for store assets and its PNG-only rules.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_core::{Document, Node, NodeId};
use serde::Deserialize;
use serde_json::json;

use crate::export::exports_dir;
use crate::tools::{CallCtx, Output, ToolError, node_id, parse, tool};

pub const NAMES: [&str; 1] = ["export_share"];

const MAX_PAGES: usize = 50;
const JPEG_QUALITY: u8 = 92;
const MAX_ALT_CHARS: usize = 600;

pub fn definitions() -> Vec<serde_json::Value> {
    vec![tool(
        "export_share",
        "Render frames to one file to send to someone, in exports/: .jpg (one frame), .pdf (one \
         page per frame, in the order given) or .html (one self-contained page: the frames as \
         pictures with their text as alt text, no external files). Raster output at the frame's \
         size times `scale`. For App Store or Google Play files use export, which checks the \
         store's rules.",
        json!({
            "frames": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 50, "description": "ids of frame nodes"},
            "file": {"type": "string", "description": "plain file name ending in .jpg, .jpeg, .pdf or .html"},
            "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
            "title": {"type": "string", "description": "page title for .html and .pdf readers; default the project name"},
            "locale": {"type": "string", "description": "a language from list_locales"}
        }),
        &["frames", "file"],
        false,
    )]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    frames: Vec<String>,
    file: String,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Jpeg,
    Pdf,
    Html,
}

fn kind_of(file: &str) -> Result<Kind, ToolError> {
    let plain = file.len() <= 100
        && !file.contains("..")
        && file
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    let kind = match file.rsplit('.').next() {
        Some("jpg" | "jpeg") => Some(Kind::Jpeg),
        Some("pdf") => Some(Kind::Pdf),
        Some("html") => Some(Kind::Html),
        _ => None,
    };
    match (plain, kind) {
        (true, Some(kind)) => Ok(kind),
        _ => Err(ToolError(
            "file must be a plain name ending in .jpg, .jpeg, .pdf or .html (letters, digits . _ -)".into(),
        )),
    }
}

fn collect_text(node: &Node, out: &mut Vec<String>) {
    if let Some(text) = node
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        out.push(text.to_string());
    }
    for child in &node.children {
        collect_text(child, out);
    }
}

/// The words in a frame, in tree order, for the page's alt text.
fn alt_text(doc: &Document, frame: &NodeId) -> String {
    let mut words = Vec::new();
    if let Some(node) = doc.node(frame) {
        collect_text(node, &mut words);
    }
    let joined = words.join(" / ");
    joined.chars().take(MAX_ALT_CHARS).collect()
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            c if c.is_control() && c != ' ' => " ".to_string(),
            c => c.to_string(),
        })
        .collect()
}

fn html_page(title: &str, pages: &[(Vec<u8>, String)]) -> String {
    let mut body = String::new();
    for (png, alt) in pages {
        body.push_str(&format!(
            "<figure><img alt=\"{}\" src=\"data:image/png;base64,{}\"></figure>\n",
            escape(alt),
            STANDARD.encode(png)
        ));
    }
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>body{{margin:0;padding:24px;background:#f4f4f4;display:flex;flex-direction:column;align-items:center;gap:24px}}figure{{margin:0;max-width:100%}}img{{display:block;max-width:100%;height:auto;box-shadow:0 2px 16px rgba(0,0,0,.15)}}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape(title)
    )
}

pub fn call(ctx: &mut CallCtx, _name: &str, args: serde_json::Value) -> Result<Output, ToolError> {
    let a: Args = parse(args)?;
    let kind = kind_of(&a.file)?;
    if a.frames.is_empty() || a.frames.len() > MAX_PAGES {
        return Err(ToolError(format!(
            "export 1 to {MAX_PAGES} frames, not {}",
            a.frames.len()
        )));
    }
    if kind == Kind::Jpeg && a.frames.len() != 1 {
        return Err(ToolError(
            "a JPEG holds one frame; use .pdf or .html for several".into(),
        ));
    }
    let ids: Vec<NodeId> = a
        .frames
        .iter()
        .map(|f| node_id(f))
        .collect::<Result<_, _>>()?;
    let dir = exports_dir(ctx.ws)?;
    let localized;
    let doc = match a.locale.as_deref() {
        Some(l) => {
            localized =
                ctx.ws.session.document().localized(l).ok_or_else(|| {
                    ToolError(format!("the project has no translations for {l:?}"))
                })?;
            &localized
        }
        None => ctx.ws.session.document(),
    };
    let scale = a.scale.unwrap_or(1.0);
    let mut warnings = Vec::new();
    let mut pixmaps = Vec::new();
    for id in &ids {
        let rendered = crate::video_export::render_still(doc, id, scale, ctx.ws.dir())?;
        warnings.extend(rendered.warnings.iter().map(ToString::to_string));
        pixmaps.push(rendered.pixmap);
    }
    let title = a.title.clone().unwrap_or_else(|| doc.name.clone());
    let bytes = match kind {
        Kind::Jpeg => od_render::encode_jpeg(&pixmaps[0], JPEG_QUALITY)?,
        Kind::Pdf => od_render::encode_pdf(&pixmaps)?,
        Kind::Html => {
            let mut pages = Vec::new();
            for (id, pixmap) in ids.iter().zip(&pixmaps) {
                pages.push((od_render::encode_png(pixmap, true)?, alt_text(doc, id)));
            }
            html_page(&title, &pages).into_bytes()
        }
    };
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(&a.file), &bytes)?;
    Ok(Output::json(&json!({
        "path": format!("exports/{}", a.file),
        "bytes": bytes.len(),
        "pages": pixmaps.len(),
        "warnings": warnings,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_names_with_a_share_extension_pass() {
        assert!(kind_of("deck.pdf").is_ok());
        assert!(kind_of("a-b_1.jpeg").is_ok());
        assert!(kind_of("page.html").is_ok());
        for bad in [
            "x.png",
            "../x.pdf",
            "a/b.pdf",
            ".pdf",
            "x.pdf.exe",
            "a b.pdf",
            "x",
        ] {
            assert!(kind_of(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn html_escapes_text_and_embeds_the_picture() {
        let page = html_page("A <b> & \"c\"", &[(vec![1, 2, 3], "x\" onload=\"y".into())]);
        assert!(page.contains("<title>A &lt;b&gt; &amp; &quot;c&quot;</title>"));
        assert!(page.contains("alt=\"x&quot; onload=&quot;y\""));
        assert!(page.contains("data:image/png;base64,AQID"));
        assert!(!page.contains("<script"));
    }
}
