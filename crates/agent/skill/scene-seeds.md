# Seeds and the beat map: start from a scene, plan it in beats

An empty frame is the slowest start. A seed is a finished first draft: a composed frame, a video
with one clip, ambient motion and a SceneSpec, built in one undoable turn from the same blocks
`add_block` uses. You replace the words, then spend your corrections on the details.

## Pick one

`list_seeds` shows the six, each with its blocks, its beats and its layout notes.

| seed | for | length |
|---|---|---|
| `explainer-opener` | a title over moving scenery, a waving character, a speech bubble | 9 s |
| `product-reveal` | a 3D phone turns in beside a headline, a New badge and a button | 9 s |
| `kinetic-quote` | one sentence, words rising in, then who said it | 8 s |
| `stats-callout` | one big number, a donut that breaks it down, a ring back on the number | 9 s |
| `before-after` | two bars, an arrow drawn between them, the gain | 10 s |
| `logo-sting` | a wordmark springs in over stars, an accent rule, a tagline | 6 s |

Apply a style pack first (`get_playbook packs`): the seed then paints with its tokens and writes
them into the spec's palette. No pack: the block defaults (dark ground, violet accent).

## Apply

`apply_seed {id, brand?, video?}`. `brand` swaps the copy and colors (`name`, `headline`,
`tagline`, `cta`, `accent`, `background`, `foreground`); what you leave out keeps the seed's own
words, which are placeholders. `video` must be empty; omit it and the seed makes a video in the
workspace's chosen format (else 1280x720 at 30 fps). Apply it twice and you get two separate
scenes (`name`, `name-2`). If any step fails the whole turn is rolled back.

The result is the work order:

- `layers`: layer id to node id, for `apply_ops`, `animate_preset` and `get_tree`.
- `layouts`: where each element sits (fractions of the frame) and what to swap.
- `beat_times`: beat name to milliseconds. Put a sound on a beat by name, not by guessing a number.
- `p0_gates`: what is passing and what is open. Lint and the spec check ran; `review_video`, the
  contact sheet and the sound are yours to run.

## The beat map

A beat is one moment the viewer must take something from. Write each as
`time -> takeaway -> focal object -> transformation -> copy / audio -> exit`:

```json
{"id": "reveal", "at_ms": 1000, "takeaway": "The product looks real",
 "focal_node": "device", "transformation": "turns from flat to its pose",
 "copy": null, "audio_cue": "swoosh", "exit_ms": null, "built": true}
```

Rules `scene_spec_validate` enforces: times strictly increase and sit inside the video, every
`focal_node` is a `layer_plan` id (or its node id), `exit_ms` is after `at_ms` and inside the
video, names are unique, and no stretch of 8 s is without a beat. A beat with no takeaway is a
decoration: cut it or say what it teaches.

`scene_status` reports `next_beat`: the first beat not marked `built`. Build that one, mark it
`built: true` with `scene_spec_write`, ask again. A seed builds every beat, so it starts empty.

## After the seed

1. Replace the copy and brand colors; keep to the palette tokens.
2. `validate_document`, then `review_video` (the seeds avoid RV001 and RV002 by construction;
   a swap that removes the scenery brings them back).
3. One contact sheet (`video_contact_sheet`), then `record_review`.
4. Sound: place each `audio_cue` at its beat time (`make_sound`, `get_playbook motion`).
5. Change the scene by editing nodes (`apply_ops`), not by adding a second seed on top.
