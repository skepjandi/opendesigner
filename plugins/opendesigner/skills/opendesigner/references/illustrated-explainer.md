# Illustrated explainer: flat-vector animation in the Kurzgesagt, In a Nutshell style

Read this first whenever the request says animated, animation, animate, scene, explainer, motion
graphics, cartoon, 2D, illustration, illustrated, character, or names Kurzgesagt or In a Nutshell. It
is the craft of a film made of flat shapes that never stop moving: a rich layered world, a simple
expressive hero, a camera that carries the eye, and motion that tells the story.

## The one rule

**A JPG, a PNG, a photo or any single image as a scene's backdrop, with text animated on top, is
never an animated scene.** It is a slideshow, and it is the failure this playbook exists to prevent. A scene is
built from vector nodes (`path`, `ellipse`, `rect`, groups, blocks) in depth layers, and most of those
nodes move. Raster images are for the user's own screenshots and photos only, inside a device or a
frame the story needs; never for the world. `review_video` measures this and fails it.

## What the look is made of

- **Flat shapes with soft gradients.** Two or three stop gradients (`linear` for sky and ground, `radial`
  for glows and planets), never hard outlines; forms are rounded; every shape has one light direction
  (pick top-left once and keep it in every scene).
- **Long soft shadows and glows.** `shadow` (`drop_shadow` on set) with a large blur and a low-alpha
  dark token for objects; a `radial` gradient fading to transparent behind lights.
- **A dark rich world with one or two saturated accents.** Deep blue, purple or teal fields with warm
  orange, yellow or pink for the thing to look at. Contrast tells the eye where to go.
- **Density.** A scene has 30 to 150 shapes in five layers; empty areas are filled with scenery, not
  left flat.
- **Simple characters** (round bodies, two eyes, no mouth detail) with idle life: they breathe, blink,
  bob and look around.
- **Constant subtle motion everywhere.** Stars twinkle, clouds drift, cells wobble, dust floats, the
  camera creeps. A frame is never still for more than a beat.
- **Motion tells the story.** Things grow, split, travel, connect and collide to show the idea; the
  words only name it.

## The process

1. **Inventory and brief.** `get_guidance`, `list_media`, `list_tokens`, `list_sounds`. Ask only what
   you cannot decide: the audience, the length, the language, where it plays (sets size, `list_formats`,
   `set_format`). Default: 1920x1080, 30 fps, 45 to 90 s, one idea.
2. **Script and storyboard first, as text in your reply.** One idea per scene; for each scene a visual
   metaphor (a planet for a population, a factory for a cell, a pipe for a network), 6 to 12 s, the
   voice-over line, what moves, the camera move and the sound. The hook is in the first 3 s: a
   question, a surprising picture already in motion, never a title card. The last scene lands the one
   thing to remember. 2.3 words a second of voice; 15 to 25 words per scene.
3. **A palette of 4 to 6 tokens, created before anything else** with `set_token`: a deep `ink`
   background, a lighter `field` for gradients, one or two saturated accents (`accent_a`, `accent_b`),
   a warm `light` and optionally a `shade` for shadows. Every fill, gradient stop and shadow is a
   `{token}` (or a lighter or darker step of one); never an invented color per shape. One palette for
   the whole film, one accent per idea.
4. **One frame per scene, the video's size**, named `scene 1 hook` and so on; one clip per frame with
   `add_clip`. Build the whole structure first, then fill each scene.
5. **Compose each scene in depth layers** (see below) with `add_block` for scenery and props, and
   hand-drawn `path` nodes or `import_svg_vector` for anything custom.
6. **Animate in this order:** entrances of the hero and the idea, ambient loops on everything, the
   camera, transitions, then sound and captions.
7. **Look at it.** `render_video_frame` at the start, middle and end of every scene and mid-transition,
   `video_contact_sheet` to see a move's arc, `review_frame` on each scene frame, `review_video` for
   the whole. Iterate at least once.
8. **Export** only when the final gate (end of this page) passes.

## Compose in depth layers

Group each layer and name it (`far`, `mid`, `hero`, `near`, `fx`); order in the tree is back to front.
Give the group a `depth` so the camera moves layers by different amounts (parallax): the engine
multiplies depths down the tree, 1 moves with the scene, 0 pins to the screen.

| layer | contents | `depth` |
|---|---|---|
| far backdrop | `sky_gradient`, `starfield`, `sun_glow`, big soft shapes, `cells_backdrop`, `grid_floor` | 0.2 to 0.4 |
| mid scenery | `parallax_hills`, `clouds`, `city_skyline`, `ocean_waves`, trees, buildings, distant props | 0.5 to 0.8 |
| hero | the `character` or the main prop and what it touches | 1 |
| foreground | big blurred leaves or cells at the edges (`blur` 4 to 12), props that pass by | 1.3 to 1.8 |
| particles and fx | `particles` (bubbles, dust, confetti, snow, embers, sparkles), `highlight_ring`, `connector` | 1.2 to 2 |
| captions, labels, HUD | text, the caption layer | 0 |

- Scenes that use a camera move must be larger than the frame: make backdrops about 115% of the frame,
  centered (negative x and y), so the edge never shows when the camera pans or zooms.
- Use `clip: true` on a group or frame to cut a layer at a window (a porthole, a screen, a cell wall,
  a progress bar fill) and `blur` for depth of field on the far and near layers.
- Keep shapes as shares of the frame's shorter side so the scene scales to 16:9, 9:16 and square. A
  portrait cut recomposes: hero larger, fewer side elements, scenery stacked vertically.
- Safe area: heroes and text inside 8% margins; scenery may bleed.

## The building kit

Call `list_blocks` (category `scenery`, `characters`, `motion`) and `get_block` for the parameters.
Blocks expand to ordinary nodes and keyframes in one undoable step, so edit their parts afterwards.

| need | block id |
|---|---|
| sky, space, dusk | `sky_gradient`, `starfield`, `sun_glow`, `clouds` |
| ground and horizon | `parallax_hills`, `ocean_waves`, `city_skyline`, `grid_floor` |
| inside a body or a lab | `cells_backdrop`, `prop_cell`, `prop_dna` |
| air and atmosphere | `particles` with `kind` bubbles, dust, confetti, snow, embers or sparkles |
| guiding the eye | `connector` (an arrow or line that draws on between two points or nodes), `highlight_ring` |
| characters and talk | `character`, `speech_bubble`, `thought_bubble` |
| props | `prop_rocket`, `prop_lightbulb`, `prop_atom`, `prop_planet`, `prop_tree`, `prop_house`, `prop_laptop_2d` (a flat illustration, not the 3D device), `prop_gear` |

Pass your tokens as the block's colors and your own seed so no two scenes share the same stars or
hills; vary `seed` per scene. Custom art: draw it as `path` nodes (SVG path data in a box; `view_box`
for exact placement; gradient fills; strokes with `cap`, `join`, `dash`) through `apply_ops`, or bring
a whole SVG in with `import_svg_vector` (parts named by SVG ids can then be animated one by one:
an arm, an eye, a wheel). Never a stock photo and never one image as a backdrop. Icons from
`search_assets` are fine as small accents when they match the palette.

## Motion language

Motion is the film. Every move named below is a `preset` of `animate_preset` (its `preset` list is the
truth): ambient ones loop by default, `duration_ms` is one cycle and `seed` shifts the phase, so give
every node its own `seed` and duration; `intensity` scales how far a move goes; `stagger` takes
`nodes`, `of` (the move), `step_ms` and `from`; `animate_along_path` takes the `path` node to follow;
camera moves take the video's own id as `node` and `direction`/`amount` for pans and whips. Eases:
`ease_out` to arrive, `ease_in_out_cubic` for camera and travel, `ease_out_back`
and `spring` for things that land with weight, `elastic` and `bounce` only for small playful things.

- **Ambient loops on everything that can live.** Every character, prop and scenery element gets a track
  with `repeat` `loop` or `pingpong`, each with its own duration (2.3 s, 3.1 s, 4.7 s) and a delayed
  start, so nothing moves in lockstep: stars twinkle (opacity), clouds drift (x), planets rotate
  (`rotation`), a character breathes (`scale_y` 1 to 1.03) and bobs (y), a flame flickers, cells wobble,
  gears turn linearly, particles rise. Use the presets: `float`, `bob`, `breathe`, `pulse`, `spin`,
  `wobble`, `shake`, `swing`, `orbit`.
- **Anticipation, overshoot, follow-through.** Before a big move go the other way a little (squash
  `scale_y` 0.94 for 150 ms), arrive past the mark and settle back (`ease_out_back`, `spring`), let
  the parts trail the body (arms, antennas, tails start 60 to 120 ms later).
- **Entrances:** `pop_in`, `bounce_in`, `elastic_in`, `slide_in_left`, `slide_in_right`, `fade_up`,
  `zoom_in`, `draw_on` (a stroke drawing itself: `trim_end` 0 to 1). Scale pops for emphasis: 1 to
  1.15 to 1 in 300 ms on the thing the voice names.
- **Stagger.** Items of a group arrive 80 to 140 ms apart in reading order (`stagger`, or the
  `stagger_in` block); never all at once, never all in lockstep.
- **Overlap.** The next move starts before the last one ends; scenes overlap by 300 to 600 ms of
  transition. Nothing waits for everything else to stop.
- **Camera that carries the eye** (the video's own id as `node`; `camera_x`, `camera_y`, `camera_zoom`,
  `camera_rotation`): `camera_push_in` toward the idea, `camera_pull_back` to reveal scale,
  `camera_pan` across a landscape, `camera_drift` as a constant slow creep under every shot,
  `camera_shake` on an impact, `camera_whip` between ideas. Every scene gets a camera move; vary
  the direction scene to scene. With layer `depth` the world gains parallax for free.
- **Along a path.** `animate_along_path` moves a rocket, a blood cell or a signal along a curve you
  draw as a `path`; `draw_on` the trail as it goes.
- **Transitions by continuity, not by effect.** Zoom through an object into the next scene (the
  porthole, the cell, the screen becomes the next frame's world); a wipe made of scenery (a hill or a
  wave that sweeps across); match shapes (a circle that is a sun becomes the next scene's cell);
  carry a color. Use the clip `transition` (`zoom_in`, `iris`, `wipe_left`, `blur_dissolve`) with the
  same shape in both frames. Cuts for beats. A fade is the last resort.
- **Rhythm.** Something new every 1 to 2 s; a hold of 400 to 800 ms after each important landing;
  vary tempo: fast build, a pause, a big reveal.

## Text and captions are the minority of the screen

Text covers well under 10% of any frame. The picture carries the idea. Kinetic type only for key terms
and numbers (one block title or a few words, `stagger` per word). Spoken words reach the screen as
captions through the caption tools (`import_captions` from a transcript or `captions_from_script`, then
`set_caption_style`), small, two lines, off the hero. Never paste the script on screen. Text sits on its
own layer with `depth` 0.

## Sound design

Plan sound with the script. A film with no sound is unfinished unless the user asked for silence.

- A **voice-over** when the user supplies one or a speech MCP server is listed in `get_guidance`; a
  `voice` track with `add_sound_track`. Time each scene to its line: 2.3 words a second.
- A **music bed** (`search_sounds` and `import_sound`, `list_sounds`, or the user's file) as a `music`
  track at `gain` 0.15 to 0.3 with `duck` so it lowers under the voice, fades of 300 to 800 ms.
- **Effects from `make_sound`** (click, pop, whoosh, swoosh_up, swoosh_down, riser, hit, chime, drone,
  beat, stinger): a whoosh on every cut and camera whip, a pop on every scale pop, a riser into the
  reveal, a chime on the idea landing, a hit on an impact, each on an `sfx` track at that moment.
  Vary them scene to scene (different seed or pitch); installed sound packs (`search_assets` kind
  `sound`) give richer ones.
- Silence for half a second before the big reveal.

## Scene checklist (every scene)

- One idea, one visual metaphor, 6 to 12 s.
- Five layers present, `depth` set, backdrop larger than the frame.
- At least 12 vector shapes, no raster backdrop, colors from tokens, one light direction.
- Hero entrance with anticipation and overshoot; at least three layers moving at any moment.
- Every living element has a looping ambient track with its own timing.
- A camera move; a transition by continuity into the next scene.
- Sound: a whoosh at the cut and one effect on the main beat.
- Text under 10% of the frame; readable at 360 px wide.

## Worked scene recipes

Each is an ordered outline of tool calls for one 8 s scene of a 1920x1080 film. IDs come from earlier
results. Replace the copy and the seeds.

### Space scene: "The Earth is one planet among billions"

1. `create` the frame `scene 2 space` (apply_ops), 1920x1080, fill `{ink}`; `add_clip` 8000 ms with
   transition `iris` 500 ms.
2. `add_block` `sky_gradient` (ink to field, vertical), `starfield` (`seed` 2, dense, twinkle loops),
   `sun_glow` at the top-left (accent_a, large soft radial) into group `far`, `depth` 0.3, 115% size.
3. `add_block` `prop_planet` (a big ringed planet, accent_b) bottom right at `depth` 0.6, and a smaller
   `prop_planet` upper left with a `blur` of 3 for depth.
4. Hero: `add_block` `prop_rocket` center-left (`depth` 1). `add_block` `particles` kind `sparkles` as
   the exhaust, behind the rocket.
5. `animate_preset` `bounce_in` the rocket 200 ms; `float` it (loop); `spin` both planets at 60 s and
   90 s a turn (rotation loop, different phases); `pulse` the sun glow at 4.3 s.
6. `add_block` `connector` from the rocket to the big planet (draws on, 1.6 s from 3 s) and
   `highlight_ring` around the planet at 3.2 s.
7. Camera: `camera_push_in` 1.0 to 1.12 from 0 to 8 s plus `camera_drift`; `camera_whip` at 7.6 s into
   the next scene.
8. Kinetic title: `add_block` title with `stagger` per word, "billions", at 3.5 s; captions from the
   voice line.
9. Sound: `make_sound` whoosh at 0 (the iris), riser from 2.4 s, chime at 3.5 s; bed continues.
10. `render_video_frame` at 0.2, 3, 7 s; `video_contact_sheet`; fix; `review_video`.

### Biology scene: "A cell is a tiny factory"

1. Frame `scene 3 cell`, fill `{ink}`; clip 9000 ms, transition `zoom_in` 600 ms (the previous scene
   ends on a close-up of a dot; this scene opens inside it).
2. `add_block` `cells_backdrop` (field to ink radial wash, soft background cells at `depth` 0.3, wobble
   loops), a second `cells_backdrop` seed at `blur` 8 as the foreground edge (`depth` 1.6).
3. Hero: `add_block` `prop_cell` large in the center (membrane stroke with `drop_shadow`, nucleus in
   accent_a); inside it `prop_gear` x3 of different sizes meshing (`spin` linear loops, opposite
   directions, ratios matched), as the organelles that make things.
4. Small blobs travelling in: `particles` kind `bubbles` upward, and three hand-drawn `path` capsules
   moving with `animate_along_path` through the membrane; `draw_on` their trails.
5. `breathe` the cell (loop 3.7 s); `pulse` the nucleus (loop 2.9 s); `wobble` the membrane.
6. `add_block` `prop_dna` on the right, `spin` slow, with a `connector` to the nucleus drawing on at 4 s
   and `highlight_ring` on the nucleus at 4.2 s.
7. Camera: `camera_push_in` into the cell for 0 to 5 s, `camera_pan` along the DNA, `camera_drift`
   under it; `camera_shake` small at the moment the product leaves the cell.
8. Labels: one kinetic word, "factory", at 5 s; nothing more.
9. Sound: soft `drone` bed track, `pop` at each organelle entrance, `chime` at the product leaving.
10. Check frames, `review_video`.

### How-it-works machine: "How the internet delivers a message"

1. Frame `scene 4 machine`, fill `{ink}`; clip 10000 ms; transition `wipe_right` with a hill-shaped
   wipe of scenery.
2. Backdrop: `add_block` `grid_floor` (perspective grid in accent_b, drifting toward the viewer, loop),
   `sky_gradient`, `city_skyline` at `depth` 0.6 with window lights twinkling.
3. Machine as hand-built `path` and `rect` parts in a group: a house (`prop_house`) on the left, a
   `prop_laptop_2d` in the middle and a server rack drawn from rounded rects on the right; `prop_gear`
   x2 on top (`spin` loops).
4. Message: a small envelope `path` travels along a curve between the three with
   `animate_along_path`; `connector` lines draw on in turn (staggered 400 ms), `highlight_ring` pops
   on each stop with `ease_out_back`.
5. `character` at the laptop: eyes look at the envelope, `breathe` loop; a `thought_bubble` with
   `prop_lightbulb` pops in at the end of the delivery.
6. Camera: `camera_pan` follows the envelope left to right with `camera_zoom` 1.0 to 1.15; `camera_drift`
   before and after.
7. Sound: `make_sound` click on each stop, whoosh as the envelope travels, chime on delivery.
8. Check frames at each stop; `review_video`; fix.

## Final gate before `export_video`

1. `review_video` returns a score of 70 or more and none of RV001, RV002, RV003, RV004 or RV009 (an
   image backdrop, text-only motion, a still stretch, no ambient loops, no sound); read each finding and
   fix it, then call it again. `validate_document` for lint (`OD090`, `OD091` say the same on the
   document alone).
2. First 3 s hold the hook; last scene lands one idea; total length matches the script.
3. Frame by frame: `render_video_frame` at the middle of every scene and every transition. Nothing
   clipped, nothing outside the frame, text readable at 360 px wide.
4. Every living element has an ambient loop; at least three layers move at any time; the camera moves.
5. Sound set, ducked under the voice, effects on cuts; captions styled and small.
6. Run the self-critique loop of `get_playbook review`: `video_strip` of every key motion, `video_crop` of every face and contact, the `overlap` list of `review_video`; no RV013 (flashing) ever.
7. Say what you made, the palette and the idea of each scene.

## Mistakes to avoid

- One image behind the scene and text sliding over it. Build the world from shapes.
- Everything arriving at once and then freezing. Stagger, overlap, keep ambient loops running.
- Every loop on the same duration. Desynchronize.
- Random colors per shape. Tokens and their shades only.
- A title card as the hook, or paragraphs on screen.
- A camera that never moves, or a scene backdrop the exact size of the frame under a camera move.
- Photoreal or outlined clip art mixed with flat shapes; one style, one light direction.
- Bare rectangles as "scenery". Rounded forms, gradients, shadows.
