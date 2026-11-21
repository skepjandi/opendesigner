//! The science props: an atom whose electrons really orbit, a planet with a ring, craters and
//! drifting clouds, a cell with wobbling organelles and a DNA helix that turns.

use od_core::animation::{AnimProp, Repeat};
use od_core::geometry::Rect;
use od_core::{Ease, Paint, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::art::{
    BLACK, Grp, Key, Named, WHITE, blob_d, blurred, centered, ellipse, faded, glow, lit, ms, path,
    pen, radial, rect_of, rounded, shade, tint, turned,
};
use super::kit::{Kit, at};
use super::props::{ACCENT, COLOR, Ctx, LOOPS, SEED};
use super::registry::{Block, Category, Layer, Place};

const TAU: f64 = std::f64::consts::TAU;

fn tall_box(frame: Rect, _margin: f64) -> Rect {
    let h = frame.height * 0.62;
    let w = (h * 0.34).min(frame.width * 0.6);
    at(
        frame.x + (frame.width - w) / 2.0,
        frame.y + (frame.height - h) / 2.0,
        w,
        h,
    )
}

fn square_box(frame: Rect, _margin: f64) -> Rect {
    let side = frame.width.min(frame.height) * 0.5;
    at(
        frame.x + (frame.width - side) / 2.0,
        frame.y + (frame.height - side) / 2.0,
        side,
        side,
    )
}

pub const ATOM: Block = Block {
    id: "prop_atom",
    category: Category::Characters,
    about: "An atom: a pulsing nucleus of protons and neutrons and three tilted orbits, each with an \
            electron that travels its ellipse while the orbits turn at different speeds.",
    tags: &[
        "atom", "science", "physics", "electron", "nucleus", "orbit", "prop",
    ],
    params: &[COLOR, ACCENT, LOOPS, SEED],
    place: Place::Custom(square_box),
    layer: Layer::Front,
    build: atom,
};

const PLANET_RING: Param = Param::flag("ring", "A ring around the planet.", true);
const PLANET_CRATERS: Param = Param::count("craters", "How many craters.", 4, 0, 8);

pub const PLANET: Block = Block {
    id: "prop_planet",
    category: Category::Characters,
    about: "A planet: a lit sphere with a terminator shade, craters, cloud streaks that drift across \
            it and an optional ring (half behind, half in front) that rocks slowly.",
    tags: &[
        "planet", "space", "world", "moon", "saturn", "craters", "prop",
    ],
    params: &[COLOR, ACCENT, PLANET_RING, PLANET_CRATERS, LOOPS, SEED],
    place: Place::Custom(square_box),
    layer: Layer::Front,
    build: planet,
};

const CELL_ORGANELLES: Param = Param::count(
    "organelles",
    "How many organelles float in the cell.",
    5,
    2,
    8,
);

pub const CELL: Block = Block {
    id: "prop_cell",
    category: Category::Characters,
    about: "A cell: a soft membrane that breathes, a nucleus with a nucleolus and organelles \
            (mitochondria, vesicles) that wobble and drift.",
    tags: &[
        "cell",
        "biology",
        "organelle",
        "nucleus",
        "science",
        "membrane",
        "prop",
    ],
    params: &[COLOR, ACCENT, CELL_ORGANELLES, LOOPS, SEED],
    place: Place::Custom(square_box),
    layer: Layer::Front,
    build: cell,
};

const DNA_RUNGS: Param = Param::count("rungs", "How many base pairs.", 20, 8, 28);
const DNA_PERIOD: Param = Param::count(
    "period_ms",
    "How long one full turn takes.",
    5000,
    1000,
    60_000,
);

pub const DNA: Block = Block {
    id: "prop_dna",
    category: Category::Characters,
    about: "A DNA double helix of beads and rungs that turns about its axis for ever; the near \
            beads are larger and brighter than the far ones.",
    tags: &[
        "dna", "helix", "biology", "genetics", "science", "prop", "gene",
    ],
    params: &[COLOR, ACCENT, DNA_RUNGS, DNA_PERIOD, LOOPS, SEED],
    place: Place::Custom(tall_box),
    layer: Layer::Front,
    build: dna,
};

fn atom(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, None, "#ffbe3d")?;
    let (w, cx, cy) = (c.w(), c.s.x + c.w() / 2.0, c.s.y + c.w() / 2.0);
    let (rx, ry) = (w * 0.46, w * 0.17);
    for (i, tilt) in [0.0, 60.0, 120.0].into_iter().enumerate() {
        orbit(k, &mut c, i, tilt, (cx, cy), (rx, ry))?;
    }
    let r = w * 0.1;
    let mut nucleus = Grp::new(k, "nucleus", centered(cx, cy, r * 2.4, r * 2.4));
    let spots = [
        (-0.3, -0.2),
        (0.3, -0.25),
        (0.0, 0.32),
        (-0.35, 0.25),
        (0.35, 0.2),
    ];
    for (i, (dx, dy)) in spots.into_iter().enumerate() {
        let tone = if i % 2 == 0 {
            lit(&c.main, c.main_c)
        } else {
            lit(&c.accent, c.accent_c)
        };
        nucleus.add(
            ellipse(
                centered(cx + dx * r * 1.5, cy + dy * r * 1.5, r * 1.1, r * 1.1),
                tone,
            )
            .named(k, &format!("particle_{i}")),
        );
    }
    k.add("nucleus", nucleus.node);
    c.sway(k, "nucleus", AnimProp::Scale, (1.0, 1.07), 900.0)
}

fn orbit(
    k: &mut Kit,
    c: &mut Ctx,
    i: usize,
    tilt: f64,
    center: (f64, f64),
    radii: (f64, f64),
) -> Result<(), BlockError> {
    let w = c.w();
    let name = format!("orbit_{i}");
    let mut g = Grp::new(k, &name, rect_of(c.s.x, c.s.y, w, w))
        .pivot(center.0, center.1)
        .rotated(tilt);
    let mut ring = ellipse(
        centered(center.0, center.1, radii.0 * 2.0, radii.1 * 2.0),
        faded(Rgba { a: 255, ..c.main_c }, 0.0),
    );
    ring.fill = None;
    ring.stroke = Some(pen(tint(c.main_c, 0.25), w * 0.013));
    g.add(ring.named(k, &format!("{name}_ring")));
    let er = w * 0.045;
    let phase = c.rng.range(0.0, TAU);
    let at_ = |th: f64| {
        (
            center.0 + radii.0 * th.cos() - er - c.s.x,
            center.1 + radii.1 * th.sin() - er - c.s.y,
        )
    };
    let (x0, y0) = at_(phase);
    g.add(
        ellipse(
            rect_of(x0 + c.s.x, y0 + c.s.y, er * 2.0, er * 2.0),
            lit(&c.accent, c.accent_c),
        )
        .named(k, &format!("electron_{i}")),
    );
    k.add(&name, g.node);
    let steps = 16;
    let period = [2600.0, 3300.0, 4100.0][i];
    let (mut xs, mut ys): (Vec<Key>, Vec<Key>) = (Vec::new(), Vec::new());
    for s in 0..=steps {
        let th = phase + TAU * f64::from(s) / f64::from(steps);
        let (x, y) = at_(th);
        let t = ms(period * f64::from(s) / f64::from(steps));
        xs.push((t, x, Ease::Linear));
        ys.push((t, y, Ease::Linear));
    }
    let part = format!("electron_{i}");
    c.track(k, &part, AnimProp::X, &xs, Repeat::Loop)?;
    c.track(k, &part, AnimProp::Y, &ys, Repeat::Loop)?;
    let spin = [24_000.0, 33_000.0, 45_000.0][i];
    let dir = if i == 1 { -360.0 } else { 360.0 };
    c.track(
        k,
        &name,
        AnimProp::Rotation,
        &[
            (0, tilt, Ease::Linear),
            (ms(spin), tilt + dir, Ease::Linear),
        ],
        Repeat::Loop,
    )
}

fn planet(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, Some("#6c8cff"), "#ffd08a")?;
    let w = c.w();
    let (cx, cy) = (c.s.x + w / 2.0, c.s.y + w / 2.0);
    let d = w * 0.58;
    let ring = a.flag("ring");
    let tilt = -18.0;
    if ring {
        ring_group(k, &c, "ring_back", (cx, cy), tilt, false);
    }
    surface(k, &mut c, (cx, cy), d, a.num("craters") as u32)?;
    if ring {
        ring_group(k, &c, "ring_front", (cx, cy), tilt, true);
        for part in ["ring_back", "ring_front"] {
            c.sway(
                k,
                part,
                AnimProp::Rotation,
                (tilt - 2.5, tilt + 2.5),
                3600.0,
            )?;
        }
    }
    Ok(())
}

fn ring_group(k: &mut Kit, c: &Ctx, name: &str, center: (f64, f64), tilt: f64, front: bool) {
    let w = c.w();
    let (rx, ry) = (w * 0.47, w * 0.13);
    let box_ = centered(center.0, center.1, rx * 2.2, ry * 2.4);
    let mut g = Grp::new(k, name, box_)
        .pivot(center.0, center.1)
        .rotated(tilt);
    let paint = Paint::Color(c.accent_c);
    let width = w * 0.05;
    if front {
        let arc = rect_of(center.0 - rx, center.1, rx * 2.0, ry);
        let d = format!("M0,0 A{rx:.2},{ry:.2} 0 0 0 {:.2},0", rx * 2.0);
        let mut n = path(arc, d, None);
        n.stroke = Some(pen(paint, width));
        g.add(n.named(k, &format!("{name}_arc")));
    } else {
        let mut n = ellipse(
            centered(center.0, center.1, rx * 2.0, ry * 2.0),
            faded(BLACK, 0.0),
        );
        n.fill = None;
        n.stroke = Some(pen(Paint::Color(shade_rgba(c.accent_c, 0.2)), width));
        g.add(n.named(k, &format!("{name}_ellipse")));
    }
    k.add(name, g.node);
}

fn shade_rgba(c: Rgba, t: f64) -> Rgba {
    Rgba::parse_hex(&super::kit::mix(c, BLACK, t)).unwrap_or(c)
}

fn surface(
    k: &mut Kit,
    c: &mut Ctx,
    center: (f64, f64),
    d: f64,
    craters: u32,
) -> Result<(), BlockError> {
    let box_ = centered(center.0, center.1, d, d);
    let mut g = Grp::new(k, "planet", box_);
    g.node.kind = od_core::NodeKind::Frame;
    g.node.corner_radius = Some(d / 2.0);
    let sphere = radial(
        0.35,
        0.3,
        0.85,
        &[
            (0.0, tint(c.main_c, 0.35)),
            (0.55, Paint::Color(c.main_c)),
            (1.0, shade(c.main_c, 0.3)),
        ],
    );
    g.add(ellipse(box_, sphere).named(k, "planet_sphere"));
    for i in 0..craters {
        let ang = c.rng.range(0.0, TAU);
        let rad = c.rng.range(0.1, 0.38) * d;
        let r = c.rng.range(0.05, 0.1) * d;
        let p = (center.0 + ang.cos() * rad, center.1 + ang.sin() * rad);
        let hole = ellipse(
            centered(p.0, p.1, r * 2.0, r * 1.7),
            faded(shade_rgba(c.main_c, 0.45), 0.55),
        );
        g.add(hole.named(k, &format!("crater_{i}")));
        let lip = ellipse(
            centered(p.0 - r * 0.15, p.1 - r * 0.2, r * 1.6, r * 1.3),
            faded(WHITE, 0.14),
        );
        g.add(lip.named(k, &format!("crater_lip_{i}")));
    }
    for i in 0..3 {
        let y = center.1 + d * (-0.22 + 0.22 * f64::from(i));
        let streak = rounded(
            rect_of(box_.x, y, d * 0.5, d * 0.07),
            d * 0.035,
            faded(WHITE, 0.5),
        );
        g.add(blurred(streak, d * 0.012).named(k, &format!("cloud_{i}")));
    }
    let terminator = super::art::linear(
        super::art::LIGHT_ANGLE,
        &[(0.45, faded(BLACK, 0.0)), (1.0, faded(BLACK, 0.4))],
    );
    g.add(ellipse(box_, terminator).named(k, "terminator"));
    k.add("planet", g.node);
    for i in 0..3u64 {
        let span = c.rng.range(14_000.0, 22_000.0);
        let part = format!("cloud_{i}");
        let x0 = -d * 0.5;
        let keys: Vec<Key> = vec![(0, x0, Ease::Linear), (ms(span), d * 1.0, Ease::Linear)];
        let fade: Vec<Key> = vec![
            (0, 0.0, Ease::Linear),
            (ms(span * 0.2), 0.5, Ease::Linear),
            (ms(span * 0.8), 0.5, Ease::Linear),
            (ms(span), 0.0, Ease::Linear),
        ];
        c.track(k, &part, AnimProp::X, &keys, Repeat::Loop)?;
        c.track(k, &part, AnimProp::Opacity, &fade, Repeat::Loop)?;
    }
    Ok(())
}

fn cell(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, Some("#7bd3a0"), "#ff8fa3")?;
    let w = c.w();
    let (cx, cy) = (c.s.x + w / 2.0, c.s.y + w / 2.0);
    let (mw, mh) = (w * 0.92, w * 0.8);
    let m = centered(cx, cy, mw, mh);
    let fill = radial(
        0.4,
        0.35,
        0.8,
        &[
            (0.0, tint(c.main_c, 0.55)),
            (0.65, Paint::Color(c.main_c)),
            (1.0, shade(c.main_c, 0.18)),
        ],
    );
    let mut membrane = turned(path(m, blob_d(mw, mh, 0.52), Some(fill)), 8.0);
    membrane.stroke = Some(pen(tint(c.main_c, 0.55), w * 0.02));
    k.add("membrane", membrane);
    let nr = w * 0.15;
    let (nx, ny) = (cx - w * 0.05, cy - w * 0.02);
    let mut nucleus = Grp::new(k, "nucleus", centered(nx, ny, nr * 2.0, nr * 2.0));
    let nfill = radial(
        0.4,
        0.35,
        0.8,
        &[(0.0, tint(c.accent_c, 0.4)), (1.0, shade(c.accent_c, 0.22))],
    );
    nucleus.add(ellipse(centered(nx, ny, nr * 2.0, nr * 2.0), nfill).named(k, "nucleus_body"));
    nucleus.add(
        ellipse(
            centered(nx + nr * 0.2, ny + nr * 0.1, nr * 0.7, nr * 0.7),
            shade(c.accent_c, 0.35),
        )
        .named(k, "nucleolus"),
    );
    k.add("nucleus", nucleus.node);
    let count = a.num("organelles") as usize;
    let mut specs = Vec::new();
    for i in 0..count {
        let ang = TAU * i as f64 / count as f64 + c.rng.range(-0.3, 0.3);
        let reach = c.rng.range(0.62, 0.86);
        specs.push((
            i,
            cx + ang.cos() * mw * 0.5 * reach,
            cy + ang.sin() * mh * 0.5 * reach,
        ));
    }
    let mut rests = Vec::new();
    for (i, x, y) in specs {
        let (ow, oh) = organelle_size(w, i);
        rests.push((x - ow / 2.0, y - oh / 2.0));
        organelle(k, &c, i, x, y);
    }
    c.sway(k, "membrane", AnimProp::Scale, (1.0, 1.022), 1800.0)?;
    c.sway(k, "nucleus", AnimProp::Scale, (1.0, 1.045), 1400.0)?;
    for (i, (rx, ry)) in rests.into_iter().enumerate() {
        let part = format!("organelle_{i}");
        let amp = c.rng.range(6.0, 12.0);
        let (s1, s2) = (c.rng.range(1400.0, 2800.0), c.rng.range(1500.0, 3000.0));
        c.sway(k, &part, AnimProp::Rotation, (-amp, amp), s1)?;
        let drift = w * 0.012;
        c.sway(k, &part, AnimProp::X, (rx - drift, rx + drift), s2)?;
        c.sway(k, &part, AnimProp::Y, (ry - drift, ry + drift), s2 * 1.17)?;
    }
    Ok(())
}

/// Organelle size by kind: mitochondria are long, vesicles round, dots small.
fn organelle_size(w: f64, i: usize) -> (f64, f64) {
    match i % 3 {
        0 => (w * 0.2, w * 0.1),
        1 => (w * 0.09, w * 0.09),
        _ => (w * 0.06, w * 0.06),
    }
}

fn organelle(k: &mut Kit, c: &Ctx, i: usize, x: f64, y: f64) {
    let (ow, oh) = organelle_size(c.w(), i);
    let r = centered(x, y, ow, oh);
    let mut g = Grp::new(k, &format!("organelle_{i}"), r);
    let tone = if i % 3 == 0 {
        Rgba::parse_hex("#ff9a5c").unwrap_or(c.accent_c)
    } else {
        c.accent_c
    };
    g.add(ellipse(r, lit(&Paint::Color(tone), tone)).named(k, &format!("organelle_{i}_body")));
    if i % 3 == 0 {
        let d = format!(
            "M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} T{:.2},{:.2}",
            ow * 0.2,
            oh * 0.5,
            ow * 0.32,
            oh * 0.15,
            ow * 0.46,
            oh * 0.5,
            ow * 0.74,
            oh * 0.5
        );
        let mut squiggle = path(r, d, None);
        squiggle.stroke = Some(pen(tint(tone, 0.55), (c.w() * 0.008).max(1.0)));
        g.add(squiggle.named(k, &format!("organelle_{i}_cristae")));
    } else {
        g.add(
            turned(
                ellipse(
                    centered(x - ow * 0.15, y - oh * 0.18, ow * 0.4, oh * 0.3),
                    glow(WHITE, 0.6),
                ),
                -20.0,
            )
            .named(k, &format!("organelle_{i}_shine")),
        );
    }
    k.add(&format!("organelle_{i}"), g.node);
}

fn dna(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let c = Ctx::read(k, a, None, "#ff8fa3")?;
    let b = k.b;
    let rungs = a.num("rungs") as usize;
    let period = a.num("period_ms");
    let cx = b.x + b.width / 2.0;
    let amp = b.width * 0.38;
    let spacing = b.height / rungs as f64;
    let bead = (spacing * 0.8).min(b.width * 0.2);
    let steps = 12usize;
    let twist = 0.55;
    let mut helix = Grp::new(k, "helix", b);
    for i in 0..rungs {
        let y = b.y + spacing * (i as f64 + 0.5);
        let phi = twist * i as f64;
        let at_ = |th: f64, sign: f64| cx + sign * amp * th.sin();
        let (xa, xb) = (at_(phi, 1.0), at_(phi, -1.0));
        let rung = rounded(
            rect_of(
                xa.min(xb),
                y - bead * 0.07,
                (xa - xb).abs().max(1.0),
                bead * 0.14,
            ),
            bead * 0.07,
            faded(WHITE, 0.55),
        );
        helix.add(rung.named(k, &format!("rung_{i}")));
        helix.add(
            ellipse(centered(xa, y, bead, bead), lit(&c.main, c.main_c))
                .named(k, &format!("bead_a_{i}")),
        );
        helix.add(
            ellipse(centered(xb, y, bead, bead), lit(&c.accent, c.accent_c))
                .named(k, &format!("bead_b_{i}")),
        );
    }
    k.add("helix", helix.node);
    let origin = (b.x, b.y);
    for i in 0..rungs {
        helix_tracks(
            k,
            &c,
            i,
            (cx, origin),
            amp,
            bead,
            twist * i as f64,
            steps,
            period,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn helix_tracks(
    k: &mut Kit,
    c: &Ctx,
    i: usize,
    org: (f64, (f64, f64)),
    amp: f64,
    bead: f64,
    phi: f64,
    steps: usize,
    period: f64,
) -> Result<(), BlockError> {
    let (cx, (ox, _)) = org;
    let mut cols: [Vec<Key>; 6] = Default::default();
    for s in 0..=steps {
        let th = phi + TAU * s as f64 / steps as f64;
        let t = ms(period * s as f64 / steps as f64);
        let (xa, xb) = (cx + amp * th.sin(), cx - amp * th.sin());
        let (za, zb) = (th.cos(), -th.cos());
        cols[0].push((t, xa - bead / 2.0 - ox, Ease::Linear));
        cols[1].push((t, 0.86 + 0.2 * za, Ease::Linear));
        cols[2].push((t, xb - bead / 2.0 - ox, Ease::Linear));
        cols[3].push((t, 0.86 + 0.2 * zb, Ease::Linear));
        cols[4].push((t, xa.min(xb) - ox, Ease::Linear));
        cols[5].push((t, (xa - xb).abs().max(1.0), Ease::Linear));
    }
    let parts = [
        (format!("bead_a_{i}"), AnimProp::X, 0),
        (format!("bead_a_{i}"), AnimProp::Scale, 1),
        (format!("bead_b_{i}"), AnimProp::X, 2),
        (format!("bead_b_{i}"), AnimProp::Scale, 3),
        (format!("rung_{i}"), AnimProp::X, 4),
        (format!("rung_{i}"), AnimProp::Width, 5),
    ];
    for (part, prop, col) in parts {
        c.track(k, &part, prop, &cols[col], Repeat::Loop)?;
    }
    Ok(())
}
