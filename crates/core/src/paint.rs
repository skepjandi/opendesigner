use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::finite_in;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub fn parse_hex(s: &str) -> Result<Rgba, OpError> {
        let bad = || OpError::InvalidPaint(s.to_string());
        let hex = s.strip_prefix('#').ok_or_else(bad)?;
        if !hex.is_ascii() || !(hex.len() == 6 || hex.len() == 8) {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        Ok(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if hex.len() == 8 { byte(6)? } else { 255 },
        })
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

/// A color stop of a gradient: `offset` 0 to 1 along the gradient, a solid color (literal or token).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    pub offset: f64,
    #[serde(with = "solid")]
    pub color: Paint,
}

/// Gradient stops are solid colors: a stop reads and writes as the same string a solid paint does.
mod solid {
    use super::Paint;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(p: &Paint, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&p.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Paint, D::Error> {
        Paint::parse(&String::deserialize(d)?).map_err(D::Error::custom)
    }
}

/// Fewest and most stops of a gradient.
pub const MIN_STOPS: usize = 2;
pub const MAX_STOPS: usize = 8;
const MAX_GRADIENT_ANGLE: f64 = 3_600.0;
const MAX_GRADIENT_REACH: f64 = 10.0;

/// A fill or stroke paint: a literal color (`#rrggbb`, `#rrggbbaa`), a token reference
/// (`{brand.gold}`), or a gradient across the node's box. Tokens let an agent change a brand
/// color in one place; a gradient's stops take tokens too. A solid paint is written as a string,
/// a gradient as an object with a `type`.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Color(Rgba),
    Token(String),
    /// `angle` in degrees: 0 runs left to right, 90 top to bottom (clockwise on screen). The line
    /// passes through the box's center and reaches its corners, so the first and last stop touch
    /// the box's far edges at any angle.
    Linear {
        angle: f64,
        stops: Vec<Stop>,
    },
    /// A circle (an ellipse in a box that is not square) around `cx`, `cy` as fractions of the box,
    /// reaching the last stop at `r` (a fraction of the box: 0.5 touches the edges from the center).
    Radial {
        cx: f64,
        cy: f64,
        r: f64,
        stops: Vec<Stop>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum GradientRepr {
    Linear {
        #[serde(default = "default_angle")]
        angle: f64,
        stops: Vec<Stop>,
    },
    Radial {
        #[serde(default = "half")]
        cx: f64,
        #[serde(default = "half")]
        cy: f64,
        #[serde(default = "half")]
        r: f64,
        stops: Vec<Stop>,
    },
}

fn default_angle() -> f64 {
    90.0
}

fn half() -> f64 {
    0.5
}

impl Serialize for Paint {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Paint::Color(_) | Paint::Token(_) => s.serialize_str(&self.to_string()),
            Paint::Linear { angle, stops } => GradientRepr::Linear {
                angle: *angle,
                stops: stops.clone(),
            }
            .serialize(s),
            Paint::Radial { cx, cy, r, stops } => GradientRepr::Radial {
                cx: *cx,
                cy: *cy,
                r: *r,
                stops: stops.clone(),
            }
            .serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Paint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Paint, D::Error> {
        struct PaintVisitor;
        impl<'de> serde::de::Visitor<'de> for PaintVisitor {
            type Value = Paint;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a color '#rrggbb', a token '{name}' or a gradient {type: linear|radial, stops}",
                )
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Paint, E> {
                Paint::parse(v).map_err(E::custom)
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Paint, A::Error> {
                let repr =
                    GradientRepr::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(match repr {
                    GradientRepr::Linear { angle, stops } => Paint::Linear { angle, stops },
                    GradientRepr::Radial { cx, cy, r, stops } => Paint::Radial { cx, cy, r, stops },
                })
            }
        }
        d.deserialize_any(PaintVisitor)
    }
}

impl Paint {
    pub fn parse(s: &str) -> Result<Paint, OpError> {
        if let Some(name) = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
            return if valid_token_name(name) {
                Ok(Paint::Token(name.to_string()))
            } else {
                Err(OpError::InvalidPaint(s.to_string()))
            };
        }
        Rgba::parse_hex(s).map(Paint::Color)
    }

    /// The color of a solid paint. A gradient answers with its first stop: the one color that
    /// stands for it where a single color is needed (contrast checks, text).
    pub fn resolve(&self, tokens: &BTreeMap<String, String>) -> Result<Rgba, OpError> {
        match self {
            Paint::Color(c) => Ok(*c),
            Paint::Token(name) => {
                let value = tokens
                    .get(name)
                    .ok_or_else(|| OpError::UnknownToken(name.clone()))?;
                Rgba::parse_hex(value)
            }
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => match stops.first() {
                Some(stop) => stop.color.resolve(tokens),
                None => Err(gradient_error("needs at least two stops")),
            },
        }
    }

    /// The paint as the document stores it: every token known, every number in bounds and
    /// rounded, a gradient with 2 to 8 stops of solid colors at offsets from 0 to 1.
    pub fn normalized(&self, tokens: &BTreeMap<String, String>) -> Result<Paint, OpError> {
        let reach = |what, v| finite_in(what, v, -MAX_GRADIENT_REACH, MAX_GRADIENT_REACH);
        Ok(match self {
            Paint::Color(_) | Paint::Token(_) => {
                self.resolve(tokens)?;
                self.clone()
            }
            Paint::Linear { angle, stops } => Paint::Linear {
                angle: finite_in(
                    "gradient angle",
                    *angle,
                    -MAX_GRADIENT_ANGLE,
                    MAX_GRADIENT_ANGLE,
                )?,
                stops: checked_stops(stops, tokens)?,
            },
            Paint::Radial { cx, cy, r, stops } => {
                if !(*r > 0.0) {
                    return Err(gradient_error("radius r must be above 0"));
                }
                Paint::Radial {
                    cx: reach("gradient cx", *cx)?,
                    cy: reach("gradient cy", *cy)?,
                    r: reach("gradient r", *r)?,
                    stops: checked_stops(stops, tokens)?,
                }
            }
        })
    }

    /// The stops of a gradient (`None` for a solid paint).
    pub fn stops(&self) -> Option<&[Stop]> {
        match self {
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => Some(stops),
            Paint::Color(_) | Paint::Token(_) => None,
        }
    }

    /// Whether this paint, or a stop of it, refers to the token `name`.
    pub fn uses_token(&self, name: &str) -> bool {
        match self {
            Paint::Token(t) => t == name,
            Paint::Color(_) => false,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
                stops.iter().any(|s| s.color.uses_token(name))
            }
        }
    }

    /// Names of the tokens this paint refers to that `tokens` does not define.
    pub fn undefined_tokens(&self, tokens: &BTreeMap<String, String>) -> Vec<String> {
        match self {
            Paint::Token(t) if !tokens.contains_key(t) => vec![t.clone()],
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => stops
                .iter()
                .flat_map(|s| s.color.undefined_tokens(tokens))
                .collect(),
            _ => Vec::new(),
        }
    }
}

fn gradient_error(detail: &str) -> OpError {
    OpError::OutOfBounds {
        what: "gradient",
        detail: detail.to_string(),
    }
}

fn checked_stops(stops: &[Stop], tokens: &BTreeMap<String, String>) -> Result<Vec<Stop>, OpError> {
    if !(MIN_STOPS..=MAX_STOPS).contains(&stops.len()) {
        return Err(gradient_error(&format!(
            "has {} stops; use {MIN_STOPS} to {MAX_STOPS}",
            stops.len()
        )));
    }
    stops
        .iter()
        .map(|s| {
            if !matches!(s.color, Paint::Color(_) | Paint::Token(_)) {
                return Err(gradient_error("a stop color is a solid color or a token"));
            }
            s.color.resolve(tokens)?;
            Ok(Stop {
                offset: finite_in("stop offset", s.offset, 0.0, 1.0)?,
                color: s.color.clone(),
            })
        })
        .collect()
}

pub fn valid_token_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= crate::limits::MAX_TOKEN_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl fmt::Display for Paint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stops = |f: &mut fmt::Formatter<'_>, stops: &[Stop]| {
            stops
                .iter()
                .try_for_each(|s| write!(f, ", {} {}", s.color, s.offset))
        };
        match self {
            Paint::Color(c) => f.write_str(&c.to_hex()),
            Paint::Token(t) => write!(f, "{{{t}}}"),
            Paint::Linear { angle, stops: s } => {
                write!(f, "linear({angle}deg")?;
                stops(f, s)?;
                f.write_str(")")
            }
            Paint::Radial {
                cx,
                cy,
                r,
                stops: s,
            } => {
                write!(f, "radial({cx} {cy} {r}")?;
                stops(f, s)?;
                f.write_str(")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_hex() {
        assert_eq!(
            Paint::parse("#f2ca83").unwrap(),
            Paint::Color(Rgba {
                r: 0xf2,
                g: 0xca,
                b: 0x83,
                a: 255
            })
        );
        assert_eq!(Paint::parse("#f2ca8380").unwrap().to_string(), "#f2ca8380");
    }

    #[test]
    fn rejects_malformed_paint() {
        for bad in ["f2ca83", "#f2ca8", "#gggggg", "{}", "{a b}", "#é2ca83", ""] {
            assert!(Paint::parse(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn resolves_tokens() {
        let mut tokens = BTreeMap::new();
        tokens.insert("brand.gold".to_string(), "#f2ca83".to_string());
        let paint = Paint::parse("{brand.gold}").unwrap();
        assert_eq!(paint.resolve(&tokens).unwrap().to_hex(), "#f2ca83");
        assert_eq!(
            Paint::parse("{brand.missing}").unwrap().resolve(&tokens),
            Err(OpError::UnknownToken("brand.missing".to_string()))
        );
    }
}
