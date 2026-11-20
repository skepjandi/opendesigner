//! Captions drawn over a video's picture: the cue on screen at a moment, wrapped to the picture's
//! width, with the word being spoken in the highlight colour (`docs/wiki/video-editing.md#captions`).
//!
//! The text is shaped by the same `cosmic-text` setup as every other text, so captions look the
//! same in the preview and the export. A cue that needs more lines than the style allows is set
//! smaller, in steps, until it fits; if it never fits, its first lines are shown.

use od_core::{CaptionPosition, CaptionStyle, Captions, Rgba, TextAlign, Video};
use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};

use crate::RenderError;
use crate::text::{draw_run, measure_run};

/// Share of the picture's width and height kept free on each side.
const SIDE_MARGIN: f32 = 0.05;
const EDGE_MARGIN: f32 = 0.08;
/// A smaller type is tried this many times, each 8 % smaller, before the lines are cut.
const SHRINK_STEPS: u32 = 6;
const LINE_SPACING: f32 = 1.25;
const SHRINK: f32 = 0.92;

/// What the caption looks like at `time_ms` for a picture signature: the cue and the word in
/// highlight, or nothing when no cue is on screen. Equal values mean equal pictures.
pub fn signature(video: &Video, time_ms: u64) -> Option<(usize, Option<usize>)> {
    let captions = &video.captions;
    let cue = captions.active_index(time_ms)?;
    let word = captions
        .style
        .highlight
        .as_ref()
        .and_then(|_| Captions::active_word(&captions.cues[cue], time_ms));
    Some((cue, word))
}

/// Draws the caption of `video` at `time_ms` onto `canvas`, if one is on screen.
pub fn draw(canvas: &mut Pixmap, video: &Video, time_ms: u64) -> Result<(), RenderError> {
    let Some((cue_at, highlighted)) = signature(video, time_ms) else {
        return Ok(());
    };
    let style = &video.captions.style;
    let cue = &video.captions.cues[cue_at];
    let (color, highlight, background) = style.colors()?;
    let words: Vec<String> = cue
        .timed_words()
        .into_iter()
        .map(|w| {
            if style.uppercase {
                w.text.to_uppercase()
            } else {
                w.text
            }
        })
        .collect();
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    let family = style.font_family.as_str();
    let mut size = style.size_pct / 100.0 * h;
    let mut layout = set_lines(
        &words,
        family,
        style.font_weight,
        size,
        w * (1.0 - 2.0 * SIDE_MARGIN),
    );
    for _ in 0..SHRINK_STEPS {
        if layout.len() <= usize::from(style.max_lines) {
            break;
        }
        size *= SHRINK;
        layout = set_lines(
            &words,
            family,
            style.font_weight,
            size,
            w * (1.0 - 2.0 * SIDE_MARGIN),
        );
    }
    layout.truncate(usize::from(style.max_lines));
    paint_lines(
        canvas,
        &layout,
        (style, size),
        (color, highlight, background),
        highlighted,
    );
    Ok(())
}

/// A wrapped line: the words (index into the cue's words) with their widths.
type Line = Vec<(usize, String, f32)>;

fn set_lines(words: &[String], family: &str, weight: u16, size: f32, max_width: f32) -> Vec<Line> {
    let space = space_width(family, weight, size);
    let mut lines: Vec<Line> = vec![Vec::new()];
    let mut used = 0.0;
    for (n, word) in words.iter().enumerate() {
        let (width, _) = measure_run(word, family, weight, size);
        let current = lines.last().map_or(0, Vec::len);
        if current > 0 && used + space + width > max_width {
            lines.push(Vec::new());
            used = 0.0;
        }
        used += if used > 0.0 { space } else { 0.0 } + width;
        if let Some(line) = lines.last_mut() {
            line.push((n, word.clone(), width));
        }
    }
    lines
}

fn space_width(family: &str, weight: u16, size: f32) -> f32 {
    let (apart, _) = measure_run("i i", family, weight, size);
    let (single, _) = measure_run("i", family, weight, size);
    (apart - 2.0 * single).max(size * 0.2)
}

fn paint_lines(
    canvas: &mut Pixmap,
    lines: &[Line],
    (style, size): (&CaptionStyle, f32),
    (color, highlight, background): (Rgba, Option<Rgba>, Option<Rgba>),
    highlighted: Option<usize>,
) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    let family = style.font_family.as_str();
    let space = space_width(family, style.font_weight, size);
    let line_h = size * LINE_SPACING;
    let block_h = line_h * lines.len() as f32;
    let top = match style.position {
        CaptionPosition::Top => h * EDGE_MARGIN,
        CaptionPosition::Middle => (h - block_h) / 2.0,
        CaptionPosition::Bottom => h * (1.0 - EDGE_MARGIN) - block_h,
    };
    for (row, line) in lines.iter().enumerate() {
        let width: f32 =
            line.iter().map(|(_, _, w)| w).sum::<f32>() + space * (line.len() - 1) as f32;
        let left = match style.align {
            TextAlign::Left => w * SIDE_MARGIN,
            TextAlign::Center => (w - width) / 2.0,
            TextAlign::Right => w * (1.0 - SIDE_MARGIN) - width,
        };
        let y = top + row as f32 * line_h;
        if let Some(bg) = background {
            fill_box(
                canvas,
                (left - size * 0.3, y, width + size * 0.6, line_h),
                bg,
            );
        }
        let mut x = left;
        for (n, text, word_w) in line {
            let active = highlighted == Some(*n);
            let ink = highlight.filter(|_| active).unwrap_or(color);
            draw_run(canvas, text, (family, style.font_weight, size), (x, y), ink);
            x += word_w + space;
        }
    }
}

fn fill_box(canvas: &mut Pixmap, (x, y, w, h): (f32, f32, f32, f32), color: Rgba) {
    let Some(rect) = Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.r, color.g, color.b, color.a));
    canvas.fill_rect(rect, &paint, Transform::identity(), None);
}
