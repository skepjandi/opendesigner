//! The five reveal styles as pure key builders: one piece of split text, one list of tracks.

use od_core::Ease;
use od_core::animation::{AnimProp, Key};
use serde::Deserialize;

use super::pieces::By;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Rise,
    Fade,
    Pop,
    Slide,
    Typewriter,
}

impl Style {
    pub fn as_str(self) -> &'static str {
        match self {
            Style::Rise => "rise",
            Style::Fade => "fade",
            Style::Pop => "pop",
            Style::Slide => "slide",
            Style::Typewriter => "typewriter",
        }
    }

    pub fn default_duration_ms(self) -> u64 {
        match self {
            Style::Rise => 450,
            Style::Fade => 400,
            Style::Pop => 350,
            Style::Slide => 500,
            Style::Typewriter => 1,
        }
    }

    pub fn default_ease(self) -> Ease {
        match self {
            Style::Rise | Style::Slide => Ease::Power4Out,
            Style::Fade => Ease::EaseOut,
            Style::Pop => Ease::BackOut(1200),
            Style::Typewriter => Ease::Linear,
        }
    }

    /// Milliseconds from one piece to the next.
    pub fn default_each_ms(self, by: By) -> u64 {
        match (self, by) {
            (Style::Typewriter, By::Chars) => 33,
            (_, By::Chars) => 40,
            (Style::Typewriter, By::Words) => 120,
            (_, By::Words) => 90,
            (Style::Typewriter, By::Lines) => 400,
            (_, By::Lines) => 140,
        }
    }

    pub fn default_wrap(self) -> bool {
        self == Style::Rise
    }
}

/// Where a piece rests, in its parent's coordinates, and how big it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rest {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub height: f64,
    /// The width of the box the piece moves in when it is masked, else its own.
    pub width: f64,
    pub masked: bool,
}

/// A piece appears within this long: over one frame at any rate down to 10 fps, so the two keys
/// never land on the same frame.
const TYPEWRITER_RAMP_MS: u64 = 50;
const POP_FROM: f64 = 0.5;
const RISE_FRACTION: f64 = 0.6;

fn key(at_ms: u64, value: f64, ease: Ease) -> Key {
    Key { at_ms, value, ease }
}

fn fade_in(start: u64, duration: u64, share: f64) -> Vec<Key> {
    let end = start + ((duration as f64 * share).round() as u64).max(1);
    vec![key(start, 0.0, Ease::Linear), key(end, 1.0, Ease::EaseOut)]
}

/// The tracks that bring one piece in, starting at `start`, over `duration`, with `ease`.
pub fn tracks(
    style: Style,
    rest: Rest,
    start: u64,
    duration: u64,
    ease: Ease,
) -> Vec<(AnimProp, Vec<Key>)> {
    let end = start + duration.max(1);
    let moved = |prop: AnimProp, from: f64, to: f64| {
        (
            prop,
            vec![key(start, from, Ease::Linear), key(end, to, ease)],
        )
    };
    match style {
        Style::Rise => {
            let distance = if rest.masked {
                rest.height
            } else {
                rest.size * RISE_FRACTION
            };
            let mut out = vec![moved(AnimProp::Y, rest.y + distance, rest.y)];
            if !rest.masked {
                out.push((AnimProp::Opacity, fade_in(start, duration, 0.6)));
            }
            out
        }
        Style::Slide => {
            let distance = if rest.masked { rest.width } else { rest.size };
            let mut out = vec![moved(AnimProp::X, rest.x - distance, rest.x)];
            if !rest.masked {
                out.push((AnimProp::Opacity, fade_in(start, duration, 0.6)));
            }
            out
        }
        Style::Fade => vec![(
            AnimProp::Opacity,
            vec![key(start, 0.0, Ease::Linear), key(end, 1.0, ease)],
        )],
        Style::Pop => vec![
            moved(AnimProp::Scale, POP_FROM, 1.0),
            (AnimProp::Opacity, fade_in(start, duration, 0.4)),
        ],
        Style::Typewriter => vec![(
            AnimProp::Opacity,
            vec![
                key(start, 0.0, Ease::Linear),
                key(start + TYPEWRITER_RAMP_MS, 1.0, Ease::Linear),
            ],
        )],
    }
}

const SHORT_WORD_CHARS: f64 = 6.0;

/// How long a text should stay readable once it has arrived (the RV015 rule): 1 s plus 0.4 s
/// per short word, a longer word counting for more.
pub fn hold_ms(text: &str) -> u64 {
    let words: f64 = text
        .split_whitespace()
        .map(|w| {
            (w.chars().count() as f64 / SHORT_WORD_CHARS)
                .ceil()
                .max(1.0)
        })
        .sum();
    1_000 + (words * 400.0).round() as u64
}
