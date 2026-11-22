# Build: write the plan first, then build it in locked passes

A built scene is the wrong order of work. The plan is the work: an agent that decides the
layers, the palette, the details, the features and the motion **before** it draws anything
spends its corrections on the scene instead of on the plan. This playbook is that order:
a SceneSpec per video, then one pass at a time, each pass judged once, each pass locked until
the one before it passed.

Rust does the parts that are numbers (the spec check, the pass lock, the review ledger, the
correction limits). You do the part that needs eyes (one contact sheet, one decision). Never
let the two swap: a global score is not evidence about a small feature, and a picture is not
evidence about a gap.

## Read order

1. `get_guidance` — the selection, the annotations, the memory, the brief.
2. `get_playbook` — the topic your `routing.classification` names (explainer, characters,
   scenery, sketch, 3d, logo, motion). `build` is this page.
3. `scene_spec_new {subject, intent, video, tier}` — a starter. It has the minimum number of
   detail slots for the tier and placeholders that **fail** strict validation on purpose.
4. Fill the starter in and `scene_spec_write {name, spec}`. Every placeholder must go: real
   layer ids, real details with a `maps_to`, real feature targets, real palette tokens, the
   motion plan, the audio plan.
5. `scene_spec_validate {strict: true}` — until this passes with no errors and no quality
   findings, nothing is built. A quality finding is a shallow spec, not a style opinion.
6. `scene_status {name}` **before every action**. It says which pass is open, what evidence is
   missing and the exact next call. Never continue from memory: the ledger in the file is the
   only authority, because your memory of it is exactly what a long build loses.

## The spec, block by block

- `brief`: the subject, the intent, the format and length, the audience, whether it is silent.
- `routing`: the classification with a confidence and at least one evidence reference. Below
  `0.82` the routing does not resolve: ask the user instead of guessing a track.
- `complexity`: the tier (`simple`, `moderate`, `complex`, `ultra`) and the eight scores. The
  tier sets the minima: 3, 6, 10 or 16 inventoried details.
- `palette`: the tokens (`id`, `hex`, `role`), how many are allowed, and the light direction.
- `layer_plan`: one entry per thing you will draw: `id`, `level` (`macro`, `meso`, `micro`),
  `role`, `node_ref`, `depth`, `bleed`, `parent`, `attach`, `side`, `mirror_of`,
  `expects_motion`, `min_px`.
- `detail_inventory`: the details you can point at, each with a `maps_to` that points at a
  layer, a track or a palette token. A detail in prose only is a detail that never reaches the
  render.
- `feature_targets`: the systems that must read correctly: `tier` (`critical`, `important`,
  `detail`), `pass_ids`, `minimum_score`, `must_pass`. At most 5 critical and 3 important per
  pass.
- `motion_plan`: what moves, what loops (`true`, `false` or `null` when nobody measured),
  the stance of a walk, the camera limits.
- `audio_plan`: the tracks, the ducking, the cues, the loudness.
- `budgets`: the node, path, blur, shadow and frame-time limits.
- `build_passes`: the passes you will run, in order.
- `freeze`: the pass after which the shapes are hashed and may not change.
- The review history is written by `record_review`, never by you.
- `calibration`: which thresholds are measured, which are only guessed. A guessed threshold
  warns; it never blocks.

## The passes

`blockout` → `shapes` → `palette` → `motion` → `polish` → `audio`. One at a time. A pass is
locked until the pass before it has a `continue` in the ledger, so a broken silhouette can
never be "fixed" by repainting it.

**The checklist of one pass:**

1. **Build** the pass from the spec. Only the pass: the blockout is not the final shapes.
2. **Render** it: the frames you need to judge.
3. **Run the numeric gate**: `scene_check {video}` for every pass except motion, which uses
   `verify_motion {video}`. Give `scene_check` the plan's `layer_plan` and `details`, so it can
   check that every planned layer became a node and that nothing was left out. Fix every finding
   with severity `fail` first. A hard failure means the build is wrong, and no picture of it can
   be worth looking at.
4. **Only if the gate has no hard failure**: make **one** contact sheet and look at it once.
5. **Score**: the sheet as a whole (`ai_score`), the five layers (`silhouette_proportion`,
   `component_structure`, `form_detail`, `palette_fidelity`, `motion_quality`) and up to 5
   critical features. Say what the gate does **not** measure.
6. **Choose exactly one action** and record it with `record_review`.

## The decision table

| What you see | Action |
|---|---|
| A planned layer is missing, fused, or the plan cannot express the request | `refine-spec` |
| The plan is clear and the build does not match it; a numeric gate fails on the code | `refine-code` |
| The input hides what is needed, routing is under 0.82, the loop plateaued, the limit is reached | `request-input` |
| The fidelity is reached, the user accepted the approximation, or the rest needs hand work | `stop` |
| Every gate passes, every critical feature reaches its minimum, the score is at least 0.7, the evidence files exist | `continue` |

`continue` is the only action that unlocks the next pass. A `refine-code` edits the existing
nodes: rebuilding from scratch throws away what already worked.

## Say what the gate does not measure

A number is evidence about one thing. `scene_check` measures geometry, coverage and the
contract; it does not measure whether the shape reads as the thing in the brief. Write that
line in the `summary` of every review: "the gate says the parts are attached; it does not say
the machine reads as a bicycle". A 2D gate that passes is never the last word, and a picture
that looks right is never evidence that the parts are there.

**Small features:** a feature smaller than 2 % of the frame height does not exist on a contact
sheet. Review it with `video_crop` on the rendered frame, not on the sheet. A feature you can
only judge by zooming in is a feature you have not judged.

## Diagnose the symptom, not the cause

Most wrong attributions are wrong because they named a cause instead of a symptom. Name the
symptom first ("the wing is missing from the silhouette"), then test the cause you suspect:

- **Flat-fill render.** Hide the gradients and the grade and render again. If the shape is
  still wrong, the geometry is wrong; if it is right, the colour is wrong.
- **Restore and re-measure.** `restore_checkpoint` to the state before the change and measure
  again. If the number moves, the change did it.
- **Hit test.** `hit_test {video, x, y}` lists the nodes under a point, topmost first. It is the
  ray test: it says what is in the way, without guessing.
- **Bracket the parameter.** Push it the wrong way. If the symptom does not get worse, it is
  not the cause.

Two failed attributions in a row means stop guessing and run a discriminating test.

## Stop early

The correction limits (3 per pass, 6 in total) are safety limits, not targets. Stop earlier
when the fidelity is reached, when the same defect survives two fixes in a row (that is a
`refine-spec`), when two fixes undo each other (that is a `refine-spec` too), or when the
numbers stop moving (that is a `request-input`). `scene_status` reports the counters and the
reason; it never asks you to keep going to reach a limit.

## The review record

`record_review {name, pass_id, action, fidelity, ai_score, layer_scores, feature_reviews,
summary, evidence}` appends to the ledger. The evidence must be files in the project
(`exports/<name>.png`): the contact sheet and the frames you judged. The tool refuses a
`continue` without them, without the scores and without the feature reviews, and it refuses a
pass that is still locked.
