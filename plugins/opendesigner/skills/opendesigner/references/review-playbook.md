# Reviewing your own video

A first render is a draft. Before you call an animated video done, look at it the way a harsh editor
would, fix what you find, and look again. This is the loop, the checklist and the tools.

## The loop

1. **Sheet.** `video_contact_sheet` across the whole video (8 to 16 moments). Read it as a story: does
   each picture say one thing, does the order make sense, does anything repeat?
2. **Strips of every key motion.** For each entrance, hit, transition and settle, `video_strip` the
   half second around it (`from_ms`, `to_ms`; every frame, or `every` 2 or 3 for a long move). You are
   looking for anticipation before the move, overshoot and settle after it, a pop on the first frame, and
   frames where two things collide.
3. **Crops of every face, hand and contact.** `frame_grid` to read positions, then `video_crop` at 1:1
   (or `node` to follow a node through the camera; `zoom` 2 to 4 for tiny parts). Check eyes and mouths,
   hands on objects, feet on the ground, a cup on a table, the edge where two shapes meet, the smallest text.
4. **Numbers.** `review_video` (pass `bpm` when there is music). Fix every error and every finding you
   cannot defend. Go through the `overlap` list: `video_crop` each region and decide which layer belongs on
   top; most mistakes in a long animation are layering mistakes.
5. **Compare.** After an edit, `video_diff` the moment before and after it (two times, or two videos of
   the project) to prove you changed what you meant to and nothing else.
6. **Rounds.** Do at most three automatic rounds of look, list flaws, fix. If it still is not right,
   stop and ask the user what they want, with the three worst flaws and what you tried.

## Be harsh

- If you hesitate between two readings of a picture and cannot tell which is right, it does not read. Fix
  it; do not argue yourself into "probably fine".
- Write every flaw as three things: **what you see**, **at which second**, **why it is wrong**. "At 4.2 s the
  hand is behind the cup although it is holding it: the cup layer is above the hand." A flaw you cannot
  place at a second is not yet a flaw you can fix.
- Do not praise. List flaws, rank them by how much a viewer would notice, fix the top ones first.
- A fix that makes something else worse is not a fix: look at the neighbours (`video_diff`).

## Checklist

- **Legibility.** Every text readable at 360 px wide; on screen 1 s plus 0.4 s per short word (RV015);
  contrast against what is behind it; in a vertical video nothing important in the top 150 px or the bottom
  250 px at 1080x1920 (RV016).
- **Clarity.** One idea per scene, one thing the eye goes to first. Nothing the viewer has to puzzle out.
- **Density.** Something moves all the time, at least three layers in any two seconds (RV012, RV017), but
  never so much that nothing can be followed.
- **Motion.** Things anticipate, overshoot and settle; eases fit the object (heavy things ease in, light things
  float); no sliding without cause; entrances staggered, not all at once; no loop that visibly jumps.
- **Photosensitivity.** No more than three large brightness changes in any second (RV013 is an error and blocks
  export readiness). Fade instead of flashing; no full-frame white flashes in quick succession.
- **Rhythm.** Scene lengths vary on purpose; cuts and hits land on the beat of the music (RV014) or on the
  words of the voice; silence is a choice.
- **Origin and physics.** Everything that emanates has a source: sparks leave the hit, steam rises from the cup,
  light comes from the lamp, a shadow falls away from it. Falling things accelerate; things thrown arc; what is
  heavy lands and settles.
- **Continuity.** The same character, palette, light direction and outline in every scene; an object that leaves
  right comes in left; what was on the table stays on the table.
- **Composition.** The hero is bigger and clearer than the scenery; eye lines and motion lead into the frame,
  not out of it; empty areas are on purpose; layers overlap the way depth says (the `overlap` list).
- **Ending as poster.** The last picture holds for 0.8 s or more and could be a poster: one clear idea, nothing
  cut off mid-move (RV019). The first frame is as strong as a thumbnail.
- **Detail at 1:1.** Crops show clean edges, no gaps between shapes that should touch, no stray points, no text
  clipped by its box, eyes pointing where the character looks.

## The tools

| to see | call |
|---|---|
| the whole video in one picture | `video_contact_sheet` |
| a move frame by frame, at most 36 pictures | `video_strip` `{video, from_ms, to_ms, every?}` |
| where things are, in video pixels | `frame_grid` `{video, time_ms, spacing?}` |
| a face, hand or contact at 1:1 | `video_crop` `{video, time_ms, x, y, width, height}` or `{..., node, padding?}`, `zoom?` |
| what an edit changed | `video_diff` `{video, time_ms, other_time_ms? or other_video?}` |
| numbers, flash safety, layer overlaps | `review_video` `{video?, bpm?}` |

All are read-only and return one picture with a line of text. In a terminal, the same views are written to
`exports/` by `opendesigner inspect` (`strip`, `crop`, `grid`, `diff`).

## Before you say it is done

Name the three worst flaws you fixed and the one you could not. Say which crops you checked. If a round of
review found nothing, you did not look hard enough: take the strip of the longest scene and the crop of the
smallest detail.
