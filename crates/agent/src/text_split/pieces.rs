//! Cutting laid-out text into characters, words and lines. Pure: it reads the rows the renderer
//! wrapped (`od_render::text_lines`) and returns where each piece was drawn.

use std::collections::BTreeMap;

use od_render::{PlacedGlyph, PlacedLine};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    Chars,
    Words,
    Lines,
}

impl By {
    pub fn as_str(self) -> &'static str {
        match self {
            By::Chars => "chars",
            By::Words => "words",
            By::Lines => "lines",
        }
    }

    pub fn prefix(self) -> char {
        match self {
            By::Chars => 'c',
            By::Words => 'w',
            By::Lines => 'l',
        }
    }
}

/// One glyph of a piece: where it was drawn and how much room it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Its left edge, from the left of the text box.
    pub x: f32,
    /// Its top, from the baseline.
    pub y: f32,
    /// Its advance.
    pub width: f32,
}

/// One piece of text: the glyphs it holds, where they were drawn, and the row they sit on.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub text: String,
    /// Where each glyph was drawn, from the top left of the text box.
    pub glyphs: Vec<Glyph>,
    /// The row of the text the piece is on, from 0.
    pub line: usize,
    /// The row's top, its baseline and its height, from the top of the text box.
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
}

impl Piece {
    /// The left edge of the ink, and the right edge of the last glyph.
    pub fn edges(&self) -> (f32, f32) {
        let left = self
            .glyphs
            .iter()
            .map(|g| g.x)
            .fold(f32::INFINITY, f32::min);
        let right = self
            .glyphs
            .iter()
            .map(|g| g.x + g.width)
            .fold(f32::NEG_INFINITY, f32::max);
        (left, right)
    }
}

fn is_blank(row: &PlacedLine, g: &PlacedGlyph) -> bool {
    row.text
        .get(g.start..g.end)
        .is_none_or(|s| s.chars().all(char::is_whitespace))
}

fn piece(row: &PlacedLine, glyphs: &[&PlacedGlyph], line: usize) -> Option<Piece> {
    let (first, last) = (glyphs.first()?, glyphs.last()?);
    let start = glyphs.iter().map(|g| g.start).min().unwrap_or(first.start);
    let end = glyphs.iter().map(|g| g.end).max().unwrap_or(last.end);
    Some(Piece {
        text: row.text.get(start..end)?.to_string(),
        glyphs: glyphs
            .iter()
            .map(|g| Glyph {
                x: g.x,
                y: g.y,
                width: g.width,
            })
            .collect(),
        line,
        top: row.top,
        baseline: row.baseline,
        height: row.height,
    })
}

/// The pieces of `rows` in reading order. Spaces are not pieces. A ligature or a letter with its
/// marks is one glyph and so one character piece.
pub fn split(rows: &[PlacedLine], by: By) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut line = 0;
    for row in rows {
        let mut glyphs: Vec<&PlacedGlyph> = row.glyphs.iter().collect();
        glyphs.sort_by_key(|g| g.start);
        let before = out.len();
        match by {
            By::Chars => {
                for g in glyphs.iter().filter(|g| !is_blank(row, g)) {
                    out.extend(piece(row, &[g], line));
                }
            }
            By::Words => {
                let mut word: Vec<&PlacedGlyph> = Vec::new();
                for g in &glyphs {
                    if is_blank(row, g) {
                        out.extend(piece(row, &word, line));
                        word.clear();
                    } else {
                        word.push(g);
                    }
                }
                out.extend(piece(row, &word, line));
            }
            By::Lines => {
                // the spaces inside a line stay: the line is laid out on its own as one run
                let first = glyphs.iter().position(|g| !is_blank(row, g));
                let last = glyphs.iter().rposition(|g| !is_blank(row, g));
                if let (Some(first), Some(last)) = (first, last) {
                    out.extend(piece(row, &glyphs[first..=last], line));
                }
            }
        }
        if out.len() > before {
            line += 1;
        }
    }
    out
}

/// The way to cut text for a reveal when none is asked for: characters for a short headline,
/// words for a line or two, lines for more.
pub fn default_by(rows: &[PlacedLine]) -> By {
    let chars = split(rows, By::Chars).len();
    let words = split(rows, By::Words).len();
    let lines = split(rows, By::Lines).len();
    if chars <= HEADLINE_CHARS && lines <= 1 {
        By::Chars
    } else if lines > MAX_WORD_LINES || words > MAX_WORDS {
        By::Lines
    } else {
        By::Words
    }
}

/// Characters a headline moment may have to be revealed one by one.
pub const HEADLINE_CHARS: usize = 12;
const MAX_WORD_LINES: usize = 4;
const MAX_WORDS: usize = 40;

/// The whole-pixel column a piece's box starts at so that its glyphs land on the pixels the
/// original's did: the offset most of the piece's glyphs agree on. The glyphs of a piece are
/// laid out on their own, so their positions differ from the original's by the text's alignment
/// and by the rounding of the pixels; the offset that most of them share is the one that keeps
/// most of them where they were.
pub fn column_offset(drawn: &[Glyph], own: &[Glyph]) -> i32 {
    let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
    for (drawn_glyph, own_glyph) in drawn.iter().zip(own.iter()) {
        let shift = drawn_glyph.x.trunc() as i32 - own_glyph.x.trunc() as i32;
        *counts.entry(shift).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_shift, count)| count)
        .map(|(shift, _)| shift)
        .unwrap_or(0)
}

/// The row a piece's box starts at so that its baseline lands on the original's: the row the
/// baseline falls in, when the piece is set in the same style on its own.
pub fn row_offset(piece: &Piece, own_baseline: f32) -> i32 {
    let y = piece.glyphs.first().map(|g| g.y).unwrap_or(0.0);
    let want = (piece.baseline + y).trunc() as i32;
    let start = (piece.baseline - own_baseline).round() as i32;
    for row in [start, start - 1, start + 1] {
        if (row as f32 + own_baseline + y).trunc() as i32 == want {
            return row;
        }
    }
    start
}
