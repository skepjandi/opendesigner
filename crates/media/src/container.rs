//! The files a video can be written as, and the ffmpeg arguments that make each
//! (`docs/wiki/video-editing.md#formats`): an H.264 MP4, a GIF, a VP9 WebM and a ProRes MOV, the
//! last three with an alpha channel when the picture has one.

use crate::{EncodeSettings, Ffmpeg, MediaError, bitrate};

/// What kind of file an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Container {
    /// H.264 in an MP4: plays everywhere, never transparent.
    #[default]
    Mp4,
    /// An animated GIF: 256 colours, one bit of transparency, no sound.
    Gif,
    /// VP9 in a WebM: transparent in browsers, no sound here.
    Webm,
    /// Apple ProRes in a MOV (4444 with alpha, 422 HQ without): for other editors, no sound here.
    Mov,
}

impl Container {
    pub const EXTENSIONS: [&'static str; 4] = ["mp4", "gif", "webm", "mov"];

    /// The container a file name asks for, from its ending.
    pub fn of_file(name: &str) -> Option<Container> {
        match name.rsplit_once('.')?.1 {
            "mp4" => Some(Container::Mp4),
            "gif" => Some(Container::Gif),
            "webm" => Some(Container::Webm),
            "mov" => Some(Container::Mov),
            _ => None,
        }
    }

    /// Whether a file of this kind can hold transparency.
    pub fn has_alpha(self) -> bool {
        !matches!(self, Container::Mp4)
    }

    /// Whether the sound of a video can be mixed into this kind of file.
    pub fn has_sound(self) -> bool {
        matches!(self, Container::Mp4)
    }

    fn muxer(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Gif => "gif",
            Container::Webm => "webm",
            Container::Mov => "mov",
        }
    }
}

/// Picture data with the colour multiplied by its alpha (what the renderer draws) turned back
/// into straight RGBA, which is what ffmpeg's `rgba` input means.
pub fn unpremultiply(premultiplied: &[u8]) -> Vec<u8> {
    let mut out = premultiplied.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    out
}

/// A GIF's colours are chosen from the whole picture sequence, so ffmpeg keeps every frame in
/// memory until the last one: this is the most it may hold, in bytes of scaled RGBA.
pub const MAX_GIF_BYTES: u64 = 512 << 20;

/// The size a picture is written at: the video's own, or scaled to `scale_width` with an even
/// height.
pub fn output_size(settings: &EncodeSettings) -> (u32, u32) {
    match settings.scale_width {
        Some(w) if w != settings.width && settings.width > 0 => {
            let h = (u64::from(w) * u64::from(settings.height) / u64::from(settings.width)) as u32;
            (w, (h + 1) & !1)
        }
        _ => (settings.width, settings.height),
    }
}

/// Everything after `-an` up to the output path, and the name of the encoder it uses.
pub(crate) fn output_args(
    ffmpeg: &Ffmpeg,
    settings: &EncodeSettings,
) -> Result<(Vec<String>, &'static str), MediaError> {
    let s = |v: &str| v.to_string();
    let rate = f64::from(settings.fps) / f64::from(settings.every_nth.max(1));
    let (out_w, _) = output_size(settings);
    let scale = (out_w != settings.width).then(|| format!("scale={out_w}:-2:flags=lanczos"));
    let mut args: Vec<String> = Vec::new();
    let encoder = match settings.container {
        Container::Mp4 => {
            let encoder = ffmpeg.h264_encoder()?;
            args.extend([s("-c:v"), s(encoder), s("-b:v")]);
            args.push(bitrate(settings.width, settings.height, rate.ceil() as u32).to_string());
            args.extend([s("-pix_fmt"), s("yuv420p"), s("-movflags"), s("+faststart")]);
            encoder
        }
        Container::Gif => {
            let palette = if settings.alpha {
                "palettegen=reserve_transparent=1:stats_mode=diff"
            } else {
                "palettegen=stats_mode=diff"
            };
            let apply = if settings.alpha {
                "paletteuse=dither=bayer:bayer_scale=5:alpha_threshold=128"
            } else {
                "paletteuse=dither=bayer:bayer_scale=5"
            };
            let graph = format!("split[a][b];[a]{palette}[p];[b][p]{apply}");
            let filter = match &scale {
                Some(scale) => format!("{scale},{graph}"),
                None => graph,
            };
            let loops = match settings.gif_loops {
                None => 0,
                Some(0) => -1,
                Some(n) => i64::from(n),
            };
            args.extend([s("-vf"), filter, s("-loop"), loops.to_string()]);
            "gif"
        }
        Container::Webm => {
            if !ffmpeg.has_encoder("libvpx-vp9") {
                return Err(MediaError::Failed(
                    "this ffmpeg has no VP9 encoder (libvpx-vp9), which a .webm needs; install a build that has it or export an .mp4".into(),
                ));
            }
            if let Some(scale) = &scale {
                args.extend([s("-vf"), scale.clone()]);
            }
            args.extend([
                s("-c:v"),
                s("libvpx-vp9"),
                s("-b:v"),
                s("0"),
                s("-crf"),
                s("30"),
            ]);
            args.extend([s("-row-mt"), s("1"), s("-auto-alt-ref"), s("0")]);
            args.extend([
                s("-pix_fmt"),
                s(if settings.alpha {
                    "yuva420p"
                } else {
                    "yuv420p"
                }),
            ]);
            "libvpx-vp9"
        }
        Container::Mov => {
            if !ffmpeg.has_encoder("prores_ks") {
                return Err(MediaError::Failed(
                    "this ffmpeg has no ProRes encoder (prores_ks), which a .mov needs; install a build that has it or export an .mp4".into(),
                ));
            }
            if let Some(scale) = &scale {
                args.extend([s("-vf"), scale.clone()]);
            }
            let (profile, format) = if settings.alpha {
                ("4444", "yuva444p10le")
            } else {
                ("3", "yuv422p10le")
            };
            args.extend([s("-c:v"), s("prores_ks"), s("-profile:v"), s(profile)]);
            args.extend([s("-pix_fmt"), s(format), s("-vendor"), s("apl0")]);
            "prores_ks"
        }
    };
    args.extend([s("-f"), s(settings.container.muxer())]);
    Ok((args, encoder))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_name_picks_its_container() {
        assert_eq!(Container::of_file("a.mp4"), Some(Container::Mp4));
        assert_eq!(Container::of_file("a.b.gif"), Some(Container::Gif));
        assert_eq!(Container::of_file("a.webm"), Some(Container::Webm));
        assert_eq!(Container::of_file("a.mov"), Some(Container::Mov));
        assert_eq!(Container::of_file("a.avi"), None);
        assert_eq!(Container::of_file("mp4"), None);
        assert!(Container::Gif.has_alpha() && !Container::Mp4.has_alpha());
        assert!(Container::Mp4.has_sound() && !Container::Mov.has_sound());
    }

    #[test]
    fn colour_is_divided_by_alpha_and_opaque_or_clear_pixels_are_left_alone() {
        let out = unpremultiply(&[64, 32, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
        assert_eq!(out, [128, 64, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn a_scaled_picture_keeps_its_shape_with_an_even_height() {
        let settings = EncodeSettings {
            width: 1080,
            height: 1920,
            scale_width: Some(480),
            ..EncodeSettings::default()
        };
        assert_eq!(output_size(&settings), (480, 854));
        let same = EncodeSettings {
            width: 100,
            height: 50,
            ..EncodeSettings::default()
        };
        assert_eq!(output_size(&same), (100, 50));
    }
}
