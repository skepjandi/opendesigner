//! SVG in, crisp PNG out: recolor an icon, then draw it at the size the design needs.
//!
//! `resvg` is the SVG renderer of the same family as the one the canvas uses (tiny-skia). It is
//! built without text and raster-image support, so an SVG is drawn from its own paths, gradients
//! and filters and nothing it names (a font, a linked picture) is ever read.

use resvg::{tiny_skia, usvg};

use super::{Result, msg};

/// The largest SVG text we parse.
pub const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;
/// The largest side of a picture we draw.
pub const MAX_SIDE: u32 = 4096;
/// The most pixels of a picture we draw.
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// The size of the longest side when the caller names none.
pub const DEFAULT_SIDE: u32 = 512;

/// Whether `value` is a hex color (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`).
pub fn is_hex_color(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|h| {
        matches!(h.len(), 3 | 4 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Wraps the inner shapes of a bundled Lucide icon in the svg element they all share.
pub fn lucide_svg(body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</svg>"
    )
}

/// Replaces `currentColor` with `color`. An SVG that never says `currentColor` gets its `fill`
/// and `stroke` colors replaced instead (`none` and gradients stay), and one that names no color
/// at all gets a `fill` on its root.
pub fn recolor(svg: &str, color: &str) -> String {
    let lower = svg.to_ascii_lowercase();
    if lower.contains("currentcolor") {
        let mut out = String::with_capacity(svg.len());
        let mut at = 0;
        while let Some(found) = lower[at..].find("currentcolor") {
            out.push_str(&svg[at..at + found]);
            out.push_str(color);
            at += found + "currentcolor".len();
        }
        out.push_str(&svg[at..]);
        return out;
    }
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    let mut changed = false;
    while let Some(at) = ["fill=\"", "stroke=\""]
        .iter()
        .filter_map(|k| rest.find(k).map(|p| (p, k.len())))
        .min()
    {
        let (pos, key) = at;
        let value_start = pos + key;
        let Some(len) = rest[value_start..].find('"') else {
            break;
        };
        let value = &rest[value_start..value_start + len];
        out.push_str(&rest[..value_start]);
        if matches!(value, "none" | "transparent") || value.starts_with("url(") {
            out.push_str(value);
        } else {
            out.push_str(color);
            changed = true;
        }
        rest = &rest[value_start + len..];
    }
    out.push_str(rest);
    if !changed && let Some(at) = out.find("<svg") {
        out.insert_str(at + 4, &format!(" fill=\"{color}\""));
    }
    out
}

/// Draws `svg` to a PNG. With both sides the picture is scaled to fit inside and centered; with
/// one, the other follows the aspect ratio; with none, the longest side is [`DEFAULT_SIDE`].
/// Returns the PNG and its size.
pub fn rasterize(
    svg: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(Vec<u8>, u32, u32)> {
    if svg.len() > MAX_SVG_BYTES {
        return msg("the SVG is larger than 2 MB");
    }
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
        .map_err(|e| super::Error::Msg(format!("not a drawable SVG: {e}")))?;
    let size = tree.size();
    let (sw, sh) = (size.width().max(0.001), size.height().max(0.001));
    let (w, h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, ((w as f32) * sh / sw).round().max(1.0) as u32),
        (None, Some(h)) => (((h as f32) * sw / sh).round().max(1.0) as u32, h),
        (None, None) => {
            let k = DEFAULT_SIDE as f32 / sw.max(sh);
            (
                (sw * k).round().max(1.0) as u32,
                (sh * k).round().max(1.0) as u32,
            )
        }
    };
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || u64::from(w) * u64::from(h) > MAX_PIXELS
    {
        return msg(format!(
            "{w}x{h} is outside what is drawn: each side 1 to {MAX_SIDE} and at most {MAX_PIXELS} pixels"
        ));
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return msg("cannot make a picture that size");
    };
    let k = (w as f32 / sw).min(h as f32 / sh);
    let transform =
        tiny_skia::Transform::from_translate((w as f32 - sw * k) / 2.0, (h as f32 - sh * k) / 2.0)
            .pre_scale(k, k);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let png = pixmap
        .encode_png()
        .map_err(|e| super::Error::Msg(format!("cannot encode the picture: {e}")))?;
    Ok((png, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CIRCLE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12\" r=\"10\" fill=\"currentColor\"/></svg>";

    #[test]
    fn colors_are_replaced_in_the_three_ways_an_svg_names_them() {
        assert!(recolor(CIRCLE, "#ff0000").contains("fill=\"#ff0000\""));
        let named = "<svg viewBox=\"0 0 1 1\"><path fill=\"#000\" stroke=\"none\"/><path fill=\"url(#g)\"/></svg>";
        let out = recolor(named, "#00ff00");
        assert!(
            out.contains("fill=\"#00ff00\"")
                && out.contains("stroke=\"none\"")
                && out.contains("url(#g)")
        );
        let bare = "<svg viewBox=\"0 0 1 1\"><path d=\"M0 0\"/></svg>";
        assert!(recolor(bare, "#123456").contains("<svg fill=\"#123456\""));
        assert!(
            is_hex_color("#a1b2c3")
                && is_hex_color("#abc")
                && !is_hex_color("red")
                && !is_hex_color("#abcd1")
        );
    }

    #[test]
    fn rasterizing_fits_centers_and_refuses_oversize() {
        let (png, w, h) = rasterize(CIRCLE, Some(64), None).unwrap();
        assert_eq!((w, h), (64, 64));
        assert!(png.starts_with(b"\x89PNG"));
        let (_, w, h) = rasterize(CIRCLE, Some(100), Some(40)).unwrap();
        assert_eq!((w, h), (100, 40));
        let (_, w, _) = rasterize(CIRCLE, None, None).unwrap();
        assert_eq!(w, DEFAULT_SIDE);
        assert!(rasterize(CIRCLE, Some(5000), None).is_err());
        assert!(rasterize("<svg", Some(10), None).is_err());
    }
}
