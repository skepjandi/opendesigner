#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::beats::{beat_times, next_unbuilt, sound_plan};
use super::schema::{Beat, Layer, SceneSpec};
use super::validate::validate;

fn spec() -> SceneSpec {
    let mut s = SceneSpec {
        id: "main".into(),
        ..SceneSpec::default()
    };
    s.complexity.tier = "simple".into();
    s.brief.duration_ms = 20_000;
    s.layer_plan = vec![Layer {
        id: "hero".into(),
        level: "macro".into(),
        role: "hero".into(),
        node_ref: Some("n7".into()),
        ..Layer::default()
    }];
    let beat = |id: &str, at: u64| Beat {
        id: id.into(),
        at_ms: at,
        takeaway: "the point".into(),
        focal_node: Some("hero".into()),
        transformation: "it rises".into(),
        ..Beat::default()
    };
    s.beat_map = vec![
        beat("hook", 500),
        beat("reveal", 6_000),
        beat("close", 13_000),
    ];
    s
}

fn error_paths(s: &SceneSpec) -> Vec<String> {
    validate(s)
        .errors
        .into_iter()
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

#[test]
fn a_good_beat_map_has_no_errors_and_the_node_ref_resolves_a_focal() {
    let mut s = spec();
    assert!(error_paths(&s).is_empty(), "{:?}", error_paths(&s));
    s.beat_map[0].focal_node = Some("n7".into());
    s.beat_map[0].exit_ms = Some(4_000);
    assert!(error_paths(&s).is_empty());
}

#[test]
fn an_empty_beat_map_is_not_checked() {
    let mut s = spec();
    s.beat_map.clear();
    assert!(error_paths(&s).is_empty());
}

#[test]
fn times_must_strictly_increase() {
    let mut s = spec();
    s.beat_map[1].at_ms = 500;
    let e = error_paths(&s);
    assert!(
        e.iter().any(|m| m.starts_with("beat_map[1].at_ms")),
        "{e:?}"
    );
}

#[test]
fn times_must_be_inside_the_video() {
    let mut s = spec();
    s.beat_map[2].at_ms = 20_000;
    let e = error_paths(&s);
    assert!(
        e.iter()
            .any(|m| m.contains("not inside the 20000 ms video")),
        "{e:?}"
    );
}

#[test]
fn a_focal_node_must_be_a_layer() {
    let mut s = spec();
    s.beat_map[0].focal_node = Some("ghost".into());
    let e = error_paths(&s);
    assert!(
        e.iter().any(|m| m.starts_with("beat_map[0].focal_node")),
        "{e:?}"
    );
}

#[test]
fn there_is_a_beat_at_least_every_eight_seconds() {
    let mut s = spec();
    s.beat_map.remove(1);
    let e = error_paths(&s);
    assert!(
        e.iter().any(|m| m.contains("at least one beat every 8 s")),
        "{e:?}"
    );
    let mut late = spec();
    late.beat_map[0].at_ms = 8_500;
    assert!(
        error_paths(&late)
            .iter()
            .any(|m| m.contains("no beat between 0 ms and 8500 ms"))
    );
    let mut tail = spec();
    tail.brief.duration_ms = 21_100;
    assert!(
        error_paths(&tail)
            .iter()
            .any(|m| m.contains("between 13000 ms and 21100 ms"))
    );
    let mut exact = spec();
    exact.brief.duration_ms = 21_000;
    assert!(error_paths(&exact).is_empty());
}

#[test]
fn an_exit_must_come_after_the_beat_and_inside_the_video() {
    let mut s = spec();
    s.beat_map[1].exit_ms = Some(6_000);
    assert!(
        error_paths(&s)
            .iter()
            .any(|m| m.starts_with("beat_map[1].exit_ms"))
    );
    s.beat_map[1].exit_ms = Some(20_001);
    assert!(error_paths(&s).iter().any(|m| m.contains("past the end")));
    s.beat_map[1].exit_ms = Some(9_000);
    assert!(error_paths(&s).is_empty());
}

#[test]
fn names_are_unique_and_a_takeaway_is_required() {
    let mut s = spec();
    s.beat_map[1].id = "hook".into();
    s.beat_map[2].takeaway = " ".into();
    let e = error_paths(&s);
    assert!(
        e.iter().any(|m| m.contains("beat names must be unique")),
        "{e:?}"
    );
    assert!(
        e.iter().any(|m| m.starts_with("beat_map[2].takeaway")),
        "{e:?}"
    );
}

#[test]
fn beat_times_maps_names_to_milliseconds_and_numbers_the_unnamed() {
    let mut s = spec();
    s.beat_map[1].id.clear();
    let t = beat_times(&s);
    assert_eq!(t["hook"], 500);
    assert_eq!(t["beat-2"], 6_000);
    assert_eq!(t["close"], 13_000);
    assert_eq!(t.len(), 3);
}

#[test]
fn the_next_unbuilt_beat_is_the_first_not_built() {
    let mut s = spec();
    assert_eq!(next_unbuilt(&s).unwrap()["beat"], "hook");
    s.beat_map[0].built = true;
    let next = next_unbuilt(&s).unwrap();
    assert_eq!(next["beat"], "reveal");
    assert_eq!(next["at_ms"], 6_000);
    assert_eq!(next["remaining"], 2);
    for b in &mut s.beat_map {
        b.built = true;
    }
    assert!(next_unbuilt(&s).is_none());
}

#[test]
fn scene_status_surfaces_the_next_unbuilt_beat() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec();
    let status = super::status::status(&s, dir.path(), "main");
    assert_eq!(status["next_beat"]["beat"], "hook");
    assert_eq!(status["next_beat"]["focal_node"], "hero");
    let none = super::status::status(&SceneSpec::default(), dir.path(), "main");
    assert!(none["next_beat"].is_null());
}

#[test]
fn a_spec_without_beats_round_trips_and_old_specs_still_load() {
    let old = r#"{"schema":"opendesigner.scene_spec/1","id":"x"}"#;
    let s: SceneSpec = serde_json::from_str(old).unwrap();
    assert!(s.beat_map.is_empty() && s.seed.is_none());
    let back: SceneSpec = serde_json::from_str(&serde_json::to_string(&spec()).unwrap()).unwrap();
    assert_eq!(back.beat_map.len(), 3);
}

#[test]
fn the_sound_plan_gives_every_cue_its_own_seed_name_and_a_kind_make_sound_knows() {
    let mut s = spec();
    for b in &mut s.beat_map {
        b.audio_cue = Some("swoosh".into());
    }
    let plan = sound_plan(&s);
    assert_eq!(plan.len(), 3);
    let names: std::collections::HashSet<_> =
        plan.iter().map(|p| p["make_sound"]["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 3);
    assert!(plan.iter().all(|p| crate::synth::Kind::parse(p["make_sound"]["kind"].as_str().unwrap()).is_some()));
    s.beat_map[0].audio_cue = Some("chime".into());
    s.beat_map[1].audio_cue = Some("chime".into());
    let plan = sound_plan(&s);
    assert_ne!(plan[0]["make_sound"]["pitch_hz"], plan[1]["make_sound"]["pitch_hz"]);
}
