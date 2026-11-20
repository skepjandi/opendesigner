//! Motion wrappers: an entrance with stagger for nodes that already exist, a logo sting and an
//! end card.

use od_core::animation::{AnimProp, Preset};
use od_core::{Ease, ImageFit, TextAlign};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Move, Role, Row, Style, Target, Ty, at, picture, rect};
use super::registry::{Block, Category, Layer, Place};

const ENTRANCES: &[&str] = &[
    "rise_in",
    "fade_in",
    "drop_in",
    "slide_in_left",
    "slide_in_right",
];

const STAGGER: &[Param] = &[
    Param::nodes(
        "nodes",
        "Ids of the nodes to bring in, in the order they arrive.",
    ),
    Param::choice(
        "preset",
        "The named move each one performs.",
        "rise_in",
        ENTRANCES,
    ),
    Param::count(
        "step_ms",
        "Delay between one node and the next.",
        120,
        0,
        2000,
    ),
    Param::count("duration_ms", "How long each move takes.", 600, 100, 5000),
    Param::number(
        "strength",
        "How far each move goes; 1 is as designed.",
        1.0,
        0.1,
        4.0,
    ),
];

fn stagger_in(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let preset = [
        Preset::RiseIn,
        Preset::FadeIn,
        Preset::DropIn,
        Preset::SlideInLeft,
        Preset::SlideInRight,
    ]
    .into_iter()
    .find(|p| p.as_str() == a.text("preset"))
    .ok_or_else(|| BlockError::Param("stagger_in.preset is not an entrance".into()))?;
    let ids = a.nodes("nodes");
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            return Err(BlockError::Param(format!(
                "stagger_in.nodes lists {id} twice"
            )));
        }
        k.moves.push(Move::Preset {
            target: Target::Node(id.clone()),
            preset,
            at: i as u64 * a.num("step_ms") as u64,
            duration: a.num("duration_ms") as u64,
            strength: a.num("strength"),
            ease: None,
        });
    }
    Ok(())
}

const STING: &[Param] = &[
    Param::text(
        "wordmark",
        "The name, set as a wordmark when no image is given.",
        "Acme",
        24,
    ),
    Param::text(
        "tagline",
        "A line under the logo (empty for none).",
        "Built for makers",
        60,
    ),
    Param::image(
        "image",
        "A logo from list_media, shown instead of the wordmark.",
    ),
    Param::flag("panel", "Fill the box with the background color.", true),
];

fn logo_sting(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    if a.flag("panel") {
        let bg = k.paint(Role::Background);
        k.add("panel", rect(k.b, bg));
    }
    let area = k.content();
    let mut rows = Vec::new();
    let logo_h = k.s(if k.portrait() { 22.0 } else { 20.0 });
    if a.image("image").is_some() {
        rows.push(Row::Gap(logo_h));
    } else {
        let ty = k
            .ty(logo_h * 0.8, 700.0, Role::Foreground)
            .align(TextAlign::Center)
            .line(1.0);
        rows.push(k.row(
            "wordmark",
            a.text("wordmark"),
            &ty,
            area.width,
            logo_h * 1.2,
            1,
        )?);
    }
    rows.push(Row::Gap(k.s(4.0)));
    let tagline = a.text("tagline");
    if !tagline.is_empty() {
        let size = k.s(3.4);
        let ty = k.ty(size, 400.0, Role::Muted).align(TextAlign::Center);
        rows.push(k.row("tagline", tagline, &ty, area.width, size * 3.0, 2)?);
    }
    let placed = k.stack(rows, area, true);
    if let Some(file) = a.image("image") {
        let box_w = area.width.min(logo_h * 2.2);
        let at_logo = at(
            area.x + (area.width - box_w) / 2.0,
            placed[0].y,
            box_w,
            logo_h,
        );
        k.add(
            "logo",
            picture(at_logo, file.to_string()).fit(ImageFit::Contain),
        );
    }
    let part = if a.image("image").is_some() {
        "logo"
    } else {
        "wordmark"
    };
    k.moves.push(Move::Preset {
        target: k.part(part),
        preset: Preset::RiseIn,
        at: 0,
        duration: 800,
        strength: 0.5,
        ease: Some(Ease::Back),
    });
    let (w, h) = (k.s(10.0), k.s(0.7).max(3.0));
    let accent = k.paint(Role::Accent);
    let x = area.x + (area.width - w) / 2.0;
    k.add(
        "rule",
        rect(
            at(x, placed[1].y + (placed[1].height - h) / 2.0, w, h),
            accent,
        )
        .radius(h / 2.0),
    );
    k.grow_x("rule", x, w, true, 500, 600);
    if !tagline.is_empty() {
        k.rise("tagline", 900, 700);
    }
    Ok(())
}

const END_CARD: &[Param] = &[
    Param::text("headline", "The closing line.", "Thanks for watching", 60),
    Param::text(
        "cta",
        "The button text (empty for none).",
        "Get the app",
        28,
    ),
    Param::text(
        "url",
        "A link or handle under the button (empty for none).",
        "example.com",
        50,
    ),
    Param::image("image", "A logo from list_media above the headline."),
    Param::flag("panel", "Fill the box with the background color.", true),
];

fn end_card(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    if a.flag("panel") {
        let bg = k.paint(Role::Background);
        k.add("panel", rect(k.b, bg));
    }
    let area = k.content();
    let mut rows = Vec::new();
    let logo_h = k.s(14.0);
    let has_logo = a.image("image").is_some();
    if has_logo {
        rows.push(Row::Gap(logo_h));
        rows.push(Row::Gap(k.s(3.0)));
    }
    let size = k.s(if k.portrait() { 8.0 } else { 7.0 });
    let ty = k
        .ty(size, 700.0, Role::Foreground)
        .align(TextAlign::Center)
        .line(1.1);
    rows.push(k.row(
        "headline",
        a.text("headline"),
        &ty,
        area.width,
        area.height * 0.4,
        3,
    )?);
    let cta = a.text("cta");
    let btn_size = k.s(3.6);
    let btn_ty = Ty::of(btn_size, 700.0, k.paint(Role::Background));
    let (btn_w, btn_h) = (
        k.width_of(cta, &btn_ty) + btn_size * 3.0,
        (btn_size * 1.2).ceil() + btn_size * 1.4,
    );
    let btn_row = rows.len() + 1;
    if !cta.is_empty() {
        if btn_w > area.width {
            return Err(BlockError::Fit(
                "the button text is wider than the card".into(),
            ));
        }
        rows.push(Row::Gap(k.s(5.0)));
        rows.push(Row::Gap(btn_h));
    }
    let url = a.text("url");
    if !url.is_empty() {
        rows.push(Row::Gap(k.s(3.0)));
        let u_size = k.s(3.0);
        let u_ty = k.ty(u_size, 400.0, Role::Muted).align(TextAlign::Center);
        rows.push(k.row("url", url, &u_ty, area.width, u_size * 1.8, 1)?);
    }
    let placed = k.stack(rows, area, true);
    if let Some(file) = a.image("image") {
        let w = area.width.min(logo_h * 2.2);
        k.add(
            "logo",
            picture(
                at(area.x + (area.width - w) / 2.0, placed[0].y, w, logo_h),
                file.to_string(),
            )
            .fit(ImageFit::Contain),
        );
        k.fade("logo", 0, 500);
    }
    k.rise("headline", 150, 700);
    if !cta.is_empty() {
        let slot = placed[btn_row];
        let accent = k.paint(Role::Accent);
        let ink = k.ink_on(&accent);
        let x = area.x + (area.width - btn_w) / 2.0;
        k.begin("button");
        k.add(
            "button/plate",
            rect(at(x, slot.y, btn_w, btn_h), accent).radius(btn_h / 2.0),
        );
        let ty = Ty::of(btn_size, 700.0, ink);
        k.text(
            "button/label",
            cta,
            &ty,
            x + btn_size * 1.5,
            slot.y + btn_size * 0.7,
            btn_w - btn_size * 3.0,
        );
        k.end();
        let (t, dy) = (k.part("button"), k.s(3.0));
        k.keys(
            t.clone(),
            AnimProp::Y,
            &[(450, dy, Ease::Linear), (1100, 0.0, Ease::Back)],
        );
        k.keys(
            t,
            AnimProp::Opacity,
            &[(450, 0.0, Ease::Linear), (700, 1.0, Ease::EaseOut)],
        );
    }
    if !url.is_empty() {
        k.fade("url", 800, 600);
    }
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "stagger_in",
        category: Category::Motion,
        about: "Brings existing nodes in one after another with a named entrance (rise, fade, drop, slide). Makes no nodes of its own; needs a video.",
        tags: &[
            "stagger", "entrance", "animate", "sequence", "reveal", "in", "cascade", "motion",
        ],
        params: STAGGER,
        place: Place::Frame,
        layer: Layer::Front,
        build: stagger_in,
    },
    Block {
        id: "logo_sting",
        category: Category::Motion,
        about: "A logo reveal: the wordmark (or your logo image) springs in, an accent rule opens under it, then the tagline.",
        tags: &[
            "logo", "sting", "intro", "brand", "reveal", "wordmark", "outro", "bumper",
        ],
        params: STING,
        place: Place::Frame,
        layer: Layer::Front,
        build: logo_sting,
    },
    Block {
        id: "end_card",
        category: Category::Motion,
        about: "A closing card: optional logo, a headline, a call-to-action button and a link, staggered in.",
        tags: &[
            "end card",
            "outro",
            "closing",
            "cta",
            "call to action",
            "thanks",
            "credits",
        ],
        params: END_CARD,
        place: Place::Frame,
        layer: Layer::Front,
        build: end_card,
    },
];
