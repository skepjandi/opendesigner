//! Bounds enforced on every operation and on every loaded project (`docs/wiki/security.md`).

use crate::error::OpError;

/// Coordinates and sizes are in document units (pixels at scale 1).
pub const MAX_COORD: f64 = 1_000_000.0;
/// Largest width or height of a node; also the export limit per side.
pub const MAX_SIZE: f64 = 16_384.0;
pub const MAX_NAME_LEN: usize = 200;
pub const MAX_TEXT_LEN: usize = 10_000;
pub const MAX_PATH_LEN: usize = 1_024;
pub const MAX_FONT_SIZE: f64 = 2_000.0;
pub const MAX_FAMILY_LEN: usize = 100;
pub const MIN_FONT_WEIGHT: f64 = 100.0;
pub const MAX_FONT_WEIGHT: f64 = 900.0;
pub const MIN_LINE_HEIGHT: f64 = 0.5;
pub const MAX_LINE_HEIGHT: f64 = 4.0;
pub const MAX_STROKE_WIDTH: f64 = 1_000.0;
pub const MAX_OPS_PER_TRANSACTION: usize = 1_000;
pub const MAX_NODES: usize = 100_000;
pub const MAX_DEPTH: usize = 64;
pub const MAX_PAGES: usize = 1_000;
pub const MAX_TOKENS: usize = 1_000;
pub const MAX_TOKEN_NAME_LEN: usize = 100;
/// Largest project file read from disk.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Document values keep four decimals so saved files are deterministic.
pub fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

pub fn finite_in(what: &'static str, v: f64, min: f64, max: f64) -> Result<f64, OpError> {
    if v.is_finite() && (min..=max).contains(&v) {
        Ok(round4(v))
    } else {
        Err(OpError::OutOfBounds {
            what,
            detail: format!("{v} is not a finite number in {min}..={max}"),
        })
    }
}

pub fn text_within(what: &'static str, s: &str, max: usize) -> Result<(), OpError> {
    if s.chars().count() > max {
        return Err(OpError::OutOfBounds {
            what,
            detail: format!("longer than {max} characters"),
        });
    }
    if s.chars().any(|c| c.is_control() && c != '\n') {
        return Err(OpError::OutOfBounds {
            what,
            detail: "contains control characters".to_string(),
        });
    }
    Ok(())
}

/// A font family name: non-empty, bounded, printable.
pub fn family_name(s: &str) -> Result<(), OpError> {
    if s.trim().is_empty() {
        return Err(OpError::OutOfBounds {
            what: "font_family",
            detail: "is empty".to_string(),
        });
    }
    text_within("font_family", s, MAX_FAMILY_LEN)?;
    if s.contains('\n') {
        return Err(OpError::OutOfBounds {
            what: "font_family",
            detail: "contains a line break".to_string(),
        });
    }
    Ok(())
}
