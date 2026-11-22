//! Colour space maths shared by the linter and the agent: sRGB to OKLab, the distance between
//! two colours, and mixing two of them. Pure functions on channels from 0 to 1; no dependency.

/// A colour as three channels from 0 to 1.
pub type Rgb = [f64; 3];

/// One sRGB channel to linear light.
pub fn linear(channel: f64) -> f64 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// An sRGB colour to OKLab: lightness, green-red, blue-yellow.
///
/// OKLab is the space where a distance means what the eye sees: 0 is the same colour, 0.04 is a
/// difference you can just see, 0.2 is a different colour entirely.
pub fn oklab(rgb: Rgb) -> [f64; 3] {
    let [r, g, b] = [linear(rgb[0]), linear(rgb[1]), linear(rgb[2])];
    let long = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let medium = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let short = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    let cube = |v: f64| v.cbrt();
    let (l, m, s) = (cube(long), cube(medium), cube(short));
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// How far two colours are from each other in OKLab.
pub fn distance(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (oklab(a), oklab(b));
    ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
}

/// Two colours mixed: `share` of the first over the rest of the second.
pub fn mix(a: Rgb, b: Rgb, share: f64) -> Rgb {
    let s = share.clamp(0.0, 1.0);
    [
        a[0] * s + b[0] * (1.0 - s),
        a[1] * s + b[1] * (1.0 - s),
        a[2] * s + b[2] * (1.0 - s),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = [1.0, 0.0, 0.0];
    const NEAR_RED: Rgb = [1.0, 0.02, 0.0];

    #[test]
    fn a_colour_is_at_no_distance_from_itself() {
        assert!(distance(RED, RED) < 1e-9);
    }

    #[test]
    fn a_nearby_colour_is_nearer_than_a_different_one() {
        let near = distance(RED, NEAR_RED);
        let far = distance(RED, [0.0, 0.0, 1.0]);
        assert!(near < 0.05, "{near}");
        assert!(near < far, "{near} {far}");
    }

    #[test]
    fn white_and_black_are_the_farthest_apart() {
        // white is L 1 and black is L 0 in OKLab, so they are 1 apart (to the matrix's precision)
        assert!((distance([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn mixing_takes_the_share_of_the_first_colour() {
        let mixed = mix([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.25);
        assert!((mixed[0] - 0.25).abs() < 1e-9);
        assert!(mixed[1].abs() < 1e-9);
    }
}
