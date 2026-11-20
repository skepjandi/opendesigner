//! The canvas picture the editor keeps between edits, without Qt.
//!
//! The picture is a [`Retained`] frame: after an edit only the tiles the touched nodes reach are
//! drawn again, and the pixels go to the window as raw premultiplied RGBA (no PNG round trip).
//! The picture is reused only while it was drawn for the same project, frame, scale and language;
//! anything else, and anything the renderer cannot tell from the touched ids, draws it again.

use std::path::{Path, PathBuf};

use od_core::{Document, NodeId, Workspace};
use od_render::{Refresh, Retained};

/// What a draw did, for the status line and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    /// The whole frame was drawn.
    Full,
    /// Only `redrawn` of `tiles` tiles were drawn again.
    Tiles { redrawn: usize, tiles: usize },
    /// No pixel changed since the last draw.
    Reused,
}

#[derive(Debug, Clone, PartialEq)]
struct Key {
    dir: Option<PathBuf>,
    frame: NodeId,
    scale: u32,
    language: Option<String>,
}

struct Kept {
    key: Key,
    log_len: u64,
    retained: Retained,
}

/// The kept picture of the open frame, if there is one.
#[derive(Default)]
pub struct Canvas {
    kept: Option<Kept>,
}

/// What a draw is for: the frame of `doc` (the document as the canvas shows it, a translation
/// applied) at `scale`.
pub struct Request<'a> {
    pub ws: &'a Workspace,
    pub doc: &'a Document,
    pub language: Option<&'a str>,
    pub frame: &'a NodeId,
    pub scale: f32,
}

impl Canvas {
    pub fn forget(&mut self) {
        self.kept = None;
    }

    /// Draws the requested frame and hands the premultiplied RGBA pixels to `sink`. `full` draws
    /// the frame whole when the picture cannot be kept (a 3D device, a recording on a screen).
    pub fn draw(
        &mut self,
        request: &Request,
        full: impl FnOnce() -> Result<od_render::Rendered, od_render::RenderError>,
        sink: &mut dyn FnMut(u32, u32, &[u8]),
    ) -> Result<Drawn, String> {
        let Request {
            ws,
            doc,
            language,
            frame,
            scale,
        } = *request;
        let key = Key {
            dir: ws.dir().map(Path::to_path_buf),
            frame: frame.clone(),
            scale: scale.to_bits(),
            language: language.map(str::to_string),
        };
        let log_len = ws.session.log().len() as u64;
        if let Some(drawn) = self.refresh(request, &key, log_len)? {
            self.publish(sink);
            return Ok(drawn);
        }
        self.kept = None;
        if !plays_recording(doc, frame) {
            let retained =
                Retained::render(doc, frame, scale, ws.dir()).map_err(|e| e.to_string())?;
            if let Some(retained) = retained {
                self.kept = Some(Kept {
                    key,
                    log_len,
                    retained,
                });
                self.publish(sink);
                return Ok(Drawn::Full);
            }
        }
        let rendered = full().map_err(|e| e.to_string())?;
        sink(
            rendered.pixmap.width(),
            rendered.pixmap.height(),
            rendered.pixmap.data(),
        );
        Ok(Drawn::Full)
    }

    /// Brings a kept picture up to date; `None` when there is none or it must be drawn again.
    fn refresh(
        &mut self,
        request: &Request,
        key: &Key,
        log_len: u64,
    ) -> Result<Option<Drawn>, String> {
        let Request { ws, doc, .. } = *request;
        let Some(kept) = self.kept.as_mut() else {
            return Ok(None);
        };
        if &kept.key != key || kept.log_len > log_len {
            return Ok(None);
        }
        let changed = ws.session.touched_since(kept.log_len);
        let refresh = kept
            .retained
            .update(doc, &changed, ws.dir())
            .map_err(|e| e.to_string())?;
        kept.log_len = log_len;
        Ok(match refresh {
            Refresh::Stale => None,
            Refresh::Unchanged => Some(Drawn::Reused),
            Refresh::Redrawn { redrawn, tiles } => Some(Drawn::Tiles { redrawn, tiles }),
        })
    }

    fn publish(&self, sink: &mut dyn FnMut(u32, u32, &[u8])) {
        if let Some(kept) = &self.kept {
            let pixmap = kept.retained.pixmap();
            sink(pixmap.width(), pixmap.height(), pixmap.data());
        }
    }
}

fn plays_recording(doc: &Document, frame: &NodeId) -> bool {
    doc.node(frame)
        .is_some_and(od_render::video::plays_recording)
}
