//! Exports into `<project>/exports/`: one frame, a set of frames, or one wide frame cut into the
//! slides of a panorama. Everything is rendered and validated against the store preset *before*
//! anything is written, so a set that breaks a rule leaves no files behind. Shared by the MCP tools,
//! the editor and the CLI.

use std::path::PathBuf;

use od_core::{NodeId, Workspace};
use od_render::Pixmap;

use crate::tools::ToolError;

/// Largest number of files one export writes.
pub const MAX_SET_FILES: usize = 50;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

/// File names only: `shot-01.png`. No separators, no leading dot, no traversal.
pub(crate) fn export_name(file: &str) -> Result<&str, ToolError> {
    let ok = file.len() <= 100
        && file.ends_with(".png")
        && !file.contains("..")
        && file
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(file)
    } else {
        fail("file must be a plain name like shot-01.png (letters, digits . _ -)")
    }
}

pub(crate) fn exports_dir(ws: &Workspace) -> Result<PathBuf, ToolError> {
    match ws.dir() {
        Some(d) => Ok(d.join("exports")),
        None => fail("the project has no folder on disk, so there is nowhere to export to"),
    }
}

pub(crate) fn preset(id: &str) -> Result<&'static od_store::Preset, ToolError> {
    od_store::find(id).ok_or_else(|| {
        let ids: Vec<&str> = od_store::PRESETS.iter().map(|p| p.id).collect();
        ToolError(format!(
            "unknown preset {id:?}; available: {}",
            ids.join(", ")
        ))
    })
}

/// What to export.
pub enum Plan<'a> {
    /// One frame to `exports/<file>`.
    Frame { frame: &'a NodeId, file: &'a str },
    /// Several frames to `exports/<prefix>-01.png`, `-02`, ... in the order given.
    Set {
        frames: &'a [NodeId],
        prefix: &'a str,
    },
    /// One wide frame cut into `slices` equal columns, `exports/<prefix>-01.png`, ...
    Panorama {
        frame: &'a NodeId,
        slices: u32,
        prefix: &'a str,
    },
}

pub struct ExportedFile {
    /// Relative to the project folder, for example `exports/shot-01.png`.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
}

pub struct ExportOutcome {
    pub files: Vec<ExportedFile>,
    /// Things that did not come out as designed: an image that could not be drawn, text that
    /// overflows its box.
    pub warnings: Vec<String>,
    /// Whether the preset's rules were checked against Apple's or Google's own pages.
    pub preset_verified: Option<bool>,
}

struct Pending {
    file: String,
    png: Vec<u8>,
    width: u32,
    height: u32,
}

fn stem(prefix: &str, locale: Option<&str>) -> String {
    match locale {
        Some(l) => format!("{prefix}-{l}"),
        None => prefix.to_string(),
    }
}

fn numbered(prefix: &str, count: usize) -> Result<Vec<String>, ToolError> {
    if count == 0 || count > MAX_SET_FILES {
        return fail(format!(
            "export 1 to {MAX_SET_FILES} files at a time, not {count}"
        ));
    }
    (1..=count)
        .map(|n| export_name(&format!("{prefix}-{n:02}.png")).map(str::to_string))
        .collect()
}

/// Renders `plan`, validates every image against `preset_id` (and, for sets and panoramas, the
/// count rule), and only then writes the files. With `locale`, text nodes show that language's
/// translations, and the files of a set or panorama are named `<prefix>-<locale>-01.png`, ....
pub fn export_assets(
    ws: &Workspace,
    plan: &Plan,
    preset_id: Option<&str>,
    scale: Option<f32>,
    alpha: bool,
    locale: Option<&str>,
) -> Result<ExportOutcome, ToolError> {
    let dir = exports_dir(ws)?;
    let target = preset_id.map(preset).transpose()?;
    let scale = scale.unwrap_or(1.0);
    let localized;
    let doc = match locale {
        Some(l) => {
            localized = ws.session.document().localized(l).ok_or_else(|| {
                ToolError(format!(
                    "the project has no translations for {l:?}; languages: {}",
                    ws.session
                        .document()
                        .locales
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
            &localized
        }
        None => ws.session.document(),
    };
    let render = |frame: &NodeId| od_render::render_frame(doc, frame, scale, ws.dir());

    let mut warnings = Vec::new();
    // a frame with no background is transparent; an export that flattens it is white there
    let mut notes: Vec<String> = Vec::new();
    let note_transparency = |frame: &NodeId, pixmap: &Pixmap, notes: &mut Vec<String>| {
        let share = od_render::transparent_share(pixmap);
        if share > 0.02 {
            notes.push(format!(
                "{} has no background: {:.0}% of it is transparent, so the file is {} there. Give the frame a fill, or export with alpha.",
                frame,
                share * 100.0,
                if alpha { "transparent" } else { "white" }
            ));
        }
    };
    let (names, images): (Vec<String>, Vec<Pixmap>) = match plan {
        Plan::Frame { frame, file } => {
            let name = export_name(file)?.to_string();
            let rendered = render(frame)?;
            warnings.extend(rendered.warnings);
            note_transparency(frame, &rendered.pixmap, &mut notes);
            (vec![name], vec![rendered.pixmap])
        }
        Plan::Set { frames, prefix } => {
            let names = numbered(&stem(prefix, locale), frames.len())?;
            let mut images = Vec::new();
            for frame in *frames {
                let rendered = render(frame)?;
                warnings.extend(rendered.warnings);
                note_transparency(frame, &rendered.pixmap, &mut notes);
                images.push(rendered.pixmap);
            }
            (names, images)
        }
        Plan::Panorama {
            frame,
            slices,
            prefix,
        } => {
            let names = numbered(&stem(prefix, locale), *slices as usize)?;
            let rendered = render(frame)?;
            warnings.extend(rendered.warnings);
            note_transparency(frame, &rendered.pixmap, &mut notes);
            (names, od_render::slice_columns(&rendered.pixmap, *slices)?)
        }
    };

    let keep_alpha = alpha && target.is_none_or(|p| p.allow_alpha);
    let mut pending = Vec::with_capacity(images.len());
    for (file, pixmap) in names.into_iter().zip(&images) {
        let png = od_render::encode_png(pixmap, keep_alpha)?;
        pending.push(Pending {
            file,
            png,
            width: pixmap.width(),
            height: pixmap.height(),
        });
    }

    if let Some(p) = target {
        let mut facts = Vec::with_capacity(pending.len());
        for item in &pending {
            facts.push(od_store::inspect_png(&item.png).map_err(|e| ToolError(e.to_string()))?);
        }
        let found: Vec<(Option<usize>, od_store::Violation)> = match plan {
            Plan::Frame { .. } => facts
                .iter()
                .flat_map(|f| p.validate_image(f))
                .map(|v| (Some(0), v))
                .collect(),
            Plan::Set { .. } | Plan::Panorama { .. } => p.validate_set(&facts),
        };
        if !found.is_empty() {
            let list: Vec<String> = found
                .iter()
                .map(|(i, v)| match i {
                    Some(i) => format!("- {}: {v}", pending[*i].file),
                    None => format!("- the set: {v}"),
                })
                .collect();
            return fail(format!(
                "not written: the export does not meet {} ({}):\n{}",
                p.id,
                p.source,
                list.join("\n")
            ));
        }
    }

    std::fs::create_dir_all(&dir)?;
    let mut written: Vec<PathBuf> = Vec::new();
    for item in &pending {
        let path = dir.join(&item.file);
        if let Err(e) = std::fs::write(&path, &item.png) {
            for done in &written {
                let _ = std::fs::remove_file(done);
            }
            return Err(e.into());
        }
        written.push(path);
    }
    Ok(ExportOutcome {
        files: pending
            .iter()
            .map(|i| ExportedFile {
                path: format!("exports/{}", i.file),
                width: i.width,
                height: i.height,
                bytes: i.png.len(),
            })
            .collect(),
        warnings: warnings
            .iter()
            .map(ToString::to_string)
            .chain(notes)
            .collect(),
        preset_verified: target
            .map(|p| matches!(p.verification, od_store::Verification::PrimaryChecked(_))),
    })
}
