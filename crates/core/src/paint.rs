use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::OpError;

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

/// A fill or stroke color: a literal (`#rrggbb`, `#rrggbbaa`) or a token reference (`{brand.gold}`).
/// Tokens let an agent change a brand color in one place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Paint {
    Color(Rgba),
    Token(String),
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

    pub fn resolve(&self, tokens: &BTreeMap<String, String>) -> Result<Rgba, OpError> {
        match self {
            Paint::Color(c) => Ok(*c),
            Paint::Token(name) => {
                let value = tokens
                    .get(name)
                    .ok_or_else(|| OpError::UnknownToken(name.clone()))?;
                Rgba::parse_hex(value)
            }
        }
    }
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
        match self {
            Paint::Color(c) => f.write_str(&c.to_hex()),
            Paint::Token(t) => write!(f, "{{{t}}}"),
        }
    }
}

impl TryFrom<String> for Paint {
    type Error = OpError;
    fn try_from(s: String) -> Result<Self, OpError> {
        Paint::parse(&s)
    }
}

impl From<Paint> for String {
    fn from(p: Paint) -> String {
        p.to_string()
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
