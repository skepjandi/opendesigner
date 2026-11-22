# Scene spec and the locked build passes

> **Status: Implemented** (`crates/agent/src/scene_spec/`, playbook topic `build`). The rules
> below are pinned by the unit tests in `crates/agent/src/scene_spec/tests.rs`; the full-suite
> run that lands this change is the proof. The numeric gates the playbook names (`scene_check`,
> `verify_motion`) are built by another change and are not part of this page; where this page
> mentions them, they are assumed to exist with the argument `{video}`.

## What it is

The agent writes a plan before it builds: a **SceneSpec** per video, one JSON file in the
project folder. Rust then does everything that is a number — it checks the spec, it keeps the
review ledger, it says which pass is open, what evidence is missing and which call comes next.
The agent keeps everything that needs eyes — it looks at **one** contact sheet per pass and
chooses **one** action.

The order is the point. A pass is locked until the pass before it has a `continue` in the
ledger, so a broken silhouette can never be "fixed" by repainting it, and the corrections are
spent on the scene instead of on the plan.

## The file

`scene_specs/<name>.json` inside the project folder (the folder the user opened, the same one
that holds `media/` and `exports/`). The name is plain: lowercase letters, digits, `.`, `_`,
`-`, at most 64 characters, no leading dot and no `..`; the default name is `main`. The file is
written atomically (a temporary file in the same folder, then a rename), is capped at 256 KB
and is never written through a symbolic link. It is plain JSON, so it is diffable and can be
reviewed like any other text file.

Unknown fields are kept, not rejected: the schema is forward compatible, and a file written by
a newer agent still loads.

## The schema

`schema` is `opendesigner.scene_spec/1`. The blocks:

| Block | What it decides |
|---|---|
| `brief` | subject, intent, format and length, audience, whether it is silent |
| `routing` | the classification (`kind`, `confidence`, `evidence_refs`), the track it names, the status, the conflicts |
| `complexity` | the tier (`simple`, `moderate`, `complex`, `ultra`) and the eight scores |
| `palette` | the tokens (`id`, `hex`, `role`), `max_tokens`, `min_pair_delta_e`, `min_text_contrast`, the light direction |
| `layer_plan` | one entry per thing to draw: `id`, `level`, `role`, `node_ref`, `depth`, `bleed`, `parent`, `attach`, `side`, `mirror_of`, `expects_motion`, `min_px` |
| `detail_inventory` | the details, each with a `maps_to` that points at a layer, a track or a palette token |
| `feature_targets` | the systems that must read: `tier` (`critical`, `important`, `detail`), `pass_ids`, `minimum_score`, `must_pass` |
| `motion_plan` | what moves, the three-state `loop` (`true`, `false`, `null`), the `stance` of a walk, the camera limits |
| `audio_plan` | the tracks, the ducking, the cues, the loudness |
| `budgets` | node, path, blur, shadow and frame-time limits |
| `build_passes` | the passes to run, in order |
| `limits` | the correction limits: `max_per_pass` (3), `max_total` (6) |
| `freeze` | the pass after which the shapes are hashed |
| `review_history` | the ledger, written by `record_review` and never by the agent |
| `calibration` | which thresholds are `measured`, `derived` or `uncalibrated` |

## The tools

| Tool | What it does |
|---|---|
| `scene_spec_new {name?, subject, intent?, audience?, video?, tier?, silent?, max_per_pass?, max_total?, overwrite?}` | writes a starter: the tier's minimum of detail slots and placeholders that **fail** strict validation until they are replaced |
| `scene_spec_write {name?, spec}` | replaces the spec (a refine-spec, or filling the starter in); the review history is kept as recorded |
| `scene_spec_validate {name?, strict?, include_spec?}` | the errors and the quality findings |
| `scene_status {name?}` | the whole state: current pass, passes completed, the loop counters, `active` or `stopped` with the reason, the missing evidence and the exact next call |
| `record_review {name?, pass_id, action, fidelity, ai_score?, layer_scores?, feature_reviews?, summary, evidence?, defect_tags?, reverted?, hard_gate_failures?}` | appends one review to the ledger after checking every refusal rule |

## The rules

**Validation** (`scene_spec_validate`) separates two kinds of finding. *Errors* always fail: a
wrong schema string, a `routing.status` that contradicts the classification, layer ids used
twice, a `parent`, `mirror_of`, `attach` target, `maps_to.ref` or `palette_refs` entry that
does not resolve, a `side` without a mirror partner, a depth outside 0 to 1.5, a hex that is
not a hex, a motion target that is neither a layer nor `@camera`, a `zoom_max` above 1.035,
`build_passes` that is not a subset of the six in order, an unknown review action.
*Quality* findings fail only under `strict` (which a build needs): routing below 0.82, fewer
details than the tier floor (3, 6, 10, 16), a detail that maps to nothing, a pass with more
than 5 critical or 3 important features, a critical feature without a numeric minimum, an
ambient motion without `loop: true`, a gait without a stance, no audio track in a video that
is not silent, and a hierarchy shallower than the tier asks for.

**Routing** is fail-closed at 0.82. A record that is malformed in any part (no kind, a
confidence outside 0 to 1, a confidence that is not a number, no evidence reference) becomes the
synthetic `unknown` with confidence 0.0. `hybrid` and `unknown` always ask for input. An
explicit track that contradicts a reliable classification is a conflict.

**The feature gate** uses the agent's scores. A critical target (or one with `must_pass`)
needs a review entry that is visible and at least its minimum: 0.8, 0.65 and 0.5 by tier, or
its own `minimum_score`. A gating feature with no entry means it was never built, so the
action is `refine-spec`; one that was built and scored under its minimum means the action is
`refine-code`. The reviewed important features must average at least 0.65.

**The ledger** completes passes strictly in prefix order. A `continue` for a pass whose
predecessor has no `continue` is refused (a ledger disagreement) and is reported by
`scene_status` when the file was edited by hand. A `continue` for a visual pass also needs the
contact sheet and the frames as files in `exports/`, an `ai_score` of at least 0.7, the five
layer scores (`silhouette_proportion`, `component_structure`, `form_detail`,
`palette_fidelity`, `motion_quality`) and a clean feature gate.

**The correction loop** counts the refine actions in the file, never in the agent's memory.
Three on one pass stop the run with `max-correction-loops-reached:<pass>:<n>/<max>`; six in
total stop it with `max-total-correction-loops-reached:<n>/<max>`. The limits are refused when
`max_per_pass < 1`, `max_total < 1` or `max_per_pass > max_total`. The limits are safety
limits, not targets: `scene_status` also advises the correction policy (the same defect twice
running, two reverts in a row, a plateau under the target, the iteration ceiling).

**Calibration** is the honesty rule: a threshold that was only guessed carries
`calibration: uncalibrated` and can warn but never block. A corpus needs both good and bad
pairs before a threshold can be suggested; then it is the midpoint between the worst good and
the best bad.

## What is not measured here

This page covers the plan, the ledger and the limits. It does not measure the picture: the
geometry gates, the motion measurements and the contact sheet are described in
[Video Editing](./video-editing.md) and in the `review` and `build` playbooks. The claims about
how a 2D picture corresponds to a reference image are still `[INFERENCE]` until a labelled
corpus exists.
