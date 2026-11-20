//! Captions as data: timed cues with optional word timings and one style per video
//! (`docs/wiki/video-editing.md#captions`).
//!
//! A caption is never a script or an effect: it is a list of cues and a style that `render` draws
//! over the picture and that `export_video` therefore burns in. They change as a whole through
//! `set_video` `captions`; the helpers here compute the new value for one added cue, one removed
//! cue or a new style, and the importers turn SRT, WebVTT, word-timing JSON and a plain script
//! into cues. Nothing here recognizes speech: timings come from the user's own transcript, and a
//! script spread over a time range is an estimate by word length.

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::{self, finite_in, text_within};
use crate::model::{NodeId, TextAlign};
use crate::paint::Rgba;
use crate::video::{MAX_VIDEO_MS, Video};

pub const MAX_CUES: usize = 2_000;
pub const MAX_WORDS_PER_CUE: usize = 80;
pub const MAX_CAPTION_WORDS: usize = 30_000;
pub const MAX_CUE_TEXT: usize = 400;
pub const MAX_WORD_TEXT: usize = 60;
pub const MAX_CAPTION_LINES: u8 = 4;
pub const DEFAULT_WORDS_PER_CUE: usize = 7;
/// A gap this long between two spoken words starts a new cue when word timings are grouped.
const GROUP_GAP_MS: u32 = 700;
const GROUP_WORDS: usize = 8;
const GROUP_CHARS: usize = 42;

/// One spoken word with its time, counted from the start of the video.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Word {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

/// One caption on screen from `start_ms` to `end_ms`. With `words`, the active word can be
/// highlighted; without, the cue is shown whole (or its words are timed by estimate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    pub id: NodeId,
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
}

impl Cue {
    /// The words with their times: the recorded ones, or the cue's text spread over its time by
    /// word length when none were given.
    pub fn timed_words(&self) -> Vec<Word> {
        if !self.words.is_empty() {
            return self.words.clone();
        }
        let words: Vec<&str> = self.text.split_whitespace().collect();
        spread_words(&words, self.start_ms, self.end_ms).unwrap_or_default()
    }
}

/// Where on the picture the captions sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptionPosition {
    Top,
    Middle,
    #[default]
    Bottom,
}

/// Named looks for the style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionPreset {
    /// Plain white text with no box: the quiet default.
    Clean,
    /// Large, heavy, uppercase, the spoken word in yellow.
    BoldPop,
    /// Mid-screen, the spoken word in a bright colour.
    Karaoke,
    /// White text in a dark box.
    Boxed,
}

impl CaptionPreset {
    pub const ALL: [CaptionPreset; 4] = [
        CaptionPreset::Clean,
        CaptionPreset::BoldPop,
        CaptionPreset::Karaoke,
        CaptionPreset::Boxed,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CaptionPreset::Clean => "clean",
            CaptionPreset::BoldPop => "bold_pop",
            CaptionPreset::Karaoke => "karaoke",
            CaptionPreset::Boxed => "boxed",
        }
    }
}

/// How captions look. Colours are `#rrggbb` or `#rrggbbaa`; the size is a share of the video's
/// height so the same style fits every output size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptionStyle {
    #[serde(default = "default_family")]
    pub font_family: String,
    /// Percent of the video's height (1 to 20).
    #[serde(default = "default_size")]
    pub size_pct: f32,
    #[serde(default = "default_weight")]
    pub font_weight: u16,
    #[serde(default = "default_color")]
    pub color: String,
    /// The word being spoken, when the cue has word timings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    /// A box behind the lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default)]
    pub position: CaptionPosition,
    #[serde(default = "default_align")]
    pub align: TextAlign,
    #[serde(default = "default_lines")]
    pub max_lines: u8,
    #[serde(default)]
    pub uppercase: bool,
}

fn default_family() -> String {
    "Lato".into()
}
fn default_size() -> f32 {
    5.0
}
fn default_weight() -> u16 {
    700
}
fn default_color() -> String {
    "#ffffff".into()
}
fn default_align() -> TextAlign {
    TextAlign::Center
}
fn default_lines() -> u8 {
    2
}

impl Default for CaptionStyle {
    fn default() -> CaptionStyle {
        CaptionStyle {
            font_family: default_family(),
            size_pct: default_size(),
            font_weight: default_weight(),
            color: default_color(),
            highlight: None,
            background: None,
            position: CaptionPosition::Bottom,
            align: TextAlign::Center,
            max_lines: default_lines(),
            uppercase: false,
        }
    }
}

impl CaptionStyle {
    pub fn preset(preset: CaptionPreset) -> CaptionStyle {
        let base = CaptionStyle::default();
        match preset {
            CaptionPreset::Clean => base,
            CaptionPreset::BoldPop => CaptionStyle {
                size_pct: 8.0,
                font_weight: 900,
                highlight: Some("#ffd400".into()),
                max_lines: 2,
                uppercase: true,
                position: CaptionPosition::Middle,
                ..base
            },
            CaptionPreset::Karaoke => CaptionStyle {
                size_pct: 6.0,
                highlight: Some("#35e0a1".into()),
                color: "#ffffffcc".into(),
                position: CaptionPosition::Middle,
                ..base
            },
            CaptionPreset::Boxed => CaptionStyle {
                size_pct: 4.5,
                background: Some("#000000b3".into()),
                ..base
            },
        }
    }

    /// The colour of the plain words, of the active word and of the box, as the renderer wants them.
    pub fn colors(&self) -> Result<(Rgba, Option<Rgba>, Option<Rgba>), OpError> {
        let parse = |s: &Option<String>| s.as_deref().map(Rgba::parse_hex).transpose();
        Ok((
            Rgba::parse_hex(&self.color)?,
            parse(&self.highlight)?,
            parse(&self.background)?,
        ))
    }
}

/// A video's captions: the style and the cues in time order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Captions {
    #[serde(default)]
    pub style: CaptionStyle,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cues: Vec<Cue>,
}

impl Captions {
    /// A video with no cues and the default style stores nothing.
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty() && self.style == CaptionStyle::default()
    }

    /// The index of the cue on screen at `time_ms`, if any.
    pub fn active_index(&self, time_ms: u64) -> Option<usize> {
        let at = self
            .cues
            .partition_point(|c| u64::from(c.start_ms) <= time_ms)
            .checked_sub(1)?;
        (time_ms < u64::from(self.cues[at].end_ms)).then_some(at)
    }

    /// The cue on screen at `time_ms`, if any.
    pub fn active(&self, time_ms: u64) -> Option<&Cue> {
        self.active_index(time_ms).map(|n| &self.cues[n])
    }

    /// The words the style paints with the highlight at `time_ms`, as an index into
    /// `cue.timed_words()`: the word being spoken, or the last one spoken in a pause.
    pub fn active_word(cue: &Cue, time_ms: u64) -> Option<usize> {
        let words = cue.timed_words();
        words
            .partition_point(|w| u64::from(w.start_ms) <= time_ms)
            .checked_sub(1)
    }
}

fn bad(what: &'static str, detail: String) -> OpError {
    OpError::OutOfBounds { what, detail }
}

pub fn check_style(s: &CaptionStyle) -> Result<(), OpError> {
    limits::family_name(&s.font_family)?;
    finite_in("caption size_pct", f64::from(s.size_pct), 1.0, 20.0)?;
    if !(100..=900).contains(&s.font_weight) {
        return Err(bad(
            "caption font_weight",
            format!("{} is not from 100 to 900", s.font_weight),
        ));
    }
    if !(1..=MAX_CAPTION_LINES).contains(&s.max_lines) {
        return Err(bad(
            "caption max_lines",
            format!("{} is not from 1 to {MAX_CAPTION_LINES}", s.max_lines),
        ));
    }
    s.colors().map(|_| ())
}

fn check_word(w: &Word, cue: &Cue) -> Result<(), OpError> {
    text_within("caption word", &w.text, MAX_WORD_TEXT)?;
    if w.text.is_empty() || w.text.contains(char::is_whitespace) {
        return Err(bad(
            "caption word",
            format!("{:?} must be one word with no spaces", w.text),
        ));
    }
    if w.end_ms <= w.start_ms || w.start_ms < cue.start_ms || w.end_ms > cue.end_ms {
        return Err(bad(
            "caption word time",
            format!(
                "{:?} at {}..{} ms lies outside its cue {} ({}..{} ms)",
                w.text, w.start_ms, w.end_ms, cue.id, cue.start_ms, cue.end_ms
            ),
        ));
    }
    Ok(())
}

fn check_cue(c: &Cue) -> Result<(), OpError> {
    text_within("caption text", &c.text, MAX_CUE_TEXT)?;
    if c.text.trim().is_empty() {
        return Err(bad("caption text", format!("cue {} has no text", c.id)));
    }
    if c.end_ms <= c.start_ms || u64::from(c.end_ms) > MAX_VIDEO_MS {
        return Err(bad(
            "caption time",
            format!(
                "cue {} at {}..{} ms must end after it starts and within {MAX_VIDEO_MS} ms",
                c.id, c.start_ms, c.end_ms
            ),
        ));
    }
    if c.words.len() > MAX_WORDS_PER_CUE {
        return Err(OpError::Limit(format!(
            "at most {MAX_WORDS_PER_CUE} words per caption cue"
        )));
    }
    for (n, w) in c.words.iter().enumerate() {
        check_word(w, c)?;
        if c.words[..n].last().is_some_and(|p| p.end_ms > w.start_ms) {
            return Err(bad(
                "caption word time",
                format!("{:?} starts before the word before it ends", w.text),
            ));
        }
    }
    let spoken = c.text.split_whitespace();
    if !c.words.is_empty() && !spoken.eq(c.words.iter().map(|w| w.text.as_str())) {
        return Err(bad(
            "caption words",
            format!("the words of cue {} do not spell its text", c.id),
        ));
    }
    Ok(())
}

/// The shape rules of a video's captions: the style, at most [`MAX_CUES`] cues with unique ids,
/// in time order without overlap, each valid. Where the video ends is [`check_within`]'s business.
pub fn check_captions(c: &Captions) -> Result<(), OpError> {
    check_style(&c.style)?;
    if c.cues.len() > MAX_CUES {
        return Err(OpError::Limit(format!(
            "at most {MAX_CUES} caption cues per video"
        )));
    }
    let mut total_words = 0;
    for (n, cue) in c.cues.iter().enumerate() {
        check_cue(cue)?;
        total_words += cue.words.len();
        if let Some(prev) = n.checked_sub(1).map(|p| &c.cues[p]) {
            if prev.end_ms > cue.start_ms {
                return Err(bad(
                    "caption time",
                    format!(
                        "cue {} starts at {} ms before cue {} ends at {} ms: cues are in time order and do not overlap",
                        cue.id, cue.start_ms, prev.id, prev.end_ms
                    ),
                ));
            }
            if c.cues[..n].iter().any(|p| p.id == cue.id) {
                return Err(OpError::DuplicateId(cue.id.to_string()));
            }
        }
    }
    if total_words > MAX_CAPTION_WORDS {
        return Err(OpError::Limit(format!(
            "at most {MAX_CAPTION_WORDS} timed words per video"
        )));
    }
    Ok(())
}

/// Every cue ends inside the video, which is what a front end asks of a new caption list. (The
/// document itself only keeps the shape rules, so changing clips never invalidates captions that
/// an undo then cannot restore; a cue past the end is simply never drawn.)
pub fn check_within(c: &Captions, video_ms: u64) -> Result<(), OpError> {
    check_captions(c)?;
    match c.cues.iter().find(|q| u64::from(q.end_ms) > video_ms) {
        Some(q) => Err(bad(
            "caption time",
            format!(
                "cue {} ends at {} ms, after the video's {video_ms} ms",
                q.id, q.end_ms
            ),
        )),
        None => Ok(()),
    }
}

/// A cue to add: the id is made for it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewCue {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
    #[serde(default)]
    pub words: Vec<Word>,
}

impl Video {
    fn free_cue_id(&self) -> Result<NodeId, OpError> {
        let next = self
            .captions
            .cues
            .iter()
            .filter_map(|c| c.id.as_str().strip_prefix('q')?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        NodeId::parse(&format!("q{next}"))
    }

    /// The captions after adding one cue in its place in time, and its id. Apply the result with
    /// `set_video` `captions`.
    pub fn captions_with_cue(&self, new: NewCue) -> Result<(Captions, NodeId), OpError> {
        let id = self.free_cue_id()?;
        let mut captions = self.captions.clone();
        let at = captions
            .cues
            .partition_point(|c| c.start_ms <= new.start_ms);
        captions.cues.insert(
            at,
            Cue {
                id: id.clone(),
                start_ms: new.start_ms,
                end_ms: new.end_ms,
                text: new.text,
                words: new.words,
            },
        );
        check_within(&captions, self.duration_ms())?;
        Ok((captions, id))
    }

    /// The captions after removing cue `id`.
    pub fn captions_without_cue(&self, id: &NodeId) -> Result<Captions, OpError> {
        let at = self
            .captions
            .cues
            .iter()
            .position(|c| &c.id == id)
            .ok_or_else(|| bad("caption cue", format!("video has no caption cue {id}")))?;
        let mut captions = self.captions.clone();
        captions.cues.remove(at);
        Ok(captions)
    }

    /// The captions with `cues` in place of the old ones (checked against the video's length).
    pub fn captions_with_cues(&self, cues: Vec<Cue>) -> Result<Captions, OpError> {
        let captions = Captions {
            style: self.captions.style.clone(),
            cues,
        };
        check_within(&captions, self.duration_ms())?;
        Ok(captions)
    }

    /// The captions with a new style.
    pub fn captions_with_style(&self, style: CaptionStyle) -> Result<Captions, OpError> {
        let captions = Captions {
            style,
            cues: self.captions.cues.clone(),
        };
        check_style(&captions.style)?;
        Ok(captions)
    }
}

// ---- importing ----

/// A time such as `00:01:02,500`, `01:02.500` or `1:02.5` as milliseconds.
pub fn parse_timestamp(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    let (h, m, rest) = match parts.as_slice() {
        [m, s] => ("0", *m, *s),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };
    let (secs, frac) = rest.split_once(['.', ',']).unwrap_or((rest, "0"));
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    if ![h, m, secs, frac].iter().all(|t| digits(t)) || frac.len() > 3 {
        return None;
    }
    let ms_frac = frac.parse::<u64>().ok()? * 10_u64.pow(3 - frac.len() as u32);
    let total =
        (h.parse::<u64>().ok()? * 3600 + m.parse::<u64>().ok()? * 60 + secs.parse::<u64>().ok()?)
            * 1000
            + ms_frac;
    u32::try_from(total).ok()
}

/// `text` split into words and each given a share of `start_ms..end_ms` in proportion to its
/// length (an estimate, not a measurement). `None` when the range is shorter than one
/// millisecond per word.
pub fn spread_words(words: &[&str], start_ms: u32, end_ms: u32) -> Option<Vec<Word>> {
    let n = words.len() as u64;
    let span = u64::from(end_ms.checked_sub(start_ms)?);
    if n == 0 || span < n {
        return None;
    }
    let weights: Vec<u64> = words
        .iter()
        .map(|w| w.chars().count().max(1) as u64)
        .collect();
    let total: u64 = weights.iter().sum();
    let (mut out, mut edge, mut cumulative) = (Vec::new(), u64::from(start_ms), 0);
    for (i, (word, weight)) in words.iter().zip(&weights).enumerate() {
        cumulative += weight;
        let ideal = u64::from(start_ms) + span * cumulative / total;
        let left = n - i as u64 - 1;
        let next = ideal.clamp(edge + 1, u64::from(end_ms) - left);
        out.push(Word {
            text: (*word).to_string(),
            start_ms: edge as u32,
            end_ms: next as u32,
        });
        edge = next;
    }
    Some(out)
}

fn numbered(drafts: Vec<(u32, u32, String, Vec<Word>)>) -> Result<Vec<Cue>, OpError> {
    if drafts.len() > MAX_CUES {
        return Err(OpError::Limit(format!(
            "at most {MAX_CUES} caption cues per video; the source has {}",
            drafts.len()
        )));
    }
    drafts
        .into_iter()
        .enumerate()
        .map(|(n, (start_ms, end_ms, text, words))| {
            Ok(Cue {
                id: NodeId::parse(&format!("q{}", n + 1))?,
                start_ms,
                end_ms,
                text,
                words,
            })
        })
        .collect()
}

fn unescape(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The words of a cue's payload, with the times that WebVTT's inline `<00:00:01.500>` tags give;
/// markup tags (`<i>`, `<c.yellow>`, `<v Name>`) and `{\an8}` positioning are dropped.
fn payload(raw: &str, cue_start: u32, cue_end: u32) -> (String, Vec<Word>) {
    let mut segments: Vec<(u32, String)> = vec![(cue_start, String::new())];
    let mut timed = false;
    let mut rest = raw;
    while let Some(open) = rest.find(['<', '{']) {
        let closer = if rest[open..].starts_with('<') {
            '>'
        } else {
            '}'
        };
        if let Some(s) = segments.last_mut() {
            s.1.push_str(&rest[..open]);
        }
        let Some(close) = rest[open..].find(closer) else {
            rest = &rest[open + 1..];
            continue;
        };
        let inner = &rest[open + 1..open + close];
        if closer == '>'
            && let Some(at) = parse_timestamp(inner)
        {
            timed = true;
            segments.push((at, String::new()));
        } else if closer == '}' && !inner.starts_with('\\') {
            if let Some(s) = segments.last_mut() {
                s.1.push_str(&rest[open..=open + close]);
            }
        }
        rest = &rest[open + close + 1..];
    }
    if let Some(s) = segments.last_mut() {
        s.1.push_str(rest);
    }
    let spoken = |t: &str| {
        unescape(t)
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let text = segments
        .iter()
        .flat_map(|(_, t)| spoken(t))
        .collect::<Vec<_>>()
        .join(" ");
    if !timed {
        return (text, Vec::new());
    }
    let mut words = Vec::new();
    for (i, (at, t)) in segments.iter().enumerate() {
        let end = segments.get(i + 1).map_or(cue_end, |s| s.0);
        let list = spoken(t);
        let refs: Vec<&str> = list.iter().map(String::as_str).collect();
        match spread_words(&refs, *at, end) {
            Some(w) => words.extend(w),
            None if refs.is_empty() => {}
            None => return (text, Vec::new()),
        }
    }
    (text, words)
}

/// Cues from SRT or WebVTT text. Both are accepted by the same reader: a block holds an
/// optional identifier, a `start --> end` line (`,` or `.` before the milliseconds, hours
/// optional) and the text lines, which are joined with spaces. Blocks with no text are skipped;
/// a block whose times are wrong is an error that names its line. Cues are put in time order
/// and one that runs into the next is cut where the next begins.
pub fn parse_subtitles(text: &str) -> Result<Vec<Cue>, OpError> {
    let text = text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut drafts: Vec<(u32, u32, String, Vec<Word>)> = Vec::new();
    let mut line_no = 0;
    for block in text.split("\n\n") {
        let first_line = line_no + 1;
        line_no += block.lines().count() + 1;
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(timing) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let at = first_line + timing;
        let fail = |why: &str| bad("subtitles", format!("line {at}: {why}"));
        let tokens: Vec<&str> = lines[timing].split_whitespace().collect();
        let [start, "-->", end, ..] = tokens.as_slice() else {
            return Err(fail("expected `start --> end`"));
        };
        let (Some(start), Some(end)) = (parse_timestamp(start), parse_timestamp(end)) else {
            return Err(fail("a time is not HH:MM:SS,mmm"));
        };
        if end <= start {
            return Err(fail("the cue does not end after it starts"));
        }
        let (text, words) = payload(&lines[timing + 1..].join(" "), start, end);
        if text.is_empty() {
            continue;
        }
        drafts.push((start, end, text, words));
    }
    drafts.sort_by_key(|d| d.0);
    for n in 1..drafts.len() {
        let next = drafts[n].0;
        let prev = &mut drafts[n - 1];
        if prev.1 > next {
            if next <= prev.0 {
                return Err(bad("subtitles", format!("two cues start at {next} ms")));
            }
            prev.1 = next;
            prev.3.retain(|w| w.end_ms <= next);
            if prev.3.len() != prev.2.split_whitespace().count() {
                prev.3.clear();
            }
        }
    }
    let cues = numbered(drafts)?;
    cues.iter().try_for_each(check_cue)?;
    Ok(cues)
}

/// One word of a speech-to-text answer. Both spellings are read: `text` or `word`, and
/// `start_ms`/`end_ms` or `start`/`end` in seconds.
#[derive(Debug, Deserialize)]
struct RawWord {
    #[serde(alias = "word")]
    text: String,
    start_ms: Option<u32>,
    end_ms: Option<u32>,
    start: Option<f64>,
    end: Option<f64>,
}

impl RawWord {
    fn word(self) -> Result<Word, OpError> {
        let secs = |s: f64| {
            (s.is_finite() && (0.0..=MAX_VIDEO_MS as f64 / 1000.0).contains(&s))
                .then(|| (s * 1000.0).round() as u32)
        };
        let start = self.start_ms.or(self.start.and_then(secs));
        let end = self.end_ms.or(self.end.and_then(secs));
        match (start, end) {
            (Some(start_ms), Some(end_ms)) => Ok(Word {
                text: self.text.trim().to_string(),
                start_ms,
                end_ms,
            }),
            _ => Err(bad(
                "word timings",
                format!(
                    "{:?} needs start_ms and end_ms (or start and end in seconds)",
                    self.text
                ),
            )),
        }
    }
}

fn ends_sentence(w: &str) -> bool {
    w.ends_with(['.', '?', '!'])
}

/// Cues from a list of timed words (`[{"text": "Hello", "start_ms": 0, "end_ms": 400}, ...]`, the
/// shape a speech-to-text server gives). Words are grouped into cues that break after a
/// sentence end, at a pause of 700 ms or more, and at 8 words or 42 characters.
pub fn parse_word_timings(json: &str) -> Result<Vec<Cue>, OpError> {
    let raw: Vec<RawWord> = serde_json::from_str(json)
        .map_err(|e| bad("word timings", format!("not a list of timed words: {e}")))?;
    if raw.len() > MAX_CAPTION_WORDS {
        return Err(OpError::Limit(format!(
            "at most {MAX_CAPTION_WORDS} timed words per video"
        )));
    }
    let words = raw
        .into_iter()
        .map(RawWord::word)
        .collect::<Result<Vec<_>, _>>()?;
    let mut groups: Vec<Vec<Word>> = Vec::new();
    for w in words.into_iter().filter(|w| !w.text.is_empty()) {
        let start_new = groups.last().is_none_or(|g| {
            let chars: usize = g.iter().map(|x| x.text.chars().count() + 1).sum();
            g.last().is_some_and(|l| {
                ends_sentence(&l.text)
                    || w.start_ms.saturating_sub(l.end_ms) >= GROUP_GAP_MS
                    || g.len() >= GROUP_WORDS
                    || chars + w.text.chars().count() > GROUP_CHARS
            })
        });
        if start_new {
            groups.push(Vec::new());
        }
        if let Some(g) = groups.last_mut() {
            g.push(w);
        }
    }
    let drafts = groups
        .into_iter()
        .map(|g| {
            let text = g
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            (g[0].start_ms, g[g.len() - 1].end_ms, text, g)
        })
        .collect();
    let cues = numbered(drafts)?;
    cues.iter().try_for_each(check_cue)?;
    check_captions(&Captions {
        style: CaptionStyle::default(),
        cues: cues.clone(),
    })?;
    Ok(cues)
}

/// `cues` shifted by `offset_ms` and fitted to a video of `video_ms`: a cue that ends before the
/// start or begins at or after the end is dropped (and counted), one that crosses an edge is cut
/// there, losing its word timings when a word would be cut. Ids are made again from `q1`.
pub fn fit_cues(
    cues: Vec<Cue>,
    offset_ms: i64,
    video_ms: u64,
) -> Result<(Vec<Cue>, usize), OpError> {
    let limit = i64::try_from(video_ms).unwrap_or(i64::MAX);
    let total = cues.len();
    let mut fitted = Vec::new();
    for cue in cues {
        let shifted = |ms: u32| i64::from(ms) + offset_ms;
        let (start, end) = (shifted(cue.start_ms).max(0), shifted(cue.end_ms).min(limit));
        if end <= start {
            continue;
        }
        let words: Vec<Word> = cue
            .words
            .iter()
            .filter(|w| shifted(w.start_ms) >= start && shifted(w.end_ms) <= end)
            .map(|w| Word {
                text: w.text.clone(),
                start_ms: shifted(w.start_ms) as u32,
                end_ms: shifted(w.end_ms) as u32,
            })
            .collect();
        let words = if words.len() == cue.words.len() {
            words
        } else {
            Vec::new()
        };
        fitted.push((start as u32, end as u32, cue.text, words));
    }
    let dropped = total - fitted.len();
    Ok((numbered(fitted)?, dropped))
}

/// Cues from a plain script, spread over `start_ms..end_ms` in proportion to word length. An
/// estimate: it knows nothing of how the words are actually spoken, so it fits a voice-over only
/// roughly; use a transcript with real timings when there is one. Cues break after a sentence end
/// and at `words_per_cue` words.
pub fn captions_from_script(
    script: &str,
    start_ms: u32,
    end_ms: u32,
    words_per_cue: usize,
) -> Result<Vec<Cue>, OpError> {
    if !(1..=MAX_WORDS_PER_CUE).contains(&words_per_cue) {
        return Err(bad(
            "words_per_cue",
            format!("{words_per_cue} is not from 1 to {MAX_WORDS_PER_CUE}"),
        ));
    }
    let list: Vec<&str> = script.split_whitespace().collect();
    if list.is_empty() {
        return Err(bad("script", "has no words".into()));
    }
    if list.len() > MAX_CAPTION_WORDS {
        return Err(OpError::Limit(format!(
            "at most {MAX_CAPTION_WORDS} words per video"
        )));
    }
    let timed = spread_words(&list, start_ms, end_ms).ok_or_else(|| {
        bad(
            "script",
            format!(
                "{} words need at least {} ms, not {start_ms}..{end_ms}",
                list.len(),
                list.len()
            ),
        )
    })?;
    let mut groups: Vec<Vec<Word>> = vec![Vec::new()];
    for w in timed {
        let full = groups.last().is_some_and(|g| {
            g.len() >= words_per_cue || g.last().is_some_and(|l| ends_sentence(&l.text))
        });
        if full {
            groups.push(Vec::new());
        }
        if let Some(g) = groups.last_mut() {
            g.push(w);
        }
    }
    let drafts = groups
        .into_iter()
        .map(|g| {
            let text = g
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            (g[0].start_ms, g[g.len() - 1].end_ms, text, g)
        })
        .collect();
    let cues = numbered(drafts)?;
    cues.iter().try_for_each(check_cue)?;
    Ok(cues)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(id: &str, start: u32, end: u32, text: &str) -> Cue {
        Cue {
            id: NodeId::parse(id).unwrap(),
            start_ms: start,
            end_ms: end,
            text: text.into(),
            words: Vec::new(),
        }
    }

    #[test]
    fn timestamps_take_both_separators_and_short_forms() {
        assert_eq!(parse_timestamp("00:01:02,500"), Some(62_500));
        assert_eq!(parse_timestamp("01:02.5"), Some(62_500));
        assert_eq!(parse_timestamp("1:00:00.001"), Some(3_600_001));
        for bad in ["", "1", "00:00:00,0000", "a:00", "00:-1", "00:00:00,"] {
            assert_eq!(parse_timestamp(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn srt_with_crlf_bom_multiline_text_and_markup() {
        let srt = "\u{feff}1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Hello</i>\r\nthere &amp; you\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n{\\an8}Second\r\n";
        let cues = parse_subtitles(srt).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(
            (cues[0].start_ms, cues[0].end_ms, cues[0].text.as_str()),
            (1000, 2500, "Hello there & you")
        );
        assert_eq!(cues[1].text, "Second");
        assert_eq!(cues[1].id.as_str(), "q2");
    }

    #[test]
    fn vtt_header_notes_settings_and_inline_word_times() {
        let vtt = "WEBVTT - demo\n\nNOTE a comment\nspanning lines\n\nintro\n00:01.000 --> 00:03.000 align:start position:0%\n<v Ann>One <00:02.000>two three</v>\n";
        let cues = parse_subtitles(vtt).unwrap();
        assert_eq!(cues.len(), 1);
        let c = &cues[0];
        assert_eq!(c.text, "One two three");
        assert_eq!(c.words.len(), 3);
        assert_eq!((c.words[0].start_ms, c.words[0].end_ms), (1000, 2000));
        assert_eq!(c.words[1].start_ms, 2000);
        assert_eq!(c.words[2].end_ms, 3000);
        check_cue(c).unwrap();
    }

    #[test]
    fn subtitle_errors_name_the_line_and_empty_cues_are_skipped() {
        let err = parse_subtitles("1\n00:00:02,000 --> 00:00:01,000\nx\n").unwrap_err();
        assert!(err.to_string().contains("line 2"), "{err}");
        let err = parse_subtitles("1\n00:00:xx,000 --> 00:00:01,000\nx\n").unwrap_err();
        assert!(err.to_string().contains("HH:MM:SS"), "{err}");
        let cues = parse_subtitles(
            "1\n00:00:00,000 --> 00:00:01,000\n<i></i>\n\n2\n00:00:01,000 --> 00:00:02,000\nok\n",
        )
        .unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].id.as_str(), "q1");
    }

    #[test]
    fn cues_are_sorted_and_overlaps_cut_where_the_next_starts() {
        let srt =
            "2\n00:00:02,000 --> 00:00:05,000\nlate\n\n1\n00:00:00,000 --> 00:00:03,000\nearly\n";
        let cues = parse_subtitles(srt).unwrap();
        assert_eq!(cues[0].text, "early");
        assert_eq!(cues[0].end_ms, 2000);
        let twin = "1\n00:00:00,000 --> 00:00:03,000\na\n\n2\n00:00:00,000 --> 00:00:01,000\nb\n";
        assert!(parse_subtitles(twin).is_err());
    }

    #[test]
    fn too_many_cues_are_refused() {
        let t = |ms: u32| {
            format!(
                "{:02}:{:02}:{:02},{:03}",
                ms / 3_600_000,
                ms / 60_000 % 60,
                ms / 1000 % 60,
                ms % 1000
            )
        };
        let mut srt = String::new();
        for n in 0..=MAX_CUES as u32 {
            srt.push_str(&format!(
                "{n}\n{} --> {}\nx\n\n",
                t(n * 100),
                t(n * 100 + 50)
            ));
        }
        assert!(matches!(parse_subtitles(&srt), Err(OpError::Limit(_))));
    }

    #[test]
    fn word_timings_group_into_cues_and_read_both_spellings() {
        let json = r#"[
            {"text": "Hello", "start_ms": 0, "end_ms": 400},
            {"word": "world.", "start": 0.4, "end": 0.9, "probability": 0.98},
            {"text": "Next", "start_ms": 1000, "end_ms": 1300},
            {"text": "after", "start_ms": 2500, "end_ms": 2800}
        ]"#;
        let cues = parse_word_timings(json).unwrap();
        let texts: Vec<&str> = cues.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["Hello world.", "Next", "after"]);
        assert_eq!(cues[0].words[1].start_ms, 400);
        assert_eq!((cues[2].start_ms, cues[2].end_ms), (2500, 2800));
        assert!(parse_word_timings(r#"[{"text": "x", "start_ms": 5}]"#).is_err());
        assert!(parse_word_timings(r#"{"text": "x"}"#).is_err());
    }

    #[test]
    fn a_script_is_spread_by_word_length_and_ends_exactly_at_the_range() {
        let cues = captions_from_script(
            "Hi there. A much longer sentence follows here.",
            1000,
            5000,
            4,
        )
        .unwrap();
        assert_eq!(cues[0].text, "Hi there.");
        assert_eq!(cues.last().map(|c| c.end_ms), Some(5000));
        assert_eq!(cues[0].start_ms, 1000);
        let words: Vec<&Word> = cues.iter().flat_map(|c| &c.words).collect();
        assert!(words.windows(2).all(|p| p[0].end_ms == p[1].start_ms));
        let long = words.iter().find(|w| w.text == "sentence").unwrap();
        let short = words.iter().find(|w| w.text == "A").unwrap();
        assert!(long.end_ms - long.start_ms > short.end_ms - short.start_ms);
        assert!(captions_from_script("a b c", 0, 2, 3).is_err());
        assert!(captions_from_script("  ", 0, 1000, 3).is_err());
        assert!(captions_from_script("a", 0, 1000, 0).is_err());
    }

    #[test]
    fn bounds_cover_order_overlap_words_and_length() {
        let ok = Captions {
            style: CaptionStyle::default(),
            cues: vec![cue("q1", 0, 1000, "a b"), cue("q2", 1000, 2000, "c")],
        };
        assert!(check_captions(&ok).is_ok());
        assert!(check_within(&ok, 2000).is_ok());
        assert!(check_within(&ok, 1500).is_err());

        let mut overlap = ok.clone();
        overlap.cues[1].start_ms = 900;
        assert!(
            check_captions(&overlap)
                .unwrap_err()
                .to_string()
                .contains("overlap")
        );

        let mut twin = ok.clone();
        twin.cues[1].id = NodeId::parse("q1").unwrap();
        assert!(matches!(
            check_captions(&twin),
            Err(OpError::DuplicateId(_))
        ));

        let mut backwards = ok.clone();
        backwards.cues[0].end_ms = 0;
        assert!(check_captions(&backwards).is_err());

        let mut words = ok.clone();
        words.cues[0].words = vec![
            Word {
                text: "a".into(),
                start_ms: 0,
                end_ms: 400,
            },
            Word {
                text: "b".into(),
                start_ms: 300,
                end_ms: 900,
            },
        ];
        assert!(
            check_captions(&words)
                .unwrap_err()
                .to_string()
                .contains("before the word")
        );
        words.cues[0].words[1].start_ms = 400;
        assert!(check_captions(&words).is_ok());
        words.cues[0].words[1].text = "z".into();
        assert!(
            check_captions(&words)
                .unwrap_err()
                .to_string()
                .contains("spell")
        );

        let mut long = ok.clone();
        long.cues[0].text = "x".repeat(MAX_CUE_TEXT + 1);
        assert!(check_captions(&long).is_err());

        let mut empty = ok;
        empty.cues[0].text = "  ".into();
        assert!(check_captions(&empty).is_err());
    }

    #[test]
    fn cues_are_shifted_and_cut_to_the_video_and_those_outside_are_counted() {
        let words = spread_words(&["a", "b"], 1000, 2000).unwrap();
        let mut a = cue("q1", 1000, 2000, "a b");
        a.words = words;
        let b = cue("q2", 9000, 10_000, "late");
        let c = cue("q3", 0, 600, "early");
        let (fitted, dropped) = fit_cues(vec![c, a.clone(), b], 500, 5000).unwrap();
        assert_eq!(dropped, 1);
        assert_eq!(fitted.len(), 2);
        assert_eq!((fitted[0].start_ms, fitted[0].end_ms), (500, 1100));
        assert_eq!(fitted[1].id.as_str(), "q2");
        assert_eq!(
            (fitted[1].words[0].start_ms, fitted[1].words[1].end_ms),
            (1500, 2500)
        );
        let (cut, _) = fit_cues(vec![a], -1500, 5000).unwrap();
        assert_eq!((cut[0].start_ms, cut[0].end_ms), (0, 500));
        assert!(
            cut[0].words.is_empty(),
            "a word was cut off, so no word is timed"
        );
    }

    #[test]
    fn style_bounds_and_presets() {
        for p in CaptionPreset::ALL {
            let style = CaptionStyle::preset(p);
            check_style(&style).unwrap();
            assert_eq!(serde_json::to_value(p).unwrap(), p.name());
        }
        assert!(CaptionStyle::preset(CaptionPreset::BoldPop).uppercase);
        for broken in [
            CaptionStyle {
                size_pct: 0.0,
                ..CaptionStyle::default()
            },
            CaptionStyle {
                size_pct: f32::NAN,
                ..CaptionStyle::default()
            },
            CaptionStyle {
                font_weight: 50,
                ..CaptionStyle::default()
            },
            CaptionStyle {
                max_lines: 0,
                ..CaptionStyle::default()
            },
            CaptionStyle {
                color: "white".into(),
                ..CaptionStyle::default()
            },
            CaptionStyle {
                highlight: Some("#12".into()),
                ..CaptionStyle::default()
            },
            CaptionStyle {
                font_family: " ".into(),
                ..CaptionStyle::default()
            },
        ] {
            assert!(check_style(&broken).is_err(), "{broken:?}");
        }
    }

    #[test]
    fn the_active_cue_and_word_follow_the_time() {
        let mut first = cue("q1", 1000, 2000, "one two");
        first.words = spread_words(&["one", "two"], 1000, 2000).unwrap();
        let captions = Captions {
            style: CaptionStyle::default(),
            cues: vec![first, cue("q2", 3000, 4000, "three")],
        };
        assert!(captions.active(999).is_none());
        assert_eq!(captions.active(1000).map(|c| c.id.as_str()), Some("q1"));
        assert!(captions.active(2000).is_none());
        assert_eq!(captions.active(3999).map(|c| c.id.as_str()), Some("q2"));
        let c = captions.active(1900).unwrap();
        assert_eq!(Captions::active_word(c, 1000), Some(0));
        assert_eq!(Captions::active_word(c, 1900), Some(1));
        assert_eq!(Captions::active_word(c, 999), None);
    }

    #[test]
    fn an_empty_default_captions_stores_nothing() {
        assert!(Captions::default().is_empty());
        let styled = Captions {
            style: CaptionStyle::preset(CaptionPreset::Boxed),
            cues: Vec::new(),
        };
        assert!(!styled.is_empty());
        let json = serde_json::to_string(&Captions::default()).unwrap();
        let back: Captions = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Captions::default());
    }
}
