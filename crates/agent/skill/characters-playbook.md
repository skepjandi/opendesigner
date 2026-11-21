# Characters, bubbles and props: flat-vector explainer animation

Use these blocks when the brief is an explainer in the flat-vector, long-shadow, constantly
moving style (Kurzgesagt, Ted-Ed, a product explainer): simple expressive characters, props that
carry the idea, speech that appears when someone speaks. Never answer such a brief with a
background picture and animated text; build the scene from shapes and let things move.

Blocks (all in the `characters` category; `list_blocks` with `category: "characters"`):
`character`, `emote`, `speech_bubble`, `thought_bubble`, `prop_rocket`, `prop_lightbulb`,
`prop_atom`, `prop_planet`, `prop_tree`, `prop_house`, `prop_laptop_2d` (a flat drawing, not the
3D device), `prop_gear`, `prop_cell`, `prop_dna`. Backdrops, particles and camera moves are other
blocks (scenery); this page is about what stands in front of them.

## The loop

1. Set tokens first (`set_token`): a deep background, two or three saturated body colors, one warm
   accent (`accent`, `brand.accent`). The blocks take their colors from the tokens, so the whole
   scene stays one palette. Pass `body_color`, `accent_color` or `skin_color` as `#rrggbb` or
   `{token}` for a second character that must differ.
2. Build the stage back to front (backdrop, props, characters, bubbles): a block is added on top
   of what exists. Give every block its `box` (the frame's coordinates): a character stands on the
   bottom edge of its box, a prop is drawn in the box's largest centered square.
3. `add_block` with a `video` so the motion is keyed. Without one you get a still layout, which
   is what a store image wants.
4. Look: `render_video_frame` at three moments (an idle moment, the action, after it) and
   `render_preview`; fix proportions, overlaps and contrast, then continue.

## character

Styles: `blob` (big round head, the friendliest), `bean` (tall, a person), `robot` (boxy, glowing
eyes), `creature` (blob with antennae). Expressions: see Acting below (18 of them). Flags: `hat`,
`glasses`, `backpack`. `facing` turns the face toward `left` or `right`; `auto` follows a walk.
`look_at` (a node id or point) sends the pupils there and turns the face a little toward it.

The result names the parts. The root is `block:character#<n>`; its parts, by name, are `shadow`,
`rig` and, inside the rig, `body`, `head`, `eyes`, `eye_l`, `eye_r`, `pupil_l`, `pupil_r`,
`brows`, `mouth`, `mouth_open`, `arm_l`, `arm_r` (the arms are named by the side of the screen
they are on), `leg_l`, `leg_r`, `hat`, `glasses`, `backpack`, `antenna_l`, `antenna_r` (creature) or
`antenna` (robot). Each joint pivots where it should: arms at the shoulder, legs at the hip, the
head at the neck, the body at its base, each eye at its center. To pose a part use `apply_ops`
(`set rotation`); to move it in time use `animate` with `rotation` (clockwise degrees: an arm that
hangs down swings its hand toward the screen's left for a positive value) or `scale_y`.

Life is always on once there is a video: the body breathes, the head bobs and sways, each eye
blinks every 2 to 4 seconds, the arms sway, the pupils drift. Every track loops (`repeat: loop` or
`pingpong`) with its own length, and `seed` shifts all of them: give every character in a scene a
different `seed` so none moves in step.

`action` (plus `action_start_ms`, `action_end_ms`, from the start of the block):

- `idle`: nothing more.
- `wave`: the right arm rises with an elastic overshoot, waves and settles. `loops` repeats it.
- `talk`: the mouth opens and closes. Give `talk: [[start_ms, end_ms], ...]` (also with any other
  action) to make the mouth move only while the narrator speaks; take the ranges from the voice
  track's timing, in milliseconds from the start of the block. Outside the ranges it is shut.
- `walk`: legs and arms swing, the body bobs and the character travels from `from_x` to `to_x`
  (the character's center, frame coordinates) between `action_start_ms` and `action_end_ms`
  (800 ms to 40 s). Leaving the frame is allowed: start at `-200` or end past the width.
- `jump`: squash, stretch, a hop and an elastic landing, the shadow shrinking under it; `loops` hops.
- `point`: the arm on the target's side turns to aim at `target`; with `action_end_ms` it lowers.
- `celebrate`: a jump with both arms thrown up; `loops` repeats it.

An action replaces the idle motion of the parts it uses, so a walking character's arms swing
instead of sway. One action per block: for a sequence (wave, then walk away) add the character
once, and re-time the parts with `animate`, or add a second block with the same box and hide it.

## Acting: make the character feel played

A face that never changes is a mascot, not an actor. Eighteen expressions, each its own drawing
(eyes, lids, brows, mouth, cheeks), so the silhouette of the face changes and not just a brow:
`neutral`, `happy`, `excited` (sparkle eyes, wide grin), `surprised`, `shocked` (huge eyes, tiny
pupils, gaping mouth), `sad`, `crying` (tears run), `angry`, `annoyed` (side-eye, flat mouth),
`thinking`, `confused` (one brow up, wavy mouth), `sleepy` (heavy lids, yawn), `bored`, `smug`
(half lids, one brow, smirk), `worried`, `scared` (tiny pupils, zigzag mouth, a shiver), `love`
(heart eyes that beat) and `determined` (set teeth, level glare).

`expression_changes: [{"at_ms": 2400, "expression": "surprised"}, ...]` lets one character act
through several faces on one video (the first change at 200 ms or later, 500 ms apart or more, up
to 40 changes through 6 different faces). Each change anticipates: the old face squints, the face
swaps with a short cross-fade, the new one overshoots and settles, all as ordinary keyframes on
the `face` groups (the first face is `face`, the others `<expression>_face`, and their parts carry
the same prefix: `sad_eye_l`, `sad_mouth_open`). Blinking, talking, looking and tears work on every
face. Check the timing with `render_video_frame` just before, at and 300 ms after a change.

`looks: [{"at_ms": 1000, "x": 400, "y": 300}, ...]` sends the eyes to a point at each time: the
pupils jump at once and the face turns a beat later (400 ms apart or more). `look_at` is the
still version: pupils and face turn toward a node or point for the whole video.

`reaction` (with `reaction_at_ms`): `take` (a startle, a look away, then a squashed and stretched
snap back: the double-take), `shrug` (shoulders up, palms out, head sinking), `nod`, `shake_head`
and `laugh` (the body hops for `reaction_ms`, the mouth flaps). `loops` repeats the others. A
reaction moves parts an action may own: `take` and `laugh` not with `walk`, `jump` or
`celebrate`, `shrug` also not with `wave` or `point`.

Acting rules (the difference between a puppet and a character):

- Never twin. Two characters on screen get different `seed`s, different timing for the same beat
  (one reacts 150 to 300 ms after the other) and different faces and sizes. Never mirror a pose
  or a motion; offset arm and eye timing instead.
- Anticipate. Every reaction starts with a small move the other way (the squint, the squash, the
  glance away) before the big one. The changes and reactions already do; do not cut the lead-in
  by putting a change at 200 ms when the story beat is at 200 ms: start it earlier.
- Eyes lead. Look first (`looks`, or the pupils of a `look_at`), then react: put the look 300 to
  500 ms before the expression change or the reaction, in the direction of what causes it.
- Show the thought. Whatever the character feels or knows appears as a mark (an `emote`: a
  lightbulb, a question, a sweat drop, hearts), not only as words in a bubble.
- Hold the beat. A reaction needs to be seen: keep the new face (and its emote) on screen for at
  least 800 ms, one second if the line is long, before the next change.
- Tie every change to a story beat (a line of the voice-over, a prop arriving, a result): if you
  cannot say what happened, the face should not change. Three to five faces in thirty seconds
  is plenty; a character that changes every second is jittery, not alive.

## speech_bubble and thought_bubble

A bubble hugs its text (up to four lines, fitted to the box: too long a line is refused, so shorten
it or give a larger `box`). `target` is a node id (the tail aims at the upper middle of the node, so
pass the character's `head` or `rig`) or `{x, y}`. Place the `box` above and beside the speaker;
the bubble leans toward its target inside the box. The bubble pops in from the tip of its tail
with overshoot (`pop_ms`; the thought bubble's circles follow one by one) and floats gently
(`float`). Time it with `start_ms` to the moment the line is spoken and use `hold_ms` to fade it
out after; give the speaker a `talk` range for the same moment. Colors default to the foreground
role for the bubble and whichever of foreground or background reads better for the text.

## emote

A painted mark instead of words, placed beside a character. `mark`: `exclamation`, `question`,
`sweat` (drops that slide down), `heart` (rising hearts, or one beating), `lightbulb` (glow and
rays pulse), `zzz` (letters drifting up), `anger` (a pulsing vein cross), `steam` (puffs), `sparkle`
(twinkling stars), `music` (notes bobbing), `rain_cloud` (cloud and falling rain), `dizzy` (turning
spirals), `stars` (stars orbiting). `target` is the character's `head` part id (or `{x, y}`): the
mark sits where that mark usually sits (`side: auto`) or at `above`, `upper_left`, `upper_right`,
`left`, `right`, sized from the target's width (`scale` 0.4 to 3), kept inside the frame's safe
area. Without a `target` it fills its `box`. `count` sets how many drops, hearts, notes and so on.
Time it with `start_ms` (it pops in with overshoot over `pop_ms`) and `hold_ms` (it fades out after
that; at least `pop_ms`). Its idle motion loops for as long as it is there. Colors come from a
fitting hue (blue drops, red hearts, yellow bulbs) eased toward the `accent` token; pass `color` to
decide. Put it where the viewer is already looking, at the same time as the expression it
illustrates, and never two marks on one head at once.

## Props

All take `color`, `accent_color`, `box` and `loops` (0 repeats for as long as the video plays; a
number writes that many cycles out and then holds still) and `seed`.

- `prop_rocket`: flame flicker, exhaust puffs, hover. Parts `rocket`, `flame`, `exhaust`.
- `prop_lightbulb`: glow and rays pulse; filament flickers. Parts `glow`, `rays`, `bulb`, `base`.
- `prop_atom`: three orbits that turn at different speeds and electrons that really travel the
  ellipses. Parts `nucleus`, `orbit_0..2`, `electron_0..2`.
- `prop_planet`: sphere, craters, drifting cloud streaks, an optional `ring` (`craters` 0 to 8).
- `prop_tree`: `kind` `round` or `pine`; crown and leaves sway. Parts `crown`, `leaves_0..2`.
- `prop_house`: windows glow, chimney smoke rises. Parts `smoke`, `window_0`, `window_glow_0`.
- `prop_laptop_2d`: lines of code grow and shrink, the cursor blinks.
- `prop_gear`: `teeth` (5 to 24), `period_ms`, `clockwise`; spins for ever. Part `gear`.
- `prop_cell`: breathing membrane, nucleus, `organelles` (2 to 8) that wobble and drift.
- `prop_dna`: `rungs` (8 to 28) turning about the axis every `period_ms`; give it a tall box.

Props sit in the middle of the frame by default. Always pass a `box`; leave air around it, and
overlap props and characters only on purpose (a character in front of a house, a rocket crossing
the planet). Put a prop's ground shadow where its feet are, not in the air.

## Scene recipes

- Explainer opening: backdrop block, a `prop_planet` or `prop_house` in the middle ground, a
  `character` (`happy`, `wave`) in front, a `speech_bubble` at the voice-over's first line.
- Idea beat: character `thinking` with a `thought_bubble` whose text is a few words and a `looks`
  entry toward the thing it noticed; 400 ms later, as the idea lands, an `expression_changes` entry
  to `excited` (or `surprised`), a `reaction: take` and an `emote` lightbulb on its head, all at
  the same time. Keep that face and the mark for a full second.
- Mood beat: `emote` sweat drops (`worried`), zzz (`sleepy`), a rain cloud (`sad`), anger (`angry`),
  hearts (`love`), a question (`confused`): the mark carries the feeling, the face agrees.
- Journey: a `character` with `action: walk` across the frame while a scenery block scrolls, or a
  `prop_rocket` moved with `animate` (`x`, `y`, `rotation`) along an arc.
- Science: `prop_atom`, `prop_cell`, `prop_dna` large in the middle with a `character` small at
  the side pointing at them (`action: point`, `target` the prop's root id).

## Checks

- The idle motion is subtle on purpose: the arm sway is a few degrees, the breath a few percent.
  Anything bigger is the action. If a still frame looks dead, check the video was passed.
- Same palette for everything; no pure black or white fills, and one light direction (the blocks
  light from the upper left, so shadow-side edges are on the lower right).
- Characters must not all stand in a line at the same size: vary box heights (the nearer, the
  larger and lower), vary `seed`, expression and facing.
- Run `review_frame` on the still and `render_video_frame` mid-action for each beat.
