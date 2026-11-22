#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::{Animation, MAX_KEYS};
use od_core::{NodeId, transform};

use super::*;

fn pose() -> Pose {
    Pose {
        x: 400.0,
        y: 300.0,
        width: 200.0,
        height: 120.0,
        rotation: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        opacity: 1.0,
        stage_w: 1920.0,
        stage_h: 1080.0,
    }
}

fn camera() -> CameraPose {
    CameraPose {
        x: 0.0,
        y: 0.0,
        zoom: 1.0,
        rotation: 0.0,
        stage_w: 1920.0,
        stage_h: 1080.0,
    }
}

fn animation(t: &Track) -> Animation {
    Animation {
        node: NodeId::parse("n_1").unwrap(),
        prop: t.prop,
        keys: t.keys.clone(),
        repeat: t.repeat,
    }
}

fn all_names() -> Vec<Named> {
    Named::NAMES
        .iter()
        .map(|n| Named::parse(n).unwrap())
        .collect()
}

fn tracks_of(named: Named, spec: &Spec) -> Vec<Track> {
    if named.is_camera() {
        camera_tracks(named, spec, &camera())
    } else {
        node_tracks(named, spec, &pose())
    }
}

#[test]
fn every_name_parses_and_round_trips() {
    assert_eq!(all_names().len(), Named::NAMES.len());
    for side in ["left", "right", "up", "down"] {
        assert_eq!(Side::parse(side).map(Side::as_str), Some(side));
    }
}

#[test]
fn every_move_gives_strictly_increasing_keys_inside_its_time_range() {
    for named in all_names() {
        let spec = Spec::new(1_500, named.default_duration());
        let tracks = tracks_of(named, &spec);
        assert!(!tracks.is_empty(), "{named:?} made no track");
        for t in &tracks {
            assert!(
                t.keys.len() >= 2 && t.keys.len() <= MAX_KEYS,
                "{named:?} {:?}",
                t.prop
            );
            assert!(
                t.keys.windows(2).all(|w| w[0].at_ms < w[1].at_ms),
                "{named:?} {:?} keys do not increase",
                t.prop
            );
            assert_eq!(t.keys.first().map(|k| k.at_ms), Some(spec.start_ms));
            assert!(t.keys.last().is_some_and(|k| k.at_ms <= spec.end_ms()));
            assert!(t.keys.iter().all(|k| k.value.is_finite()));
        }
    }
}

#[test]
fn keys_keep_a_frame_apart_even_for_a_short_move() {
    let mut spec = Spec::new(0, 100);
    spec.gap_ms = 42;
    for named in all_names() {
        for t in tracks_of(named, &spec) {
            assert!(
                t.keys.windows(2).all(|w| w[1].at_ms - w[0].at_ms >= 42),
                "{named:?} {:?}",
                t.prop
            );
        }
    }
}

#[test]
fn ambient_moves_loop_and_end_where_they_start() {
    for named in all_names().into_iter().filter(|n| n.ambient()) {
        let spec = Spec::new(0, named.default_duration());
        for t in tracks_of(named, &spec) {
            assert_eq!(t.repeat, Repeat::Loop, "{named:?}");
            let (first, last) = (t.keys[0].value, t.keys[t.keys.len() - 1].value);
            if named == Named::Spin {
                assert!((last - first - 360.0).abs() < 1e-9, "a spin ends a turn on");
            } else {
                assert!(
                    (first - last).abs() < 1e-9,
                    "{named:?} {:?} has a seam",
                    t.prop
                );
            }
            let a = animation(&t);
            let span = spec.duration_ms as f64;
            let (v0, v1) = (a.value_at(0.0).unwrap(), a.value_at(span).unwrap());
            if named != Named::Spin {
                assert!((v0 - v1).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn entrances_play_once() {
    for named in all_names().into_iter().filter(|n| !n.ambient()) {
        for t in tracks_of(named, &Spec::new(0, 600)) {
            assert_eq!(t.repeat, Repeat::Once, "{named:?}");
        }
    }
}

#[test]
fn a_seed_moves_the_phase() {
    let at = |seed: u64| {
        let mut s = Spec::new(0, 4_000);
        s.seed = seed;
        node_tracks(Named::Float, &s, &pose())[0].keys[0].value
    };
    assert!((at(1) - at(2)).abs() > 0.1);
    assert_eq!(at(7), at(7));
}

#[test]
fn pop_in_overshoots_then_settles() {
    let spec = Spec::new(0, 450);
    let t = node_tracks(Named::PopIn, &spec, &pose());
    let scale = t.iter().find(|t| t.prop == AnimProp::ScaleX).unwrap();
    let a = animation(scale);
    let peak = (0..=450)
        .map(|ms| a.value_at(f64::from(ms)).unwrap())
        .fold(0.0_f64, f64::max);
    assert!(peak > 1.05 && peak < 1.25, "peak {peak}");
    assert!(a.value_at(0.0).unwrap() < 0.01);
    assert!((a.value_at(450.0).unwrap() - 1.0).abs() < 1e-9);
}

#[test]
fn bounce_in_lands_on_the_node_after_bouncing() {
    let t = node_tracks(Named::BounceIn, &Spec::new(0, 800), &pose());
    let y = animation(t.iter().find(|t| t.prop == AnimProp::Y).unwrap());
    assert!(y.value_at(0.0).unwrap() < 300.0 - 100.0);
    assert!((y.value_at(800.0).unwrap() - 300.0).abs() < 1e-9);
    let touches = (0..800)
        .filter(|ms| (y.value_at(f64::from(*ms)).unwrap() - 300.0).abs() < 1.0)
        .count();
    assert!(touches > 0);
}

#[test]
fn slides_come_from_their_side_and_overshoot() {
    for (side, horizontal, sign) in [
        (Side::Left, true, -1.0),
        (Side::Right, true, 1.0),
        (Side::Up, false, -1.0),
        (Side::Down, false, 1.0),
    ] {
        let t = node_tracks(Named::SlideIn(side), &Spec::new(0, 550), &pose());
        let prop = if horizontal { AnimProp::X } else { AnimProp::Y };
        let home = if horizontal { 400.0 } else { 300.0 };
        let a = animation(t.iter().find(|t| t.prop == prop).unwrap());
        assert!(
            (a.value_at(0.0).unwrap() - home) * sign < -50.0,
            "{side:?} starts on its own side"
        );
        let over = (0..=550)
            .map(|ms| (a.value_at(f64::from(ms)).unwrap() - home) * sign)
            .fold(f64::MIN, f64::max);
        assert!(over > 1.0, "{side:?} overshoots by {over}");
        assert!((a.value_at(550.0).unwrap() - home).abs() < 1e-9);
    }
}

#[test]
fn shake_dies_away_and_ends_on_the_node() {
    let t = node_tracks(Named::Shake, &Spec::new(0, 500), &pose());
    let x = animation(t.iter().find(|t| t.prop == AnimProp::X).unwrap());
    let early = (0..120)
        .map(|ms| (x.value_at(f64::from(ms)).unwrap() - 400.0).abs())
        .fold(0.0, f64::max);
    let late = (380..=500)
        .map(|ms| (x.value_at(f64::from(ms)).unwrap() - 400.0).abs())
        .fold(0.0, f64::max);
    assert!(early > 3.0 && late < early / 2.0, "{early} {late}");
    assert!((x.value_at(500.0).unwrap() - 400.0).abs() < 1e-9);
}

#[test]
fn draw_on_runs_the_trim_from_nothing_to_all() {
    let t = node_tracks(Named::DrawOn, &Spec::new(200, 900), &pose());
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].prop, AnimProp::TrimEnd);
    assert_eq!((t[0].keys[0].value, t[0].keys[1].value), (0.0, 1.0));
}

#[test]
fn orbit_samples_a_circle_about_the_center() {
    let mut spec = Spec::new(0, 8_000);
    spec.repeat = None;
    let p = pose();
    let t = orbit(&spec, &p, Some((300.0, 360.0)), None);
    let (x, y) = (animation(&t[0]), animation(&t[1]));
    assert!(t[0].keys.windows(2).all(|w| w[1].at_ms - w[0].at_ms <= 37));
    for ms in (0..8_000).step_by(250) {
        let (px, py) = (
            x.value_at(f64::from(ms)).unwrap() + 100.0,
            y.value_at(f64::from(ms)).unwrap() + 60.0,
        );
        let r = (px - 300.0).hypot(py - 360.0);
        assert!((r - 200.0).abs() < 1.5, "radius {r} at {ms}");
    }
    assert_eq!(t[0].repeat, Repeat::Loop);
    assert!((t[0].keys[0].value - t[0].keys[t[0].keys.len() - 1].value).abs() < 1e-9);
}

#[test]
fn following_a_path_starts_and_ends_on_it() {
    let points = [(100.0, 100.0), (400.0, 100.0), (400.0, 500.0)];
    let p = pose();
    let t = follow(&Spec::new(0, 1_000), &p, &points, true);
    let (x, y, r) = (animation(&t[0]), animation(&t[1]), animation(&t[2]));
    let mid = |a: &Animation, ms: f64, half: f64| a.value_at(ms).unwrap() + half;
    assert!(
        (mid(&x, 0.0, 100.0) - 100.0).abs() < 1e-6 && (mid(&y, 0.0, 60.0) - 100.0).abs() < 1e-6
    );
    assert!(
        (mid(&x, 1_000.0, 100.0) - 400.0).abs() < 1e-6
            && (mid(&y, 1_000.0, 60.0) - 500.0).abs() < 1e-6
    );
    assert!(
        r.value_at(100.0).unwrap().abs() < 1.0,
        "heads right on the first leg"
    );
    assert!(
        (r.value_at(1_000.0).unwrap() - 90.0).abs() < 1.0,
        "heads down on the last"
    );
}

#[test]
fn a_straight_path_with_a_plain_ease_needs_two_keys() {
    let t = follow(
        &Spec::new(0, 1_000),
        &pose(),
        &[(0.0, 0.0), (300.0, 0.0)],
        false,
    );
    assert_eq!(t[0].keys.len(), 2);
    assert_eq!(t[0].keys[1].ease, Ease::EaseInOut);
}

#[test]
fn camera_moves_write_camera_props() {
    let spec = Spec::new(0, 2_000);
    let zoom = camera_tracks(Named::CameraPushIn, &spec, &camera());
    assert_eq!(zoom[0].prop, AnimProp::CameraZoom);
    assert!(zoom[0].keys[1].value > 1.1);
    let back = camera_tracks(Named::CameraPullBack, &spec, &camera());
    assert!(back[0].keys[0].value > back[0].keys[1].value);
    let mut right = Spec::new(0, 2_000);
    right.side = Some(Side::Left);
    let pan = camera_tracks(Named::CameraPan, &right, &camera());
    assert_eq!(pan[0].prop, AnimProp::CameraX);
    assert!(pan[0].keys[1].value < -100.0);
    right.amount = Some(50.0);
    right.side = Some(Side::Down);
    let down = camera_tracks(Named::CameraPan, &right, &camera());
    assert_eq!(
        (down[0].prop, down[0].keys[1].value),
        (AnimProp::CameraY, 50.0)
    );
    let drift = camera_tracks(Named::CameraDrift, &Spec::new(0, 12_000), &camera());
    assert_eq!(drift.len(), 3);
    assert!(drift.iter().all(|t| t.repeat == Repeat::Loop));
    let whip = camera_tracks(Named::CameraWhip, &Spec::new(0, 450), &camera());
    assert_eq!(whip[0].keys[1].ease, Ease::EaseInOutCubic);
}

#[test]
fn camera_values_stay_inside_the_camera_limits() {
    let mut spec = Spec::new(0, 2_000);
    spec.intensity = 4.0;
    for named in all_names().into_iter().filter(|n| n.is_camera()) {
        for t in camera_tracks(named, &spec, &camera()) {
            for k in &t.keys {
                match t.prop {
                    AnimProp::CameraZoom => {
                        assert!(
                            (transform::MIN_CAMERA_ZOOM..=transform::MAX_CAMERA_ZOOM)
                                .contains(&k.value)
                        );
                    }
                    AnimProp::CameraX | AnimProp::CameraY => {
                        assert!(k.value.abs() <= transform::MAX_CAMERA_PAN)
                    }
                    _ => {}
                }
            }
        }
    }
}

/// A stagger of `step` milliseconds, one step per unit of distance.
fn each(step: f64) -> StaggerSpec {
    StaggerSpec {
        step: Step::Each(step),
        ..StaggerSpec::default()
    }
}

#[test]
fn stagger_orders_by_origin() {
    let delays = |n: usize, from: Origin| {
        stagger_offsets_ms(
            n,
            &StaggerSpec {
                from,
                ..each(100.0)
            },
        )
    };
    assert_eq!(delays(4, Origin::First), vec![0, 100, 200, 300]);
    assert_eq!(delays(4, Origin::Last), vec![300, 200, 100, 0]);
    assert_eq!(delays(5, Origin::Center), vec![200, 100, 0, 100, 200]);
    let random = stagger_offsets_ms(
        6,
        &StaggerSpec {
            from: Origin::Random,
            seed: Some(5),
            ..each(100.0)
        },
    );
    let mut sorted = random.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 100, 200, 300, 400, 500]);
    assert_eq!(
        random,
        stagger_offsets_ms(
            6,
            &StaggerSpec {
                from: Origin::Random,
                seed: Some(5),
                ..each(100.0)
            }
        )
    );
}

#[test]
fn the_generator_is_deterministic_and_spread() {
    let (mut a, mut b) = (Rng::new(9), Rng::new(9));
    assert_eq!(a.next_u64(), b.next_u64());
    let mean: f64 = (0..2_000).map(|_| a.unit()).sum::<f64>() / 2_000.0;
    assert!((mean - 0.5).abs() < 0.05);
    assert!((0..200).all(|_| (3..=7).contains(&a.count(3, 7))));
}

#[test]
fn edges_go_together_from_both_ends_inward() {
    let delays = |n: usize| {
        stagger_offsets_ms(
            n,
            &StaggerSpec {
                from: Origin::Edges,
                ..each(100.0)
            },
        )
    };
    assert_eq!(delays(5), vec![0, 100, 200, 100, 0]);
    assert_eq!(delays(4), vec![0, 100, 100, 0]);
    assert_eq!(Origin::parse("edges"), Some(Origin::Edges));
}

#[test]
fn a_grid_stagger_delays_by_distance_from_the_start_cell() {
    // 3 columns x 2 rows, indices 0 1 2 / 3 4 5
    let grid = |from, metric| {
        stagger_offsets_ms(
            6,
            &StaggerSpec {
                from,
                grid: Some((3, 2)),
                metric,
                ..each(100.0)
            },
        )
    };
    assert_eq!(
        grid(Origin::First, Metric::Manhattan),
        vec![0, 100, 200, 100, 200, 300]
    );
    assert_eq!(
        grid(Origin::Last, Metric::Manhattan),
        vec![300, 200, 100, 200, 100, 0]
    );
    assert_eq!(
        grid(Origin::Index(4), Metric::Manhattan),
        vec![200, 100, 200, 100, 0, 100]
    );
    assert_eq!(
        grid(Origin::First, Metric::Euclid),
        vec![0, 100, 200, 100, 141, 224]
    );
    // a 3x3 grid: the middle cell is last from the edges, first from the center
    let edges = stagger_offsets_ms(
        9,
        &StaggerSpec {
            from: Origin::Edges,
            grid: Some((3, 3)),
            ..each(100.0)
        },
    );
    assert_eq!(edges, vec![0, 0, 0, 0, 100, 0, 0, 0, 0]);
    let center = stagger_offsets_ms(
        9,
        &StaggerSpec {
            from: Origin::Center,
            grid: Some((3, 3)),
            metric: Metric::Manhattan,
            ..each(100.0)
        },
    );
    assert_eq!(center, vec![200, 100, 200, 100, 0, 100, 200, 100, 200]);
    let random = stagger_offsets_ms(
        9,
        &StaggerSpec {
            from: Origin::Random,
            grid: Some((3, 3)),
            seed: Some(4),
            ..each(100.0)
        },
    );
    assert_eq!(
        random,
        stagger_offsets_ms(
            9,
            &StaggerSpec {
                from: Origin::Random,
                grid: Some((3, 3)),
                seed: Some(4),
                ..each(100.0)
            }
        )
    );
}

#[test]
fn the_stagger_vectors_are_the_ones_the_research_gives() {
    let delays = |n: usize, spec: StaggerSpec| stagger_offsets_ms(n, &spec);
    assert_eq!(delays(5, each(100.0)), vec![0, 100, 200, 300, 400]);
    assert_eq!(
        delays(
            5,
            StaggerSpec {
                from: Origin::Center,
                ..each(100.0)
            }
        ),
        vec![200, 100, 0, 100, 200]
    );
    assert_eq!(
        delays(
            4,
            StaggerSpec {
                from: Origin::Center,
                ..each(100.0)
            }
        ),
        vec![150, 50, 50, 150]
    );
    assert_eq!(
        delays(
            5,
            StaggerSpec {
                from: Origin::Last,
                ..each(100.0)
            }
        ),
        vec![400, 300, 200, 100, 0]
    );
    assert_eq!(
        delays(
            5,
            StaggerSpec {
                from: Origin::Index(1),
                ..each(100.0)
            }
        ),
        vec![100, 0, 100, 200, 300]
    );
    assert_eq!(
        delays(
            5,
            StaggerSpec {
                reversed: true,
                ..each(100.0)
            }
        ),
        vec![400, 300, 200, 100, 0]
    );
    assert_eq!(
        delays(
            5,
            StaggerSpec {
                ease: Some(Ease::PowerOut(2000)),
                ..each(100.0)
            }
        ),
        vec![0, 175, 300, 375, 400]
    );
    // a grid of 3 columns and 2 rows
    let grid = |from, metric, axis| {
        delays(
            6,
            StaggerSpec {
                from,
                grid: Some((3, 2)),
                metric,
                axis,
                ..each(100.0)
            },
        )
    };
    assert_eq!(
        grid(Origin::First, Metric::Euclid, None),
        vec![0, 100, 200, 100, 141, 224]
    );
    assert_eq!(
        grid(Origin::Center, Metric::Euclid, None),
        vec![112, 50, 112, 112, 50, 112]
    );
    assert_eq!(
        grid(Origin::First, Metric::Manhattan, None),
        vec![0, 100, 200, 100, 200, 300]
    );
    assert_eq!(
        grid(Origin::First, Metric::Euclid, Some(Axis::X)),
        vec![0, 100, 200, 0, 100, 200]
    );
    assert_eq!(
        grid(Origin::First, Metric::Euclid, Some(Axis::Y)),
        vec![0, 0, 0, 100, 100, 100]
    );
    // reading one axis from the last cell would start before zero: the delays shift
    assert_eq!(
        grid(Origin::Last, Metric::Euclid, Some(Axis::X)),
        vec![0, 100, 200, 0, 100, 200]
    );
}

#[test]
fn a_value_stagger_spreads_a_range_over_the_distances() {
    let values = |n: usize, range: [f64; 2], from: Origin| {
        stagger_values(
            n,
            &StaggerSpec {
                step: Step::Range(range[0], range[1]),
                from,
                ..StaggerSpec::default()
            },
        )
    };
    let got = values(4, [1.0, 0.1], Origin::First);
    assert!((got[0] - 1.0).abs() < 1e-9);
    assert!((got[1] - 0.7).abs() < 1e-9);
    assert!((got[2] - 0.4).abs() < 1e-9);
    assert!((got[3] - 0.1).abs() < 1e-9);
    // one node alone has no distance to spread over
    let alone = values(1, [1.0, 0.1], Origin::First);
    assert_eq!(alone.len(), 1);
    assert!((alone[0] - 1.0).abs() < 1e-9);
    // a scale that falls off from the middle
    let scales = values(5, [1.0, 0.6], Origin::Center);
    let want = [0.6, 0.8, 1.0, 0.8, 0.6];
    for (got, want) in scales.iter().zip(want) {
        assert!((got - want).abs() < 1e-9, "{got} against {want}");
    }
}

#[test]
fn a_spatial_stagger_measures_between_the_real_centres() {
    let square = stagger_offsets_ms(
        4,
        &StaggerSpec {
            positions: Some(vec![(0.0, 0.0), (100.0, 0.0), (0.0, 100.0), (100.0, 100.0)]),
            ..each(100.0)
        },
    );
    assert_eq!(square, vec![0, 100, 100, 141]);
    let row = stagger_offsets_ms(
        3,
        &StaggerSpec {
            positions: Some(vec![(0.0, 0.0), (100.0, 0.0), (300.0, 0.0)]),
            ..each(100.0)
        },
    );
    assert_eq!(row, vec![0, 100, 300]);
}

#[test]
fn the_seeded_generator_is_the_same_every_run() {
    let mut rng = Mulberry32::new(0);
    assert!((rng.next_f64() - 0.26642920868471265).abs() < 1e-15);
    let mut rng = Mulberry32::new(0);
    assert!((rng.range(-1.0, 1.0, 4) + 0.4672).abs() < 1e-9);
    let mut rng = Mulberry32::new(0);
    assert_eq!(rng.range(0.0, 1.0, 0), 0.0);
    let mut rng = Mulberry32::new(0);
    assert_eq!(rng.range(0.0, 100.0, 0), 26.0);
    let mut items: Vec<u32> = (0..8).collect();
    let mut rng = Mulberry32::new(3);
    rng.shuffle(&mut items);
    let mut sorted = items.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..8).collect::<Vec<_>>());
    assert_eq!(items, {
        let mut again: Vec<u32> = (0..8).collect();
        Mulberry32::new(3).shuffle(&mut again);
        again
    });
}

#[test]
fn a_jittered_stagger_is_seeded_and_shifts_what_it_pushes_below_zero() {
    let jittered = |seed: u64| {
        stagger_values(
            1,
            &StaggerSpec {
                jitter: Some((10.0, 10.0)),
                seed: Some(seed),
                ..each(100.0)
            },
        )
    };
    let value = jittered(0)[0];
    assert!((value + 4.672).abs() < 1e-6, "{value}");
    assert_eq!(jittered(0), jittered(0));
    assert_ne!(jittered(0), jittered(7));
    // the delays never go below zero
    let offsets = stagger_offsets_ms(
        5,
        &StaggerSpec {
            jitter: Some((200.0, 200.0)),
            seed: Some(1),
            ..each(100.0)
        },
    );
    assert_eq!(offsets.iter().min(), Some(&0));
}

#[test]
fn a_camera_burst_is_fastest_at_its_first_frame_then_holds() {
    let mut spec = Spec::new(1_000, 600);
    spec.gap_ms = 35;
    spec.side = Some(Side::Left);
    spec.amount = Some(300.0);
    let tracks = camera_tracks(Named::CameraBurst, &spec, &camera());
    assert_eq!(tracks.len(), 1);
    let t = &tracks[0];
    assert_eq!(t.prop, AnimProp::CameraX);
    assert_eq!(t.keys[0].value, 0.0);
    assert_eq!(t.keys[1].ease, Ease::Power4Out);
    assert_eq!(t.keys[1].value, -300.0);
    assert_eq!(t.keys[1].at_ms, 1_000 + 9 * 33);
    assert_eq!(t.keys[2].value, -300.0);
    assert_eq!(t.keys[2].at_ms, 1_600);
    let a = animation(t);
    let step = |ms: f64| (a.value_at(ms + 33.0).unwrap() - a.value_at(ms).unwrap()).abs();
    assert!(step(1_000.0) > 3.0 * step(1_000.0 + 3.0 * 33.0), "decays");
    assert_eq!(a.value_at(1_400.0), Some(-300.0));
    assert_eq!(a.value_at(1_500.0), Some(-300.0), "then holds");
}

#[test]
fn a_vertical_burst_moves_the_camera_y() {
    let mut spec = Spec::new(0, 600);
    spec.side = Some(Side::Down);
    let tracks = camera_tracks(Named::CameraBurst, &spec, &camera());
    assert_eq!(tracks[0].prop, AnimProp::CameraY);
    assert!((tracks[0].keys[1].value - 1080.0 / 6.0).abs() < 1e-9);
}

#[test]
fn a_steps_ease_holds_each_pose_in_a_preset_track() {
    let mut spec = Spec::new(0, 1_000);
    spec.ease = Some(Ease::Steps4);
    let track = camera_tracks(Named::CameraPan, &spec, &camera());
    let a = animation(&track[0]);
    let (first, mid, late) = (
        a.value_at(100.0).unwrap(),
        a.value_at(400.0).unwrap(),
        a.value_at(990.0).unwrap(),
    );
    assert_eq!(first, a.value_at(240.0).unwrap(), "held for a quarter");
    assert!(mid > first && late > mid);
}
