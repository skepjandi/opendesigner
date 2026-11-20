//! Data blocks: charts, a progress bar and a comparison table. Data arrives as JSON, is validated
//! and capped, and is drawn with rectangles and ellipses (the document has no path node, so
//! curved and diagonal strokes are chains of small dots).

use std::f64::consts::PI;

use od_core::animation::AnimProp;
use od_core::geometry::Rect;
use od_core::{Ease, Paint, TextAlign};
use serde_json::Value;

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Role, Row, Style, Ty, at, ellipse, mix, rect, ring};
use super::registry::{Block, Category, Layer, Place};

const MAX_BARS: usize = 12;
const MAX_POINTS: usize = 24;
const MAX_SERIES: usize = 3;
const MAX_SLICES: usize = 6;
const MAX_COLUMNS: usize = 6;
const MAX_ROWS: usize = 8;
const MAX_LABEL: usize = 24;
const MAX_CELL: usize = 28;
const MAX_VALUE: f64 = 1.0e9;
const MAX_DOTS_PER_LINE: f64 = 700.0;

fn bad(param: &str, why: impl std::fmt::Display) -> BlockError {
    BlockError::Param(format!("{param}: {why}"))
}

fn short_text(v: &Value, param: &str, max: usize) -> Result<String, BlockError> {
    let s = v.as_str().ok_or_else(|| bad(param, "expected text"))?;
    if s.chars().count() > max || s.chars().any(char::is_control) {
        return Err(bad(
            param,
            format!("{s:?} is longer than {max} characters or has control characters"),
        ));
    }
    Ok(s.to_string())
}

fn value(v: &Value, param: &str) -> Result<f64, BlockError> {
    let n = v
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| bad(param, "expected a number"))?;
    if n.abs() > MAX_VALUE {
        return Err(bad(param, format!("{n} is outside +-{MAX_VALUE}")));
    }
    Ok(n)
}

fn list<'a>(v: &'a Value, param: &str, min: usize, max: usize) -> Result<&'a [Value], BlockError> {
    let l = v.as_array().ok_or_else(|| bad(param, "expected a list"))?;
    if l.len() < min || l.len() > max {
        return Err(bad(
            param,
            format!("needs {min} to {max} entries, got {}", l.len()),
        ));
    }
    Ok(l)
}

/// `[{"label": text, "value": number}, ...]`.
fn labelled(
    v: &Value,
    param: &str,
    min: usize,
    max: usize,
    positive: bool,
) -> Result<Vec<(String, f64)>, BlockError> {
    list(v, param, min, max)?
        .iter()
        .map(|item| {
            let label = short_text(
                item.get("label").unwrap_or(&Value::Null),
                &format!("{param}.label"),
                MAX_LABEL,
            )?;
            let n = value(
                item.get("value").unwrap_or(&Value::Null),
                &format!("{param}.value"),
            )?;
            if positive && n < 0.0 {
                return Err(bad(
                    param,
                    format!("{label:?} is negative; this chart draws sizes, so values start at 0"),
                ));
            }
            Ok((label, n))
        })
        .collect()
}

fn number_text(v: f64, unit: &str) -> String {
    let s = if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    format!("{s}{unit}")
}

fn title_row(k: &mut Kit, a: &Args, width: f64) -> Result<Option<Row>, BlockError> {
    let title = a.text("title");
    if title.is_empty() {
        return Ok(None);
    }
    let size = k.s(3.8);
    let ty = k.ty(size, 700.0, Role::Foreground);
    Ok(Some(k.row("title", title, &ty, width, size * 1.7, 1)?))
}

const BAR_DEFAULT: &str = r#"[{"label":"Jan","value":12},{"label":"Feb","value":19},{"label":"Mar","value":27},{"label":"Apr","value":34},{"label":"May","value":52}]"#;

const BAR: &[Param] = &[
    Param::text(
        "title",
        "Chart title (empty for none).",
        "Signups per month",
        60,
    ),
    Param::data(
        "data",
        "2 to 12 bars: [{\"label\": text, \"value\": number >= 0}].",
        BAR_DEFAULT,
    ),
    Param::text("unit", "Appended to each value, like % or k.", "", 4),
    Param::count(
        "highlight",
        "Index (from 0) of one bar to emphasize, or -1 for none.",
        -1,
        -1,
        11,
    ),
];

fn bar_chart(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let bars = labelled(a.data("data"), "data", 2, MAX_BARS, true)?;
    let area = k.b;
    let mut top = area.y;
    if let Some(row) = title_row(k, a, area.width)? {
        let h = row.height();
        k.stack(vec![row], at(area.x, area.y, area.width, h), false);
        k.rise("title", 0, 500);
        top += h + k.s(3.0);
    }
    let n = bars.len();
    let slot = area.width / n as f64;
    let size = k.s(2.6);
    let fg = k.paint(Role::Foreground);
    let muted = k.paint(Role::Muted);
    let unit = a.text("unit");
    let label_ty = Ty::of(size, 400.0, muted.clone()).align(TextAlign::Center);
    let value_ty = Ty::of(size, 700.0, fg).align(TextAlign::Center);
    let label_size = bars
        .iter()
        .map(|(l, _)| k.fit_size(l, &label_ty, slot * 0.96, size * 1.6, 1, k.s(2.3)))
        .collect::<Result<Vec<_>, _>>()?;
    let label_size = label_size.into_iter().fold(size, f64::min);
    let value_size = bars
        .iter()
        .map(|(_, v)| {
            k.fit_size(
                &number_text(*v, unit),
                &value_ty,
                slot * 0.96,
                size * 1.6,
                1,
                k.s(2.3),
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .fold(size, f64::min);
    let label_h = (label_size * 1.2).ceil();
    let value_h = (value_size * 1.2).ceil();
    let base = area.y + area.height - label_h - k.s(1.5);
    let ceiling = top + value_h + k.s(1.0);
    let plot = (base - ceiling).max(8.0);
    let max = bars.iter().map(|b| b.1).fold(0.0_f64, f64::max).max(1e-9);
    let accent = k.paint(Role::Accent);
    k.add(
        "baseline",
        rect(at(area.x, base, area.width, 2.0), muted.clone()).alpha(0.6),
    );
    let bar_w = (slot * 0.58).min(k.s(14.0));
    let highlight = a.num("highlight");
    for (i, (label, v)) in bars.iter().enumerate() {
        let h = (v / max * plot).max(1.0);
        let x = area.x + slot * i as f64 + (slot - bar_w) / 2.0;
        let on = highlight < 0.0 || highlight as usize == i;
        let fill = if on { accent.clone() } else { muted.clone() };
        let id = i + 1;
        let delay = 150 + 90 * i as u64;
        k.add(
            &format!("bar-{id}"),
            rect(at(x, base - h, bar_w, h), fill).radius((bar_w * 0.18).min(k.s(0.9))),
        );
        let t = k.part(&format!("bar-{id}"));
        k.keys(
            t.clone(),
            AnimProp::Height,
            &[(delay, 0.0, Ease::Linear), (delay + 750, h, Ease::EaseOut)],
        );
        k.keys(
            t,
            AnimProp::Y,
            &[
                (delay, base, Ease::Linear),
                (delay + 750, base - h, Ease::EaseOut),
            ],
        );
        let vt = Ty {
            size: value_size,
            ..value_ty.clone()
        };
        k.text(
            &format!("value-{id}"),
            &number_text(*v, unit),
            &vt,
            area.x + slot * i as f64,
            base - h - value_h - k.s(0.8),
            slot,
        );
        k.fade(&format!("value-{id}"), delay + 500, 350);
        let lt = Ty {
            size: label_size,
            ..label_ty.clone()
        };
        k.text(
            &format!("label-{id}"),
            label,
            &lt,
            area.x + slot * i as f64,
            base + k.s(1.5),
            slot,
        );
    }
    Ok(())
}

const LINE_DEFAULT_LABELS: &str = r#"["Jan","Feb","Mar","Apr","May","Jun"]"#;
const LINE_DEFAULT_SERIES: &str = r#"[{"name":"Active users","values":[12,18,17,26,31,44]}]"#;

const LINE: &[Param] = &[
    Param::text("title", "Chart title (empty for none).", "Active users", 60),
    Param::data(
        "labels",
        "Labels along the bottom, one per point (up to 24), or [] for none.",
        LINE_DEFAULT_LABELS,
    ),
    Param::data(
        "series",
        "1 to 3 lines: [{\"name\": text, \"values\": [numbers]}], each with one value per label.",
        LINE_DEFAULT_SERIES,
    ),
    Param::text("unit", "Appended to the axis values.", "", 4),
];

struct Series {
    name: String,
    values: Vec<f64>,
}

fn series_of(a: &Args) -> Result<(Vec<String>, Vec<Series>), BlockError> {
    let labels: Vec<String> = list(a.data("labels"), "labels", 0, MAX_POINTS)?
        .iter()
        .map(|l| short_text(l, "labels", 12))
        .collect::<Result<_, _>>()?;
    let series: Vec<Series> = list(a.data("series"), "series", 1, MAX_SERIES)?
        .iter()
        .map(|s| {
            let name = short_text(
                s.get("name").unwrap_or(&Value::Null),
                "series.name",
                MAX_LABEL,
            )?;
            let values = list(
                s.get("values").unwrap_or(&Value::Null),
                "series.values",
                2,
                MAX_POINTS,
            )?
            .iter()
            .map(|v| value(v, "series.values"))
            .collect::<Result<Vec<_>, _>>()?;
            Ok(Series { name, values })
        })
        .collect::<Result<_, BlockError>>()?;
    let points = series[0].values.len();
    if series.iter().any(|s| s.values.len() != points) {
        return Err(bad("series", "every line needs the same number of values"));
    }
    if !labels.is_empty() && labels.len() != points {
        return Err(bad(
            "labels",
            format!("{} labels for {points} points", labels.len()),
        ));
    }
    Ok((labels, series))
}

/// Round dots `d` across, no further apart than `step`, in one color.
struct Pen {
    d: f64,
    step: f64,
    fill: Paint,
}

/// Dots along a straight segment from `from` to `to`, numbered on from `count`.
fn dots_along(
    k: &mut Kit,
    group: &str,
    (from, to): ((f64, f64), (f64, f64)),
    pen: &Pen,
    count: &mut usize,
) {
    let len = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
    let n = (len / pen.step).ceil().max(1.0) as usize;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let (x, y) = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        *count += 1;
        k.add(
            &format!("{group}/dot-{count}"),
            ellipse(
                at(x - pen.d / 2.0, y - pen.d / 2.0, pen.d, pen.d),
                pen.fill.clone(),
            ),
        );
    }
}

fn line_chart(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let (labels, series) = series_of(a)?;
    let area = k.b;
    let unit = a.text("unit");
    let mut top = area.y;
    if let Some(row) = title_row(k, a, area.width)? {
        let h = row.height();
        k.stack(vec![row], at(area.x, area.y, area.width, h), false);
        k.rise("title", 0, 500);
        top += h + k.s(2.5);
    }
    let size = k.s(2.4);
    let muted = k.paint(Role::Muted);
    let fg = k.paint(Role::Foreground);
    let accent = k.paint(Role::Accent);
    let colors = [accent.clone(), fg.clone(), muted.clone()];
    if series.len() > 1 {
        let mut x = area.x;
        let ty = Ty::of(size, 400.0, fg.clone());
        for (i, s) in series.iter().enumerate() {
            let w = k.width_of(&s.name, &ty);
            let d = size * 0.8;
            k.add(
                &format!("key-dot-{}", i + 1),
                ellipse(at(x, top + (size * 1.2 - d) / 2.0, d, d), colors[i].clone()),
            );
            k.text(
                &format!("key-{}", i + 1),
                &s.name,
                &ty,
                x + d + size * 0.5,
                top,
                w,
            );
            x += d + size * 0.5 + w + size * 1.5;
        }
        top += size * 1.2 + k.s(2.0);
    }
    let all: Vec<f64> = series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .collect();
    let lo = all.iter().copied().fold(f64::INFINITY, f64::min).min(0.0);
    let mut hi = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if hi - lo < 1e-9 {
        hi = lo + 1.0;
    }
    let tick_ty = Ty::of(size, 400.0, muted.clone()).align(TextAlign::Right);
    let ticks: Vec<f64> = (0..4)
        .map(|i| lo + (hi - lo) * f64::from(i) / 3.0)
        .collect();
    let axis_w = ticks
        .iter()
        .map(|t| k.width_of(&number_text(*t, unit), &tick_ty))
        .fold(0.0_f64, f64::max);
    let label_h = if labels.is_empty() {
        0.0
    } else {
        (size * 1.2).ceil() + k.s(1.5)
    };
    let plot = at(
        area.x + axis_w + k.s(1.5),
        top + size * 0.6,
        area.width - axis_w - k.s(1.5) - k.s(1.0),
        area.y + area.height - label_h - top - size * 0.6,
    );
    if plot.width < 40.0 || plot.height < 40.0 {
        return Err(BlockError::Fit(
            "the chart box is too small for its axes".into(),
        ));
    }
    let y_at = |v: f64| plot.y + plot.height - (v - lo) / (hi - lo) * plot.height;
    let n = series[0].values.len();
    let x_at = |i: usize| plot.x + plot.width * i as f64 / (n - 1) as f64;
    k.begin("grid");
    for (i, t) in ticks.iter().enumerate() {
        let y = y_at(*t);
        k.add(
            &format!("grid/line-{}", i + 1),
            rect(at(plot.x, y - 0.5, plot.width, 1.0), muted.clone()).alpha(0.3),
        );
        k.text(
            &format!("grid/tick-{}", i + 1),
            &number_text(*t, unit),
            &tick_ty,
            area.x,
            y - size * 0.6,
            axis_w,
        );
    }
    let x_ty = Ty::of(size, 400.0, muted.clone()).align(TextAlign::Center);
    let every = n.div_ceil(8).max(1);
    for (i, l) in labels.iter().enumerate().filter(|(i, _)| i % every == 0) {
        let w = k.width_of(l, &x_ty);
        let x = (x_at(i) - w / 2.0).clamp(area.x, (area.x + area.width - w).max(area.x));
        k.text(
            &format!("grid/x-{}", i + 1),
            l,
            &x_ty,
            x,
            plot.y + plot.height + k.s(1.5),
            w,
        );
    }
    k.end();
    let thick = k.s(0.55).max(3.0);
    k.fade("grid", 0, 500);
    for (si, s) in series.iter().enumerate() {
        let group = format!("series-{}", si + 1);
        k.begin(&group);
        let mut count = 0;
        let length: f64 = (1..n)
            .map(|i| {
                ((x_at(i) - x_at(i - 1)).powi(2)
                    + (y_at(s.values[i]) - y_at(s.values[i - 1])).powi(2))
                .sqrt()
            })
            .sum();
        let step = (thick * 0.45).max(length / MAX_DOTS_PER_LINE).max(1.5);
        let pen = Pen {
            d: thick,
            step,
            fill: colors[si].clone(),
        };
        for i in 1..n {
            let segment = (
                (x_at(i - 1), y_at(s.values[i - 1])),
                (x_at(i), y_at(s.values[i])),
            );
            dots_along(k, &group, segment, &pen, &mut count);
        }
        for (i, v) in s.values.iter().enumerate() {
            let d = thick * 2.4;
            count += 1;
            k.add(
                &format!("{group}/point-{count}"),
                ellipse(
                    at(x_at(i) - d / 2.0, y_at(*v) - d / 2.0, d, d),
                    colors[si].clone(),
                ),
            );
        }
        k.end();
        let t = k.part(&group);
        let start = 400 + 450 * si as u64;
        k.keys(
            t,
            AnimProp::Opacity,
            &[
                (start, 0.0, Ease::Linear),
                (start + 700, 1.0, Ease::EaseOut),
            ],
        );
    }
    Ok(())
}

const DONUT_DEFAULT: &str = r#"[{"label":"Direct","value":46},{"label":"Search","value":31},{"label":"Referral","value":23}]"#;

const DONUT: &[Param] = &[
    Param::text(
        "title",
        "Chart title (empty for none).",
        "Traffic sources",
        60,
    ),
    Param::data(
        "data",
        "2 to 6 slices: [{\"label\": text, \"value\": number >= 0}].",
        DONUT_DEFAULT,
    ),
    Param::text(
        "caption",
        "Small text under the total in the middle.",
        "total",
        14,
    ),
    Param::text("unit", "Appended to values.", "", 4),
    Param::flag("legend", "List the slices with their share.", true),
];

fn slice_paints(k: &mut Kit) -> Vec<Paint> {
    let (bg, fg, accent) = (
        k.paint(Role::Background),
        k.paint(Role::Foreground),
        k.paint(Role::Accent),
    );
    let (b, f, c) = (k.rgba(&bg), k.rgba(&fg), k.rgba(&accent));
    let literal = |hex: String| Paint::parse(&hex).unwrap_or_else(|_| accent.clone());
    vec![
        accent.clone(),
        literal(mix(c, f, 0.5)),
        k.paint(Role::Muted),
        literal(mix(c, b, 0.45)),
        fg,
        literal(mix(f, b, 0.5)),
    ]
}

fn donut_chart(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let slices = labelled(a.data("data"), "data", 2, MAX_SLICES, true)?;
    let total: f64 = slices.iter().map(|s| s.1).sum();
    if total <= 0.0 {
        return Err(bad("data", "the values add up to 0"));
    }
    let area = k.b;
    let mut top = area.y;
    if let Some(row) = title_row(k, a, area.width)? {
        let h = row.height();
        k.stack(vec![row], at(area.x, area.y, area.width, h), false);
        k.rise("title", 0, 500);
        top += h + k.s(2.5);
    }
    let body = at(area.x, top, area.width, area.y + area.height - top);
    let legend = a.flag("legend");
    let wide = body.width >= body.height * 1.25;
    let (ring_box, legend_box) = match (legend, wide) {
        (false, _) => (body, None),
        (true, true) => (
            at(body.x, body.y, body.width * 0.55, body.height),
            Some(at(
                body.x + body.width * 0.58,
                body.y,
                body.width * 0.42,
                body.height,
            )),
        ),
        (true, false) => (
            at(body.x, body.y, body.width, body.height * 0.66),
            Some(at(
                body.x,
                body.y + body.height * 0.7,
                body.width,
                body.height * 0.3,
            )),
        ),
    };
    let diameter = ring_box.width.min(ring_box.height);
    if diameter < 80.0 {
        return Err(BlockError::Fit(
            "the chart box is too small for a ring".into(),
        ));
    }
    let r = diameter / 2.0;
    let thick = r * 0.22;
    let rc = r - thick / 2.0;
    let (cx, cy) = (
        ring_box.x + ring_box.width / 2.0,
        ring_box.y + ring_box.height / 2.0,
    );
    let muted = k.paint(Role::Muted);
    k.add(
        "track",
        ring(at(cx - rc, cy - rc, 2.0 * rc, 2.0 * rc), muted, thick).alpha(0.18),
    );
    k.fade("track", 0, 500);
    let paints = slice_paints(k);
    let gap = if slices.iter().filter(|s| s.1 > 0.0).count() > 1 {
        thick * 1.15 / rc
    } else {
        0.0
    };
    let mut at_angle = -PI / 2.0;
    for (i, (_, v)) in slices.iter().enumerate() {
        let sweep = 2.0 * PI * v / total;
        let (from, to) = (at_angle + gap / 2.0, at_angle + sweep - gap / 2.0);
        at_angle += sweep;
        if *v <= 0.0 {
            continue;
        }
        let group = format!("slice-{}", i + 1);
        k.begin(&group);
        let step = (thick * 0.2 / rc).max(0.004);
        let dots = if to > from {
            ((to - from) / step).ceil() as usize
        } else {
            0
        };
        for j in 0..=dots {
            let t = if dots == 0 {
                (from + to) / 2.0
            } else {
                from + (to - from) * j as f64 / dots as f64
            };
            let (x, y) = (
                cx + rc * t.cos() - thick / 2.0,
                cy + rc * t.sin() - thick / 2.0,
            );
            k.add(
                &format!("{group}/dot-{}", j + 1),
                ellipse(at(x, y, thick, thick), paints[i].clone()),
            );
        }
        k.end();
        let start = 250 + 220 * i as u64;
        k.keys(
            k.part(&group),
            AnimProp::Opacity,
            &[
                (start, 0.0, Ease::Linear),
                (start + 450, 1.0, Ease::EaseOut),
            ],
        );
    }
    let unit = a.text("unit");
    let inner = (r - thick) * 1.5;
    let fg = k.paint(Role::Foreground);
    let value_ty = Ty::of(k.s(9.0), 700.0, fg)
        .align(TextAlign::Center)
        .line(1.0);
    let total_text = number_text(total, unit);
    let (value_ty, vh) = k.fit(&total_text, &value_ty, inner, inner * 0.5, 1)?;
    let caption = a.text("caption");
    let muted = k.paint(Role::Muted);
    let cap_size = k.s(2.6);
    let cap_ty = Ty::of(cap_size, 400.0, muted).align(TextAlign::Center);
    let ch = if caption.is_empty() {
        0.0
    } else {
        (cap_size * 1.2).ceil()
    };
    let y0 = cy - (vh + ch) / 2.0;
    k.text("total", &total_text, &value_ty, cx - inner / 2.0, y0, inner);
    k.fade("total", 100, 500);
    if !caption.is_empty() {
        k.text(
            "caption",
            caption,
            &cap_ty,
            cx - inner / 2.0,
            y0 + vh,
            inner,
        );
        k.fade("caption", 250, 500);
    }
    if let Some(lb) = legend_box {
        legend_rows(k, &slices, &paints, total, lb)?;
    }
    Ok(())
}

fn legend_rows(
    k: &mut Kit,
    slices: &[(String, f64)],
    paints: &[Paint],
    total: f64,
    lb: Rect,
) -> Result<(), BlockError> {
    let size = k.s(2.8);
    let row_h = (lb.height / slices.len() as f64).min(size * 2.4);
    let used = row_h * slices.len() as f64;
    let y0 = lb.y + (lb.height - used) / 2.0;
    let fg = k.paint(Role::Foreground);
    let muted = k.paint(Role::Muted);
    let d = size * 0.8;
    let pct_w = k.width_of("100%", &Ty::of(size, 700.0, fg.clone()));
    for (i, (label, v)) in slices.iter().enumerate() {
        let y = y0 + row_h * i as f64;
        let id = i + 1;
        let label_w = lb.width - d - size * 0.6 - pct_w - size * 0.6;
        let label_ty = Ty::of(size, 400.0, fg.clone());
        let (label_ty, _) = k.fit(label, &label_ty, label_w, size * 1.6, 1)?;
        let pct = format!("{}%", (v / total * 100.0).round() as i64);
        let pct_ty = Ty::of(size, 700.0, muted.clone()).align(TextAlign::Right);
        k.begin(&format!("legend-{id}"));
        let part = |s: &str| format!("legend-{id}/{s}");
        k.add(
            &part("dot"),
            ellipse(at(lb.x, y + (row_h - d) / 2.0, d, d), paints[i].clone()),
        );
        k.text(
            &part("label"),
            label,
            &label_ty,
            lb.x + d + size * 0.6,
            y + (row_h - size * 1.2) / 2.0,
            label_w,
        );
        k.text(
            &part("share"),
            &pct,
            &pct_ty,
            lb.x + lb.width - pct_w,
            y + (row_h - size * 1.2) / 2.0,
            pct_w,
        );
        k.end();
        k.reveal_group(&format!("legend-{id}"), 500 + 90 * i as u64, 450, k.s(1.0));
    }
    Ok(())
}

const PROGRESS: &[Param] = &[
    Param::text("label", "What is progressing.", "Storage used", 50),
    Param::count("value", "Percent complete.", 72, 0, 100),
    Param::flag("show_value", "Show the percentage at the right.", true),
];

fn progress_bar(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let area = k.b;
    let size = k.s(3.2);
    let fg = k.paint(Role::Foreground);
    let accent = k.paint(Role::Accent);
    let value = a.num("value");
    let pct = format!("{}%", value as i64);
    let pct_ty = Ty::of(size, 700.0, accent.clone()).align(TextAlign::Right);
    let pct_w = if a.flag("show_value") {
        k.width_of("100%", &pct_ty)
    } else {
        0.0
    };
    let label_w = area.width - pct_w - k.s(2.0);
    let (label_ty, label_h) = k.fit(
        a.text("label"),
        &Ty::of(size, 700.0, fg),
        label_w,
        size * 1.6,
        1,
    )?;
    let track_h = k.s(2.2);
    let gap = k.s(1.8);
    let total = label_h + gap + track_h;
    let y = area.y + (area.height - total) / 2.0;
    k.text("label", a.text("label"), &label_ty, area.x, y, label_w);
    if a.flag("show_value") {
        k.text(
            "value",
            &pct,
            &pct_ty,
            area.x + area.width - pct_w,
            y,
            pct_w,
        );
        k.fade("value", 300, 500);
    }
    let muted = k.paint(Role::Muted);
    let ty = y + label_h + gap;
    k.add(
        "track",
        rect(at(area.x, ty, area.width, track_h), muted)
            .radius(track_h / 2.0)
            .alpha(0.25),
    );
    let fill_w = (area.width * value / 100.0).max(0.0);
    k.add(
        "fill",
        rect(at(area.x, ty, fill_w, track_h), accent).radius(track_h / 2.0),
    );
    k.rise("label", 0, 450);
    k.fade("track", 0, 400);
    k.keys(
        k.part("fill"),
        AnimProp::Width,
        &[(200, 0.0, Ease::Linear), (1300, fill_w, Ease::EaseOut)],
    );
    Ok(())
}

const TABLE_COLUMNS: &str = r#"["Feature","Free","Pro"]"#;
const TABLE_ROWS: &str =
    r#"[["Projects","3","Unlimited"],["Exports","720p","4K"],["Support","Community","Priority"]]"#;

const TABLE: &[Param] = &[
    Param::data(
        "columns",
        "2 to 6 column headings; the first names the rows.",
        TABLE_COLUMNS,
    ),
    Param::data(
        "rows",
        "1 to 8 rows, each a list with one text per column.",
        TABLE_ROWS,
    ),
    Param::count(
        "highlight",
        "Index (from 0) of a column to emphasize, or -1.",
        2,
        -1,
        5,
    ),
];

fn comparison_table(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let columns: Vec<String> = list(a.data("columns"), "columns", 2, MAX_COLUMNS)?
        .iter()
        .map(|c| short_text(c, "columns", MAX_CELL))
        .collect::<Result<_, _>>()?;
    let rows: Vec<Vec<String>> = list(a.data("rows"), "rows", 1, MAX_ROWS)?
        .iter()
        .map(|r| {
            let cells = list(r, "rows", columns.len(), columns.len())?;
            cells
                .iter()
                .map(|c| short_text(c, "rows", MAX_CELL))
                .collect()
        })
        .collect::<Result<_, _>>()?;
    let area = k.b;
    let n = columns.len();
    let first = 1.35;
    let unit_w = area.width / (first + (n - 1) as f64);
    let col_w = |c: usize| if c == 0 { unit_w * first } else { unit_w };
    let col_x = |c: usize| {
        area.x
            + if c == 0 {
                0.0
            } else {
                unit_w * first + unit_w * (c - 1) as f64
            }
    };
    let row_h = (area.height / (rows.len() + 1) as f64).min(k.s(9.0));
    let table_top = area.y + ((area.height - row_h * (rows.len() + 1) as f64) / 2.0).max(0.0);
    let pad = k.s(1.6);
    let fg = k.paint(Role::Foreground);
    let muted = k.paint(Role::Muted);
    let accent = k.paint(Role::Accent);
    let base = Ty::of(k.s(2.9), 400.0, fg.clone());
    let mut size = base.size;
    for (r, line) in std::iter::once(&columns).chain(rows.iter()).enumerate() {
        for (c, cell) in line.iter().enumerate() {
            let weight = if r == 0 || c == 0 { 700.0 } else { 400.0 };
            let ty = Ty {
                weight,
                ..base.clone()
            };
            size = size.min(k.fit_size(cell, &ty, col_w(c) - 2.0 * pad, row_h, 1, k.s(2.3))?);
        }
    }
    let text_h = (size * 1.2).ceil();
    let highlight = a.num("highlight");
    if highlight >= 0.0 && (highlight as usize) < n {
        let c = highlight as usize;
        k.add(
            "highlight",
            rect(
                at(col_x(c), area.y, col_w(c), row_h * (rows.len() + 1) as f64),
                accent.clone(),
            )
            .radius(k.s(1.0))
            .alpha(0.14),
        );
        k.fade("highlight", 0, 500);
    }
    for (r, line) in std::iter::once(&columns).chain(rows.iter()).enumerate() {
        let y = table_top + row_h * r as f64;
        let group = if r == 0 {
            "header".to_string()
        } else {
            format!("row-{r}")
        };
        k.begin(&group);
        if r % 2 == 0 && r > 0 {
            k.add(
                &format!("{group}/band"),
                rect(at(area.x, y, area.width, row_h), fg.clone())
                    .radius(k.s(0.8))
                    .alpha(0.06),
            );
        }
        for (c, cell) in line.iter().enumerate() {
            let (color, weight) = match (r, c) {
                (0, _) => (accent.clone(), 700.0),
                (_, 0) => (fg.clone(), 700.0),
                _ if highlight as usize == c && highlight >= 0.0 => (fg.clone(), 700.0),
                _ => (muted.clone(), 400.0),
            };
            let ty = Ty::of(size, weight, color).align(if c == 0 {
                TextAlign::Left
            } else {
                TextAlign::Center
            });
            k.text(
                &format!("{group}/cell-{}", c + 1),
                cell,
                &ty,
                col_x(c) + pad,
                y + (row_h - text_h) / 2.0,
                col_w(c) - 2.0 * pad,
            );
        }
        if r == 0 {
            k.add(
                "header/rule",
                rect(at(area.x, y + row_h - 1.0, area.width, 2.0), accent.clone()),
            );
        }
        k.end();
        k.reveal_group(&group, 100 + 90 * r as u64, 450, k.s(1.0));
    }
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "bar_chart",
        category: Category::Data,
        about: "A bar chart of 2 to 12 labelled values that grow up from the baseline, with value labels and an optional highlighted bar.",
        tags: &[
            "bar",
            "chart",
            "graph",
            "data",
            "compare",
            "values",
            "histogram",
        ],
        params: BAR,
        place: Place::Safe,
        layer: Layer::Front,
        build: bar_chart,
    },
    Block {
        id: "line_chart",
        category: Category::Data,
        about: "A line chart of up to 3 series and 24 points with gridlines and axis labels, drawn on one line at a time (lines are chains of dots: the document has no path node).",
        tags: &[
            "line",
            "chart",
            "graph",
            "trend",
            "data",
            "time series",
            "growth",
        ],
        params: LINE,
        place: Place::Safe,
        layer: Layer::Front,
        build: line_chart,
    },
    Block {
        id: "donut_chart",
        category: Category::Data,
        about: "A ring of up to 6 slices with the total in the middle and a legend of shares (slices are rounded arcs made of dots).",
        tags: &[
            "donut",
            "pie",
            "chart",
            "share",
            "data",
            "ring",
            "proportion",
            "breakdown",
        ],
        params: DONUT,
        place: Place::Safe,
        layer: Layer::Front,
        build: donut_chart,
    },
    Block {
        id: "progress_bar",
        category: Category::Data,
        about: "A labelled progress bar whose fill grows to a percentage.",
        tags: &[
            "progress",
            "bar",
            "percent",
            "loading",
            "meter",
            "goal",
            "completion",
        ],
        params: PROGRESS,
        place: Place::Safe,
        layer: Layer::Front,
        build: progress_bar,
    },
    Block {
        id: "comparison_table",
        category: Category::Data,
        about: "A table of up to 6 columns and 8 rows with a header, banded rows and one emphasized column, for plans and feature comparisons.",
        tags: &[
            "table",
            "comparison",
            "compare",
            "pricing",
            "features",
            "plans",
            "grid",
            "data",
        ],
        params: TABLE,
        place: Place::Safe,
        layer: Layer::Front,
        build: comparison_table,
    },
];
