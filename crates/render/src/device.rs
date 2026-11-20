//! Device3d nodes in a frame: the screen picture is prepared (a capture fitted to the screen, or
//! a recording's picture at this moment), the device is drawn by [`crate::three_d`], and the
//! result is kept, because the editor redraws the frame after every edit and most edits do not
//! touch the device.

use std::collections::{HashMap, VecDeque};
use std::path::Path as FsPath;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use od_core::{ImageFit, Node, NodeId, NodeKind, Rgba};
use tiny_skia::Pixmap;

use crate::three_d::{Scene, render_device, screen_texture_size};
use crate::video::FootageSource;
use crate::{RenderError, Warning};

/// Drawn devices kept between renders.
const CACHE_BYTES: usize = 192 * 1024 * 1024;

/// The finish of a body with no fill: dark graphite.
const DEFAULT_BODY: Rgba = Rgba {
    r: 0x2b,
    g: 0x2d,
    b: 0x31,
    a: 0xff,
};

/// What a frame's screens may play beyond the project's images.
#[derive(Default)]
pub struct Media<'a> {
    /// Where pictures of recordings come from (`od_media` runs ffmpeg). Without one, a device
    /// whose screen plays a recording shows its `image` instead and the render says so.
    pub footage: Option<&'a mut dyn FootageSource>,
    /// How long the frame has been on screen in a video, in milliseconds: a recording on a
    /// screen plays from its `in_ms` plus this. 0 for a still.
    pub played_ms: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<String, Arc<Pixmap>>,
    order: VecDeque<String>,
    bytes: usize,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

impl Cache {
    fn get(&mut self, key: &str) -> Option<Arc<Pixmap>> {
        let hit = self.entries.get(key).cloned()?;
        self.order.retain(|k| k != key);
        self.order.push_back(key.to_string());
        Some(hit)
    }

    fn insert(&mut self, key: String, pixmap: Arc<Pixmap>) {
        self.bytes += pixmap.data().len();
        self.order.push_back(key.clone());
        if let Some(old) = self.entries.insert(key, pixmap) {
            self.bytes -= old.data().len();
        }
        while self.bytes > CACHE_BYTES && self.order.len() > 1 {
            if let Some(oldest) = self.order.pop_front()
                && let Some(gone) = self.entries.remove(&oldest)
            {
                self.bytes -= gone.data().len();
            }
        }
    }
}

/// Draws every visible device3d under `node` at `scale`, keyed by node id.
pub(crate) fn prepare(
    node: &Node,
    scale: f32,
    tokens: &std::collections::BTreeMap<String, String>,
    project_dir: Option<&FsPath>,
    media: &mut Media,
    warnings: &mut Vec<Warning>,
) -> Result<HashMap<NodeId, Arc<Pixmap>>, RenderError> {
    let mut out = HashMap::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if !n.visible {
            continue;
        }
        if n.kind == NodeKind::Device3d
            && let Some(pixmap) = draw_device(n, scale, tokens, project_dir, media, warnings)?
        {
            out.insert(n.id.clone(), pixmap);
        }
        stack.extend(n.children.iter());
    }
    Ok(out)
}

fn draw_device(
    node: &Node,
    scale: f32,
    tokens: &std::collections::BTreeMap<String, String>,
    project_dir: Option<&FsPath>,
    media: &mut Media,
    warnings: &mut Vec<Warning>,
) -> Result<Option<Arc<Pixmap>>, RenderError> {
    let Some(device) = &node.device3d else {
        return Ok(None);
    };
    let (w, h) = (
        (node.width * f64::from(scale)).ceil() as u32,
        (node.height * f64::from(scale)).ceil() as u32,
    );
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let body = match &node.fill {
        Some(fill) => fill.resolve(tokens)?,
        None => DEFAULT_BODY,
    };
    let screen = screen_source(node, device, project_dir, media, warnings);
    let (tw, th) = match screen {
        // A recording is streamed by a reader per picture size, so a zoom that animates would
        // restart the reader every frame; ask for sizes in steps of a quarter instead.
        Some(ScreenSource::Recording { .. }) => {
            let step = 1.25_f64.powf(device.zoom.max(0.05).log(1.25).ceil());
            let stepped = od_core::Device3d {
                zoom: step,
                ..device.clone()
            };
            screen_texture_size(&stepped, w, h)
        }
        _ => screen_texture_size(device, w, h),
    };
    let key = format!(
        "{device:?}|{}|{}|{w}x{h}|{tw}x{th}",
        body.to_hex(),
        screen.as_ref().map_or_else(String::new, ScreenSource::key),
    );
    if let Some(hit) = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return Ok(Some(hit));
    }
    let texture = match screen {
        Some(source) => source.texture(node, device.screen_scroll, tw, th, media, warnings),
        None => None,
    };
    let drawn = render_device(&Scene {
        device,
        body,
        screen: texture.as_ref(),
        width: w,
        height: h,
    })
    .ok_or(RenderError::TooLarge {
        width: u64::from(w),
        height: u64::from(h),
    })?;
    let drawn = Arc::new(drawn);
    CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, Arc::clone(&drawn));
    Ok(Some(drawn))
}

/// What a screen shows, named well enough to key the cache without decoding it.
enum ScreenSource {
    Recording {
        file: String,
        at_ms: u64,
    },
    Capture {
        path: std::path::PathBuf,
        stamp: String,
    },
}

impl ScreenSource {
    fn key(&self) -> String {
        match self {
            ScreenSource::Recording { file, at_ms } => format!("rec:{file}@{at_ms}"),
            ScreenSource::Capture { path, stamp } => format!("img:{}:{stamp}", path.display()),
        }
    }

    fn texture(
        self,
        node: &Node,
        scroll: Option<f64>,
        w: u32,
        h: u32,
        media: &mut Media,
        warnings: &mut Vec<Warning>,
    ) -> Option<Pixmap> {
        let mut fail = |reason: String| {
            warnings.push(Warning::ImageNotRendered {
                id: node.id.clone(),
                reason,
            });
            None
        };
        match self {
            ScreenSource::Recording { file, at_ms } => {
                let source = media.footage.as_deref_mut()?;
                match source.frame(&file, at_ms, w, h) {
                    Ok(rgba) => {
                        let mut pixmap = Pixmap::new(w, h)?;
                        if rgba.len() != pixmap.data().len() {
                            return fail(format!("{file} gave a picture of the wrong size"));
                        }
                        pixmap.data_mut().copy_from_slice(&rgba);
                        Some(pixmap)
                    }
                    Err(e) => fail(format!(
                        "the recording on its screen could not be read: {e}"
                    )),
                }
            }
            ScreenSource::Capture { path, .. } => match crate::images::load(&path) {
                Ok(image) => crate::images::fit_into(
                    &image,
                    node.image_fit.unwrap_or(ImageFit::Cover),
                    scroll,
                    w,
                    h,
                ),
                Err(e) => fail(e),
            },
        }
    }
}

/// The recording when it can be played here, else the capture, else nothing (screen off).
fn screen_source(
    node: &Node,
    device: &od_core::Device3d,
    project_dir: Option<&FsPath>,
    media: &Media,
    warnings: &mut Vec<Warning>,
) -> Option<ScreenSource> {
    if let Some(f) = &device.footage {
        if media.footage.is_some() {
            return Some(ScreenSource::Recording {
                file: f.file.clone(),
                at_ms: f.in_ms + media.played_ms,
            });
        }
        warnings.push(Warning::ImageNotRendered {
            id: node.id.clone(),
            reason: format!(
                "the recording {} on its screen needs ffmpeg to be shown here; {}",
                f.file,
                if node.image.is_some() {
                    "its image is shown instead"
                } else {
                    "the screen is shown off"
                }
            ),
        });
    }
    let reference = node.image.as_deref()?;
    let resolved = project_dir
        .ok_or_else(|| "the document has no project folder to read media from".to_string())
        .and_then(|dir| od_core::media::resolve(dir, reference).map_err(|e| e.to_string()));
    match resolved {
        Ok(path) => {
            let stamp = std::fs::metadata(&path)
                .map(|m| {
                    let modified = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos());
                    format!("{}:{modified}", m.len())
                })
                .unwrap_or_default();
            Some(ScreenSource::Capture { path, stamp })
        }
        Err(reason) => {
            warnings.push(Warning::ImageNotRendered {
                id: node.id.clone(),
                reason,
            });
            None
        }
    }
}
