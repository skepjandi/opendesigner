# Motion playbook: making video and animation that does not look like a slideshow

Distilled from public motion-graphics and animation skills (Remotion, explainer-video and
animation skills on GitHub), rewritten for what OpenDesigner can do. Read `video-direction` for the
whole video process; this page is about movement.

## What the engine gives you

- **Clips** of a design frame or of footage, in order, each with a **transition** into it (cut, fade,
  slide_left/right/up/down, dip, zoom_in) and a slow **motion** (zoom from `scale_from` to
  `scale_to`, 1 to 3) with an **ease**.
- **Overlays**: any design frame laid over the whole video for a time, with an **enter** and **exit**
  (fade, slide_up/down/left/right, pop, zoom, none), a duration for each (`fade_ms`) and an **ease**.
  A frame without a fill is transparent, so an overlay can be one headline, one badge, one arrow.
- **Eases**: `linear`, `ease_in`, `ease_out`, `ease_in_out`, `back` (overshoots, like a pop).
- No per-frame keyframes, no sound generation: footage keeps its sound, nothing else is mixed in.
  Say so when a user expects music; ask them for a track rather than promising one.

## Timing that reads as intentional

| thing | value |
|---|---|
| a title or caption on screen | at least 1.25 s, about 3 words a second, never under 1 s |
| a shot (clip) | 1 to 3 s for energetic pieces, 3 to 6 s for explaining; the opening shot 1 to 2 s |
| an entrance | 250 to 400 ms; an exit 150 to 250 ms (leave faster than you arrive) |
| a transition between clips | 300 to 600 ms for fades and slides; a dip 500 to 800 ms; cuts need no time |
| stagger between items that arrive together | 80 to 140 ms each, never all at once |
| a hold after something lands | 400 to 800 ms before the next thing moves |
| a slow push (zoom) | 5 to 10% over the whole clip (1.0 to 1.08), eased `ease_in_out` |

- **Something changes every 2 to 4 seconds** (a cut, a new line, a move). A still stretch longer
  than that needs life: a slow zoom, a small overlay arriving.
- **Snap, then hold.** A change that arrives quickly (250 ms, `ease_out`) and then holds reads as
  confident. A change that drifts for a full second reads as a slideshow.
- **Ease out to arrive, ease in to leave.** Never `linear` for things with weight. `back` only for
  small playful elements (a badge, a pop) with a tiny overshoot, never for big type or backgrounds.
- **One thing moves at a time** unless it is a group; a group staggers, in reading order.
- **Match the transition to the meaning.** Slide for "next", fade for "and then", dip for a chapter
  change, zoom_in for "closer look", cut for energy and for beats. Use one or two kinds in a
  piece, not all eight. Prefer cuts on the beat of the story over fancy transitions.
- **The camera is the clip's zoom.** Use a slow push on every static frame; vary the direction
  (some push in, some pull out from 1.08 to 1.0) so it feels directed, not automatic.
- **Frame 0 is not empty.** Start with something on screen; open on the strongest picture.
- **Open still and quiet, close with a held end card** of 1.5 to 3 s that carries the one thing to
  remember or do.

## Recipes (all with overlays on a transparent frame)

- **Lower third:** a frame with a name and a role in a bar at 70% height; `slide_left`, 350 ms,
  `ease_out`, in at 0.5 s, out at 3.5 s with `fade`.
- **Kinetic headline:** one frame per line (or per key word), each an overlay, `slide_up` 300 ms,
  starts staggered 120 ms apart; hold 1.2 s; all exit together with `fade` 200 ms.
- **Call-out:** a ring or arrow shape and a short label as one overlay, `pop` with `back` ease, 300
  ms, placed on the UI element it explains; the clip underneath holds still.
- **Before / after:** two clips with a `slide_left` transition and a label overlay on each.
- **Feature tour:** one clip per feature (a frame or footage), a title overlay for each (`slide_up`),
  the same transition between all of them, the same position for every title.
- **Progress / counter:** several overlays with different numbers on consecutive short starts
  (each 100 to 200 ms), no transition.
- **Screen recording polish:** footage clip with a 1.00 to 1.06 push; overlays that highlight the
  area being used (a rounded rectangle outline, `fade`), titles that name the step, not the click.

## Checks before you export

- `render_video_frame` at: the first frame, the middle of every transition, the moment each
  overlay is fully in, the last frame. Look for text clipped by the frame, overlap between an
  overlay and the footage's own text, low contrast over a moving picture (add a dark band under
  captions), and overlays outside the platform safe zone (see `formats`).
- Read the sequence in `list_videos` (`story`): does each line follow from the one before it?
- Total length against the preset; exports are checked, but a good length is the shortest that tells
  the story.
