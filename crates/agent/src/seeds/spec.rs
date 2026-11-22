//! The SceneSpec a seed writes: the layers it built, the palette it used, the beats it is
//! planned around and the motion it keyed, so `scene_status` and the gates start from a real plan.

use std::collections::BTreeMap;

use serde_json::json;

use super::recipes::{BeatDef, Motion, Recipe, Seed, Step};
use crate::scene_spec::routing::TRACK_BY_KIND;
use crate::scene_spec::schema::{
    AudioCue, AudioTrack, Beat, Detail, FeatureTarget, Format, Layer, MapsTo, Motion as PlanMotion,
    SceneSpec, Token,
};

pub struct Facts<'a> {
    pub seed: &'a Seed,
    pub recipe: &'a Recipe,
    pub video: &'a str,
    pub format: Format,
    pub duration_ms: u64,
    pub roots: &'a BTreeMap<String, String>,
    pub tokens: Vec<(String, String)>,
    pub style_pack: Option<&'a str>,
}

fn motion_kind(m: Motion) -> &'static str {
    match m {
        Motion::Entrance => "entrance",
        Motion::Ambient => "ambient",
    }
}

fn beat(def: &BeatDef) -> Beat {
    Beat {
        id: def.id.into(),
        at_ms: def.at,
        takeaway: def.takeaway.into(),
        focal_node: Some(def.focal.into()),
        transformation: def.transformation.into(),
        copy: def.copy.clone(),
        audio_cue: def.audio.map(String::from),
        exit_ms: def.exit,
        built: true,
    }
}

pub fn build(f: &Facts, name: &str) -> SceneSpec {
    let mut spec = SceneSpec {
        id: name.into(),
        video_id: Some(f.video.into()),
        seed: Some(f.seed.id.into()),
        ..SceneSpec::default()
    };
    spec.brief.subject = f.seed.about.into();
    spec.brief.intent = f.seed.name.to_lowercase();
    spec.brief.format = f.format.clone();
    spec.brief.duration_ms = f.duration_ms;
    spec.brief.style = match f.style_pack {
        Some(id) => json!({"style_pack": id}),
        None => json!({}),
    };
    spec.complexity.tier = "simple".into();
    spec.routing.classification = json!({
        "kind": f.recipe.kind, "confidence": 0.9,
        "evidence_refs": [format!("seed:{}", f.seed.id)], "provider": "seed", "version": "1",
    });
    spec.routing.track = TRACK_BY_KIND
        .iter()
        .find(|(k, _)| *k == f.recipe.kind)
        .map(|(_, t)| (*t).to_string());
    spec.routing.status = "resolved".into();
    spec.palette.tokens = f
        .tokens
        .iter()
        .map(|(id, hex)| Token {
            id: id.clone(),
            hex: hex.clone(),
            role: id.clone(),
            source: Some("declared".into()),
            ..Token::default()
        })
        .collect();

    for step in &f.recipe.steps {
        match step {
            Step::Block {
                layer,
                level,
                role,
                depth,
                motion,
                ..
            } => spec.layer_plan.push(Layer {
                id: (*layer).into(),
                level: (*level).into(),
                role: (*role).into(),
                node_ref: f.roots.get(*layer).cloned(),
                depth: *depth,
                bleed: *role == "backdrop",
                expects_motion: Some(motion_kind(*motion).into()),
                ..Layer::default()
            }),
            Step::Device { layer, .. } => spec.layer_plan.push(Layer {
                id: (*layer).into(),
                level: "macro".into(),
                role: "hero".into(),
                node_ref: f.roots.get(*layer).cloned(),
                expects_motion: Some("entrance".into()),
                ..Layer::default()
            }),
            Step::Move { .. } => {}
        }
    }

    for step in &f.recipe.steps {
        match step {
            Step::Block {
                layer, at, motion, ..
            } => {
                spec.motion_plan.push(PlanMotion {
                    id: format!("m-{layer}"),
                    target: (*layer).into(),
                    kind: motion_kind(*motion).into(),
                    start_ms: Some(*at),
                    looping: (*motion == Motion::Ambient).then_some(true),
                    ..PlanMotion::default()
                });
            }
            Step::Device { .. } => {}
            Step::Move {
                layer,
                preset,
                at,
                duration,
                motion,
                ..
            } => spec.motion_plan.push(PlanMotion {
                id: format!("m-{}-{preset}", layer.unwrap_or("camera")),
                target: layer.map_or("@camera".into(), String::from),
                kind: motion_kind(*motion).into(),
                preset: Some((*preset).into()),
                start_ms: Some(*at),
                end_ms: duration.map(|d| at + d),
                looping: (*motion == Motion::Ambient).then_some(true),
                ..PlanMotion::default()
            }),
        }
    }

    spec.detail_inventory.details = spec
        .layer_plan
        .iter()
        .map(|l| Detail {
            id: format!("d-{}", l.id),
            kind: l.role.clone(),
            maps_to: Some(MapsTo {
                kind: "node".into(),
                reference: l.id.clone(),
            }),
            confidence: 0.9,
            ..Detail::default()
        })
        .collect();

    let hero = spec
        .layer_plan
        .iter()
        .find(|l| l.role == "hero" || l.role == "headline" || l.role == "stat" || l.role == "bar")
        .map(|l| l.id.clone());
    spec.feature_targets = vec![
        FeatureTarget {
            id: "focal-reads".into(),
            tier: "critical".into(),
            pass_ids: vec!["blockout".into(), "shapes".into()],
            minimum_score: Some(crate::scene_spec::features::CRITICAL_MIN),
            must_pass: true,
            node_refs: hero.into_iter().collect(),
            ..FeatureTarget::default()
        },
        FeatureTarget {
            id: "palette-fidelity".into(),
            tier: "important".into(),
            pass_ids: vec!["palette".into()],
            ..FeatureTarget::default()
        },
    ];

    spec.beat_map = f.recipe.beats.iter().map(beat).collect();
    spec.audio_plan.tracks = vec![AudioTrack {
        id: "music".into(),
        kind: "music".into(),
        from_ms: 0,
        to_ms: f.duration_ms,
        ..AudioTrack::default()
    }];
    spec.audio_plan.cues = f
        .recipe
        .beats
        .iter()
        .filter_map(|b| {
            b.audio.map(|sfx| AudioCue {
                at_ms: b.at,
                sfx: sfx.into(),
                sync_to: None,
                tolerance_ms: Some(80),
            })
        })
        .collect();
    spec.freeze.after_pass = Some("shapes".into());
    spec
}
