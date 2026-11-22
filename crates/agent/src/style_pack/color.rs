//! The colours of a style pack: reading a hex value, mixing two of them and measuring how far
//! apart two of them are. The OKLab maths lives in the core (`od_core::color`), so the linter and
//! the packs judge colour the same way.

use od_core::Rgba;

/// The channels of a hex colour, or `None` when it is not one.
pub fn rgb(hex: &str) -> Option<[f64; 3]> {
    let c = Rgba::parse_hex(hex).ok()?;
    Some([
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
    ])
}

/// Two colours mixed: `share` of the first over the rest of the second. A colour that cannot be
/// read is left alone.
pub fn mix(a: &str, b: &str, share: f64) -> String {
    let (Some(x), Some(y)) = (rgb(a), rgb(b)) else {
        return a.to_string();
    };
    let m = od_core::color::mix(x, y, share);
    format!(
        "#{:02x}{:02x}{:02x}",
        (m[0] * 255.0).round() as u8,
        (m[1] * 255.0).round() as u8,
        (m[2] * 255.0).round() as u8
    )
}

/// The WCAG contrast ratio of two colours, 1 to 21.
#[cfg(test)]
pub fn contrast(a: &str, b: &str) -> Option<f64> {
    Some(crate::review::contrast(
        Rgba::parse_hex(a).ok()?,
        Rgba::parse_hex(b).ok()?,
    ))
}

/// How far two colours are from each other in OKLab.
pub fn distance(a: &str, b: &str) -> Option<f64> {
    Some(od_core::color::distance(rgb(a)?, rgb(b)?))
}
