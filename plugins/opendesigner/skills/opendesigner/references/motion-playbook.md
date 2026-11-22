# Motion playbook: making video and animation that does not look like a slideshow

Distilled from public motion-graphics and animation skills (Remotion, explainer-video and
animation skills on GitHub), rewritten for what OpenDesigner can do. Read `video-direction` for the
whole video process; this page is about movement.

## What the engine gives you

- **Clips** of a design frame or of footage, in order, each with a **transition** into it (`cut`,
  `fade`, `slide_left/right/up/down` which push the old clip out, `dip` through black, `flash`
  through white, `zoom_in`, `wipe_left/right/up/down` which uncover the new clip from an edge,
  `iris` which opens a circle from the middle, `blur_dissolve`, `glitch_slice`) and a slow
  **motion** (zoom from `scale_from` to `scale_to`, 1 to 3) with an **ease**.
- **Speed** of footage: `set_clip` `speed` (0.25 to 4) plays the file faster or slower, and
  `speed_ramp` (keys `{at_ms, rate, ease?}` in clip time) eases the rate for a slow-motion
  moment or a speed-up. The clip's `duration_ms` is its length on the timeline; it plays
  `duration_ms * speed` of the file, and its sound follows (pitch kept).
- **Grade**: `set_clip` `grade` on a clip of footage or a frame, and `set` `grade` on an image
  node, take numbers `exposure` (-3 to 3 stops), `contrast`, `saturation`, `temperature` (cool to
  warm), `tint` (green to magenta) all -1 to 1, `vignette` 0 to 1 and `blur` 0 to 50, or a
  `look` (`warm`, `cool`, `mono`, `faded`, `punchy`) that fills them in (numbers you add win). Keep
  one look across a piece. The numbers of an image node's grade can be keyframed with `animate`
  (`grade_exposure`, `grade_contrast`, `grade_saturation`, `grade_temperature`, `grade_tint`,
  `grade_vignette`, `grade_blur`), for example a pull-focus blur from 20 to 0.
- **Time snaps to frames.** Every start, duration, in point, transition and key you give is rounded
  to the video's frame grid (at 24 fps, 1010 ms becomes 1000 and 1030 becomes 1042); the result of
  the operation and `list_videos` show the value that was kept.
- **Overlays**: any design frame laid over the whole video for a time, with an **enter** and **exit**
  (fade, slide_up/down/left/right, pop, zoom, none), a duration for each (`fade_ms`) and an **ease**.
  A frame without a fill is transparent, so an overlay can be one headline, one badge, one arrow.
- **Eases**: `linear`, `ease_in`, `ease_out`, `ease_in_out` (also `ease_in_out_cubic`), `back` (overshoots,
  like a pop; also `ease_out_back`), `spring` (overshoots and settles, for an arrival with weight),
  `elastic` (rings past the end a few times) and `bounce` (hits the end and bounces off it).
- **Keyframes**: any number of a node (`x`, `y`, `opacity`, `width`, `height`, `corner_radius`,
  `rotation`, `scale_x`, `scale_y`, `scale`, `blur`, `shadow_dx`, `shadow_dy`, `shadow_blur`, and
  on a 3D device its turn, tilt, roll, zoom, lens, light, shadow, glare and screen scroll) can be
  keyframed over the video's time with `animate` in `apply_ops`, or with a named move through
  `animate_preset`. A video can also blur what moves (`motion_blur`, 2 to 16 pictures). See the
  `3d` playbook for devices in 3D.
- **Repeating tracks**: `animate` takes `repeat`: `once` (hold the last value, the default), `loop`
  (start again from the first key for as long as the video plays; end on the first key's value so
  the cycle has no jump) or `pingpong` (forward, then back). A few keys make an idle that lasts the
  whole video: a float, a breath, a blink, a spin (`rotation` 0 to 360 with `ease: linear` and `loop`).
  Send `repeat` again whenever you replace the keys of a looping track.
- **Camera**: `camera_x`, `camera_y` (the scene pans the other way), `camera_zoom` and
  `camera_rotation` are keyframed with `animate` using the video's own id as `node`. They move
  every node of a clip's frame by its `depth` (1 with the scene, 0 stays on the screen, 0.5 for
  parallax); overlays are not moved. A single key holds the camera still. Keep the scene larger
  than the frame where the camera moves, or its edge shows.
- Sound: footage keeps its own, and a video can carry up to eight sound tracks
  (see Sound below). `make_sound` synthesizes effects and simple loops, not music: for a music bed
  use `search_sounds` or ask the user for a track rather than promising one.

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
  piece, not all of them. Prefer cuts on the beat of the story over fancy transitions.
- **The camera is the clip's zoom.** Use a slow push on every static frame; vary the direction
  (some push in, some pull out from 1.08 to 1.0) so it feels directed, not automatic.
- **Frame 0 is not empty.** Start with something on screen; open on the strongest picture.
- **Open still and quiet, close with a held end card** of 1.5 to 3 s that carries the one thing to
  remember or do.

## Sequencing and springs

- **Place beats against each other, not in milliseconds.** `sequence` takes a list of steps, each
  with a `preset`, the `node` or `nodes` it moves, and an `at`: a number of milliseconds, or a
  phrase against what is already there - `<` (the end of the step before), `<<` (its start),
  `<+=50` (50 ms after its end), `intro+=200` (200 ms after the step named `intro`). One call is
  one undo step, and the result echoes the resolved start and end of every step, so you can see
  the timing you got. `fit_to_ms` scales the whole sequence to a length.
- **Overlap the next beat by 40 to 80 ms.** `<+=50` is the follow-through: the next thing starts
  while the last is still settling, and the two read as one movement instead of two.
- **Stagger 40 to 120 ms per item when 3 or more arrive together.** 40 for a tight group of small
  items, 80 for a row of words, 120 for a slow reveal of big elements; more than 120 reads as a
  queue rather than a reveal.
- **Anticipation 6 to 10 percent**: a small pull back before the move (a scale or an offset of 6
  to 10% of the travel), so the eye is aimed at where the thing is going.
- **Overshoot only on small playful elements** (a badge, a dot, an icon). Never on type or a
  background: a big overshoot reads as a mistake.
- **Springs** for anything that arrives with weight: `ease: spring_snappy`, `spring_smooth`,
  `spring_playful`, `spring_bouncy`, `spring_heavy`, or `spring(visual_ms,bounce)` for your own.

| preset | visual ms | bounce | use |
|---|---|---|---|
| `spring_snappy` | 250 | 0 | a small thing that arrives and stops |
| `spring_smooth` | 400 | 0 | a panel or a card that settles |
| `spring_playful` | 450 | 0.3 | a friendly badge, a soft overshoot |
| `spring_bouncy` | 500 | 0.5 | a toy, a sticker, a celebration |
| `spring_heavy` | 800 | 0.1 | a big panel that lands with weight |

- **`spring_to` sends one property to a value on a spring** and says when it looks done
  (`perceived_ms`) and when it has really stopped (`settle_ms`): put the next beat after
  `settle_ms`, or overlap it when the next thing is unrelated.
- **A spring ease stretches the move it is on.** An entrance with `ease: spring(...)` runs for as
  long as the spring needs to rest, and the result says by how much.

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

## Kinetic type

`text_reveal` splits a text node and animates its pieces in one undoable turn; `split_text` only
splits, so you can stagger the pieces yourself (`animate_preset` stagger with `nodes` set to its
`ids`, or a `sequence`).

- **Pick the unit by length.** Characters only for a headline moment of 12 characters or fewer on
  one line (a brand word, a number, "NEW"). Words for a headline or a line or two (the default for
  anything longer). Lines for a paragraph block. **Never animate body copy per character**: it
  reads as noise and is slow to read.
- **Stagger.** Characters 30 to 50 ms apart (40 is the default), words 80 to 120 ms (90), lines
  about 140 ms. A whole headline should be in within about a second and a half; if it is not,
  use a bigger unit, not a smaller `each_ms`.
- **Styles.** `rise` (450 ms, `power4_out`) from below a mask box (`wrap`, the default for rise) is
  the confident one for headlines; `fade` for quiet text; `pop` only for short, playful words;
  `slide` for a label that comes from a side; `typewriter` (33 ms per character) for a command,
  a code line or a message being typed, never for a headline.
- **Hold it long enough.** Once the last piece is in, the text needs 1 s plus 0.4 s per short word
  (a long word counts for more) before it leaves (review check `RV015`); `text_reveal` returns
  `hold_ms` and `leave_no_earlier_than_ms` so you can place the exit.
- **One reveal per text, one text moving at a time.** Do not reveal a headline and its subtitle at
  the same moment: start the subtitle after the headline ends (`ends_at_ms`) plus a beat.
- **Split late.** Write and review the copy and the layout as ordinary text first; split only when
  the words are final, because the pieces no longer re-wrap if the box changes (undo the split to
  edit the text again).

## Checks before you export

- `render_video_frame` at: the first frame, the middle of every transition, the moment each
  overlay is fully in, the last frame. Look for text clipped by the frame, overlap between an
  overlay and the footage's own text, low contrast over a moving picture (add a dark band under
  captions), and overlays outside the platform safe zone (see `formats`).
- Read the sequence in `list_videos` (`story`): does each line follow from the one before it?
- Total length against the preset; exports are checked, but a good length is the shortest that tells
  the story.

## Sound

A video with no sound feels unfinished; give every video a sound track unless the user asked for
silence (an App Store preview autoplays muted, but still gets a track for when sound is on).

- **Levels.** A music bed under a voice sits 18 to 24 dB below it: `gain` 0.15 to 0.3 on the
  track. With no voice, a bed at 0.4 to 0.7. Never leave it at 1.0 and hope.
- **Fades.** `fade_in_ms` and `fade_out_ms` of 300 to 800 on the track, so it neither starts nor
  stops with a bang. The track is cut at the video's end: fade out before it.
- **Hits on cuts.** Put accents where the picture changes: a `hit` or `pop` on a hard cut, an
  `impact` (a heavier thump with a crack and a tail) on a big reveal, a `whoosh` or
  `swoosh_up/down` across a slide or zoom transition, a `riser` into a reveal, a `chime` on a
  success state, a `click` or `tick` on a tap, a `key` (a typing click), a `blip` (a UI blip) or a
  `scratch` (a record scratch) where the picture shows typing or a turntable. One accent per event,
  not per word. Tracks mix: put a hit or whoosh on its own `sfx` track at the cut's time
  (`add_sound_track` with `start_ms`), or let one longer sound (`beat`, `drone`, `pad`) carry the
  video.
- **Silence is a tool.** A short gap before the reveal makes the hit land. Do not fill every moment.
- **Vary the sound.** Never reuse one video's sounds for the next: leave `seed` out (each call is a new variation) or pick pitch and tempo for this video's mood, and check `list_sounds` so no two videos share a bed, hit or chime. Sources beyond the synth: `search_assets` kind `sound` (Kenney sound packs, once the user has installed one) and `search_sounds` (Openverse, Incompetech and ccMixter need nothing; Freesound needs the user's token).
- **Where sound comes from.** `make_sound` (kind, duration_ms, seed, pitch_hz): synthesized in the
  project, no licence. `search_sounds {query, provider?}` then `import_sound {provider, id}` for
  music and real recordings; `provider` is `openverse` (the default when the user has no Freesound
  token), `freesound`, `incompetech` (Kevin MacLeod's music, all CC BY 4.0) or `ccmixter`.
  `import_file` with kind audio for the user's own files (`find_files` kind audio). A voice-over
  from a speech MCP server the user added, such as minimax-mcp, if `get_guidance` lists one: write
  the script short, ask before spending credits on long text, then import the file it makes.
  `list_sounds` shows what is in `media/`.
- **Licences decide what you may use.** A project is commercial by default: `sound_settings`
  reads the setting and flips it (only when the user says the video is not commercial, and then
  with their words as the reason). While it is commercial, sounds under a non-commercial, a
  no-derivatives or an unknown licence are left out of the search and refused on import; CC0 and
  CC BY are fine. Never change the setting to get a sound through.
- **Show the credit.** Every imported sound carries its licence in `media/<name>.license.json`.
  Before you finish a video that uses a sound which is not CC0 or synthesized, call
  `sound_credits {video}`: it writes `exports/<video>-credits.txt` with the credit line of every
  sound the video's tracks use (and notes what the video changed, as CC BY asks), and you show
  that text with the video - in the description, on an end card or in a credits roll. A sound
  under ShareAlike may oblige the whole video to carry the same licence; the tool names those.
- **Setting it.** `{"op": "set_video", "video": ..., "prop": {"name": "audio", "value": {"file":
  "media/x.wav", "start_ms": 0, "gain": 0.25, "fade_in_ms": 500, "fade_out_ms": 600}}}` sets the
  one older track. For several, `add_sound_track {video, file, role, start_ms, in_ms, out_ms, gain,
  fade_in_ms, fade_out_ms, pan}` (role `music`, `voice` or `sfx`; up to 8), then `set_sound_track`
  and `remove_sound_track`.
- **Ducking.** A `music` track with `duck: {depth: 0.7}` lowers itself while any `voice` track
  speaks and comes back after (`attack_ms` 40 and `release_ms` 500 by default; the mix is one ffmpeg
  pass). Prefer ducking to a very low fixed gain: the bed can be 0.4 to 0.6 between lines.
- **Pauses and waveforms.** `detect_pauses {file}` lists the quiet stretches of audio or footage,
  `remove_pauses {video, clip}` cuts them out of a footage clip as one undoable step (the clip
  becomes several; overlays, captions and tracks keep their times, so move them after), and
  `get_waveform {file}` gives peaks to find where a hit should land. They need ffmpeg.
