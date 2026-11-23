# Changelog

All notable changes to OpenDesigner are documented here.

> **Format**: loosely based on [Keep a Changelog](https://keepachangelog.com/), grouped by date descending. Each entry references the originating PR (when available) and names the area from the conventional-commit prefix.
>
> **Wiki**: the [GitHub Wiki](https://github.com/skepjandi/opendesigner/wiki) is the public-facing main wiki — architecture, document format, agent integration, security model, roadmap. Source-controlled editing surface is `docs/wiki/` in this repo; CI (`.github/workflows/sync-wiki.yml`) mirrors it to the GitHub Wiki on every merge to `main`. New single-PR changes belong here, not as new wiki pages.

## Unreleased

### docs: remove references to external video editors

**Area:** `docs(wiki)`

- Removed the named references to two external video editors from the README, the wiki (including their Prior Art and Video Editing sections) and an older changelog note. Design decisions that cited them now stand on their own reasons.

### feat(core,render,media,agent): more from Remotion: formats, parts, noise, fit to media, two transitions

**Area:** `feat(render)`, `feat(media)`, `feat(core)`, `feat(agent)`, `docs(wiki)`, `feat(skill)`

From Remotion's docs on noise, transition presentations, transparent videos, GIFs and `calculateMetadata()`, read 2026-11-22 ([Prior Art](docs/wiki/prior-art.md#video-agent-sources-2026-11-22)).

- **`export_video` formats.** `.gif`, `.webm` (VP9) and `.mov` (ProRes) by the file's ending, with `transparent` (a clear ground, `VideoRenderer::with_transparent`; premultiplied pixels turned straight at the writer), `width`, `gif_loops`. New `od_media::Container` and `container.rs` (encoder, pixel format and filter graph per kind); `EncodeSettings` gains `container`, `every_nth`, `alpha`, `scale_width`, `gif_loops`.
- **Parts and steps.** `from_ms`, `to_ms` and `every_nth_frame`: `od_render::export::export_frames_with` draws only the selected frames (the same pixels as in a full export, tested), an `.mp4` keeps the cut sound of the part, a GIF of a long video no longer draws all of it. A store preset checks whole `.mp4` files only.
- **`animate_noise`.** Seeded value noise (`od_core::noise`, integer hashing, 1 to 4 octaves, seamless loop) written as keys, up to 4 properties with their own streams, additive by default.
- **`fit_clip_to_media`.** A clip as long as an audio or footage file, or the rest of its own footage, plus `pad_ms`.
- **Transitions `flip` and `clock_wipe`.** A flat card flip (`Pose::sx`) and a clockwise radius mask; no change to existing files.
- **Docs.** Wiki: video-editing (formats and parts, noise motion, fit to media, transitions), agent-integration, security (what reaches ffmpeg), prior-art, roadmap decision log. Playbooks: video-direction and motion.

**Not verified:** colour grades and blur on partly transparent edges, playing the WebM and the MOV in a browser or in Final Cut Pro, Premiere Pro or DaVinci Resolve (only decoded back with ffmpeg), Windows and Linux, and whether agents prefer these tools to what they did before.

### feat(agent,media,core): footage memory, timeline export and audio-driven motion

**Area:** `feat(agent)`, `feat(media)`, `feat(core)`, `docs(wiki)`, `feat(skill)`

Ideas from Remotion, FrameScript, OpenFilm, ELLMPEG, Shotstack's agent guide, Dataiku's Watch Video tool and invideo's editing agent, read 2026-11-22 (table and what was not taken: [Prior Art](docs/wiki/prior-art.md#video-agent-sources-2026-11-22)).

- **`watch_footage`.** Up to 10 pictures from the middle of equal slices of a `media/` recording, labeled with their time, in one grid, with the agent's `question` repeated beside it. Read-only.
- **`index_footage` and `search_footage`.** Shots by ffmpeg scene score (`od_media::footage`: `detect_cuts`, `shots_from_cuts`), an optional word-timed transcript from the user's own whisper (the `transcribe` path, now `recognize`), and the agent's notes and tags per shot, kept in `footage-index/<name>.json`. Search matches words against notes, tags and transcript and answers with a ready `add_clip` object. A changed file is marked `stale`; a rescan carries notes to the shots that start where they did.
- **`export_timeline`.** A CMX 3600 EDL (`od_core::interchange::edl`: footage source and record times, speed, dissolves, sound tracks), SRT or WebVTT in `exports/`; what an EDL cannot hold is returned as `warnings`. A hand-over like `export_video` for the unseen-changes warning.
- **`audio_reactive`.** A property follows the loudness, or the bass, mid or treble, of a sound: ffmpeg measures it, `normalize` gates, scales and smooths it, and it is written as ordinary keys through `apply_ops` (one undoable step). Also the lip flap: a mouth's `scale_y` on a voice file.
- **Silence.** A band the sound does not have (about 0.0002 rms, measured for a 100 Hz tone through the treble filter) is not stretched to full scale: anything under -50 dBFS is silence; the first 60 ms of band-filtered sound, where the filter settles and reads twice as loud, are replaced by what follows; a curve with no movement is refused instead of written as two equal keys.
- **Docs.** Wiki: video-editing (footage memory, timeline export, audio-reactive motion), agent-integration, security (new files written, what reaches ffmpeg), prior-art, roadmap decision log. Playbooks: video-direction (footage workflow, export, lip flap) and review (why three rounds; do not resend a failing call).

**Not verified:** a real whisper (the transcript path is covered by the existing fake-program tests for `transcribe` only), importing the EDL into Final Cut Pro, Premiere Pro or DaVinci Resolve (written to the CMX 3600 layout, never opened in them), Windows and Linux, and whether agents prefer these tools to what they did before.

### feat(skill): a growth playbook for store assets

**Area:** `feat(skill)`

New `growth` playbook (`get_playbook growth`, `references/growth-playbook.md`): how people decide on a store page, what published tests do and do not show (Apple's three PPO tests, custom product page averages, vendor bundles flagged as such), the icon, the screenshot sequence, preview video, Play specifics, custom product pages, policy rails, localization, off-store cards, and the one-change-per-test plan the agent hands over. Evidence read 2026-11-22; conventions with no controlled data are labelled as such.

### feat(agent): a picture as depth layers

**Area:** `feat(agent)`, `feat(render)`, `docs(wiki)`

- **`layer_image`.** Cuts one picture into depth layers for a camera move: a subject mask (two layers) or a depth map (2 to 6 layers of equal area), from the user's own programs. Each layer is an image node of one group with a `depth`, carries colours grown out of its own edge under the nearer layers so a move does not uncover a hole, and is overscanned 6 percent. New `od_render::decode_rgba`. Playbook section "A picture as depth layers"; `remove_background` now says which model weights are non-commercial.



### feat(core,agent): arcs, percentage keyframes, text on a path and shape morphing

**Area:** `feat(core)`, `feat(agent)`, `feat(skill)`, `docs(wiki)`

- **`animate_arc` and `arc`.** A node travels on a quadratic Bezier (control point = start + run x `peak` + perpendicular x `strength` x distance; `direction` auto bulges up, or right on a vertical move, whichever way it goes, `cw`/`ccw` pick the sweep) sampled into x/y keys every ~33 ms on the frame grid; `orient` adds a tangent rotation unwrapped so it never jumps a turn. `animate_preset` takes the same `arc` object for scene moves that travel in x or y (`slide_in_*`, `bounce_in`, `fade_up`: the other axis is bent from the node's rest) and refuses it for a camera move, `animate_along_path`, looping moves and moves that do not travel.
- **`animate_keyframes`.** CSS-style `{"0%": {...}, "40%": {...}}` over several properties (camera and `fill_color` included), with the leaving-ease rule of CSS and anime.js (a keyframe's ease shapes the move that leaves it), a first keyframe after 0% starting from the current value, `allow_over` for percentages past 100, and an error when two keyframes land on one frame.
- **`text_on_path`.** One text node per glyph (the renderer's own advances, as `split_text` uses) standing on a path node's flattened curve by arc length and rotated to its tangent, in a group; `align`, `start_offset`, `mode`, optional `reveal` through the scene-move stagger; glyphs off the end of an open path are left out and counted.
- **`morph_to` and `path_morph`.** Core gains `Node::morph` (`crates/core/src/morph.rs`: equal-length point lists per contour and stop), `Property::PathMorph`, and the `morph_progress` animation, which `Document::at_time` turns into polyline path data, so the renderer needed no change. Both outlines are cut by arc length into `max(ceil(length x 0.33), 16)` (at most 256) points, rings get matching windings and the least-travel start point, contours are paired largest first (an unpaired one shrinks to its middle), a second `morph_to` continues to a third shape and `reset` starts over. A `set` that would strand `morph_progress` keys is refused so a saved project always loads.
- **Docs.** Wiki: video-editing ("Paths and morphing"), agent-integration, document-format (path morph); playbook section "Paths and morphing".

**Not verified:** how morphs between very different shapes look, text on tight curves, ancestors that are turned or scaled (their transforms are ignored when placing glyphs and morph targets), right-to-left text on a path, and whether agents prefer these tools to hand-written keys.

### feat(core,agent): additive layers, `animate_to` and layout-diff transitions

**Area:** `feat(core)`, `feat(agent)`, `feat(skill)`, `docs(wiki)`

- **Additive layers.** A track may be `additive: true`: its keys are offsets added to the node's value as the replacing track gives it, held to the property's range, so a `bob` rides on a travel. One replacing and one additive track per node and property, applied in list order; a colour cannot be additive. `Operation::Animate` gains `additive` (left out when false), `animate_preset` takes `layer: "additive"` (scene moves and older moves alike), and `Document::at_time`, `Video::camera_at`, `Video::animated_value`, the signature, the review tools and the track gate all see the layer. Project format version 6 (older projects load unchanged).
- **`animate_to`.** Sends a property to a value from where it is at `at_ms` (the track's value, else the node's own), replaces the old track's keys from there (`keep_later` keeps the later ones) and bakes a cut curved segment into 8 linear pieces, so interrupting a move never jumps.
- **`snapshot_layout` and `transition_layout`.** Record the nodes of a frame to a plain JSON file in the project's `layout-snapshots/` folder, edit, then write x, y, width, height, opacity and fill_color keys from the recorded values to the live ones (new nodes fade in, a reparented node starts where it was on the page, a deleted node is reported because it cannot be animated), with the scene-move stagger, as one attributed undoable turn and a table of what each node does.
- **Docs.** Wiki: video-editing (additive layers, layout transitions), agent-integration, document-format; playbook section "Layers and layout moves".

**Not verified:** transitions of turned or scaled nodes and parents; whether agents prefer these tools to hand-written keys.

### feat(agent): scene seeds and the beat map

**Area:** `feat(agent)`, `feat(skill)`, `docs(wiki)`

- **Six seeds.** `list_seeds` and `apply_seed {id, video?, brand?}`: explainer opener, product reveal (a 3D phone), kinetic type quote, stats callout, before and after, logo sting. Each is a deterministic recipe over the blocks `add_block` already has (same code path), with ambient loops and a camera move, built as one attributed, undoable turn; a failed step restores the turn's checkpoint. Colors are the applied style pack's tokens when there is one. The result carries the layout notes, the beat times, the open P0 gates and the next call. Applying twice makes a second scene and `<id>-2` spec.
- **Beat map.** `SceneSpec.beat_map` (`id`, `at_ms`, `takeaway`, `focal_node`, `transformation`, `copy`, `audio_cue`, `exit_ms`, `built`) and `seed`. `scene_spec_validate` checks strictly increasing times inside the video, focal nodes in `layer_plan`, `exit_ms` after `at_ms`, unique names and a beat at least every 8 s. `scene_status` returns `next_beat`; `beats::beat_times` maps names to milliseconds.
- **Docs.** Playbook topic `seeds`; wiki: scene-spec (Beat map), agent-integration (Scene seeds).

**Not verified:** how the seeds look (tests check structure, lint, spec and `review_video` findings, not pictures); `review_video` is not run inside `apply_seed`.

## 2026-11-30

### feat(agent): style packs, a locked palette and the linter that holds it

**Area:** `feat(agent)`, `feat(core)`, `feat(skill)`, `docs(wiki)`

- **Eight packs.** A style pack is a palette, a type scale, a shape posture, a motion personality, the rules that hold them together and the anti-patterns that keep the slop out: `deep-space-flat`, `paper-craft`, `swiss-kinetic`, `neon-night`, `soft-clay`, `editorial-ink`, `sunny-explainer`, `mono-terminal`. They are data in the crate, not files to load at run time. A test checks every pack's WCAG contrast (text 4.5:1 on the ground and on the surface, the accent 3:1) and that the eight differ from each other in OKLab.
- **Three tools.** `list_style_packs` (id, name, mood, swatches, motion personality), `get_style_pack {id}` (the whole pack, the rules written as guidance) and `apply_style_pack {id, overrides?}`, which writes the palette into the document as the tokens `bg`, `surface`, `fg`, `muted`, `border`, `accent`, `accent_2` and `shadow`, adds the `palette_locked` marker and records the pack's id and motion personality in the workspace brief — one attributed, checkpointed, undoable step, so undoing it takes the palette with it.
- **The palette lock.** New lint rule `OD092 palette-lock` (warn): with the marker in the document, a fill or a stroke that is neither a token, nor within 0.04 of a palette colour in OKLab, nor a blend of two palette colours at 25, 50 or 75 percent, is flagged with the node and the nearest palette colour. Without the marker the rule is off, so a project that never chose a pack behaves as before.
- **Playbook.** New topic `packs`: choosing a pack from the brief, saying which one was chosen and why, never mixing two, the 80/20 rule, the motion personality mapped to eases, durations and stagger, and the anti-slop list adapted to motion.

## 2026-11-29

### feat(agent): the motion and scene gates, and a review that says what it could not check

**Area:** `feat(agent)`, `docs(wiki)`

- **Three tools.** `verify_motion` samples every animation track (the camera's too) at least 5 moments and compares the value the track says with the value the renderer's pipeline draws (G1), checks that every value is finite and legal (G2) and that a track which repeats ends where it starts (G3: rotation 0.5 degrees, position 0.01 of the frame height, scale and opacity 0.005, that last one uncalibrated so it only warns). With `classify` it measures every moving node in fractions of the frame height and names the motion (idle, in-place, walk, run, dash, jump, leap, planted, gesture); ambient life must read as idle or it warns. `scene_check` runs the geometry gates of a plan: bleed under every camera pose, the gap between a child and its parent, chirality, part coverage, the motion roster and the shapes frozen after the shapes pass. `hit_test` lists the node ids under a point, topmost first.
- **A track that drives nothing.** Finding `RV020`: the node does not exist, the property does not apply to that kind of node, its scene is not on screen while the track plays, the node is hidden or at opacity 0, the track starts after the video ends, or the picture does not show the value the track says. It is an error; a track whose keyframes all hold one value is the same finding as a warning.
- **Unevaluated is not a pass.** Every gate answers `pass`, `fail` or `unevaluated` with its reasons. The review report names the gates it could not evaluate (the beat without a tempo, the flash scan of a video with footage) and says why; `ready` is false while one stands, unless the caller passes `allow_unevaluated`.

## 2026-11-28

### feat(agent): kinetic type, a text node split into pieces and brought in one by one

**Area:** `feat(agent)`, `feat(skill)`, `docs(wiki)`

- **Splitting.** `split_text {node, by, wrap?, mode?}` turns a text node into a group of one text node per character, word or line, each drawn where the original glyphs were: the lines are the ones the renderer's own wrapping breaks to, and the positions come from that layout rather than from a guess. The pieces keep the font, size, weight, line height, fill and stroke, spaces are not nodes, and the ids come back in reading order. `wrap` puts each piece in its own clipping mask so it can rise or slide in from outside the box; `mode: keep` hides the original, `replace` removes it, and one undo puts the text back.
- **Reveals.** `text_reveal {video, node, style}` splits and animates in one turn: rise (450 ms, `power4_out`, masked), fade (400 ms, `ease_out`), pop (350 ms, `back_out`), slide (500 ms, `power4_out`) and typewriter (33 ms per character), staggered 40 ms per character, 90 per word and 140 per line. The result gives every piece's start and end, when the reveal ends, and how long the text must stay to be readable (1 s plus 0.4 s per short word, the `RV015` rule).
- **Playbook and wiki.** A "Kinetic type" section in the motion playbook (when to split by words and when by characters, the stagger bands, never per character for body copy, the hold) and a wiki section with the tools, their defaults and what a split cannot do (the kerning between two pieces, ligatures, the alignment, right-to-left text).

### feat(agent): sequencing, springs and a stagger that measures the real nodes

**Area:** `feat(agent)`, `feat(skill)`, `docs(wiki)`

- **A position grammar.** `sequence` places a list of moves against each other in one turn and one undo step: every step says where it starts, as a number or as a phrase against what is already there (`<` the end of the step before, `<<` its start, `<+=50` an overlap, `intro+=200` after a named step). A label that does not exist yet, an empty position and a start before zero are refused; the result echoes the resolved start and end of every step, and `fit_to_ms` scales the whole sequence to a length.
- **Stagger v2.** The `stagger` object adds value staggers (`range`: each node's intensity multiplied by its value), an index or a point for the origin, a grid measured by `euclid`, `manhattan` or `chebyshev`, an `axis`, an eased or reversed distribution, a seeded `jitter_ms` and `spatial` (distances between the nodes' real centres, in units of the nearest pair). The older `step_ms`, `from`, `columns`, `metric` and `start_index` still work. The seeded generator is Mulberry32, bit for bit, so a seed gives the same jitter and the same shuffle every run.
- **Springs.** `spring_to` sends one property to a value on a spring and says when it looks done and when it has really stopped, so the next beat can be placed after it. A tuned spring ease on an entrance stretches the move to the time the spring needs to rest, and the result says by how much.
- **Playbook and wiki.** A "Sequencing and springs" section in the motion playbook with the numbers (stagger 40 to 120 ms, overlap 40 to 80 ms, anticipation 6 to 10 percent, the spring presets), and the scene motion page of the wiki documents the tools and their arguments.

## 2026-11-27

### feat(agent): the SceneSpec, the locked build passes and the review ledger

**Area:** `feat(agent)`, `feat(skill)`, `docs(wiki)`

- **Plan first.** A SceneSpec per video in `scene_specs/<name>.json` inside the project: brief, routing, complexity tier, palette tokens, layer plan, detail inventory, feature targets, motion plan, audio plan, budgets, build passes, freeze, review history, calibration. Unknown fields are kept, not rejected.
- **Tools.** `scene_spec_new` (a starter whose placeholders fail strict validation on purpose), `scene_spec_write`, `scene_spec_validate {strict}`, `scene_status` (current pass, loop counters, `active`/`stopped` with the reason, the missing evidence and the exact next call) and `record_review` (one of `continue`, `refine-spec`, `refine-code`, `request-input`, `stop`).
- **Locked passes.** A pass completes only with a `continue` that has the evidence files in `exports/`, an `ai_score` of at least 0.7, the five layer scores and every critical feature at its minimum; a pass cannot be continued before the one before it. Correction limits: 3 per pass, 6 in total, counted from the file and never from memory.
- **Fail-closed rules.** Routing needs a known kind, confidence 0.82 and an evidence reference; the feature gate uses the tiers 0.8 / 0.65 / 0.5 with at most 5 critical and 3 important per pass; the detail inventory needs 3, 6, 10 or 16 entries by tier.
- **Playbook.** New topic `build` and the wiki page `docs/wiki/scene-spec.md`.

### feat(core,agent): tunable eases, springs with a clock, and pixel verification

**Area:** `feat(core)`, `feat(agent)`, `docs(wiki)`

- **An ease is still one string, and now it can be tuned.** The curves moved to `od_core::ease`: the old names keep their shapes, and `cubic(x1,y1,x2,y2)` (the CSS `cubic-bezier`), `spring(visual_ms,bounce)`, `back_in|back_out|back_in_out(overshoot)`, `elastic_in|elastic_out(amplitude,period)`, `power_in|power_out|power_in_out|power_out_in(exponent)`, `steps(n)` and `anticipate` join them. The numbers are kept in thousandths, so an ease stays `Copy` and a file never holds a float that prints differently the next time it is saved; older projects load unchanged.
- **A spring has a clock.** `spring(visual_ms,bounce)` is a damped spring with no starting speed, read over the stretch between its two keys: it is mostly done after its visual duration and rings by its bounce (0 none, 0.3 playful, 0.5 bouncy, negative slow). What is left at the end of a short stretch is spread over it, so the value always lands exactly on its key. `Spring::settle_ms` says how long a stretch must be for the swings to show, and the presets `spring_snappy`, `spring_smooth`, `spring_playful`, `spring_bouncy` and `spring_heavy` are ready names.
- **Pixel verification.** A tool that changes the picture answers `pixel_verified: false` until the agent has called a tool that draws a picture it can see (a render, a review, a contact sheet), and the exports warn when changes went unseen. Nothing is refused: it only says what the agent has not looked at.

## 2026-11-26

### feat: hand-made looks, richer acting, self-review tools, speech and cutouts, time feel

**Area:** `feat(agent)`, `feat(core)`, `feat(render)`, `feat(media)`, `feat(cli)`

- **Hand-made look.** Blocks `sketch` (wobbly double strokes, hachure, cross-hatch, zigzag, dots, dashed fills, draw-on), `boil`, `watercolor_wash`, `paper_grain`, `flow_strokes`, `marker_underline`, `scribble_circle`; playbook topic `sketch`.
- **Acting.** `character` has 18 expressions, timed `expression_changes`, looks and reactions (take, shrug, nod, shake head, laugh); new `emote` block with 13 painted marks.
- **Self-review.** Tools `video_strip`, `video_crop`, `frame_grid`, `video_diff` (CLI `inspect`); `review_video` adds RV013 to RV019 (flash safety, beat, legibility, safe zones, density, repetition, ending) and an overlap pass; playbook topic `review`.
- **External programs.** `get_capabilities` / `opendesigner doctor`, `transcribe` (the user's whisper program, word timings into captions) and `remove_background` (the user's rembg); nothing is bundled.
- **Time feel.** Eases `steps_N`, power, expo, circ, sine; `Video.hold_frames` (animate on twos); stagger from edges and in a grid; `camera_burst`; an export `loudness` target (-14, -16, -23 LUFS).
- Project format is now version 5.

## 2026-11-25

### feat(agent): flat-vector scenes: characters, scenery, motion presets, an explainer playbook and review_video

**Area:** `feat(agent)`, `feat(cli)`

- **Blocks.** 13 character and prop blocks (`character` with a named rig, idle loops and actions wave, talk, walk, jump, point and celebrate; speech and thought bubbles; rocket, lightbulb, atom, planet, tree, house, 2D laptop, gear, cell, DNA) and 12 scenery blocks (`sky_gradient`, `parallax_hills`, `starfield`, `clouds`, `sun_glow`, `ocean_waves`, `city_skyline`, `cells_backdrop`, `particles`, `grid_floor`, `connector`, `highlight_ring`).
- **Motion presets.** `animate_preset` takes scene moves on any node (`float`, `pop_in`, `bounce_in`, `draw_on`, `orbit`, `stagger`, `animate_along_path`, ...) and camera moves (`camera_push_in`, `camera_pan`, `camera_drift`, `camera_shake`, `camera_whip`).
- **Direction.** The `explainer` playbook, routing that sends any animation request to it, and a read-only `review_video` tool (score, findings RV001 to RV012) with lint rules OD090 and OD091 for image-backdrop and text-only scenes.
- CLI: `opendesigner blocks add --box x,y,w,h`.

## 2026-11-24

### feat(core,render): the engine for flat-vector 2D animation

**Area:** `feat(core)`, `feat(render)`, `feat(agent)`

- **Vector.** A `path` node (SVG path data, validated and capped), linear and radial gradients on every shape, stroke caps, joins, dashes and draw-on trim, `import_svg_vector` (an SVG becomes one undoable group of named shapes).
- **Transform and motion.** Rotation, scale and pivot on every node, blur, drop shadow, clip masks, a parallax camera, looping and ping-pong keyframes, and more eases (`elastic`, `bounce`, `ease_out_back`, `ease_in_out_cubic`).
- Animatable: `rotation`, `scale`, `blur`, shadow, camera, `trim_start`, `trim_end`, `stroke_width`, `fill_color`.
- Project format is now version 4; older projects load unchanged.

## 2026-11-23

### feat(agent): four more sound packs

**Area:** `feat(agent)`

Kenney Sci-fi Sounds, Digital Audio, RPG Audio and Music Jingles (all CC0, about 9 MB together) join the downloadable packs; installed only after the user's yes, like the others.

### fix(agent): sounds differ between videos

**Area:** `fix(agent)`

`make_sound` defaulted to seed 1 and most kinds ignored the seed, so every video got the same sounds. An omitted seed is now random (returned, kept in the sidecar), and a seed varies pitch and beat tempo when none is given. The motion playbook tells agents to vary sounds and to use sound packs and Freesound.

## 2026-11-22

### fix(agent): memory and chat context no longer bleed between workspaces

**Area:** `fix(agent)`, `fix(app)`

Agent notes are kept per workspace (`.opendesigner/workspaces/<name>/memory/`) instead of once per repository, and the in-app chat starts the agent in the workspace's own folder. Notes written before this change stay with the legacy `main` project.

## 2026-11-21

### feat: close the gap to Hyperframes: blocks, captions, multi-track audio, motion, lint, variables, faster preview and export

**Area:** `feat(agent)`, `feat(core)`, `feat(render)`, `feat(media)`, `feat(app)`, `feat(cli)`, `feat(build)`

- **Blocks.** 31 ready-made titles, overlays, backgrounds, charts and motion wrappers (`list_blocks`, `get_block`, `add_block`; CLI `blocks`) that expand into ordinary nodes and keyframes in one undoable transaction.
- **Audio and captions.** Up to 8 sound tracks with ducking, waveforms, pause detection and removal; word-timed captions as data (SRT/VTT/JSON import, four styles) drawn in preview and export.
- **Motion.** Frame-aligned time, clip speed and speed ramps, colour grading and looks, eight more transitions, and a parallel video export (about 4x faster on an M2 Pro).
- **Agent safety and templates.** `validate_document` with stable `OD` rule codes, `dry_run` and receipts on `apply_ops`, typed template variables, `batch_export`, and a Claude Code / Codex plugin that ships the skill.
- **Editor.** Raw-pixel canvas, tile-wise redraw, node index, signature-keyed preview cache, preview sound through Qt Multimedia; Linux and Windows CI jobs for the Qt shell (unverified until they run).
- Project format is now version 3; every addition is an optional field, so older projects load unchanged.

## 2026-11-20

### feat(app): canvas overhaul, Variants and Preview panels, timeline, menu bar, chat fixes

**Area:** `feat(app)`, `feat(core)`, `feat(agent)`, `fix(app)`

**Change:**
- **Chat:** fixed `Required property modelData was not initialized` (the model/effort/mode selectors repeated a `required property` inside the delegate). The slash-command list closes with Esc, its close button or a click elsewhere, and Up/Down/Tab/Enter work in it. A slash command is now sent to the agent exactly as typed: before, the first message of a session carried the preamble in front of it, so the agent saw plain text instead of a command.
- **Variants and Preview panels** (`variants`, `preview`): options the agent offers appear as cards you pick from, not as frames on the canvas; the Preview shows the end product clean, chosen by the agent. New MCP tools `offer_variants`, `get_variants`, `clear_variants`, `show_preview`. Layouts saved earlier get the two panels hidden.
- **Canvas:** a toolbar inside the canvas panel (tools, overlays, snapping, rulers, zoom, arrange); rectangle, ellipse, text and frame tools; multi-selection (Shift-click, marquee), align, distribute, stack, duplicate, group, ungroup, copy and paste (`od_core::arrange`); snapping guides, rulers, pixel grid, right-click menu; the inspector shows several selected nodes. The window toolbar keeps only global controls and the old "more" menu is gone.
- **Menu bar:** the native application menu bar (File, Edit, Insert, Arrange, View, Agent, Window, Help); every Ctrl/Cmd shortcut moved into it.
- **Video:** a new editor-style timeline (transport bar, ruler, playhead across tracks, zoom, labelled tracks, details of the picked clip).
- **Agent and video:** a video now starts with an inventory (`list_media`, `list_footage`, `list_sounds`, `search_assets`, `list_asset_sources`) and uses bundled icons, generated backgrounds and pack art through image nodes in clip and overlay frames; the skill, the `video` playbook, the chat's first message and the asset tool descriptions say so, and a test pins every tool name the guidance uses.

**Verified:** unit and integration tests in every crate, clippy `-D warnings`, the offscreen window session; the menu bar's items read back from macOS System Events. **Not verified:** how it looks or feels by eye (canvas gestures, timeline interaction, panels), text-field copy/paste with the menu shortcuts, Windows and Linux.

### feat(render): devices in 3D, keyframes over a video's time, and motion blur

**Area:** `feat(core)`, `feat(render)`, `feat(mcp)`, `feat(cli)`, `feat(app)`, `feat(skill)`

**Why:** the user asked for 3D animations the agent can drive: a 3D iPhone with an image or a video from the iOS simulator on its screen, a monitor with a web page, an Android phone, and the agent moving those things in a video.

**Change:** a new node kind `device3d` (`crates/core/src/device3d.rs`) is a phone, tablet, laptop or monitor drawn in perspective inside its own box, with a capture or a recording on its screen, turned (`rotate_x`, `rotate_y`, `rotate_z`), lit (`light_angle`, `shadow`, `glare`) and lensed (`fov`, `zoom`); the models are generated from parameters, so none is a manufacturer's artwork. `render` draws them with a deterministic CPU rasterizer (`od_render::three_d`: depth buffer, supersampled anti-aliasing, a studio environment for the reflections, a soft shadow), so the canvas and the export are the same picture. A video may now **keyframe** any number of a node over its own time (`Video.animations`, the operation `animate`, `Ease::Spring`, `Video.motion_blur` blending the moments across a half-frame shutter); the named moves (`spin`, `turntable`, `swing`, `flip_in`, `hero_reveal`, `rise_in`, `drop_in`, `float`, `push_in`, `pull_back`, `orbit`, `scroll_screen`, `fade_in`, `fade_out`, `slide_in_left`, `slide_in_right`) are written as ordinary keyframes by the MCP tool `animate_preset` and the CLI `animate`, so a move is data the user can edit. New tools `animate_preset` and `video_contact_sheet` (several moments in one image, the way to judge a move); `list_videos` now carries the animations, the motion blur and a sentence for each move. The project format is version 2 (only new optional fields; version 1 projects load unchanged). A new `3d` playbook teaches the capture commands (`xcrun simctl`, `adb`, a headless browser), the pose that reads as a product shot and the moves.

**Verified:** unit tests for the model, the bounds, the interpolation and the overshoot, the operations (undo, redo, refusal to delete an animated node), the round trip through a project folder and a version 1 project; renderer tests for a keyframed node and for motion blur (a smear only while something moves, a still moment untouched); MCP tests that make a 3D phone, animate it, read the story back and draw a contact sheet; a CLI test that prints a device, applies it and animates it. **Not verified:** how the devices look (no one looked at the pictures); the renderer's speed on the reference machine; the app's new inspector fields and the video lanes by eye.

**PR:** n/a

## 2026-11-19

### feat(app): every Direction section has "Let the agent decide"

**Area:** `feat(app)`

**Change:** Look, Colors, Type, Mood, Motion, Layout and Imagery each end with a "Let the agent decide" button that clears that section (it shows as chosen when nothing is picked). The picker no longer pre-selects a default in each section, so an untouched section adds nothing to the prompt.

**Verified:** the window session passes with no QML warning. **Not verified:** by eye.
### feat(agent): logo design: a playbook, `import_svg` and `export_icon_set`

**Area:** `feat(agent)`

**Why:** "need logo design capabilities too, use kaankiziltug/logo-design-skill". **Change:** a `logo` playbook (also a skill reference) paraphrases that MIT-licensed skill's method for our tools: brief, category clichés, word map, 8 to 12 one-line concepts, three built in SVG black first, test and refine twice, a concept checkpoint, then the kit; plus SVG construction rules, optical corrections, the test list and red flags. Two tools: `import_svg` (checks an agent-written SVG for live text, embedded pictures, filters, many colors, gradients, no viewBox; draws it crisp into `media/`, keeps the master) and `export_icon_set` (11 PNG sizes from 1024 to 16 into `exports/<name>-icons/`, opaque with a background, optional padding). The skill's Python tools and its 1,400-logo library are not bundled (size, and the editor has its own preview).

**Verified:** unit tests for the audit and for import then export (sizes, refusals), the playbook tool-name test. **Not verified:** that an agent produces a good logo with it; the PNG sizes by eye; no `.ico` or web manifest is written.

**PR:** n/a

### feat(mcp): audio in import_file, a built-in sound maker, and the agent is told what it has

**Area:** `feat(mcp)`, `feat(skill)`, `feat(app)`

**Why:** An agent without a Freesound token gave up on sound (it thought of ffmpeg, then doubted that `import_file` takes audio), and never knew about the `minimax-mcp` server the user added. **Change:** `find_files` and `import_file` take audio (`.mp3 .wav .m4a .aac .ogg`, first bytes checked); `list_sounds` lists all audio in `media/` with length and source; new `make_sound` (`crates/agent/src/synth.rs`: click, pop, tick, whoosh, swoosh up/down, riser, hit, chime, drone, beat, stinger; 16-bit 44.1 kHz WAV, normalized, faded, deterministic per seed, no licence); the chat's first message and `get_guidance` (`extra_mcp_servers`, names only) tell the agent about the user's MCP servers and the sound tools; the video and motion playbooks get a *Sound* section and the quality bar a line on sound.

**Verified:** unit tests for every kind (length, peak at most 0.89, non-silent, exact zero at both ends, determinism, loop seam of the drone, kick on every beat, WAV header), audio sniffing and import rules, the server hints; an MCP test that imports a wav, makes a sound, sets it as the track and exports an MP4 with a real ffmpeg and asserts an audio stream; `cargo test -p opendesigner-agent`. **Not verified:** how any sound *sounds* (nobody listened; only waveform properties were checked numerically); the minimax-mcp hint against a real MiniMax server.
### feat(app): a toggle for the editor overlays, foldable layers, vector icons

**Area:** `feat(app)`

**Why:** The agent outlines and selection chrome hid the design; the toolbar repeated the workspace name beside the picker; layers could not be folded; icons were emoji and Unicode glyphs. **Change:** a toolbar toggle (and `⇧H`, listed in the shortcuts dialog) hides the selection, agent outlines, pins and markers for the session; the workspace name next to the logo is gone (the window title keeps it); layers with children fold, with Expand all / Collapse all, and selecting on the canvas unfolds the ancestors; all icons are vendored Lucide SVGs, copied unchanged from `lucide-static` 1.76.0 (`qml/icons/`, ISC licence, `Icon.qml`, `Icons.qml`), including Undo, Redo, Select, Hand, Note, zoom, Fit, Layout, Export, the more menu, layer eye and kinds, chat options, send and the video panel marks; AGENTS.md gains the icon rule.

**Verified:** `cargo fmt --check`, workspace clippy `-D warnings`, `cargo test`, `cargo deny check`, app clippy and `cargo test -p opendesigner-app` (the offscreen window session, extended to fold a layer and check the row count, to select and see the ancestors unfold, to toggle the overlays and to load the icons, ends with no QML warning). **Not verified:** how any of it looks (no one looked at the window), the overlay state is not saved in the layout file.
### feat(agent): assets for the agent: icons, generated blobs and backgrounds, downloadable packs

**Area:** `feat(agent)`, `feat(app)`

**Why:** The agent had no assets to work with and drew every icon by hand. **Change:** seven MCP tools (`list_asset_sources`, `search_assets`, `make_asset`, `import_asset`, `list_asset_packs`, `install_asset_pack`, `unload_asset_pack`); Lucide (2,404 icons) bundled as one 149,898-byte compressed container; procedural blob, gradient, aurora, grain, pattern and shape generators (deterministic per seed); SVG recolored and rasterized to PNG with `resvg` at the size asked for; packs (Tabler, Phosphor, Heroicons, Iconoir, Bootstrap Icons, seven Kenney CC0 packs) downloaded only after the user clicks Download in the Connect tab, stored as one compressed file each in the config folder with their hash and size, unloadable; a sidecar names source, licence and credit for every imported asset; a "Recommended servers" list (3dassets, sozai, asset-gate) with one-click Add; an Assets section in the design playbook and one line in the quality bar. `http.rs` gained an optional `Referer`; the agent crate depends on `resvg` (13 new small crates; decision logged) and `flate2` directly (already in the tree).

**Verified:** unit tests (containers, zip and tar readers, recolor, rasterize, every generator and variant draws, determinism per seed, search ranking) and `tests/assets.rs` against a local mock server (request, approval, install, size accounting equals the file's length, search, import with sidecar, second import never overwrites, sounds copied as is, unload, failed downloads); one real run by hand of `install_asset_pack` for Kenney Interface Sounds (834,536 B zip, 100 files, 542,348 B stored), Kenney Board Game Icons (1,039,954 B, 255 SVGs, 56,416 B stored) and Heroicons (166,699 B, 324 icons, 30,788 B stored) from kenney.nl and registry.npmjs.org, then a search and an import at 128 px recolored (`cargo test -p opendesigner-agent --test assets -- --ignored`). **Size added (measured):** the repository gains `lucide.oda` 149,898 B + its licence 3,208 B; a release build of the CLI grows from 13,481,888 B to 14,864,288 B (+1,382,400 B; gzip -9: 5,231,438 B to 5,877,684 B, +646,246 B) with the bundled set and `resvg` linked in; packs add nothing until the user downloads them (0.17 to 2.0 MB each, 256 MB cap). **Not verified:** the Connect tab section by eye; the other packs' downloads (only the three above were fetched); the recommended servers were not started; kitbitz.art is not used (terms and download interface unreadable); 3D models cannot be placed.

**PR:** n/a

### fix(app): the start screen's model and effort selectors work

**Area:** `fix(app)`

**Why:** They stayed grayed out on "Reading the models…": the property carrying the answer (`agentProbe`) was declared without its QML name, so the window never saw it. **Change:** the property is exposed correctly; the probe no longer gives the agent an editor endpoint that does not exist yet, and gives up with a message after 45 s.

**Verified:** the real `omp acp` answers the probe in about 2 s from a test (mode, model and effort offered); found the cause by logging the running app (the property read `undefined`). **Not verified:** the selectors after the fix, by eye.

**PR:** n/a

### feat(app): the start screen has a model and an effort selector

**Area:** `feat(app)`

**Why:** The model and effort could only be changed after connecting. **Change:** on the start screen, once an agent is chosen, the app starts it briefly to read the models and efforts it offers (a spinner and a Refresh button), lists them next to the agent, and applies the picks as soon as the real session is up, before the first task is sent. Agents that offer neither say so.

**Verified:** the app builds, clippy and the window session pass (the session's agent offers no settings, so the selectors' content was not driven). **Not verified:** the probe against a real agent from the window (the same ACP call was read from omp by hand earlier), the selectors by eye, and that every agent honours a setting made right after `session/new`.

**PR:** n/a

### fix(app): /model, /effort and /mode open a chooser instead of being sent to the model

**Area:** `fix(app)`

**Why:** Typing `/model` sent it to the LLM as text. **Change:** commands that are settings of the session (`/model`, `/switch`, `/effort`, `/thinking`, `/mode`, `/plan`) open a list of the agent's real choices (the current one marked) and change the setting; other commands still go to the agent. The `/` button is now a settings button (⚙) with a menu of Model / Effort / Mode and the agent's command list.

**Verified:** the window session passes with no QML warning. **Not verified:** by eye, and against a live agent; the choice list is fed by the same settings the row above the chat already shows.

**PR:** n/a

### fix(app): modal dialogs no longer turn the window white, and the Direction picker and slash-command list behave

**Area:** `fix(app)`

**Why:** Screenshots: opening the Direction picker (or any dialog) turned the whole window white, the picker sat in the wrong place and was cut off, its Look / Mood / Imagery picks did not light up when clicked, and the slash-command list was drawn at the top of the window instead of above the message field. **Change:** every dialog now dims with an explicit dark layer (the style's default dim was white here); the picker is centered in the window and sized by it, replaces its pick lists instead of editing them in place (so buttons update), and shows each palette's colors; the command list is anchored above the message field, sized to its rows, with hover.

**Verified:** the window session still passes with no QML warning (the earlier `Dialog.qml` type error from the picker's old sizing is gone). **Not verified:** by eye, which is the whole point: I could not grab the overlay offscreen, so the white dim, the picker's position and the command list were fixed from the screenshots and the code, not seen after the change.

**PR:** n/a

### feat(app): the chat can call the MCP servers the user adds, so the agent reaches MiniMax's speech models

**Area:** `feat(acp)`, `feat(app)`

**Why:** "the agent should be able to reach other MCP servers, specifically minimax-mcp (MiniMax speech models: text to speech and speech to text), so it can make a voice-over or transcribe footage". The chat offered the agent exactly one MCP server: our own editor server. **Change:** (1) The Connect tab has a list of MCP servers the user adds: a name, the command that starts it, an optional working directory and optional environment variables (`KEY=value` per line, for example `MINIMAX_API_KEY`). They are kept in the app's config folder (`mcp-servers.json`, readable by the user only, because a value may be a key) and are written only by the user: no MCP tool touches the list, and an agent cannot. (2) The ACP `session/new` now carries them after the editor's own server, in the ACP stdio form (`{"type": "stdio", "name", "command", "args", "env": [{"name", "value"}]}`). The client asks for the ACP `notice` updates, so an agent that could not start a server says so, the window shows it in the chat and the session carries on without that server. The window also explains what these servers are and how to install `minimax-mcp`: the user installs it, the app never ships or starts a third-party server.

**Verified:** ACP tests (the user's servers are sent in the stdio form with the editor's server first; a notice from the agent is reported and the session goes on), config tests (the list round-trips, a bad name or command is refused, a variable keeps the value that is stored), app tests, window session (the agent is given the server the session added). **Not verified:** against a real `minimax-mcp` server (it was not installed on this machine) or a real agent (only the scripted one); the Connect tab by eye.

**PR:** n/a

### fix(app): a frame with no background is transparent, not a white screen

**Area:** `fix(app)`, `fix(mcp)`, `fix(render)`

**Why:** "its a white screen even after a export": the design frames the agent made for the video had no background (they sit over the footage), and the editor flattened them onto white, so the canvas showed a white screen and the exported PNG was white too. **Change:** The canvas keeps the picture's alpha and draws a checkerboard behind it, so a transparent frame reads as empty rather than broken. The export says how much of the frame is transparent and whether the file is white or see-through there, and the export dialog has a "keep the transparency" option.

**Verified:** a render test (a frame with no fill is fully transparent, one with a fill is not, the checkerboard encodes), an MCP test (the export of a frame with no background warns, a filled frame does not), the window session. **Not verified:** by eye (the canvas was never looked at); the export with alpha was not written and inspected.
### feat(agent): videos carry a sound track, and the agent finds music and sound effects on Freesound

**Area:** `feat(core)`, `feat(agent)`, `feat(mcp)`, `feat(app)`, `feat(media)`

**Why:** "videos need audio": the user wants the agent to be able to put music and sound effects on the videos it makes, and to be able to find them (Freesound, https://freesound.org/). **Change:** (1) A video carries at most one sound track (`Video.audio {file: media/<name>.mp3|wav|m4a|aac|ogg, start_ms (0-600,000), gain (0-2), fade_in_ms, fade_out_ms}`), set with `set_video` (`prop {name: "audio", value: {...}}`, `null` takes it away) as an attributed, undoable, checkpointed edit; `list_videos` shows it and the `story` mentions it. The export mixes it under the picture with ffmpeg (`volume`, `afade`, `adelay`, `amix`); the video's own length rules, so the sound never lengthens it. (2) `od_agent::freesound` talks to the Freesound API v2 over a small HTTP/1.1 client written in-house (`od_agent::http`: one `GET`, timeouts, a cap on the answer's size, and a redirect that may only stay on the same site) with `rustls` for the secure channel; the MCP tools `search_sounds`, `import_sound` (the preview mp3 into `media/` with a `media/<name>.freesound.json` sidecar) and `list_sounds` bring sounds in and say which license and attribution the user must show. The user's free Freesound token is pasted into the Connect tab, stored in the config folder and sent only to freesound.org; without it the tools say what to do. (3) The Video tab shows the sound track and can take it away, and offers to ask the agent for a sound.

**Verified:** core tests (the sound track is set, undone, bounded and survives saving and loading), the Freesound client and its tools against a local server (search parsing, the token only in the query string, the missing token's message, the import with its sidecar, a free name instead of an overwrite, a missing sound, an answer that is too big), an export test (a video with a sound track exports to an MP4 that has an audio stream and keeps its length), app state tests (the token is stored and removed; the user can take the track off a video). **Not verified:** anything against the real Freesound service (the API was read from its documentation on 2026-11-20; the paths, the field names and the preview host were checked by hand, but no call was ever made with a real account); the token flow with a real account; the sound in an exported video by ear; the Connect and Video panels by eye.

**PR:** n/a

### feat(app): the chat can change the agent's model and effort, run its commands, and starts from a direction the user picks

**Area:** `feat(acp)`, `feat(app)`, `feat(agent)`

**Why:** "the chat has no way to change models or effort", "add a button with a full selection menu (styles, color palettes) that injects into the prompt", and the agent's words could not be copied. **Change:** (1) The ACP client reads the settings the agent offers (`configOptions`, or the older `modes`/`models`), shows them in the chat (model, mode, effort) and changes them (`session/set_config_option`, `session/set_mode`, `session/set_model`); the agent's slash commands are listed behind `/` and typed into the message. (2) A **Direction** button (chat and start screen) opens a picker of looks, palettes, type pairings, mood, motion, layout and imagery, plus free text and three working switches (look for skills first, show three directions first, remember the choices); the picks become a block in the prompt. (3) The agent's messages are text edits, so they can be selected and copied.

**Verified:** ACP tests (settings and commands arrive, a choice can be changed, a refused setting is reported, an agent with only modes/models gets the same controls), direction tests (ids unique, the looks match the style playbook, the picks become instructions, typed colors beat a listed palette), window session. **Not verified:** against real agents by hand (the protocol was read from omp's own answers over ACP: 3 settings and 116 commands); the picker by eye; whether every agent supports the calls.
### feat(app): Play in the Video tab really plays, from a background preview cache

**Area:** `feat(app)`

**Why:** Play drew one frame per 100 ms tick on the UI thread (footage frames need ffmpeg seeks), so the picture stalled and did not advance. **Change:** a worker thread draws the whole video small (longest side <= 360 px, <= 15 fps or the video's rate) from a detached copy of the project, streaming footage and drawing held pictures once, and keeps PNGs in memory (cap about 256 MiB; the panel says when capped). The panel shows "Preparing preview N%", starts playing once a second is drawn, follows the clock, shows "Buffering" when it catches up, loops at the end, and cancels and redraws when the project changes, another video is chosen or the panel closes. New: `od_agent::video_preview`, invokables `previewStart/previewStatus/previewFrame/previewStop`. The preview is silent; export has the sound.

**Verified:** unit tests in `od_agent::video_preview` for the plan (size, rate, frame count, moment to picture), a held picture drawn once, different pictures during a fade, cancel, the memory cap, an empty and a missing video, and footage through ffmpeg (Homebrew ffmpeg 9.x, skipped without it); a test in `crates/app/src/state.rs` for starting, the progress, going stale after an edit, restarting and stopping; `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo deny check`, `cargo clippy -p opendesigner-app --all-targets -- -D warnings` and `cargo test -p opendesigner-app` (the offscreen window session, with a new step that asks the panel's preview for a status, a start and a picture: all pass, no QML warning). **Not verified:** the Video tab with a real video in a window (the session has no video to play, so nothing drove Play, the progress bar, the buffering state or the loop in a window), the smoothness on screen, a long video at the memory cap, and Windows and Linux (only macOS was run).
### feat(mcp): agent memory in `.opendesigner/memory/`

**Area:** `feat(mcp)`

**Why:** Agents began every task blank. **Change:** `od_agent::memory` keeps small text notes in `<repo>/.opendesigner/memory/` (separate from the asset folder `opendesigner/`, meant to be tracked in git): plain names, no symlinks, 64 KB per note, 200 notes, 2 MB total, atomic writes. New tools `list_memory`, `read_memory`, `write_memory`; `get_guidance` returns the index; the quality bar and the design and video playbooks tell agents to read notes before building and write lessons back (never secrets). The app creates the folder with a README when a working folder opens and lists the notes in the Memory panel. `AGENTS.md` invariant 5 names the new write surface.

**Verified:** unit tests for names, traversal, symlinks, caps, atomic replace; MCP tests (write, append, list, read, traversal and oversize refused, guidance index, no-folder error); playbook test for the new tool names; the app test (the folder appears when a working folder opens, the notes are listed) and the offscreen window session. **Not verified:** that agents actually read and write notes well on real tasks; Windows (the symlink test is unix only).

**PR:** n/a

### feat(mcp): exports tell the agent when a frame was never reviewed or changed since

**Area:** `feat(mcp)`

**Why:** The quality bar asks agents to check their work; an agent that skips it should be told at the moment it exports. **Change:** `export`, `export_set` and `export_panorama` add `before_you_call_it_done` when a frame was never checked with `review_frame` or anything inside it changed afterwards. The export still goes through.

**Verified:** MCP test (never reviewed, reviewed, unrelated change, change inside the frame, set export). **Not verified:** whether agents act on the note; video exports do not carry it (their export runs on a detached copy).

**PR:** n/a

### feat(app): a fully rearrangeable layout: tabs, splits, windows of their own, hidden panels

**Area:** `feat(app)`

**Why:** The window had one fixed arrangement. **Change:** Layers, canvas, inspector, notes, history, memory, video, connect and chat are separate panels (the agent panel's tabs are now panels of their own) that can sit side by side, stacked, as tabs, in a floating window, or hidden. Drag a tab onto another panel's middle (join its tabs) or edge (split next to it), or off the window (its own window); right-click a tab or use `⋯` for the same by menu; drag the line between panels to resize; toolbar **Layout** shows/hides panels and resets. Closing a floating window puts its panels back. The arrangement is saved in the config folder and restored. The rules are in `crates/app/src/layout.rs` (every operation checked; a refused one changes nothing).

**Verified:** 10 unit tests of the model (joining tabs, edge splits and merging, refusals, floating, the main window keeping a panel, hide/show/reveal, weights, save/load with a bad file, 600 random operations all leaving it valid) and a window session that floats, splits, docks, hides, reveals, closes and resets panels with no QML warning. **Not verified:** the dragging itself (tabs, divider, dropping outside to float) and the floating windows by hand or by eye; keyboard shortcuts inside floating windows (they have none); behavior on a second monitor.

**PR:** n/a

### feat(agent): every agent gets the quality bar, playbooks, a design review tool and optional formats

**Area:** `feat(mcp)`, `feat(app)`, `feat(skill)`

**Why:** "Our agent output is ass": agents did not use our guidance unless the skill was installed and loaded, and the skill had no craft in it. **Change:** (1) The quality bar is sent with the MCP `initialize` instructions and the in-app chat's first prompt, so every agent gets it with or without the skill. (2) `get_playbook` serves five playbooks (design, style: eight named looks, video, motion, formats with safe zones) that are also `references/` in the installed skill; the skill description now covers video and motion. They are rewritten from public skills and platform pages (listed in the wiki). (3) `review_frame`: contrast, small or edge-hugging text, overlaps, overflow, misalignment, fonts, loose colors, size vs. the chosen format. (4) Formats: sections (Video, Store images, Social images, Web and icons) plus a custom size, chosen when starting or from the toolbar, or left to the agent (`list_formats`, `get_brief`, `set_format`); a format the user chose is enforced for videos and changes only through a request shown as Allow / Keep mine.

**Verified:** unit tests (WCAG contrast, each review finding, the approval state machine, formats vs. store preset sizes, playbooks name only real tools), MCP tests (instructions carry the bar, guidance carries the brief, playbooks readable, review finds low contrast, a chosen video size is refused and a change waits for the answer), app test (choose, ask, answer, hand back), window session. **Not verified:** that agents behave better (no model turn compared before and after; only that the text reaches them), the format dialog and approval banner by eye, the safe-zone numbers beyond the pages they came from.

**PR:** n/a

### feat(video): motion that is worth watching, and a timeline that shows what follows what

**Area:** `feat(core)`, `feat(render)`, `feat(app)`, `feat(mcp)`

**Why:** Videos were fades and linear slides over still frames; the user could not see the order of things from the workspace. **Change:** Easing (`linear`, `ease_in`, `ease_out`, `ease_in_out`, `back`) on transitions, zooms and overlays; transitions `slide_up`, `slide_down`, `dip` (through black), `zoom_in`; overlays enter and leave with `fade`, `slide_*`, `pop` or `zoom`. `list_videos` returns a `story` (one sentence per clip and overlay). The Video tab is now a timeline: ruler, clip lane with the overlapped stretch of every transition, stacked overlay lanes, a playhead you can click or drag, the preview and Play, and the same story in words.

**Verified:** core and render tests (ease curves incl. overshoot, slide_up/dip/pop poses at known times, overlay entrance), an MCP test of the story text, window session with the new panel. **Not verified:** the timeline by eye, the preview at playback speed (it is still a stepped still image), real footage with the new transitions.

**PR:** n/a

### fix(app): chat while the agent works, always allow, scrollable panels, and a 20x faster export in dev builds

**Area:** `fix(app)`, `perf(render)`, `build(repo)`

**Change:** (1) You can type while the agent is working: the message is queued (shown "queued"), sent when the turn ends, and dropped if you press Cancel. (2) "Always allow" in the chat answers the agent's permission questions with Allow (always, else once; never a rejection) and notes each one. (3) The Connect and Inspector panels always leave room below their last control (a layout cannot know the height of wrapped text at its final width), and the start panel scrolls. (4) Export: a debug build of the pixel code was about 30 times slower than release, so a 5.4 s 886x1920 video took 3.5 minutes; the rendering and media crates and all dependencies are now optimized in dev builds, and while nothing moves (no zoom, transition or fade) the export draws a frame once and re-sends it. Same video: 11 s in a dev build.

**Verified:** chat unit and scripted-agent tests (queue order, no duplicate, always-allow answers waiting and later questions, `best_allow`), a render test for the held-frame signature, the 5.4 s export timed before and after, window session. **Not verified:** the panels by eye (only reasoned: layouts underestimate wrapped text; the extra room is a safety margin, not a measured fix), footage exports (every frame differs there, so only the build speed helps).

**PR:** n/a

### feat(app): workspaces: several deliverables side by side in one repository

**Area:** `feat(mcp)`, `feat(app)`

**Why:** An agent asked for an app store asset and a reel put both on one page. **Change:** A repository can hold several workspaces, each its own project folder under `opendesigner/workspaces/<name>/` (pages, videos, history, notes, media); a project from before sits in `opendesigner/` and is listed as `main`. The agent gets `list_workspaces`, `create_workspace` and `open_workspace` (the skill says: one per deliverable); the start screen and the toolbar have a workspace picker and "New workspace…"; an agent's switch is announced and the window follows it (view, language and outlines reset). Switching swaps the workspace behind the running connection, so the agent and the chat stay connected.

**Verified:** agent tests (separate frames per workspace, empty new one, listing, switch announced, bad and duplicate names refused), app tests (open/create/recent, switching keeps each one's work), window session. **Not verified:** the pickers by hand; a real agent choosing to split work into workspaces. Media is per workspace, so the same picture added in two workspaces is two copies.

**PR:** n/a

### fix(video): the window no longer freezes during a video export; progress bars and spinners; agents can use pictures from anywhere

**Area:** `fix(mcp)`, `feat(app)`, `feat(mcp)`

**Bug:** The agent's `export_video` call held the editor's state for the whole export (minutes), and the window reads that same state, so it hung until the export ended. **Fix:** the export now works on a copy of the project, reports progress, and can be stopped; a strip at the bottom-left of the window shows a bar with a percentage and a Stop button. Copying a large video into the agent's memory now happens on a thread with a spinner, the chat shows a spinner while the agent works or starts.

**New:** `find_files` and `import_file` let the agent search the home folder for pictures and videos and copy one into the project (a switch in the Connect tab turns it off). Invariant 5 is reworded and the decision and risks are written down (security.md, roadmap).

**Verified:** an MCP test with a real ffmpeg that the editor's state is free at every progress report, progress reaches the total, and a stopped export leaves no file; app tests for the job rows; device tools tests (copy, refuse a fake picture, refuse paths, search limited to home). **Not verified:** the bar and spinners by eye, a very long export, the Stop button by hand, the search over a real home folder.

**PR:** n/a

### feat(video): footage clips with sound, overlays, and a Video tab

**Area:** `feat(core)`, `feat(render)`, `feat(media)`, `feat(mcp)`, `feat(app)`

**Change:** The user imports video files into the agent's memory; the agent cuts them into a video (`add_clip` with `footage`), lays titles and call-outs over it (`add_overlay`: a transparent design frame that fades in and out) and exports an MP4 with the footage's sound mixed under the picture. New tool `list_footage`; `export_video` refuses a clip that runs past its file. The editor has a Video tab: preview, play, scrub and the clip list. The skill teaches the flow.

**Verified (macOS, Homebrew ffmpeg 9.0.1):** core tests (source rules, overlays, undo, history-only ops refused, footage naming/symlinks/sniffing), render tests with a fake footage source (in point, fades, timing), media tests (log parsing, paths), an MCP end-to-end with real ffmpeg that makes a 3 s test video with a tone, imports it, cuts two clips and an overlay and exports 200x400 10 fps: audio present, ~2 s, length error for a clip past the end; app tests (import to memory as footage, preview). **Not verified:** real screen recordings or phone footage (codecs, variable frame rate, rotation metadata, long files and speed), more than a few clips, the Video tab by hand, Windows/Linux. **Not built:** animated overlays, volume/music, speed, captions from speech, notes pinned to a time range, clip selection on a timeline.

**PR:** n/a

### feat(app): the agent's memory tab; the skill learns from public agent skills

**Area:** `feat(app)`, `feat(skill)`

**Change:** A Memory tab lists the pictures in the project's `media/` folder with thumbnails and whether the design uses them; "Add pictures" (or dropping on the window) copies files there for the agent (`list_media`). The generated skill gained "You decide the shape" (the agent names and sizes everything), a subject-and-taste section and a video section. Sources read on 2026-11-19: anthropics/skills `frontend-design` (ground in the subject, avoid generated-art tells), ParthJadhav/app-store-screenshots (screenshots are ads; copy rules; deck arc; thumbnail test), and the video-editing skills affaan-m/ECC `video-editing`, kajisho5/ffmpeg-skill and arhen/motion-film (probe first, keep originals, plan-run-check, one motion language, never invent captions). Advice was paraphrased, no text copied.

**Verified:** an app test for the memory list; skill tests pass. **Not verified:** the tab by hand; whether agents behave better with the new text (no agent turn was run on it). Footage (video files) cannot be imported yet: `media/` takes pictures only.

**PR:** n/a

### feat(app): start screen: choose the working folder, the agent and the first task

**Area:** `feat(app)`

**Change:** The launch screen (and "Open another project") now asks where you are working (usually a repository), which agent to start (installed ones marked) and what it should make first. The agent starts in that folder, so it can run your app and take screenshots; the design lives in `opendesigner/` inside it, opened if present or set up empty (no frame is made: the agent decides). The task you type is sent once the agent is connected. Recent folders are remembered in the config folder. The "new project with a first frame and size" dialog is gone. When the agent makes the first frame, it becomes the open one.

**Verified:** app tests (inspect, refuse without a yes, create empty, reopen from folder or project, recent list, first frame adoption, the first task reaching the scripted agent) and the window session loading the panel with no QML warnings. **Not verified:** the panel by hand (folder dialog, clicks), a real agent receiving the task.

**PR:** n/a

### feat(ui): an agent-first window: pointing tools only, macOS shortcuts and gestures, drops go to the agent's memory

**Area:** `feat(ui)`

**Change:** The toolbar loses the rectangle, ellipse, text, frame, image and device tools and the colors/languages/open/save buttons (the last four are in a "⋯" menu): the agent creates things and decides their names and sizes; the user selects, moves, edits properties, pins notes and corrects. Added the Hand tool, a shortcut set taken from Figma and Penpot (`V` `H` `C`, `⇧1` fit, `⇧2` selection, `⌘0` 100%, `N` frames, `Tab` layers, arrows to nudge, `⌘]` `⌘[` order, `Return` to edit text, `⌘L` agent chat, `⇧⌘E`, `?` list) and a shortcut dialog, ⌘/⌥-scroll zoom. Files dropped on the window are copied to the project's `media/` folder (the agent's memory) and not placed on the design.

**Verified:** the window session loads the new QML with no warnings (it fails on any) and the existing scripted session passes; app unit tests pass. **Not verified:** the keys and gestures themselves (no synthesized key events; not tried on a real keyboard or trackpad here), Magic Mouse.

**PR:** n/a

### fix(ui): the canvas and the panels update without flashing or jumping

**Area:** `fix(ui)`

**Problem:** While an agent edited (or the user did), the canvas flashed, the layers list, chat and notes were rebuilt on every change (losing scroll position and flickering), and the view jumped when an edit changed the frame's size.

**Cause and fix:** (1) A QML `Image` given a new `source` drops its old picture while the new one loads (a `data:` URL always loads asynchronously), so the canvas went blank for a moment each redraw. The picture is now drawn by two images that take turns: the new drawing loads in the hidden one and replaces the other only when ready. (2) Every list was a fresh JavaScript array each time its JSON changed, which rebuilds every delegate. `JsonModel` (new) syncs an array into a `ListModel` and touches only the rows that changed, moved, appeared or went; layers, notes, activity, restore points, chat lines, plan, permission buttons, frame tabs and the canvas overlays use it. (3) The view now fits only when a different frame is opened, not when a frame is resized, and redraws are throttled (at most every 90 ms during a burst of changes) instead of debounced until the burst ends.

**Verified:** the window session now checks that a changed list builds only its new rows (changed and moved rows keep their delegates) and that the picture in front stays ready while a new one loads (three redraws); app tests pass on macOS. **Not verified:** by eye (no screenshots are used as evidence here); the user's report is the reference, so please say if any flash remains.

**PR:** n/a

### feat(video): videos of design frames, an ffmpeg export, Apple app-preview rules

**Area:** `feat(core)`, `feat(render)`, `feat(media)`, `feat(mcp)`, `feat(cli)`, `feat(store)`

**Change:** A video is a timeline of clips that show design frames, with transitions (cut, fade, slide) and a zoom, changed only through undoable operations (`create_video`, `add_clip`, `set_clip`, ...). `od_render::video` draws any moment; the new `crates/media` pipes frames to `ffmpeg` for an H.264 MP4 (OS encoder first). MCP: `list_videos`, `render_video_frame`, `export_video`, `list_video_presets`; CLI: `videos`, `export-video`, `video-presets`; the skill teaches it. `od_store::video` checks Apple's app-preview rules (read 2026-11-19). Invariant 10 is reworded and the decision logged: ffmpeg runs as a separate process.

**Verified (macOS arm64):** core tests (overlap maths, bounds, undo/redo, persistence, a clip on a missing frame refused to load), render tests (cut, fade, slide, zoom, cover-and-crop), media tests with a stand-in ffmpeg and with Homebrew's real ffmpeg 9.0.1, MCP and CLI end-to-end encodes, preset refusals before encoding, and a real 886x1920 30 fps export probed with ffprobe (h264, 162 frames). **Not verified:** other encoders (Media Foundation, NVENC, VAAPI), Windows/Linux, a bundled LGPL ffmpeg, playback in the editor (not built), audio (none is written), footage clips (not built).

### build(repo): a Linux build-and-test container; the ACP test no longer guesses where `target/` puts the mock agent

**Area:** `build(repo)`, `test(agent)`

**Change:** `scripts/test-linux.sh` builds `packaging/linux/Dockerfile` (Ubuntu 24.04, distribution Qt 6.4, lld) and runs the AGENTS.md checks inside it. Running it found two real problems, both fixed: the ACP tests located the mock agent by walking up from the test executable, which breaks with the newer Cargo `target/` layout (they now ask Cargo for the path), and the Qt shell did not link on Linux because the cxx-qt initializers were never referenced from Rust (`run()` now calls `init_crate!`/`init_qml_module!`; the container links with lld, which resolves archives in any order).

**Verified (Ubuntu 24.04 aarch64 in Docker, not Windows, not x86_64):** `cargo fmt --check`, `clippy -D warnings` and the 123 default-member tests pass; the editor crate builds and links and 24 of its 25 unit tests pass. **Not verified / failing:** `the_chat_runs_a_scripted_agent_through_the_editor_and_hands_it_the_local_server` fails there (not investigated), the window session was not reached, and nothing was run on a real Linux desktop. macOS stays the supported platform for now.

**PR:** n/a

### feat(app): first-run skill offer and the agent's plan in the chat

**Area:** `feat(app)`, `feat(agent)`

**Change:** On the first start with an agent that reads skills present and ours not installed, a dialog lists every file it would write and offers Install, Not now or Don't ask again (a marker in the config folder; nothing is written without a click). The chat shows ACP `plan` updates as a checklist (replaced whole by each update, cleared at the next user message). The session list stays unbuilt.

**Verified:** app tests (offer once, no agent nothing to offer, don't-ask-again installs nothing, installed is not offered), a chat test for plans, the ACP test and the window session (the mock agent's plan, the offer's API), and a real omp turn whose `todo` tool arrived as two `plan` updates. **Not verified:** the dialog itself (the window session drives the API, not clicks; it is not opened in the session), plans from Claude Code, Codex or Gemini, Windows.

**PR:** n/a

### feat(render): text that does not fit its box is detected and shown

**Area:** `feat(render)`, `feat(app)`, `feat(mcp)`

**Problem:** A longer translation or an agent's long headline could run past its box and nothing said so.

**Change:** `od_render::check_text` measures each text node's layout (the cached `cosmic-text` layout, no drawing) and reports `Warning::TextOverflow` when it wraps to more rows than the box is tall; `render_frame` includes it. The MCP preview note and the export tools, the CLI and the Export result carry it as `warnings` (the key `not_drawn` is renamed, since the text is drawn). The canvas outlines the box in red with the numbers; the inspector explains it; the skill text tells agents to fix it.

**Verified:** render tests (overflowing, wrapped word, fitting, unboxed text), an MCP test (preview note and export `warnings`), an app test (the snapshot lists the box and the numbers, and clears after resizing), and the window session. **Not verified:** boxes narrower than one glyph, right-to-left or vertical text, other fonts than the bundled Lato.

**PR:** n/a

### perf(render): frames with many nodes are drawn in parallel bands; laid-out text is cached

**Area:** `perf(render)`

**Problem:** Redrawing the canvas took 64 ms at 10,000 nodes (budget 16 ms) and an export at scale 1 took 231 ms.

**Change:** A frame of 256 or more nodes is drawn as up to 8 horizontal bands on `std::thread::scope` threads (4 rows of overlap per side so seams match), each band skipping nodes wholly outside it (a frame's subtree goes with it). Text layouts (shaping) are kept between renders in a 4096-entry cache keyed by text, size, family, weight, alignment and wrap width. No new dependency.

**Verified:** `cargo run --release -p opendesigner-render --example bench` on an M2 Pro: canvas at 10,000 nodes 64 ms to about 31-34 ms (still over the 16 ms budget), 1,000 nodes 9 ms, export at scale 1 and 10,000 nodes 230 ms to 133 ms. A test compares 1, 2, 3 and 8 bands on a busy frame (strokes, rounded corners, nested clipping frame, wrapped text across seams, nodes outside the frame) at three scales: not bit-identical (anti-aliased edge samples differ by float rounding: fewer than 0.02% of values, by at most 64/255). **Not verified:** other machines (the speed-up depends on core count; a 1-core machine renders in one band as before), Windows and Linux.

**PR:** n/a

### feat(agent): agent-agnostic connection (omp, opencode, Codex, Gemini, Cursor, any MCP client) and a real omp turn

**Area:** `feat(agent)`, `feat(app)`, `feat(cli)`

**Problem:** Connection help covered Claude Code only, the skill had no omp location, and the chat offered three presets.

**Change:** `od_agent::connect::recipes` gives the exact setup per agent (omp, Claude Code, Codex CLI, Gemini CLI, opencode, Cursor, any client); none contains the token. The Connect tab has an agent picker; `serve-mcp --agent <id>` prints one. New skill target `omp` (`~/.omp/agent/skills`). The chat offers `omp acp` and `opencode acp` besides the Claude, Codex and Gemini ones and marks the installed ones. `acp_probe --prompt=` runs a turn.

**Verified:** with omp 19.0.0 (signed in) a turn through the chat's ACP client called `get_guidance`, `list_pages` and `apply_ops` on a served project, created a frame and a text node, and the history attributes it to `agent:omp-coding-agent`; `omp -p` read the same server from a project `.omp/mcp.json`; `opencode mcp list` showed it connected and `opencode acp` opened a session; unit tests for the recipes; app tests and the window session. **Not verified:** a model turn with Claude Code, Codex or Gemini CLI (not signed in), Codex/Gemini/Cursor connecting to MCP, the omp skill location with a running omp, Windows shells.

**PR:** n/a

### test(agent): the chat's ACP setup checked against real agents; MCP checked with real Claude Code

**Area:** `test(agent)`, `docs(wiki)`

**Problem:** The earlier entries said no real agent was installed. That was wrong: I had not looked. Claude Code, Codex, Gemini CLI and opencode are installed on this machine (none is signed in).

**Change:** Added `examples/acp_probe` (starts a real ACP agent and runs the chat's setup, no prompt). Findings: the first run of `npx -y @agentclientprotocol/claude-agent-acp` takes over a minute (it downloads the adapter), so the 60-second setup timeout failed on a cold cache even though the adapter was fine; it is now 4 minutes. Real Claude Code 2.1.318 connected to `serve-mcp` over HTTP with the bearer token (`claude -p --mcp-config --strict-mcp-config --debug mcp`, which does not touch your Claude config): "Successfully connected", tools advertised, protocol 2025-06-18.

**Verified:** the probes above, run once each. **Not verified:** any prompt turn or MCP tool call by a real agent (all three agents' model calls fail with an expired sign-in), Codex and opencode against MCP, ACP v2.

**PR:** n/a

### docs(repo): status banners and README match what is built

**Area:** `docs(repo)`

**Change:** The roadmap, agent-integration, security and architecture banners and the README still described the editor, the chat and device frames as not built. They now say what exists and, in the same places, what has not been verified (no real agent, one OS, no video, no first-run skill offer).

**Verified:** read against the code and the changelog entries above; no code changed. **PR:** n/a

### feat(agent): in-app chat over the Agent Client Protocol

**Area:** `feat(agent)`, `feat(ui)`, `docs(wiki)`

**Problem:** Using the agent meant leaving the editor for a terminal and pasting a connect command.

**Change:** `od_agent::acp` is a client for ACP v1 (JSON-RPC over the child's stdio): it starts the agent the user configures, negotiates the version, opens a session in the project folder with the editor's MCP server (HTTP, bearer token) as its tool, streams text, thoughts and tool calls, surfaces permission requests, and cancels. It serves no file or terminal capability. The editor gets a Chat tab: presets or a custom command (split without a shell), Connect (which also turns on the agent connection), a conversation view, permission buttons, Cancel and Disconnect. A first-prompt preamble points the agent at `get_guidance`, so the user's selection and notes travel with every request.

**Verified:** `fmt`, `clippy -D warnings`, tests against a scripted agent (`examples/mock_acp_agent`): streaming and ordering, permission allow/reject/cancel, a cancelled turn, refusing a file request, the MCP server and token handed over, a wrong protocol version, no HTTP support, an authentication error with the agent's own hint, a missing program, an agent that dies mid-turn; the editor's state tests and the offscreen window session run a whole conversation. **Not verified:** any real agent (Claude Code, Codex and Gemini adapters were not installed or signed in), the preset commands and package names (taken from published names, which have changed before), ACP v2, Windows (`.exe` lookup in the tests is unchecked), long conversations and resizing of the chat view.

**PR:** n/a

### build(app): a macOS app bundle script with a licensing guard

**Area:** `build(app)`, `docs(repo)`

**Problem:** The editor could only be started with `cargo run`; there was no installable app, and the "no GPL-only Qt module" rule (invariant 12) was a convention, not a check.

**Change:** `scripts/package-macos.sh` produces `dist/OpenDesigner.app`: release build, Info.plist, icon from the logo, license notices (`packaging/THIRD_PARTY.txt`, the font license, the project license), `macdeployqt` with the QML folder, a licensing step, ad-hoc signing, and a smoke test that runs the bundled app's scripted session offscreen. The licensing step exists because the first bundle contained two GPL-only Qt parts (the Qt Quick Timeline and Qt Virtual Keyboard plug-ins, copied by `macdeployqt` though the app never imports them): the script now deletes anything matching a banned module and fails if any remains. It also removes the Homebrew rpath so Qt cannot load twice. Also removes leftover CMake/C++/`bridge` wording from AGENTS.md, CONTRIBUTING.md, README and the wiki index.

**Verified:** the script ran to the end on macOS arm64 with Qt 6.12 from Homebrew and the bundle printed `SELFTEST OK`; the frameworks list contains no banned module. **Not verified:** the bundle on another Mac (no other machine), a Developer ID signature or notarization (not done), Windows and Linux packaging (none), drag-to-Applications install and first-run behavior from Finder.

**PR:** n/a

### feat(core): per-language text and exports

**Area:** `feat(core)`, `feat(render)`, `feat(mcp)`, `feat(cli)`, `feat(ui)`, `feat(skill)`

**Problem:** A store listing ships in many languages, and re-asking an agent to redraw each one is the cost this studio exists to remove.

**Change:** `Document::locales` holds translations of text nodes (language tag, then node id to text) in `od.toml`; the new operation `set_localized_text` sets or removes one (undoable, attributed, checkpointed) and a language with no translations disappears. `Document::localized(tag)` gives the document as it reads in a language, which `export_assets` renders: `export`, `export_set` and `export_panorama` take a `locale`, and set and panorama files are named `<prefix>-<locale>-NN.png`. New tool `list_locales` (languages, what each lacks, every text node) and CLI `locales`, `--locale` on the export commands. The editor has a language switcher (the canvas, the text field and exports follow it) and a Languages dialog; adding a language copies the current text of every text node as a starting point, in one undoable step.

**Verified:** `fmt`, `clippy -D warnings`, tests (core: operation, undo/redo, removal, refusals, persistence and restore after a reopen; MCP: translate then export both languages with different pixels and unknown-language refusal; app unit test; the window session adds a language, edits a translation, exports a set in it and checks the saved manifest). **Not verified:** the toolbar combo box and the Languages dialog controls (only the calls behind them run); right-to-left text and complex scripts (shaping uses the bundled Lato plus system fonts, unchecked); that a longer translation fits its box (nothing detects overflow).

**PR:** n/a

### perf(render): a benchmark for the render path, and the first measured numbers

**Area:** `perf(render)`

**Problem:** The roadmap's performance budgets (invariant 11) had never been checked; the canvas design (re-render a PNG per change) was known to be a risk but unmeasured.

**Change:** `cargo run --release -p opendesigner-render --example bench` times render and PNG encode at canvas and export scale, and a 100-operation transaction with a checkpoint, for 10 to 10,000 nodes. Results and the one budget that is not met (canvas at 10,000 nodes: 64 ms against 16 ms) are in the roadmap.

**Verified:** the example runs and its numbers were recorded from that run (Apple M2 Pro). **Not verified:** other machines, cold start, the Qt-side cost of the canvas, scrub latency (no video yet). The benchmark is not a CI gate.

**PR:** n/a

### feat(core): design tokens change through history, with a Colors dialog

**Area:** `feat(core)`, `feat(mcp)`, `feat(skill)`, `feat(ui)`

**Problem:** Tokens (named colors) were setup-only: neither the user nor an agent could change one, and a change would not have been undoable or attributed, so "change the brand color in one place" did not work.

**Change:** New operation `set_token {name, value}` (value `null` removes). It stores the canonical lowercase hex, reports the nodes that use the token as touched (so an agent's recolor is outlined), refuses to remove a token that a node still uses, and carries its inverse, so undo, redo, checkpoint restore and a restore after reopening all work. The editor gets a Colors dialog (list, recolor, add, remove unused); `apply_ops` and the skill text describe the operation; `core::view::tokens` feeds the panel.

**Verified:** `fmt`, `clippy -D warnings`, tests (core: change/undo/redo/remove/refusals/transaction rollback/checkpoint restore, persistence across a reopen; app window session sets a token, uses it in a fill, is refused when removing it, and the saved files contain it). **Not verified:** the dialog's own controls (only the calls behind them run); token types other than color (type scale, spacing) are not built.

**PR:** n/a

### feat(store): panorama and set export, generic device frames, drop-to-swap captures

**Area:** `feat(store)`, `feat(render)`, `feat(mcp)`, `feat(cli)`, `feat(ui)`

**Problem:** A store listing is a set of slides, but the editor could only export one frame at a time, there was no way to make a background cross the seams between screenshots, and putting a screenshot in a device frame was manual geometry.

**Change:** `export_assets` (moved from the MCP tools into `agent::export`) renders a frame, a set of frames, or one wide frame cut into equal columns, validates every image and the count against the store preset, and only then writes, so a set never ends half-exported. New tools `export_set`, `export_panorama`, `device_template`, CLI commands `export-set`, `export-panorama`, `device`. `od_store::devices` builds generic phone and tablet frames (body, `Screen`, camera) from ordinary nodes. The editor gets a Device button, an Export dialog with the three modes, a Replace button for images, and dropping a file on an image swaps its picture (one undoable edit, layout kept). `store` now depends on `core`; `od_render` re-exports `Pixmap` and adds `slice_columns`.

**Verified:** `fmt`, `clippy -D warnings`, default-member tests (new: slicing, every device validates and applies, MCP panorama/set/device with refusals leaving no files, CLI flow from device to validated panorama), the app's unit tests (set/panorama/device/drop-to-swap) and the offscreen window session (set, panorama and a device added). **Not verified:** that the presets match Apple's and Google's current pages (still copied from the reference projects); that real captures look right inside the frames; the Export dialog's radio buttons and the Replace file dialog (only the calls behind them run).

**PR:** n/a

### feat(render): images are imported, validated and drawn

**Area:** `feat(render)`, `feat(core)`, `feat(ui)`, `feat(mcp)`, `feat(skill)`

**Problem:** Image nodes existed in the model but were placeholders: no way to bring a screenshot in, nothing validated the path, and exports warned that they were not drawn.

**Change:** `core::media` defines the only valid reference (`media/<plain name>.png|jpg|jpeg|webp`), checked on every create, set and load, and imports a user-chosen file into `<project>/media/` after sniffing its first bytes (safe unique name, never overwrites, 64 MB cap, symlinks never listed or resolved). `render` decodes with the `image` crate (limits on size and allocation; decoded pixels cached by path, size and modification time) and draws images with `cover` (default), `contain` or `fill`, rounded corners, opacity and scale; a large picture drawn small is box-filtered first so it does not alias; a missing or unreadable image is reported with the reason (`Warning` now has `Display`). New `image_fit` property; `corner_radius` now applies to images. The editor gets an Image button (file dialog) and drop-onto-canvas, which copy the file, size the node to at most 80% of the frame and select it. New MCP tool `list_media` (paths, bytes, pixel sizes); there is deliberately no agent import. `render_frame` takes the project folder as a fourth argument.

**Verified:** `fmt`, `clippy -D warnings` (default members and the app), 94 default-member tests (new: media references and import, image fit/cover/contain/fill/radius/opacity/scale, downscale averaging, symlink refusal, warnings), the app's unit test for import and fitting, and the offscreen window session, which imports a picture and then checks that the exported PNG has its color at the expected pixel; `cargo deny`. **Not verified:** dropping files (the drop handler is untested, only the invokable it calls), large photos for speed (decode is cached but a 20-megapixel image is not benchmarked), CMYK or 16-bit JPEG/PNG variants, animated WebP, color profiles, fuzzing of the decoders.

**PR:** n/a

### feat(render): text is shaped, wrapped and drawn into the canvas and exports

**Area:** `feat(render)`, `feat(core)`, `feat(ui)`, `feat(skill)`

**Problem:** Text nodes were only previewed by a QML overlay and exports warned that they were not drawn, so the main ingredient of a store screenshot (the headline) could not be exported.

**Change:** `render` draws text with `cosmic-text` (shaping with kerning and ligatures, wrapping at the node's width, alignment, line height, weight, any installed family), blending glyph coverage through the frame's clip and the node's opacity. The bundled Lato Regular and Bold (SIL OFL) are the default and the fallback, so a document renders the same on every machine; system fonts are loaded only when a document names a family the bundle lacks or uses characters beyond Latin. `core` gains `font_family`, `font_weight`, `text_align` and `line_height` as validated, undoable properties (and on `create`), the inspector edits them, and the agent tool schema and skill text describe them. The QML text overlay is gone; images are still marked by a placeholder. Dependency: `cosmic-text` 0.19; `cargo deny` ignores the `ttf-parser` unmaintained advisory (RUSTSEC-2026-0192, no vulnerability), logged in the roadmap decision log.

**Verified:** `cargo clippy -D warnings`, `cargo test` (new render tests: drawn at the node's position and byte-identical across runs, weight and size change the ink, alignment and wrapping stay inside the box, opacity, frame clip, unknown family falls back), the shell's tests and scripted window session, `cargo deny check`. **Not verified:** other platforms' system-font lookup; complex scripts and emoji (system fonts are loaded for them but nothing checks the result); per-run styles (one style per text node), letter spacing and text on a path are not built.

**PR:** n/a

### feat(ui): the Qt Quick editor, with a canvas, layers, inspector and the agent panel

**Area:** `feat(ui)`, `feat(core)`, `feat(agent)`

**Problem:** Everything so far worked from a terminal. The product is the GUI where you guide an agent by pointing.

**Change:** New `app` crate: a Qt Quick editor built with Cargo and `cxx-qt` (pinned 0.10.0; no CMake, no C++ sources). One `Editor` QObject forwards to a Qt-free, unit-tested `EditorState`; QML presents it. The window has a toolbar with tools, a layers panel, a canvas that shows the frame rendered by `render` (pan, wheel/pinch zoom, fit; select, drag-move, 8 resize handles; rubber-band drawing of rectangles, ellipses, text and frames; a pin tool), an inspector with typed property edits (a rejected value shows the core's reason), and an agent panel: your pinned notes (resolve, reopen, delete), the history with a restore point before each agent turn, and a Connect tab that starts the MCP server in-process, prints the Claude Code command, and previews then installs the skill. What an agent changes is outlined in blue on the canvas as it happens. New dialogs create a project from a store size preset, pin a note, and export against a store preset. `core` gained `geometry` (bounds and hit testing) and `view` (layers and frames); `agent` exposes `export_frame` shared by the MCP tool and the editor. CI gained a macOS job for the shell.

**Verified:** on macOS arm64 with Qt 6.12 (Homebrew): `cargo fmt --check`, `clippy -D warnings` (default members and the app), `cargo test` (default members and `-p opendesigner-app`: 12 shell unit tests and a scripted session of the real window run offscreen that draws, edits, undoes, pins, starts the agent connection, restores a checkpoint, exports against two presets and grabs the window), `cargo deny check`. **Not verified:** pointer drags, wheel and pinch (the session calls the same invokables, it does not synthesize input); Windows and Linux; a real agent connecting; performance (the canvas re-renders a PNG on the UI thread, so large documents will be slow). Text is previewed by a QML overlay and is not drawn into exports yet.

**PR:** n/a

### feat(skill): generated Agent Skill and a previewed, reversible installer

**Area:** `feat(skill)`, `feat(cli)`

**Problem:** The whole premise is that installing the app teaches the agents on the machine how to use it, and that must not be able to damage anything.

**Change:** `skill.rs` generates the `opendesigner` Agent Skill (`SKILL.md` plus references for the tools, the store presets and a style guide) from the live tool definitions and presets, and installs it into `~/.agents/skills/` (Codex CLI, Gemini CLI, opencode) and `~/.claude/skills/` (Claude Code), both verified against those agents' docs. The installer records files and SHA-256 hashes in a manifest inside the skill folder, keeps files the user edited, refuses a folder it did not create and any symlink, and uninstalls exactly what it wrote. `opendesigner skill status|install|uninstall` previews every path and writes only with `--yes`. Dependency: `sha2`.

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (tests snapshot a temp home and prove only the skill folders change), on macOS arm64. **Not verified:** that Claude Code, Codex, Gemini CLI or opencode actually load and follow the skill (run by hand); Windows and Linux paths; Antigravity CLI's skills location; Cursor.

**PR:** n/a

### feat(mcp): local MCP server, tools and token for coding agents

**Area:** `feat(mcp)`, `feat(cli)`

**Problem:** The guidance model had no way to reach an agent. Without a tool surface an agent could only use the CLI.

**Change:** New `agent` crate. `mcp.rs` speaks MCP (JSON-RPC 2.0, Streamable HTTP, protocol versions 2025-06-18, 2025-03-26, 2024-11-05) with sessions and per-agent attribution; `tools.rs` exposes 17 tools (`get_guidance`, `apply_ops` with a checkpoint before each turn, `render_preview` returning an image, store-validated `export`, checkpoints and restore, and more); `server.rs` is the loopback transport with token, `Host` and `Origin` checks and size limits; `token.rs` creates the 0600 token file. `opendesigner serve-mcp <project>` runs it and prints the Claude Code connect command. Dependencies: axum, tokio, base64, getrandom (evidence in the roadmap decision log).

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (63 tests, including socket-level security tests and a test that starts the real binary, connects as an agent and checks the edit landed on disk with the agent's name in the history), `cargo deny check`, on macOS arm64. **Not verified:** Claude Code or Codex actually connecting (run by hand); Windows and Linux; token file permissions on Windows; behavior under concurrent agents beyond the mutex that serializes calls.

**PR:** n/a

### feat(core): selection, annotations and the guidance context

**Area:** `feat(core)`, `feat(cli)`

**Problem:** The point of the editor is that the user guides an agent by pointing, but the core had no model for what the user points at.

**Change:** `core` gains `Annotations` (pins on a node or a page region, open/resolved, attributed, persisted in `annotations.json`) and `Workspace` (session + annotations + selection + project folder) with `guidance()`: pages, selected nodes, open notes with an orphan flag, and the user's edits since the agent's last turn. The CLI gains `annotate`, `annotations`, `resolve` and `guidance`.

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (43 tests, including a CLI test of pin → read → act → resolve). **Not verified:** Windows and Linux; selection is in memory only (a front end holds it); the MCP and Qt front ends that will use it are not built yet.

**PR:** n/a

### feat(core): headless core, renderer, store presets and CLI

**Area:** `feat(core)`, `feat(render)`, `feat(store)`, `feat(cli)`

**Problem:** The project had rules and a design but no code, and the central claim (agents edit through validated, reversible operations) was unproven.

**Change:** A Cargo workspace with four crates. `core`: node model (frames, groups, rectangles, ellipses, text, image), the operations `create`/`set`/`move`/`reparent`/`delete` with bounds validation and inverses, an append-only history with atomic transactions, undo/redo, checkpoints and `restore`, and project folders (`od.toml`, `pages/*.json`, history log) that save deterministically and reject hostile input. `render`: CPU rasterization of frames, groups, rectangles and ellipses (fills, strokes, rounded corners, clipping, opacity) and PNG export as RGB or RGBA. `store`: nine App Store and Google Play presets with size, format, alpha and count validation. `cli`: the `opendesigner` binary (`new`, `show`, `apply`, `checkpoint`, `checkpoints`, `restore`, `export`, `validate`, `presets`). Also `deny.toml`, `clippy.toml` (no `unwrap`/`expect`/`panic` outside tests), `rust-toolchain.toml`, a Rust CI workflow, and wiki updates marking what is implemented. Dependencies (all popular and maintained, licenses checked by `cargo deny`): serde, serde_json, thiserror, toml, clap, tiny-skia, png, tempfile (tests).

**Verified:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (36 tests, including an end-to-end CLI test that applies an agent edit, exports a 1320x2868 App Store PNG, validates it, rejects a wrong preset without writing a file, and restores a checkpoint), `cargo deny check`, all on macOS arm64. **Not verified:** Windows and Linux; the CI workflow (never run); text and image rendering (not implemented, reported as warnings); the store preset rules against Apple's and Google's own pages (copied from a reference project); performance against the roadmap budgets (no benchmarks yet).

**PR:** n/a

### docs(repo): project rules, wiki and design for the Qt + Rust editor

**Area:** `docs(repo)`

**Problem:** GitNotēs and OpenMedia produce their App Store, Google Play and web assets by describing every detail to an agent in a terminal and re-reading the output. There is no surface to select, annotate or adjust, so the agent is guided by text alone.

**Change:** Repository bootstrap with no application code. `AGENTS.md` (wiki-first rule, worktrees, a dependency policy limiting the project to popular, well-maintained packages, and 12 architecture invariants for a local-first editor whose agents work through validated, checkpointed operations, including that Qt stays LGPL), this changelog, the wiki under `docs/wiki/` (architecture, document format, agent integration, Qt desktop, store assets, video editing, security, prior art, roadmap, development guide) with its sync workflow, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, issue and PR templates, `.editorconfig`, `.clang-format`, and a Rust/Qt/CMake `.gitignore`. `assets/logo.svg` is the app logo. Toolkit: Qt 6 (C++/QML) over a Rust core, chosen over GPUI; see the roadmap decision log.

**Verified:** nothing run; documentation only. Research inputs: the two reference repositories' own docs and store-asset scripts, Qt 6.12's licensing and Qt Multimedia docs, `cxx-qt`'s repository and crates.io page, KDDockWidgets' license, the Agent Skills specification. **Not verified:** canvas and video-preview performance on Qt, the `cxx-qt` build on all three OSes, the per-agent skill directories other than the ones present on the maintainer's machine, whether the QML build flow touches the GPL-only Qt Qml Compiler module, and every `[INFERENCE]` on the wiki pages; these are the Phase 0 spikes in the [roadmap](docs/wiki/roadmap.md).

**PR:** n/a
