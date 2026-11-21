//! The catalog: every block's declaration, and keyword search over it.

use od_core::geometry::Rect;
use serde_json::{Value, json};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::Kit;
use super::{
    backdrops, bubbles, characters, data, emotes, guides, land, micro, overlays, props, sky,
    social, titles, wrappers,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Titles,
    Overlays,
    Backgrounds,
    Data,
    Store,
    Motion,
    Characters,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Titles,
        Category::Overlays,
        Category::Backgrounds,
        Category::Data,
        Category::Store,
        Category::Motion,
        Category::Characters,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Titles => "titles",
            Category::Overlays => "overlays",
            Category::Backgrounds => "backgrounds",
            Category::Data => "data",
            Category::Store => "store",
            Category::Motion => "motion",
            Category::Characters => "characters",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Category::Titles => "Titles and text",
            Category::Overlays => "Lower thirds and overlays",
            Category::Backgrounds => "Backgrounds",
            Category::Data => "Data",
            Category::Store => "Social and store",
            Category::Motion => "Motion wrappers",
            Category::Characters => "Characters, bubbles and props",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Whether a block sits over the frame's content or behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Front,
    Back,
    /// Behind the content, in a fixed order among the scenery blocks whatever order they are
    /// added in: a lower rank sits further back (sky 0, then stars, sun, clouds, hills, water).
    Scenery(u8),
}

/// Where a block lays itself out when the caller gives no box.
#[derive(Debug, Clone, Copy)]
pub enum Place {
    /// The frame inside its margin.
    Safe,
    /// The whole frame.
    Frame,
    /// Chosen by the block from the frame and its margin.
    Custom(fn(Rect, f64) -> Rect),
}

impl Place {
    pub fn resolve(self, frame: Rect, margin: f64) -> Rect {
        match self {
            Place::Safe => super::kit::inset(frame, margin),
            Place::Frame => frame,
            Place::Custom(f) => f(frame, margin),
        }
    }
}

pub struct Block {
    pub id: &'static str,
    pub category: Category,
    pub about: &'static str,
    pub tags: &'static [&'static str],
    pub params: &'static [Param],
    pub place: Place,
    pub layer: Layer,
    pub build: fn(&mut Kit, &Args) -> Result<(), BlockError>,
}

impl Block {
    pub fn summary(&self) -> Value {
        json!({"id": self.id, "category": self.category.as_str(), "about": self.about, "tags": self.tags})
    }

    pub fn describe(&self) -> Value {
        json!({
            "id": self.id,
            "category": self.category.as_str(),
            "category_label": self.category.label(),
            "about": self.about,
            "tags": self.tags,
            "layer": match self.layer { Layer::Front => "in front of the frame's content", Layer::Back => "behind the frame's content", Layer::Scenery(_) => "behind the frame's content, stacked with other scenery from far to near" },
            "default_box": match self.place {
                Place::Safe => "the frame inside its safe margin",
                Place::Frame => "the whole frame",
                Place::Custom(_) => "an area the block chooses from the frame (see about)",
            },
            "params": self.params.iter().map(Param::describe).collect::<Vec<_>>(),
            "standard_params": {
                "frame": "target frame id (optional with one frame)",
                "video": "video id to keyframe in (optional with one video)",
                "start_ms": "when it starts, from the start of the video",
                "hold_ms": "fade out this long after start_ms",
                "box": "{x, y, width, height} in frame coordinates",
                "colors": "{background, foreground, accent, muted} as #rrggbb or {token}",
                "font": "one font family for all its text",
            },
        })
    }
}

/// Every block, in catalog order.
pub fn all() -> impl Iterator<Item = &'static Block> {
    [
        titles::BLOCKS,
        overlays::BLOCKS,
        backdrops::BLOCKS,
        data::BLOCKS,
        social::BLOCKS,
        wrappers::BLOCKS,
        characters::BLOCKS,
        bubbles::BLOCKS,
        emotes::BLOCKS,
        props::BLOCKS,
        sky::BLOCKS,
        land::BLOCKS,
        micro::BLOCKS,
        guides::BLOCKS,
    ]
    .into_iter()
    .flat_map(|b| b.iter())
}

pub fn find(id: &str) -> Result<&'static Block, BlockError> {
    all().find(|b| b.id == id).ok_or_else(|| {
        let near: Vec<&str> = search(id, None).iter().take(5).map(|b| b.id).collect();
        BlockError::Param(format!(
            "no block {id:?}{}; list_blocks searches the catalog",
            if near.is_empty() {
                String::new()
            } else {
                format!(" (did you mean {}?)", near.join(", "))
            }
        ))
    })
}

fn score(b: &Block, words: &[String]) -> u32 {
    let id = b.id.replace('_', " ");
    words
        .iter()
        .map(|w| {
            let mut s = 0;
            if id == *w {
                s += 100;
            } else if id.contains(w.as_str()) {
                s += 10;
            }
            if b.tags.iter().any(|t| t == w) {
                s += 6;
            } else if b.tags.iter().any(|t| t.contains(w.as_str())) {
                s += 3;
            }
            if b.category.as_str().contains(w.as_str()) {
                s += 4;
            }
            if b.about.to_ascii_lowercase().contains(w.as_str()) {
                s += 2;
            }
            s
        })
        .sum()
}

/// Blocks matching `query` (words, any order) best first, within `category`; all of them in
/// catalog order when the query is empty.
pub fn search(query: &str, category: Option<Category>) -> Vec<&'static Block> {
    let words: Vec<String> = query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let mut found: Vec<(u32, usize, &'static Block)> = all()
        .enumerate()
        .filter(|(_, b)| category.is_none_or(|c| b.category == c))
        .map(|(i, b)| (score(b, &words), i, b))
        .filter(|(s, ..)| words.is_empty() || *s > 0)
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, b)| b).collect()
}
