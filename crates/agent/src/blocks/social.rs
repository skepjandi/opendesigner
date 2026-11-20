//! Social and store blocks: the headline pair of a store slide, an icon feature list, a star
//! rating and a review card.

use std::path::Path;

use od_core::TextAlign;
use od_core::geometry::Rect;
use serde_json::Value;

use super::BlockError;
use super::args::{Args, Param};
use super::files::Asset;
use super::kit::{Kit, Role, Row, Style, Ty, align_of, at, ellipse, mix, picture, rect};
use super::registry::{Block, Category, Layer, Place};
use crate::assets::index;

fn store_zone(frame: Rect, margin: f64) -> Rect {
    if frame.height > frame.width * 1.1 {
        at(
            margin,
            margin * 1.4,
            frame.width - 2.0 * margin,
            frame.height * 0.3,
        )
    } else {
        at(
            margin,
            margin,
            frame.width * 0.5 - margin,
            frame.height - 2.0 * margin,
        )
    }
}

const HEADLINE_PAIR: &[Param] = &[
    Param::text(
        "headline",
        "The headline: short, one idea; shrinks to fit.",
        "Design stunning store assets in minutes",
        70,
    ),
    Param::text(
        "subhead",
        "The supporting line (empty for none).",
        "Every screenshot, preview and icon from one editor.",
        120,
    ),
    Param::choice("align", "Text alignment.", "left", &["left", "center"]),
];

fn app_store_headline_pair(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let portrait = k.portrait();
    let align = align_of(a.text("align"));
    let area = k.b;
    let size = k.s(if portrait { 8.6 } else { 7.2 });
    let h_ty = k.ty(size, 700.0, Role::Foreground).align(align).line(1.1);
    let mut rows = vec![k.row(
        "headline",
        a.text("headline"),
        &h_ty,
        area.width,
        area.height * 0.7,
        3,
    )?];
    let sub = a.text("subhead");
    if !sub.is_empty() {
        rows.push(Row::Gap(k.s(2.2)));
        let s_size = k.s(if portrait { 3.6 } else { 3.2 });
        let s_ty = k.ty(s_size, 400.0, Role::Muted).align(align).line(1.35);
        rows.push(k.row(
            "subhead",
            sub,
            &s_ty,
            area.width * 0.92,
            area.height * 0.34,
            3,
        )?);
    }
    k.stack(rows, area, !portrait);
    k.rise("headline", 0, 650);
    if !sub.is_empty() {
        k.rise("subhead", 180, 650);
    }
    Ok(())
}

const FEATURES_DEFAULT: &str = r#"[{"icon":"zap","title":"Fast exports","text":"Render a full set in seconds."},{"icon":"shield-check","title":"Private by default","text":"Everything stays on your device."},{"icon":"palette","title":"Your brand, everywhere","text":"Change a token, recolor everything."}]"#;

const FEATURES: &[Param] = &[
    Param::text(
        "title",
        "A heading above the list (empty for none).",
        "",
        50,
    ),
    Param::data(
        "items",
        "2 to 6 rows: [{\"icon\": Lucide name, \"title\": text, \"text\": optional text}].",
        FEATURES_DEFAULT,
    ),
];

struct Feature {
    icon: String,
    title: String,
    text: String,
}

fn features_of(v: &Value) -> Result<Vec<Feature>, BlockError> {
    let items = v
        .as_array()
        .filter(|l| (2..=6).contains(&l.len()))
        .ok_or_else(|| BlockError::Param("items needs 2 to 6 rows".into()))?;
    items
        .iter()
        .map(|i| {
            let get = |key: &str, max: usize, required: bool| -> Result<String, BlockError> {
                match i.get(key).and_then(Value::as_str) {
                    Some(s) if s.chars().count() <= max && !s.chars().any(char::is_control) => {
                        Ok(s.to_string())
                    }
                    None if !required => Ok(String::new()),
                    _ => Err(BlockError::Param(format!(
                        "items.{key} must be text of at most {max} characters"
                    ))),
                }
            };
            let icon = get("icon", 40, true)?;
            index::resolve(Path::new(""), &format!("lucide:{icon}")).map_err(|_| {
                BlockError::Param(format!(
                    "items.icon: no Lucide icon {icon:?}; search_assets finds names"
                ))
            })?;
            Ok(Feature {
                icon,
                title: get("title", 36, true)?,
                text: get("text", 70, false)?,
            })
        })
        .collect()
}

fn feature_list(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let items = features_of(a.data("items"))?;
    let area = k.b;
    let mut top = area.y;
    let heading = a.text("title");
    if !heading.is_empty() {
        let size = k.s(4.2);
        let ty = k.ty(size, 700.0, Role::Foreground);
        let row = k.row("title", heading, &ty, area.width, size * 1.7, 1)?;
        let h = row.height();
        k.stack(vec![row], at(area.x, top, area.width, h), false);
        k.rise("title", 0, 500);
        top += h + k.s(3.0);
    }
    let row_h = ((area.y + area.height - top) / items.len() as f64).min(k.s(14.0));
    let d = (row_h * 0.62).min(k.s(8.0));
    let glyph = d * 0.55;
    let (fg, muted, accent) = (
        k.paint(Role::Foreground),
        k.paint(Role::Muted),
        k.paint(Role::Accent),
    );
    let text_x = area.x + d + k.s(2.4);
    let text_w = area.x + area.width - text_x;
    let used = row_h * items.len() as f64;
    let y0 = top + ((area.y + area.height - top - used) / 2.0).max(0.0);
    for (i, f) in items.iter().enumerate() {
        let id = i + 1;
        let y = y0 + row_h * i as f64;
        let group = format!("row-{id}");
        k.begin(&group);
        let part = |s: &str| format!("{group}/{s}");
        let (dx, dy) = (area.x, y + (row_h - d) / 2.0);
        k.add(
            &part("disc"),
            ellipse(at(dx, dy, d, d), accent.clone()).alpha(0.18),
        );
        let file = k.icon(&f.icon, Role::Accent, glyph);
        k.add(
            &part("icon"),
            picture(
                at(dx + (d - glyph) / 2.0, dy + (d - glyph) / 2.0, glyph, glyph),
                file,
            ),
        );
        let t_size = k.s(3.3);
        let (t_ty, t_h) = k.fit(
            &f.title,
            &Ty::of(t_size, 700.0, fg.clone()),
            text_w,
            t_size * 1.6,
            1,
        )?;
        let (b_ty, b_h) = if f.text.is_empty() {
            (Ty::of(k.s(2.6), 400.0, muted.clone()), 0.0)
        } else {
            let b_size = k.s(2.7);
            k.fit(
                &f.text,
                &Ty::of(b_size, 400.0, muted.clone()).line(1.3),
                text_w,
                b_size * 3.0,
                2,
            )?
        };
        let total = t_h + b_h;
        let ty0 = y + (row_h - total) / 2.0;
        k.text(&part("title"), &f.title, &t_ty, text_x, ty0, text_w);
        if !f.text.is_empty() {
            k.text(&part("text"), &f.text, &b_ty, text_x, ty0 + t_h, text_w);
        }
        k.end();
        k.reveal_group(&group, 150 + 140 * i as u64, 500, k.s(1.5));
    }
    Ok(())
}

/// A row of five stars, `filled` of them in the accent color, rising in one after another when
/// `delay` is given; returns the width it takes.
fn stars(
    k: &mut Kit,
    prefix: &str,
    x: f64,
    y: f64,
    size: f64,
    filled: usize,
    delay: Option<u64>,
) -> f64 {
    let (accent, bg, muted) = (
        k.paint(Role::Accent),
        k.paint(Role::Background),
        k.paint(Role::Muted),
    );
    let (a, b, m) = (k.rgba(&accent), k.rgba(&bg), k.rgba(&muted));
    let full = Asset::shape("star", a.to_hex(), size);
    let empty = Asset::shape("star", mix(b, m, 0.45), size);
    let gap = size * 0.18;
    for i in 0..5 {
        let asset = if i < filled {
            full.clone()
        } else {
            empty.clone()
        };
        let file = k.asset(asset);
        let part = format!("{prefix}star-{}", i + 1);
        k.add(
            &part,
            picture(at(x + (size + gap) * i as f64, y, size, size), file),
        );
        if let Some(delay) = delay {
            k.rise(&part, delay + 90 * i as u64, 450);
        }
    }
    5.0 * size + 4.0 * gap
}

const RATING: &[Param] = &[
    Param::number(
        "rating",
        "The rating out of 5 (stars are rounded to whole ones; the number is shown exactly).",
        4.8,
        0.0,
        5.0,
    ),
    Param::text(
        "count",
        "Under the stars, like the number of ratings (empty for none).",
        "from 12,400 ratings",
        40,
    ),
    Param::choice(
        "align",
        "Where it sits in the box.",
        "left",
        &["left", "center"],
    ),
];

fn rating_stars(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let size = k.s(5.6);
    let fg = k.paint(Role::Foreground);
    let number = format!("{:.1}", a.num("rating"));
    let n_ty = Ty::of(size * 0.95, 700.0, fg);
    let n_w = k.width_of(&number, &n_ty);
    let stars_w = 5.0 * size + 4.0 * size * 0.18;
    let gap = size * 0.5;
    let total_w = stars_w + gap + n_w;
    if total_w > k.b.width {
        return Err(BlockError::Fit("the rating is wider than its box".into()));
    }
    let x = if align_of(a.text("align")) == TextAlign::Center {
        k.b.x + (k.b.width - total_w) / 2.0
    } else {
        k.b.x
    };
    stars(
        k,
        "",
        x,
        k.b.y,
        size,
        a.num("rating").round() as usize,
        Some(0),
    );
    let nh = k.measure(&number, &n_ty, 0.0).1;
    k.text(
        "number",
        &number,
        &n_ty,
        x + stars_w + gap,
        k.b.y + (size - nh) / 2.0,
        n_w,
    );
    k.fade("number", 500, 500);
    let count = a.text("count");
    if !count.is_empty() {
        let muted = k.paint(Role::Muted);
        let c_ty = Ty::of(k.s(2.8), 400.0, muted);
        let w = k.width_of(count, &c_ty).min(k.b.width);
        let cx = if align_of(a.text("align")) == TextAlign::Center {
            k.b.x + (k.b.width - w) / 2.0
        } else {
            x
        };
        k.text("count", count, &c_ty, cx, k.b.y + size + k.s(1.2), w);
        k.fade("count", 650, 500);
    }
    Ok(())
}

const REVIEW: &[Param] = &[
    Param::count("stars", "Whole stars out of 5.", 5, 0, 5),
    Param::text(
        "quote",
        "The review text, quoted from a real review.",
        "Finally a tool that makes store assets feel effortless.",
        200,
    ),
    Param::text("author", "Who wrote it.", "Sam K.", 40),
    Param::text(
        "source",
        "Where it was posted (empty for none).",
        "App Store review",
        40,
    ),
];

fn review_card(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pad = k.s(3.0);
    let w = k.b.width.min(k.s(80.0));
    let inner = w - 2.0 * pad;
    let (fg, muted) = (k.paint(Role::Foreground), k.paint(Role::Muted));
    let star = k.s(3.8);
    let q_size = k.s(3.5);
    let (q_ty, q_h) = k.fit(
        a.text("quote"),
        &Ty::of(q_size, 400.0, fg.clone()).line(1.35),
        inner,
        k.b.height * 0.55,
        7,
    )?;
    let a_size = k.s(2.9);
    let (a_ty, a_h) = k.fit(
        a.text("author"),
        &Ty::of(a_size, 700.0, fg),
        inner,
        a_size * 1.6,
        1,
    )?;
    let source = a.text("source");
    let s_size = k.s(2.4);
    let (s_ty, s_h) = if source.is_empty() {
        (Ty::of(s_size, 400.0, muted.clone()), 0.0)
    } else {
        k.fit(
            source,
            &Ty::of(s_size, 400.0, muted),
            inner,
            s_size * 1.6,
            1,
        )?
    };
    let gap = k.s(1.8);
    let h = pad + star + gap + q_h + gap + a_h + s_h + pad;
    let (x, y) = (
        k.b.x + (k.b.width - w) / 2.0,
        k.b.y + (k.b.height - h).max(0.0) / 2.0,
    );
    let (panel_fill, line) = (k.paint(Role::Foreground), k.paint(Role::Muted));
    k.begin("card");
    k.add(
        "card/panel",
        rect(at(x, y, w, h), panel_fill)
            .radius(k.s(2.0))
            .alpha(0.07)
            .outline(line, 1.0),
    );
    stars(
        k,
        "card/",
        x + pad,
        y + pad,
        star,
        a.num("stars") as usize,
        None,
    );
    let mut cy = y + pad + star + gap;
    k.text("card/quote", a.text("quote"), &q_ty, x + pad, cy, inner);
    cy += q_h + gap;
    k.text("card/author", a.text("author"), &a_ty, x + pad, cy, inner);
    if !source.is_empty() {
        k.text("card/source", source, &s_ty, x + pad, cy + a_h, inner);
    }
    k.end();
    k.reveal_group("card", 0, 650, k.s(2.0));
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "app_store_headline_pair",
        category: Category::Store,
        about: "A headline and subhead sized for a store slide: across the top of a portrait frame, in a left column of a landscape or square one.",
        tags: &[
            "app store",
            "play store",
            "screenshot",
            "headline",
            "subhead",
            "slide",
            "listing",
        ],
        params: HEADLINE_PAIR,
        place: Place::Custom(store_zone),
        layer: Layer::Front,
        build: app_store_headline_pair,
    },
    Block {
        id: "feature_list",
        category: Category::Store,
        about: "2 to 6 feature rows, each a Lucide icon on a soft accent disc with a title and a line of text (icons are written to media/ with their credit).",
        tags: &[
            "features",
            "list",
            "icons",
            "benefits",
            "bullets",
            "icon list",
            "highlights",
        ],
        params: FEATURES,
        place: Place::Safe,
        layer: Layer::Front,
        build: feature_list,
    },
    Block {
        id: "rating_stars",
        category: Category::Store,
        about: "Five stars filled to a rating with the number beside them and a ratings count under. Use real ratings only.",
        tags: &[
            "rating",
            "stars",
            "reviews",
            "score",
            "social proof",
            "app store",
        ],
        params: RATING,
        place: Place::Safe,
        layer: Layer::Front,
        build: rating_stars,
    },
    Block {
        id: "review_card",
        category: Category::Store,
        about: "A review on a soft card: stars, the quote, the author and where it was posted. Quote real reviews only.",
        tags: &[
            "review",
            "testimonial",
            "quote",
            "social proof",
            "customer",
            "card",
        ],
        params: REVIEW,
        place: Place::Safe,
        layer: Layer::Front,
        build: review_card,
    },
];
