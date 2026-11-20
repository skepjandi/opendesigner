//! Colour grading and effects: a handful of plain numbers, stored as data
//! (`docs/wiki/video-editing.md#grading`).
//!
//! A [`Grade`] is shown on a video clip (footage or a design frame) or on an image node; the
//! renderer applies the numbers in a fixed order (`od_render::grade`). Named looks are only
//! parameter sets: they are expanded into numbers when a grade is read, so what is stored,
//! undone and keyframed is always the numbers.

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::finite_in;

pub const MAX_EXPOSURE: f64 = 3.0;
pub const MAX_BLUR: f64 = 50.0;

/// A number of a grade, for `set` and for keyframing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradeParam {
    Exposure,
    Contrast,
    Saturation,
    Temperature,
    Tint,
    Vignette,
    Blur,
}

impl GradeParam {
    pub const ALL: [GradeParam; 7] = [
        GradeParam::Exposure,
        GradeParam::Contrast,
        GradeParam::Saturation,
        GradeParam::Temperature,
        GradeParam::Tint,
        GradeParam::Vignette,
        GradeParam::Blur,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            GradeParam::Exposure => "exposure",
            GradeParam::Contrast => "contrast",
            GradeParam::Saturation => "saturation",
            GradeParam::Temperature => "temperature",
            GradeParam::Tint => "tint",
            GradeParam::Vignette => "vignette",
            GradeParam::Blur => "blur",
        }
    }

    /// The allowed range of the number.
    pub fn range(self) -> (f64, f64) {
        match self {
            GradeParam::Exposure => (-MAX_EXPOSURE, MAX_EXPOSURE),
            GradeParam::Vignette => (0.0, 1.0),
            GradeParam::Blur => (0.0, MAX_BLUR),
            _ => (-1.0, 1.0),
        }
    }

    pub fn get(self, g: &Grade) -> f64 {
        match self {
            GradeParam::Exposure => g.exposure,
            GradeParam::Contrast => g.contrast,
            GradeParam::Saturation => g.saturation,
            GradeParam::Temperature => g.temperature,
            GradeParam::Tint => g.tint,
            GradeParam::Vignette => g.vignette,
            GradeParam::Blur => g.blur,
        }
    }

    pub fn set(self, g: &mut Grade, v: f64) {
        let slot = match self {
            GradeParam::Exposure => &mut g.exposure,
            GradeParam::Contrast => &mut g.contrast,
            GradeParam::Saturation => &mut g.saturation,
            GradeParam::Temperature => &mut g.temperature,
            GradeParam::Tint => &mut g.tint,
            GradeParam::Vignette => &mut g.vignette,
            GradeParam::Blur => &mut g.blur,
        };
        *slot = v;
    }

    /// Checks `v` against the range and rounds it like every stored number.
    pub fn checked(self, v: f64) -> Result<f64, OpError> {
        let (lo, hi) = self.range();
        finite_in(self.as_str(), v, lo, hi)
    }
}

/// A named set of grade numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Look {
    Warm,
    Cool,
    Mono,
    Faded,
    Punchy,
}

impl Look {
    pub const ALL: [Look; 5] = [
        Look::Warm,
        Look::Cool,
        Look::Mono,
        Look::Faded,
        Look::Punchy,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Look::Warm => "warm",
            Look::Cool => "cool",
            Look::Mono => "mono",
            Look::Faded => "faded",
            Look::Punchy => "punchy",
        }
    }

    /// The numbers of the look.
    pub fn grade(self) -> Grade {
        let none = Grade::default();
        match self {
            Look::Warm => Grade {
                temperature: 0.35,
                tint: 0.05,
                saturation: 0.1,
                contrast: 0.05,
                ..none
            },
            Look::Cool => Grade {
                temperature: -0.35,
                tint: -0.05,
                saturation: 0.05,
                contrast: 0.05,
                ..none
            },
            Look::Mono => Grade {
                saturation: -1.0,
                contrast: 0.15,
                ..none
            },
            Look::Faded => Grade {
                exposure: 0.15,
                contrast: -0.25,
                saturation: -0.2,
                ..none
            },
            Look::Punchy => Grade {
                exposure: 0.05,
                contrast: 0.3,
                saturation: 0.35,
                vignette: 0.2,
                ..none
            },
        }
    }
}

/// How a picture is graded. Every number is zero when it does nothing.
///
/// Read as `{"look": "warm", "exposure": 0.2}`, a look supplies the numbers and the others
/// override it; it is always stored and written back as the numbers alone.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(from = "GradeInput")]
pub struct Grade {
    /// Stops of light, -3 to 3: each stop doubles or halves the brightness.
    #[serde(skip_serializing_if = "is_zero")]
    pub exposure: f64,
    /// -1 flattens towards grey, 1 doubles the steepness around mid grey.
    #[serde(skip_serializing_if = "is_zero")]
    pub contrast: f64,
    /// -1 is black and white, 1 doubles the colour.
    #[serde(skip_serializing_if = "is_zero")]
    pub saturation: f64,
    /// -1 cool (blue) to 1 warm (orange).
    #[serde(skip_serializing_if = "is_zero")]
    pub temperature: f64,
    /// -1 green to 1 magenta.
    #[serde(skip_serializing_if = "is_zero")]
    pub tint: f64,
    /// 0 none to 1 corners go black.
    #[serde(skip_serializing_if = "is_zero")]
    pub vignette: f64,
    /// Gaussian blur, 0 to 50: its radius in thousandths of the picture's shorter side, so a
    /// preview and the export blur by the same amount.
    #[serde(skip_serializing_if = "is_zero")]
    pub blur: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GradeInput {
    #[serde(default)]
    look: Option<Look>,
    #[serde(default)]
    exposure: Option<f64>,
    #[serde(default)]
    contrast: Option<f64>,
    #[serde(default)]
    saturation: Option<f64>,
    #[serde(default)]
    temperature: Option<f64>,
    #[serde(default)]
    tint: Option<f64>,
    #[serde(default)]
    vignette: Option<f64>,
    #[serde(default)]
    blur: Option<f64>,
}

impl From<GradeInput> for Grade {
    fn from(i: GradeInput) -> Grade {
        let mut g = i.look.map(Look::grade).unwrap_or_default();
        let given = [
            (GradeParam::Exposure, i.exposure),
            (GradeParam::Contrast, i.contrast),
            (GradeParam::Saturation, i.saturation),
            (GradeParam::Temperature, i.temperature),
            (GradeParam::Tint, i.tint),
            (GradeParam::Vignette, i.vignette),
            (GradeParam::Blur, i.blur),
        ];
        for (param, v) in given {
            if let Some(v) = v {
                param.set(&mut g, v);
            }
        }
        g
    }
}

impl Grade {
    /// Whether the grade changes nothing.
    pub fn is_neutral(&self) -> bool {
        GradeParam::ALL.iter().all(|p| p.get(self) == 0.0)
    }

    /// The grade with every number checked against its range and rounded as stored numbers are.
    pub fn checked(&self) -> Result<Grade, OpError> {
        let mut out = Grade::default();
        for p in GradeParam::ALL {
            p.set(&mut out, p.checked(p.get(self))?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_is_numbers_and_explicit_numbers_override_it() {
        let g: Grade = serde_json::from_str(r#"{"look":"warm","temperature":0.5}"#).unwrap();
        assert_eq!(g.temperature, 0.5);
        assert_eq!(g.saturation, Look::Warm.grade().saturation);
        let text = serde_json::to_string(&g).unwrap();
        assert!(!text.contains("look"), "{text}");
        assert_eq!(serde_json::from_str::<Grade>(&text).unwrap(), g);
    }

    #[test]
    fn every_look_is_inside_the_bounds_and_not_neutral() {
        for look in Look::ALL {
            let g = look.grade();
            assert!(!g.is_neutral(), "{look:?}");
            assert_eq!(g.checked().unwrap(), g, "{look:?}");
        }
    }

    #[test]
    fn out_of_range_numbers_are_refused_and_the_default_is_neutral() {
        assert!(Grade::default().is_neutral());
        for p in GradeParam::ALL {
            let (lo, hi) = p.range();
            assert!(p.checked(hi).is_ok() && p.checked(lo).is_ok());
            assert!(p.checked(hi + 0.01).is_err() && p.checked(lo - 0.01).is_err());
            assert!(p.checked(f64::NAN).is_err());
        }
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(serde_json::from_str::<Grade>(r#"{"gamma":1}"#).is_err());
    }
}
