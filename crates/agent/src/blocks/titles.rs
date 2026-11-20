//! Titles and text: cards, headlines, captions, quotes, big numbers and small labels.

use od_core::animation::Preset;
use od_core::geometry::Rect;
use od_core::{Ease, TextAlign};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Role, Row, Style, Target, Ty, align_of, at, frame_line, rect};
use super::registry::{Block, Category, Layer, Place};

const ALIGNS: &[&str] = &["left", "center"];

/// Left edge of something `w` wide aligned inside `area`.
fn x_in(align: TextAlign, area: Rect, w: f64) -> f64 {
    match align {
        TextAlign::Center => area.x + (area.width - w) / 2.0,
        TextAlign::Right => area.x + area.width - w,
        TextAlign::Left => area.x,
    }
}

const TITLE_CARD: &[Param] = &[
    Param::text(
        "eyebrow",
        "Small label above the title (empty for none).",
        "Introducing",
        40,
    ),
    Param::text(
        "title",
        "The headline; shrinks to fit up to 3 lines.",
        "Your headline goes here",
        90,
    ),
    Param::text(
        "subtitle",
        "One supporting line (empty for none).",
        "One line that says why it matters.",
        140,
    ),
    Param::choice("align", "Text alignment.", "left", ALIGNS),
    Param::flag("rule", "An accent rule between title and subtitle.", true),
    Param::flag("panel", "Fill the box with the background color.", true),
];

fn title_card(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    if a.flag("panel") {
        let bg = k.paint(Role::Background);
        k.add("panel", rect(k.b, bg));
    }
    let area = k.content();
    let align = align_of(a.text("align"));
    let mut rows = Vec::new();
    let eyebrow = a.text("eyebrow");
    if !eyebrow.is_empty() {
        let size = k.s(2.6);
        let ty = k.ty(size, 700.0, Role::Accent).align(align);
        rows.push(k.row("eyebrow", eyebrow, &ty, area.width, area.height * 0.1, 1)?);
        rows.push(Row::Gap(k.s(2.0)));
    }
    let size = k.s(if k.portrait() { 9.5 } else { 8.0 });
    let ty = k.ty(size, 700.0, Role::Foreground).align(align).line(1.1);
    rows.push(k.row(
        "title",
        a.text("title"),
        &ty,
        area.width,
        area.height * 0.6,
        3,
    )?);
    let rule_row = rows.len() + 1;
    let subtitle = a.text("subtitle");
    if a.flag("rule") || !subtitle.is_empty() {
        rows.push(Row::Gap(k.s(if a.flag("rule") { 5.0 } else { 2.0 })));
    }
    if !subtitle.is_empty() {
        let size = k.s(3.2);
        let ty = k.ty(size, 400.0, Role::Muted).align(align).line(1.35);
        rows.push(k.row(
            "subtitle",
            subtitle,
            &ty,
            area.width * 0.85,
            area.height * 0.25,
            3,
        )?);
    }
    let has_eyebrow = !eyebrow.is_empty();
    let placed = k.stack(rows, area, true);
    if has_eyebrow {
        k.rise("eyebrow", 0, 500);
    }
    k.rise("title", 140, 700);
    if a.flag("rule") {
        let gap = placed[rule_row - 1];
        let (w, h) = (k.s(9.0), (k.s(0.7)).max(3.0));
        let x = x_in(align, area, w);
        let accent = k.paint(Role::Accent);
        k.add(
            "rule",
            rect(at(x, gap.y + (gap.height - h) / 2.0, w, h), accent).radius(h / 2.0),
        );
        k.grow_x("rule", x, w, align == TextAlign::Center, 320, 600);
    }
    if !subtitle.is_empty() {
        k.rise("subtitle", 440, 700);
    }
    Ok(())
}

const KINETIC: &[Param] = &[
    Param::text(
        "text",
        "The headline, revealed word by word (up to 24 words).",
        "Make every word land",
        160,
    ),
    Param::choice("align", "Text alignment.", "left", &["left", "center"]),
    Param::count(
        "accent_words",
        "How many words at the end take the accent color.",
        1,
        0,
        6,
    ),
    Param::count("stagger_ms", "Delay between words.", 90, 20, 400),
    Param::flag(
        "plate",
        "A background plate behind the words, for use over footage.",
        false,
    ),
];

const MAX_WORDS: usize = 24;

/// Words flowed left to right into lines no wider than `max_w`: `(word index, width)` per line.
fn flow(words: &[(f64, usize)], space: f64, max_w: f64) -> Vec<Vec<(usize, f64)>> {
    let mut lines: Vec<Vec<(usize, f64)>> = vec![Vec::new()];
    let mut used = 0.0;
    for &(w, i) in words {
        let need = if used == 0.0 { w } else { used + space + w };
        if need > max_w && used > 0.0 {
            lines.push(Vec::new());
            used = w;
        } else {
            used = need;
        }
        if let Some(line) = lines.last_mut() {
            line.push((i, w));
        }
    }
    lines
}

fn kinetic_headline(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let words: Vec<&str> = a.text("text").split_whitespace().collect();
    if words.is_empty() || words.len() > MAX_WORDS {
        return Err(BlockError::Param(format!(
            "kinetic_headline.text needs 1 to {MAX_WORDS} words"
        )));
    }
    let align = align_of(a.text("align"));
    let area = k.b;
    let pad = if a.flag("plate") { k.s(3.0) } else { 0.0 };
    let inner = super::kit::inset(area, pad);
    let first = k.s(if k.portrait() { 11.0 } else { 9.0 });
    let base = k.ty(first, 700.0, Role::Foreground).line(1.12);
    let mut size = first;
    let (lines, space, line_h) = loop {
        let ty = base.clone().size(size);
        let widths: Vec<(f64, usize)> = words
            .iter()
            .enumerate()
            .map(|(i, w)| (k.width_of(w, &ty), i))
            .collect();
        let space = size * 0.28;
        let lines = flow(&widths, space, inner.width);
        let line_h = (size * ty.line).ceil();
        let fits = widths.iter().all(|(w, _)| *w <= inner.width)
            && line_h * lines.len() as f64 <= inner.height + 0.5;
        if fits || size <= first * 0.4 {
            if !fits {
                return Err(BlockError::Fit(
                    "the headline does not fit its box even at a small size: shorten it or enlarge the box".into(),
                ));
            }
            break (lines, space, line_h);
        }
        size *= 0.94;
    };
    let ty = base.clone().size(size);
    let total = line_h * lines.len() as f64;
    let top = inner.y + (inner.height - total).max(0.0) / 2.0;
    let widest = lines
        .iter()
        .map(|l| l.iter().map(|(_, w)| w).sum::<f64>() + space * (l.len() as f64 - 1.0))
        .fold(0.0_f64, f64::max);
    if a.flag("plate") {
        let bg = k.paint(Role::Background);
        let (pw, ph) = (widest + 2.0 * pad, total + 2.0 * pad);
        let px = x_in(align, area, pw);
        k.add(
            "plate",
            rect(at(px, top - pad, pw, ph), bg)
                .radius(k.s(1.6))
                .alpha(0.82),
        );
        k.fade("plate", 0, 300);
    }
    let accent_from = words.len().saturating_sub(a.num("accent_words") as usize);
    let step = a.num("stagger_ms") as u64;
    for (row, line) in lines.iter().enumerate() {
        let line_w = line.iter().map(|(_, w)| w).sum::<f64>() + space * (line.len() as f64 - 1.0);
        let mut x = x_in(align, inner, line_w).max(inner.x);
        for &(i, w) in line {
            let role = if i >= accent_from && a.num("accent_words") > 0.0 {
                Role::Accent
            } else {
                Role::Foreground
            };
            let word_ty = k.ty(size, 700.0, role).line(ty.line);
            let part = format!("word-{}", i + 1);
            k.text(&part, words[i], &word_ty, x, top + row as f64 * line_h, w);
            k.moves.push(super::kit::Move::Preset {
                target: k.part(&part),
                preset: Preset::RiseIn,
                at: 120 + i as u64 * step,
                duration: 600,
                strength: 0.8,
                ease: Some(Ease::EaseOut),
            });
            x += w + space;
        }
    }
    Ok(())
}

fn caption_band(frame: Rect, margin: f64) -> Rect {
    at(
        margin,
        frame.height * 0.7,
        frame.width - 2.0 * margin,
        frame.height * 0.3 - margin,
    )
}

const SUBTITLE: &[Param] = &[
    Param::text(
        "text",
        "The caption (up to 2 lines).",
        "A short caption that stays easy to read",
        140,
    ),
    Param::flag(
        "plate",
        "A rounded plate behind the text, for use over footage.",
        true,
    ),
];

fn subtitle_under(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pad = k.s(1.8);
    let max_w = (k.b.width * 0.92).min(k.frame.width * 0.9) - 2.0 * pad;
    let size = k.s(3.6);
    let ty = k
        .ty(size, 700.0, Role::Foreground)
        .align(TextAlign::Center)
        .line(1.25);
    let (ty, h) = k.fit(a.text("text"), &ty, max_w, k.b.height - 2.0 * pad, 2)?;
    let (natural, _) = k.measure(a.text("text"), &ty, max_w);
    let w = (natural.ceil() + 4.0).min(max_w);
    let (px, py) = (k.b.x + (k.b.width - w) / 2.0, k.b.y + k.b.height - h);
    if a.flag("plate") {
        let bg = k.paint(Role::Background);
        k.add(
            "plate",
            rect(at(px - pad, py - pad, w + 2.0 * pad, h + 2.0 * pad), bg)
                .radius(k.s(1.4))
                .alpha(0.8),
        );
    }
    k.text("text", a.text("text"), &ty, px, py, w);
    k.fade_root(0, 300);
    Ok(())
}

const QUOTE: &[Param] = &[
    Param::text(
        "quote",
        "The quote, without quotation marks.",
        "Design is how it works, and how it makes you feel.",
        260,
    ),
    Param::text("author", "Who said it.", "Alex Rivera", 60),
    Param::text(
        "role",
        "Their role or source (empty for none).",
        "Head of Design",
        60,
    ),
    Param::flag("panel", "Fill the box with the background color.", true),
];

fn quote_card(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    if a.flag("panel") {
        let bg = k.paint(Role::Background);
        k.add("panel", rect(k.b, bg));
    }
    let area = k.content();
    let mark_size = k.s(16.0);
    let mark_ty = k.ty(mark_size, 700.0, Role::Accent).line(0.9);
    let mut rows = vec![k.row("mark", "\u{201C}", &mark_ty, area.width, mark_size * 1.2, 1)?];
    let q_size = k.s(if k.portrait() { 6.2 } else { 5.2 });
    let q_ty = k.ty(q_size, 700.0, Role::Foreground).line(1.25);
    rows.push(k.row(
        "quote",
        a.text("quote"),
        &q_ty,
        area.width,
        area.height * 0.5,
        7,
    )?);
    rows.push(Row::Gap(k.s(4.0)));
    let a_size = k.s(3.0);
    let a_ty = k.ty(a_size, 700.0, Role::Foreground);
    rows.push(k.row(
        "author",
        a.text("author"),
        &a_ty,
        area.width,
        a_size * 1.6,
        1,
    )?);
    let role = a.text("role");
    if !role.is_empty() {
        let r_size = k.s(2.6);
        let r_ty = k.ty(r_size, 400.0, Role::Muted);
        rows.push(k.row("role", role, &r_ty, area.width, r_size * 1.6, 1)?);
    }
    let placed = k.stack(rows, area, true);
    let gap = placed[2];
    let (w, h) = (k.s(7.0), k.s(0.7).max(3.0));
    let accent = k.paint(Role::Accent);
    k.add(
        "rule",
        rect(at(area.x, gap.y + (gap.height - h) / 2.0, w, h), accent).radius(h / 2.0),
    );
    k.fade("mark", 0, 500);
    k.rise("quote", 150, 800);
    k.grow_x("rule", area.x, w, false, 600, 500);
    k.rise("author", 700, 600);
    if !role.is_empty() {
        k.rise("role", 820, 600);
    }
    Ok(())
}

const STAT: &[Param] = &[
    Param::text("value", "The big number or short value.", "98%", 12),
    Param::text(
        "label",
        "What the number measures.",
        "of customers would recommend us",
        80,
    ),
    Param::text("caption", "A source or footnote (empty for none).", "", 80),
    Param::choice("align", "Alignment.", "left", ALIGNS),
];

fn stat_callout(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let area = k.b;
    let align = align_of(a.text("align"));
    let size = k.s(if k.portrait() { 24.0 } else { 20.0 });
    let v_ty = k.ty(size, 700.0, Role::Accent).align(align).line(1.0);
    let mut rows = vec![k.row(
        "value",
        a.text("value"),
        &v_ty,
        area.width,
        area.height * 0.55,
        1,
    )?];
    rows.push(Row::Gap(k.s(2.5)));
    let l_size = k.s(3.8);
    let l_ty = k
        .ty(l_size, 700.0, Role::Foreground)
        .align(align)
        .line(1.25);
    rows.push(k.row(
        "label",
        a.text("label"),
        &l_ty,
        area.width,
        area.height * 0.3,
        2,
    )?);
    let caption = a.text("caption");
    if !caption.is_empty() {
        rows.push(Row::Gap(k.s(1.5)));
        let c_size = k.s(2.6);
        let c_ty = k.ty(c_size, 400.0, Role::Muted).align(align);
        rows.push(k.row("caption", caption, &c_ty, area.width, area.height * 0.15, 2)?);
    }
    k.stack(rows, area, true);
    k.moves.push(super::kit::Move::Preset {
        target: k.part("value"),
        preset: Preset::RiseIn,
        at: 0,
        duration: 700,
        strength: 0.6,
        ease: Some(Ease::Back),
    });
    k.rise("label", 240, 600);
    if !caption.is_empty() {
        k.rise("caption", 380, 600);
    }
    Ok(())
}

const BADGE: &[Param] = &[
    Param::text("text", "The label.", "New", 24),
    Param::choice(
        "style",
        "How the pill is drawn.",
        "filled",
        &["filled", "outline", "soft"],
    ),
    Param::choice(
        "align",
        "Where it sits in the box.",
        "left",
        &["left", "center", "right"],
    ),
];

fn badge_pill(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let size = k.s(2.8);
    let accent = k.paint(Role::Accent);
    let style = a.text("style");
    let ink = match style {
        "filled" => k.ink_on(&accent),
        "outline" => accent.clone(),
        _ => k.paint(Role::Foreground),
    };
    let ty = Ty::of(size, 700.0, ink);
    let w = k.width_of(a.text("text"), &ty);
    let (px, py) = (size * 1.1, size * 0.6);
    let (bw, bh) = (w + 2.0 * px, (size * 1.2).ceil() + 2.0 * py);
    if bw > k.b.width {
        return Err(BlockError::Fit(
            "the badge text is wider than its box".into(),
        ));
    }
    let x = x_in(align_of(a.text("align")), k.b, bw);
    let plate = match style {
        "filled" => rect(at(x, k.b.y, bw, bh), accent).radius(bh / 2.0),
        "outline" => frame_line(at(x, k.b.y, bw, bh), accent, 2.0).radius(bh / 2.0),
        _ => rect(at(x, k.b.y, bw, bh), accent)
            .radius(bh / 2.0)
            .alpha(0.2),
    };
    k.add("plate", plate);
    k.text("label", a.text("text"), &ty, x + px, k.b.y + py, w);
    let dy = k.s(1.5);
    k.reveal(Target::Root, 0, 500, dy);
    Ok(())
}

const EYEBROW: &[Param] = &[
    Param::text("text", "The label.", "Product update", 40),
    Param::flag("uppercase", "Set it in capitals.", true),
    Param::choice(
        "align",
        "Where it sits in the box.",
        "left",
        &["left", "center"],
    ),
];

fn eyebrow_label(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let size = k.s(2.6);
    let ty = k.ty(size, 700.0, Role::Accent);
    let text = if a.flag("uppercase") {
        a.text("text").to_uppercase()
    } else {
        a.text("text").to_string()
    };
    let w = k.width_of(&text, &ty);
    let (rule_w, gap) = (k.s(5.0), size * 0.7);
    let total = rule_w + gap + w;
    if total > k.b.width {
        return Err(BlockError::Fit("the label is wider than its box".into()));
    }
    let x = x_in(align_of(a.text("align")), k.b, total);
    let h = (size * 1.2).ceil();
    let accent = k.paint(Role::Accent);
    let rule_h = (size * 0.18).max(2.0);
    k.add(
        "rule",
        rect(at(x, k.b.y + (h - rule_h) / 2.0, rule_w, rule_h), accent).radius(rule_h / 2.0),
    );
    k.text("label", &text, &ty, x + rule_w + gap, k.b.y, w);
    k.grow_x("rule", x, rule_w, false, 0, 400);
    k.rise("label", 120, 500);
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "title_card",
        category: Category::Titles,
        about: "A full title card: eyebrow, headline that fits up to three lines, an accent rule and a supporting line, staggered in.",
        tags: &["title", "intro", "headline", "opening", "card", "hero"],
        params: TITLE_CARD,
        place: Place::Frame,
        layer: Layer::Front,
        build: title_card,
    },
    Block {
        id: "kinetic_headline",
        category: Category::Titles,
        about: "A headline whose words rise in one after another, the last words in the accent color.",
        tags: &[
            "kinetic",
            "word",
            "reveal",
            "headline",
            "typography",
            "animated text",
        ],
        params: KINETIC,
        place: Place::Safe,
        layer: Layer::Front,
        build: kinetic_headline,
    },
    Block {
        id: "subtitle_under",
        category: Category::Titles,
        about: "A caption on a rounded plate near the bottom of the frame, for subtitles and one-line explanations over footage.",
        tags: &[
            "subtitle", "caption", "captions", "text", "footage", "bottom",
        ],
        params: SUBTITLE,
        place: Place::Custom(caption_band),
        layer: Layer::Front,
        build: subtitle_under,
    },
    Block {
        id: "quote_card",
        category: Category::Titles,
        about: "A pull quote with a large quotation mark, an attribution and a role, on its own panel.",
        tags: &["quote", "testimonial", "pull quote", "citation", "card"],
        params: QUOTE,
        place: Place::Frame,
        layer: Layer::Front,
        build: quote_card,
    },
    Block {
        id: "stat_callout",
        category: Category::Titles,
        about: "A big number in the accent color with its label and an optional source line.",
        tags: &[
            "stat",
            "number",
            "metric",
            "kpi",
            "statistic",
            "big number",
            "percent",
        ],
        params: STAT,
        place: Place::Safe,
        layer: Layer::Front,
        build: stat_callout,
    },
    Block {
        id: "badge_pill",
        category: Category::Titles,
        about: "A small pill label (New, Beta, Free) sized to its text, filled, outlined or soft.",
        tags: &["badge", "pill", "label", "tag", "chip", "new"],
        params: BADGE,
        place: Place::Safe,
        layer: Layer::Front,
        build: badge_pill,
    },
    Block {
        id: "eyebrow_label",
        category: Category::Titles,
        about: "A small capitals label with an accent rule, for the line above a headline.",
        tags: &["eyebrow", "kicker", "label", "overline", "section"],
        params: EYEBROW,
        place: Place::Safe,
        layer: Layer::Front,
        build: eyebrow_label,
    },
];
