# Scenery and motion presets: layered flat-vector scenes that never stand still

A flat-vector explainer scene (the look of Kurzgesagt or Ted-Ed) is built from many simple shapes with soft gradients, in layers that move at different speeds, with something moving everywhere all the time. This playbook is how to build that with the scenery blocks, the scene moves of `animate_preset` and the camera. Never paste one picture behind animated text: a still background is the failure this playbook exists to prevent.

## The scene recipe

1. **Palette first.** `set_token` the four roles (`brand.background` dark, `brand.accent`, `brand.text` light, `brand.muted`) before adding anything. Every scenery block mixes its tints from these four, so a scene is coherent by construction and one token change recolors it. Pick an accent with some saturation: the sky, hills, water and glows all lean toward it.
2. **Back to front.** Add the sky first, then whatever lives in it, then the ground, then ambient particles. Scenery blocks sort themselves by depth (`sky_gradient`, `starfield` and `cells_backdrop`, `sun_glow`, `clouds`, `parallax_hills`, then `ocean_waves`, `city_skyline` and `grid_floor`, then `particles`), so the order you add them in does not matter; your characters, titles and other content stay in front of all of them.
3. **A full set of layers.** A scene wants at least four things: a sky (with a light source), something far (stars, clouds, distant hills), something mid (hills, skyline, water) and something near (particles, a foreground silhouette). Two layers read as a backdrop; five read as a place.
4. **Give the camera something to do.** Layers carry a `depth` (0 pinned to the screen, 1 moves with the scene, above 1 moves more): `parallax_hills`, `city_skyline`, `clouds`, `particles` and `cells_backdrop` set it per layer from their `min_depth` and `max_depth`. A `camera_*` move then shears the scene into parallax. Keep far layers at 0.1 to 0.4 and near ones at 0.9 to 1.3.
5. **Check it moving.** `render_video_frame` at three or four moments, or `video_contact_sheet` over a camera move: layers should slide against each other, nothing should show an edge, and no two things should move in lockstep.

## The blocks

All take the standard `box`, `colors`, `start_ms`, a `seed` (the same seed draws the same scene) and, where they loop, `loop_ms`. They fill the frame by default; give a `box` to put one in part of it (what leaves the box is clipped).

- `sky_gradient`: 2 to 4 gradient stops, vertical by default or diagonal with `angle`; `light: sun|moon` adds a disc with halos that breathe. A soft glow at the horizon drifts and breathes so even a bare sky is alive. Dusk and night are the default palette; for a day sky set a light `brand.background` and a blue accent.
- `sun_glow`: a sun (rays that turn slowly), moon (craters) or bare `glow`, anywhere in the box (`x`, `y` in percent).
- `starfield`: stars of three sizes (up to 220) that twinkle out of step, 0 to 3 nebulae, 0 to 3 shooting stars that streak across every `loop_ms` or so.
- `clouds`: up to three depth layers drifting right at different speeds in a loop that tiles seamlessly; `loop_ms` is how long the nearest layer takes to cross one screen width.
- `parallax_hills`: 3 to 5 ridges, `style: hills|mountains`, hazier and paler far away, each its own group with `depth`; `sway` keeps them breathing, `shadow` casts a soft shadow on the ridge behind.
- `ocean_waves`: layered waves scrolling at different speeds with foam on the crests; `horizon` says where the water starts.
- `city_skyline`: two or three layers of buildings, far to near, with lit windows that blink out of step.
- `grid_floor`: a perspective grid for a tech look; rows scroll toward the viewer.
- `cells_backdrop`: translucent cells with membranes and nuclei in blurred depth planes: a microscope or biology scene.
- `particles`: `kind` bubbles, dust, confetti, snow, embers or sparkles in three depth planes. The loop repeats exactly every `loop_ms` (rounded to a multiple of four seconds). Bubbles and embers rise, confetti and snow fall.
- `connector`: an arrow or line between two points or two nodes (`from`, `to`: a node id or `{x, y}`), `curve: straight|arc|s_curve`, that draws itself on, pops an arrowhead, streams dots along it and can carry a `label`.
- `highlight_ring`: draws a ring around a target node, then breathes with an echo spreading outward.

Scenery is a start: tint it, add your own shapes (use `path` nodes: `import_svg_vector` for artwork, or write SVG path data) and put characters in front.

## The moves

`animate_preset` writes ordinary keyframes for any node, and for the camera; every move takes `start_ms`, `duration_ms` (a scene move has a default), `ease`, `intensity` (1 as designed), `repeat` (`once`, `loop`, `pingpong`) and `seed`.

- **Idle life** (loops by default, ends where it starts): `float`, `bob`, `breathe`, `pulse`, `wobble`, `swing`, `spin` (intensity = turns), `orbit`. Give every character, prop and light some idle life, each with a different `seed` and `duration_ms` so nothing breathes in unison. A good set: `float` on the hero, `breathe` on a character, `pulse` on a light, `swing` on a hanging thing.
- **Arrivals** (play once): `pop_in` (scale 0 to 1 with a small overshoot), `bounce_in` (drops in and bounces), `elastic_in`, `slide_in_left|right|up|down` (overshoot), `fade_up`, `zoom_in`. Pop and bounce for friendly objects, slides for panels, fade_up for text. Overlap them: use `stagger` with `of` set to the arrival, `nodes` in reading order, `step_ms` 80 to 160, `from` first, last, center or random.
- **Impacts**: `shake` decays to rest; follow a landing with it.
- **Drawing**: `draw_on` runs a stroked node's `trim_end` from 0 to 1 (ease in and out); the node needs a stroke.
- **Paths**: `animate_along_path` with `path` (a `path` node) samples the curve into x and y keys so `node` follows it over the time range with an `ease`; `orient` turns the node to face along the curve. Use it for a rocket on its arc, a ball over a hill, a cursor on a route. The node and the path should be in the same frame.
- **Camera** (no `node`): `camera_push_in` toward the subject while a point lands, `camera_pull_back` to reveal, `camera_pan` with `direction` and `amount` (document units) to follow, `camera_drift` as a slow looping handheld sway under the whole scene, `camera_shake` on an impact, `camera_whip` between ideas. Keep camera moves to 2 to 4 seconds with ease in and out; a pan of 8 to 15 percent of the frame is plenty. Layers at depth 0 (the sky) stay put.

## Timing and taste

- **Constant subtle motion everywhere**, but never so much that it competes with what the viewer must read: ambient amplitudes are small by design; leave `intensity` at 1 for things behind the story and raise it for the hero.
- **Ease and overshoot**: things arrive fast and settle (`ease_out`, `ease_out_back`, `spring`), leave with `ease_in`. Overlap actions: the next thing starts before the last one finishes (stagger by 80 to 160 ms).
- **Motion tells the story**: draw a connector where the idea flows, ring the thing being explained, push the camera in on the point and pull it back at the transition. Decoration that does not point at anything should stay in the background at low contrast.
- **Do not break the loop**: ambient tracks repeat for the whole video, so they must not be re-keyed at random. To change one, replace it (`apply_ops animate` with the same node and property) or delete the group.
- **Stay in the frame**: scenery bleeds past the frame on purpose (so a camera pan never shows an edge) but your own shapes should not.
- **Keep it light**: a block makes up to about 400 nodes; a scene of six or seven blocks is fine, but do not stack three skylines. A video holds at most 2000 animated tracks.

## Checks before you finish

- `render_video_frame` at the first second (nothing blank: blocks fade in over about a second), at the hero moment, and after the last move.
- `video_contact_sheet` across each camera move.
- Compare two frames a few seconds apart: ambient layers must have moved.
- Text over scenery needs contrast: `review_frame` the frame and put a soft shape under copy that sits on busy art.
