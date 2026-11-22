//! Where on a timeline a step starts: a number, or a phrase relative to what is already there
//! (`+=100`, `<`, `<<+=250`, `intro+=200`), so an agent places beats against each other instead of
//! adding milliseconds (`docs/wiki/video-editing.md#sequencing`).

use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;

const OPERATORS: [&str; 3] = ["+=", "-=", "*="];
const MAX_LABEL_CHARS: usize = 48;

/// A start: milliseconds from the start of the timeline, or a phrase.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Position {
    Abs(f64),
    Expr(String),
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum PositionError {
    #[error("`at` is empty; give a number such as 500 or a phrase such as \"<+=50\"")]
    Empty,
    #[error(
        "no label {0:?} on the timeline yet; a label exists once the step that names it is placed"
    )]
    UnknownLabel(String),
    #[error("{0:?} is not a position: use a number, +=n, -=n, *=n, <, <<, or label[+=n|-=n|*=n]")]
    Bad(String),
    #[error("the position works out to {0} ms; it must be a finite time of 0 or more")]
    OutOfBounds(f64),
    #[error("label {0:?} is not usable: {1}")]
    Label(String, &'static str),
    #[error("cannot fit to {0} ms: the timeline is {1} ms long")]
    Fit(f64, f64),
}

/// One step as the timeline holds it: `start_ms` is where it begins (offset plus delay).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedChild {
    pub start_ms: f64,
    pub dur_ms: f64,
}

/// Steps in the order they were added, and the named moments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimelineState {
    pub children: Vec<PlacedChild>,
    pub labels: BTreeMap<String, f64>,
}

impl TimelineState {
    /// Where the longest step ends; 0 for an empty timeline.
    pub fn duration(&self) -> f64 {
        self.children
            .iter()
            .map(|c| c.start_ms + c.dur_ms)
            .fold(0.0, f64::max)
    }

    pub fn push(&mut self, start_ms: f64, dur_ms: f64) {
        self.children.push(PlacedChild { start_ms, dur_ms });
    }

    pub fn set_label(&mut self, name: &str, at_ms: f64) -> Result<(), PositionError> {
        check_label(name)?;
        if self.labels.contains_key(name) {
            return Err(PositionError::Label(name.into(), "it is already used"));
        }
        self.labels.insert(name.into(), at_ms);
        Ok(())
    }

    /// Scales every start, duration and label so the timeline lasts `new_duration_ms`;
    /// returns the factor.
    pub fn retime(&mut self, new_duration_ms: f64) -> Result<f64, PositionError> {
        let old = self.duration();
        if !(new_duration_ms.is_finite() && new_duration_ms > 0.0 && old > 0.0) {
            return Err(PositionError::Fit(new_duration_ms, old));
        }
        let k = new_duration_ms / old;
        for c in &mut self.children {
            c.start_ms *= k;
            c.dur_ms *= k;
        }
        for v in self.labels.values_mut() {
            *v *= k;
        }
        Ok(k)
    }
}

/// A label must not read as a position: not a number, not starting with `<`, no operator in it.
pub fn check_label(name: &str) -> Result<(), PositionError> {
    let bad = |why| Err(PositionError::Label(name.into(), why));
    if name.is_empty() || name.chars().count() > MAX_LABEL_CHARS {
        return bad("it must be 1 to 48 characters");
    }
    if name != name.trim() || name.contains(char::is_whitespace) {
        return bad("it must not contain spaces");
    }
    if name.starts_with('<') {
        return bad("it must not start with <");
    }
    if OPERATORS.iter().any(|op| name.contains(op)) {
        return bad("it must not contain +=, -= or *=");
    }
    if name.parse::<f64>().is_ok() {
        return bad("it must not read as a number");
    }
    Ok(())
}

/// The start in milliseconds that `pos` names on `timeline`. No position is the end of the
/// longest step. `<` is the end of the step added last and `<<` its start (0 on an empty
/// timeline); a label is where that label was set; `+=n`, `-=n` and `*=n` act on the end of the
/// timeline or on what comes before them.
pub fn resolve_position(
    timeline: &TimelineState,
    pos: Option<&Position>,
) -> Result<f64, PositionError> {
    let at = match pos {
        None => timeline.duration(),
        Some(Position::Abs(ms)) => *ms,
        Some(Position::Expr(text)) => resolve_text(timeline, text.trim())?,
    };
    if at.is_finite() && at >= 0.0 {
        Ok(at)
    } else {
        Err(PositionError::OutOfBounds(at))
    }
}

fn resolve_text(timeline: &TimelineState, text: &str) -> Result<f64, PositionError> {
    if text.is_empty() {
        return Err(PositionError::Empty);
    }
    if let Ok(ms) = text.parse::<f64>() {
        return Ok(ms);
    }
    let tail = timeline.children.last();
    let (base, rest) = if let Some(rest) = text.strip_prefix("<<") {
        (Some(tail.map_or(0.0, |c| c.start_ms)), rest)
    } else if let Some(rest) = text.strip_prefix('<') {
        (Some(tail.map_or(0.0, |c| c.start_ms + c.dur_ms)), rest)
    } else {
        (None, text)
    };
    let (name, operation) = match first_operator(rest) {
        Some((at, op)) => {
            let amount = rest[at + 2..]
                .parse::<f64>()
                .map_err(|_| PositionError::Bad(text.into()))?;
            (&rest[..at], Some((op, amount)))
        }
        None => (rest, None),
    };
    let start = match base {
        Some(b) if name.is_empty() => b,
        Some(_) => return Err(PositionError::Bad(text.into())),
        None if name.is_empty() => timeline.duration(),
        None => *timeline
            .labels
            .get(name)
            .ok_or_else(|| PositionError::UnknownLabel(name.into()))?,
    };
    Ok(match operation {
        None => start,
        Some(('+', n)) => start + n,
        Some(('-', n)) => start - n,
        Some((_, n)) => start * n,
    })
}

fn first_operator(text: &str) -> Option<(usize, char)> {
    OPERATORS
        .iter()
        .filter_map(|op| text.find(op).map(|i| (i, op.chars().next().unwrap_or('+'))))
        .min_by_key(|(i, _)| *i)
}
