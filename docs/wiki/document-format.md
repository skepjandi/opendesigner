# Document Format

> **Status: Partially implemented.** The `core` crate (`crates/core`) implements the node model for frames, groups, rectangles, ellipses, text, image and 3D device nodes, the operation set below, keyframe animations, history with checkpoints, and project I/O at format version `2` (version 1 projects still load). Sections marked *(intended)* are not built. Deviations from the original design are listed under [Implemented](#implemented-cratescore) and in the [roadmap decision log](./roadmap.md#decision-log).

## Implemented (`crates/core`)

| Piece | Where | Notes |
|-------|-------|-------|
| Node model | `model.rs` | `Document` (pages, tokens, id counter), `Page` with an immutable `Group` root, `Node` with kind, geometry, `visible`, `opacity`, `fill`, `stroke`, `corner_radius`, text fields (`text`, `font_size`, `font_family`, `font_weight` 100 to 900, `text_align` left/center/right, `line_height` as a multiple of the size; all optional with the defaults Lato, 400, left, 1.2), `image` path, `children`. A text node's `width` wraps its lines (0 means no wrapping); its `height` is not used for layout. Text is one run in one style: per-run styles, tracking and fonts per range are not built. Ids are `n_<counter>` / `p_<counter>` minted by the document and never reused (`next_id` only grows) |
| Node index | `index.rs` | The id to position index of the node tree: `Document::locate` climbs the recorded parent chain instead of walking every node (a 100-operation transaction on 10,000 nodes: 3.8 ms before, 0.04 ms after). The index is a hint, never an authority: every answer is checked against the tree, a wrong position is repaired by looking at that parent's children, and a node the index does not know is searched for by a plain walk, so a stale index costs time and not correctness. Operations keep it fresh (`note_inserted`, `note_removed`); a copy of a document starts without one. Nothing about it is written to the project |
| Paint | `paint.rs` | `#rrggbb`, `#rrggbbaa` or `{token.name}` resolved against the document's tokens, or a gradient (`Paint::Linear`, `Paint::Radial`; see [Vector shapes](#vector-shapes)). A solid paint is a JSON string, a gradient an object with a `type`. `Paint::normalized` validates and rounds; `Paint::resolve` of a gradient answers with its first stop, the one color that stands for it where a single color is needed |
| Paths and strokes | `path.rs`, `vector.rs` | `NodeKind::Path`, SVG path data parsed and capped (`Path::parse`), `ViewBox`, `FillRule`, stroke `cap`/`join`/`dash`/`trim_start`/`trim_end`, and the checks that keep them in bounds (see [Vector shapes](#vector-shapes)) |
| Operations | `ops.rs` | `create`, `set` (one typed `Property`), `move`, `reparent`, `delete`, `set_localized_text` (a translation of a text node in a language; `text: null` removes it, and a language with no translations disappears), `set_token` (define, recolor or, with `value: null`, remove a design token; removal is refused while a node uses it; users of the token are reported as touched), and the history-only `restore`. Each returns the resolved forward operation, its inverse and the touched ids |
| Template variables | `variables.rs` | `Document.variables`: typed slots bound to node properties, written by `declare_variable`, `set_variable`, `set_variable_value` and `remove_variable` (see [Template variables](#template-variables)). Saved as `[[variables]]` tables in `od.toml`, absent when there are none |
| History | `history.rs` | `Session`: append-only log of `Transaction`s (actor, optional turn id, kind `edit`/`undo`/`redo`/`restore`), atomic apply with rollback, undo/redo as appended transactions, checkpoints as log positions, `restore(checkpoint)` appends the inverse of everything after it, `changed_since(position)` for the agent-change outline |
| History | `history.rs` | `Session`: append-only log of `Transaction`s (actor, optional turn id, kind `edit`/`undo`/`redo`/`restore`), atomic apply with rollback, undo/redo as appended transactions, checkpoints as log positions, `restore(checkpoint)` appends the inverse of everything after it, `changed_since(position)` for the agent-change outline and `touched_since(position)` for the ids an edit touched, deleted ones included (what a drawing cache redraws) |
| Limits | `limits.rs` | Coordinates ±1,000,000; sizes 0..=16,384; 1,000 operations per transaction; 100,000 nodes; nesting 64; name 200 and text 10,000 characters; 64 MB per project file. Values are rounded to four decimals |
| Project I/O | `project.rs` | `od.toml` (format, name, id counter, page ids, tokens), `pages/<id>.json`, `.od/history/log.jsonl`, `.od/history/checkpoints.json`; atomic writes; a newer `format` is refused; unknown fields, duplicate ids, a lagging id counter and ids that are not `[A-Za-z0-9_-]{1,64}` are rejected on load |
| Annotations | `annotations.rs` | Pins on a node or a page region with a note, `open`/`resolved` status, author and resolver (`Actor`), saved in `annotations.json` at the project root. Not part of the undoable history: they are conversation, not content |
| Workspace and guidance | `workspace.rs` | `Workspace` holds the session, annotations, the (ephemeral) selection and the project folder. `guidance()` returns what an agent reads first: pages, the selected nodes with their data, open notes (flagged `target_exists: false` when the node was deleted) and the user's edits since the agent's last turn |
| Videos | `video.rs` | `Video` and `Clip` (a clip shows a design frame for `duration_ms`, with a transition into it and a zoom, both eased; overlays with `enter`/`exit`/`ease`; one older sound track `audio {file, start_ms, gain, fade_in_ms, fade_out_ms}`; optional `tracks [SoundTrack]` (`audio.rs`) and `captions {style, cues}` (`captions.rs`), both omitted from the file while empty, so a project written before them loads and saves byte-identically; they change through `set_video` props `tracks` and `captions`), stored in `videos/<id>.json` and listed in `od.toml` (`videos = [...]`); operations `create_video`, `delete_video`, `set_video`, `add_clip`, `remove_clip`, `move_clip`, `set_clip`; limits in `video.rs` (100 videos, 500 clips, 10 minutes). A video file without `audio` still loads: the field is optional and missing means a silent video. See [Video Editing](./video-editing.md#built-video-v1) |
| Videos | `video.rs` | `Video` and `Clip` (a clip shows a design frame or footage for `duration_ms`, with a transition into it and a zoom, both eased; a clip of footage may also carry `speed` and a `speed_ramp` and any clip may carry a `grade`; overlays with `enter`/`exit`/`ease`; one sound track `audio {file, start_ms, gain, fade_in_ms, fade_out_ms}`), stored in `videos/<id>.json` and listed in `od.toml` (`videos = [...]`); operations `create_video`, `delete_video`, `set_video`, `add_clip`, `remove_clip`, `move_clip`, `set_clip`; limits in `video.rs` (100 videos, 500 clips, 10 minutes). A video file without `audio`, `speed`, `speed_ramp` or `grade` still loads: the fields are optional and missing means no sound, normal speed and no grade. Time is whole milliseconds, snapped to the video's frame grid when an operation is applied (`timebase.rs`). See [Video Editing](./video-editing.md#built-video-v1) |
| Media | `media.rs` | `media/<plain name>` references (png, jpg, jpeg, webp), import by the user with magic-byte sniffing and unique names, listing and resolving without following symlinks. `Workspace::import_media`, `media`, `media_path` |
| 3D devices | `device3d.rs` | `Device3d`: the model (`phone`, `phone_punch_hole`, `tablet`, `laptop`, `monitor`), the turn (`rotate_x`, `rotate_y`, `rotate_z`), `zoom`, the lens (`fov`), `light_angle`, `shadow`, `glare`, `screen_scroll` and `screen_footage`. Stored on a `device3d` node as `device3d`; the node's box is the viewport, its `fill` the body's finish and its `image` the capture on the screen. Set with `set` props `model`, `rotate_x`, `rotate_y`, `rotate_z`, `zoom`, `fov`, `light_angle`, `shadow`, `glare`, `screen_scroll`, `screen_footage` |
| Animations | `animation.rs` | `Video.animations`: keyframes of any number of a node over the video's time (`AnimProp`, `Key`), the operation `animate` (empty keys removes an animation), `Ease` (also `elastic`, `bounce`, `ease_out_back`, `ease_in_out_cubic`, `power2_*`, `power4_*`, `expo_*`, `circ_*`, `sine_*` as in/out/in_out, and `steps_2|3|4|6|8|12`), `Repeat`, `Video::camera_at`, `Video.motion_blur` (2 to 16 pictures blended while something moves), `Video.hold_frames` (1 to 6, default 1: each drawing is held for that many frames), `Document::at_time` (the document as it looks at a moment) and the named moves (`Preset`, `preset_ops`) that write ordinary keyframes |
| Transform and effects | `transform.rs` | `Node.rotation`, `scale_x`, `scale_y`, `pivot_x`, `pivot_y`, `blur`, `shadow` (`Shadow {dx, dy, blur, color}`), `clip`, `depth`; `Affine` (2D map, `Node::local_affine`), `Camera` and `Camera::affine`; the `Property` variants `rotation`, `scale_x`, `scale_y`, `scale`, `scale_xy`, `pivot_x`, `pivot_y`, `blur`, `drop_shadow`, `shadow_dx`, `shadow_dy`, `shadow_blur`, `clip`, `depth`; limits ([below](#transform-and-effects)) |

Operation JSON as agents send it (the `opendesigner apply` command and the future MCP `apply_ops` tool take this array):

```json
[
  {"op": "create", "parent": "n_2", "node": {"kind": "text", "name": "Headline", "x": 40, "y": 80, "text": "Hold for everything.", "font_size": 96, "fill": "{brand.gold}"}},
  {"op": "set", "id": "n_3", "prop": {"name": "opacity", "value": 0.5}},
  {"op": "move", "id": "n_3", "x": 120, "y": 340},
  {"op": "reparent", "id": "n_3", "parent": "n_9", "index": 0},
  {"op": "delete", "id": "n_3"}
]
```

Deviations from the design below, each deliberate for Phase 1:

- **Colors are sRGB hex, not OKLCH.** Tokens map names to hex strings. OKLCH can be added without a format break by accepting more paint syntaxes.
- **Tokens live in `od.toml`; there is no `presets/` handling yet.** `image` nodes store `media/<plain file name>` (`.png`, `.jpg`, `.jpeg`, `.webp`), validated on every create, set and load; the file is imported by the user into `<project>/media/` and never rewritten. `image_fit` is `cover` (default), `contain` or `fill`; `corner_radius` rounds an image. Crop is not built.
- **`set` takes a typed `prop` (`{"name": "x", "value": 10}`), not a dotted path.** The set of properties is closed, so every value is validated for its type and for the node kinds it applies to.
- **Ids are minted by the document.** `create` takes a node without an id; `restore` carries ids and is refused from front ends.
- **No timestamps in the log.** The log is deterministic; time is a front-end concern.
- **Undo and redo stacks are in memory.** The log and checkpoints persist; the stacks do not.
- **Pages are set up with `add_page`, not operations**, and are not undoable yet.

## Principles

1. **Files, not a database.** A project is a folder of plain files, diffable in Git, readable by an agent without the app.
2. **One source of truth for mutation: operations.** Every change, from a drag or from an agent, is an operation appended to a history. State is a fold over operations.
3. **Stable ids.** Nodes have ids that never change and are never reused, so an agent's reference survives other edits and a Git diff shows what moved.
4. **Deterministic serialization.** Sorted keys, fixed float precision, one node per block, so two saves of the same state are byte-identical.
5. **Versioned with migrations.** `format` in the manifest; a loader refuses a newer major and migrates an older one.

## Project folder (intended)

```
my-store-assets.od/
├── od.toml                  # manifest: name, format version, targets, export defaults
├── pages/
│   ├── home-screenshots.json   # a page: a tree of frames and nodes
│   └── promo-video.json        # a video composition: tracks and clips
├── media/                   # imported images, fonts, footage (originals; never rewritten)
├── presets/                 # project-specific export presets, extending the built-in ones
├── .od/
│   ├── history/             # append-only operation log and checkpoints
│   └── cache/               # decoded thumbnails, waveforms (gitignored, rebuildable)
└── exports/                 # default output location (gitignored by the template)
```

`od.toml` and `pages/` are the human-meaningful part. `.od/history/` is kept in Git by default so checkpoints travel with the project; a project can opt out.

## Node model (intended)

A page is a tree. Every node has `id`, `kind`, `name`, a transform and size, and optional `style`, `layout` and `children`.

| Kind | Purpose |
|------|---------|
| `frame` | A container with a size and clipping; the unit that exports (an artboard) |
| `group` | Transform-only grouping |
| `rect`, `ellipse`, `line`, `polygon` | Primitives |
| `path` | Bezier vector path, boolean operations stored as a derived result plus its sources |
| `text` | Runs with font, size, weight, tracking, line height; shaped at render time. Built: one run per node with family, size, weight, alignment and line height; shaped by `cosmic-text` in `render` |
| `image` | Reference into `media/` plus fit and corner radius (built); crop (not built) |
| `device` | A device frame (iPhone, iPad, Pixel, Mac window) with a screen slot for an `image` |
| `device3d` | A device drawn in 3D inside its own box (built): a phone, tablet, laptop or monitor with a capture or a recording on its screen, turned and lit. See [3D devices](#3d-devices) |
| `instance` | An instance of a component, with overrides |
| `clip`, `track` | Video composition members ([Video Editing](./video-editing.md)) |

Colors are stored in a perceptual space (OKLCH, as OpenCut's theme tokens do) and resolved to sRGB or Display P3 at export. Design tokens (colors, type scale, spacing) live in the manifest and nodes reference them by name, so an agent changes a brand color in one place.

## Operations *(design; the implemented JSON is shown above)*

```jsonc
{ "op": "set", "id": "n_42", "path": "style.fill", "value": "{brand.gold}" }
{ "op": "create", "parent": "n_7", "index": 2, "node": { "kind": "text", "name": "Headline", "text": "Hold for everything." } }
{ "op": "move", "id": "n_42", "to": { "x": 120, "y": 340 } }
{ "op": "reparent", "id": "n_42", "parent": "n_9", "index": 0 }
{ "op": "delete", "id": "n_42" }
```

Every transaction carries `actor` (`user` or `agent:<name>`) and a `turn` id grouping one agent response. A *transaction* groups many operations (one drag, one agent turn) into one undo step.

### Arranging a selection (`crates/core/src/arrange.rs`)

The editor's arrange commands add no operation type: `core::arrange` reads the document and returns ordinary `move`, `create`, `reparent` and `delete` operations that the shell applies as one transaction, so one command is one undo step and every limit above still applies (a command that needs more than `MAX_OPS_PER_TRANSACTION` operations fails whole).

- **Selection.** A command acts on the *top-level* selected nodes (a node whose ancestor is also selected is skipped), in stacking order. A **group** has no size of its own: its position is the origin of its children and its bounds are the union of its visible children. Layout is computed in page coordinates and converted back by translation.
- **Align** (`left`, `hcenter`, `right`, `top`, `vmiddle`, `bottom`) uses the union bounds of the selection; one node aligns to the frame around it (nothing happens outside a frame). **Distribute** (`h`, `v`) needs three or more nodes, keeps the outermost two and equalises the gaps. **Stack** (`row`, `column`) lays the nodes edge to edge with a gap in their current order along the axis, keeps the first node where it is and gives the others its position on the other axis; it is a one-time arrangement, not an auto-layout.
- **Duplicate** deep-copies each node with new ids into the same parent, directly above the original, offset by 20 px; a frame is placed beside the existing frames (100 px gap) instead. Locale translations are not copied.
- **Group** needs nodes that share one parent. The new group takes the place of the topmost of them, children keep their order and their page positions. **Ungroup** puts a group's children into the group's parent at its place, preserving page positions.
- **Z-order** moves the whole selection `n` places forward or backward inside each parent; selected nodes never jump over each other.
- **Clipboard** (app memory only): `arrange::copy` stores the subtrees as JSON with positions relative to their frame; `arrange::paste` creates them with new ids in the open frame at the same position, shifting by 20 px steps while an identical node already sits there. Frames always paste beside the page's other frames.
- **Marquee** (`geometry::children_in_rect`) selects the open frame's visible direct children (a group is one node) that the rectangle overlaps. **Snapping** (`geometry::snap_move`) moves a box within 6 px onto the edges and centers of the frame and of its visible direct children (minus the dragged ones), per axis independently, and returns the guide lines to draw.

### Validation

`core` rejects an operation, with a reason an agent can read, when: an id does not exist or is a descendant of its own new parent; a value is outside bounds (size, coordinates, text length, count of nodes per call); a path escapes the project (`media/` references are resolved inside the project root only); a font or token is unknown; the operation would make a frame larger than the export limit.

## 3D devices

A `device3d` node is a device drawn in perspective inside its own box. The box is the viewport: the device is fitted into it with a margin at any turn, so a spin never runs out of the frame. The node carries:

| Field | Meaning |
|-------|---------|
| `model` | `phone`, `phone_punch_hole`, `tablet`, `laptop` or `monitor`. Generated shapes with typical proportions; none is a manufacturer's artwork (see the [decision log](./roadmap.md#decision-log)) |
| `rotate_x`, `rotate_y`, `rotate_z` | Tilt (positive leans the top away), turn (positive turns the right edge away) and roll (positive clockwise), in degrees, up to 3,600 either way |
| `zoom` | How large the device is in its box (0.05 to 10); 1 fits it whatever its turn |
| `fov` | The lens in degrees (1 to 100): 30 is a natural product shot, a small value is nearly flat |
| `light_angle` | Where the key light comes from, in degrees around the device (0 above, 90 from the right) |
| `shadow`, `glare` | The strength of the shadow under the device and of the reflection on the glass (0 to 1) |
| `screen_scroll` | Where a capture taller than the screen is shown: 0 its top, 1 its bottom; unset centres it. Keyframed, it scrolls a long page |
| `screen_footage` | A recording from `media/` to play on the screen instead of the node's `image` |

The screen takes the node's `image` with its `image_fit` (cover by default); a recording plays from its `in_ms` while the frame is on screen, and a still shows the picture at `in_ms`. `render` draws the device with a CPU rasterizer (`od_render::three_d`): depth buffer, supersampled anti-aliasing, a studio environment for the reflections and a soft shadow; the picture is the same in the canvas and in an export because both are `render`'s output.

## Vector shapes

Flat-vector drawing (`crates/core/src/path.rs`, `vector.rs`, `paint.rs`; drawn by `crates/render/src/vector.rs`, `gradient.rs`, `trim.rs`). Every addition is an optional field that is absent from a file while it has its default, so a project written before them loads and saves byte-identically.

**Path nodes.** `kind: "path"` draws SVG path data in its box:

| Field | Meaning |
|-------|---------|
| `path` | The `d` string. Commands `M L H V C S Q T A Z`, absolute and relative, implicit repeats, packed numbers and arc flags. At most 4,000 segments (an arc counts as up to four curves) and 64 KB; every number finite, every coordinate within ±1,000,000; it must start with a move. Present exactly on path nodes. It is data: parsed and validated by `Path::parse`, never executed |
| `view_box` | `{x, y, width, height}` (width and height above 0): the rectangle of path coordinates that maps onto the node's box. Without it the path's own tight bounds (curve extremes, not control points) are stretched over the box, so resizing the node scales the drawing; a flat bound (a straight horizontal or vertical line) keeps its scale along that axis. Imported art uses neither trick: its path is written relative to its own top-left and its box is its bounds |
| `fill_rule` | `nonzero` (default) or `evenodd`; unset means nonzero |

A path node created with no width and no height takes the size of its view box, or of its path's bounds. Properties for `set`: `path`, `view_box` (`null` clears), `fill_rule` (`null` clears). A path node takes `fill` and `stroke` like a rect; its `corner_radius` does not apply. `Path::to_data` writes absolute data back out (three decimals), `Path::flatten` cuts it into polylines for hit tests and trims.

**Gradients.** A `fill`, or a stroke's `paint`, of a frame, rect, ellipse or path is a color string or a gradient object:

```json
{"type": "linear", "angle": 90, "stops": [{"offset": 0, "color": "#7ad0ff"}, {"offset": 1, "color": "{sky.low}"}]}
{"type": "radial", "cx": 0.5, "cy": 0.4, "r": 0.5, "stops": [{"offset": 0, "color": "#ffffffcc"}, {"offset": 1, "color": "#ffffff00"}]}
```

`angle` is in degrees, 0 runs left to right and 90 top to bottom (default 90); the line passes through the box's center and reaches its corners, so the first and last stop touch the box's far edges at any angle. A radial gradient is a circle around `cx`, `cy` (fractions of the box, default 0.5) of radius `r` (a fraction of the box's longer side, default 0.5); past the last stop the color is held. 2 to 8 stops, offsets 0 to 1, colors `#rrggbb[aa]` or `{token}` (a token used by a stop counts as used and cannot be removed). Stops out of order are drawn sorted (lint `OD071`). The gradient lives in the node's own box, so it moves, resizes and (with the node transform) turns with the shape. Text fills and 3D device bodies use a gradient's first stop. Reflecting and repeating spreads are not offered; every gradient pads.

**Strokes.** `stroke` is `{paint, width, cap?, join?, dash?, trim_start?, trim_end?}`: `cap` is `butt` (default), `round` or `square`; `join` is `miter` (default), `round` or `bevel`; `dash` is up to 16 lengths, dash then gap, an odd count repeating itself, any length 0 to 16,384 and one above 0; `trim_start` and `trim_end` (0 to 1) keep the stretch of the outline between them (the smaller to the larger, as fractions of the whole outline's length, all contours together), so moving `trim_end` from 0 to 1 draws a shape on. A width of 0 draws nothing. Dash and trim work on every shape that has an outline, rounded rects and ellipses included; a dash pattern begins at the start of the trimmed stretch. `set` takes the whole `stroke`, or `stroke_width`, `trim_start`, `trim_end` for the node's existing stroke (an error when it has none).

**Hit tests and bounds.** A path's box is its bounds, as for any node. A click hits a path inside its fill (by the fill rule) or within half the stroke width of its outline, at least 3 document units (`vector::hits`, flattening the path).

**Drawing.** `render` fits the path into the box, then fills and strokes it through the node's transform; fills and strokes of rects, ellipses and frames go through the same function, so a gradient, dash or trim works on all of them. A trimmed outline is cut into pieces of at most half a device pixel before it is stroked.

**Animation.** `trim_start`, `trim_end` and `stroke_width` are `AnimProp`s on any node with a stroke. `fill_color` moves a solid fill from color to color: its keys write the color as `"value": "#rrggbb[aa]"` (stored as the number `0xRRGGBBAA`) and each channel mixes linearly in sRGB. A fill that is a gradient or a token is replaced by the solid color while the animation plays.

**Path morph.** A path node may hold `path_morph` (`crates/core/src/morph.rs`, `Node::morph`, left out of the file when absent; present only on path nodes): `{width, height, closed: [bool per contour], stops: [[[ [x, y], ... ] per contour] per stop]}`, the points in the node's box coordinates (`width` and `height` are the box they were made in; the points scale with the node). 2 to 8 stops, 1 to 16 contours, every stop has the same contours with the same number of points (2 to 256, 3 or more for a ring), numbers finite and within ±1,000,000 (stored to 4 decimals). It is set with an `apply_ops` `set` of `path_morph` (`null` clears) and written by `morph_to`. The `morph_progress` animation (an `AnimProp`, only on a node with a `path_morph`) moves along the stops: 0 is the first, 1 the second, up to the last stop's index (a key past it is refused); between two stops the points blend linearly. A `set` that would clear the morph, or leave fewer stops than a key uses, is refused while a `morph_progress` track needs them (so a saved project always loads): replace or remove the keys first. `Document::at_time` turns the blend into path data (`M L ... Z`, 3 decimals) and a view box of the morph's box on the node it returns, so the renderer draws it as any path; the stored `path` is untouched.

**Importing an SVG** is covered in [Agent integration](./agent-integration.md#vector-import).

## Sound tracks and captions

Two optional fields of a video (`crates/core/src/audio.rs`, `captions.rs`); neither is written while empty, and an old file without them loads unchanged.

- **`tracks`**: up to 8 `SoundTrack {id (a1, a2, ...), role (music | voice | sfx, default music), file (media/<name>.mp3|wav|m4a|aac|ogg), start_ms, in_ms, out_ms?, gain (0-2), fade_in_ms, fade_out_ms, pan (-1..1), duck? {depth 0-1, attack_ms, release_ms}}` besides the older `audio`, which `Video::sound_tracks()` reads as the first track (id `audio`, role music). `duck` is allowed on music only; `out_ms` must be after `in_ms`. Defaults are not stored. The ids are the first free `a<N>` of the video, so they are stable inside a project and never collide with `audio`.
- **`captions`**: `{style, cues}`. `Cue {id (q1, q2, ...), start_ms, end_ms, text, words [{text, start_ms, end_ms}]}` in time order without overlap (at most 2,000 cues, 80 words per cue, 30,000 words, 400 characters per cue, 60 per word); when `words` is given it spells `text` exactly and lies inside the cue. `CaptionStyle {font_family, size_pct (1-20, percent of the video's height), font_weight, color, highlight?, background?, position (top | middle | bottom), align, max_lines (1-4), uppercase}` with named presets `clean`, `bold_pop`, `karaoke`, `boxed` (`CaptionStyle::preset`). Colours are `#rrggbb` or `#rrggbbaa`.
- **Operations.** There is no new operation: both are values of `set_video` (`VideoProperty::Tracks(Vec<SoundTrack>)`, `VideoProperty::Captions(Captions)`), so each change is attributed, checkpointed and undone like any other, and the inverse is the old value. `Video::tracks_with_added/changed/removed` and `captions_with_cue/without_cue/with_cues/with_style` compute the new value for one add, change or removal and validate it; the MCP tools and the CLI use them. **Deviation:** granular `add_track`/`add_cue` operation variants were not added; a whole-list replace keeps the operation enum and the history code untouched, at the cost of storing the list twice per history step (bounded by the caps above).
- **Bounds in the document vs. in the tools.** The document keeps only the shape rules (caps, order, no overlap, word spelling), so shortening a video never invalidates captions that an undo could then not restore; a cue past the video's end is simply not drawn. The tools additionally require every cue to end inside the video (`captions::check_within`).

## Keyframes

A video may animate any number of a node over its own time. `Video.animations` holds one entry per node and property, sorted, each with keys `{at_ms, value, ease}`:

```json
{"op": "animate", "video": "v_1", "node": "n_7", "prop": "rotate_y",
 "keys": [{"at_ms": 0, "value": -20, "ease": "ease_in_out"},
          {"at_ms": 1200, "value": 20}]}
```

An animation may also carry `"additive": true` (`Animation.additive`, left out of the file when false, so every older project reads unchanged; the format version is 6). Its keys are offsets added to the node's value as the replacing track gives it, held to the property's range; a video has at most one replacing and one additive animation per node and property, sorted by node, property, replacing first. A colour (`fill_color`) cannot be additive. `animate` takes the same `additive` flag, written into the transaction so the undo removes or restores the right track. See [Video editing](./video-editing.md#additive-layers-and-implicit-from).

The value holds before the first key and after the last, and eases in between, unless the track `repeat`s: `"repeat": "loop"` starts again from the first key (the cycle is the stretch from the first key to the last) and `"repeat": "pingpong"` plays back and forth, for as long as the video plays. `repeat` is part of the track, not of its keys: `animate` without it sets `once`, it is stored on the track (`Animation.repeat`, left out of the file when `once`) and the undo restores it. Sampling (`Animation::value_at`) folds the time into the cycle, a pure function of the time. The animatable properties are `x`, `y`, `width`, `height`, `opacity`, `corner_radius`, `rotation`, `scale_x`, `scale_y`, `scale`, `blur`, `shadow_dx`, `shadow_dy`, `shadow_blur`, the camera's `camera_x`, `camera_y`, `camera_zoom`, `camera_rotation` ([Camera](#camera)) and, on a 3D device, `rotate_x`, `rotate_y`, `rotate_z`, `zoom`, `fov`, `light_angle`, `shadow`, `glare` and `screen_scroll`. Keys with no animation (an empty list) remove it; a node with an animation cannot be deleted until the animation is removed. `Video.motion_blur` (2 to 16 pictures) blends the moments across a half-frame shutter while something moves.
The value holds before the first key and after the last, and eases in between. The animatable properties are `x`, `y`, `width`, `height`, `opacity`, `corner_radius`, `trim_start`, `trim_end`, `stroke_width` (a node with a stroke), `fill_color` (a node with a fill; see [Vector shapes](#vector-shapes)) and, on a 3D device, `rotate_x`, `rotate_y`, `rotate_z`, `zoom`, `fov`, `light_angle`, `shadow`, `glare` and `screen_scroll`. Keys with no animation (an empty list) remove it; a node with an animation cannot be deleted until the animation is removed. `Video.motion_blur` (2 to 16 pictures) blends the moments across a half-frame shutter while something moves.

The named moves (`spin`, `turntable`, `swing`, `flip_in`, `hero_reveal`, `rise_in`, `drop_in`, `float`, `push_in`, `pull_back`, `orbit`, `scroll_screen`, `fade_in`, `fade_out`, `slide_in_left`, `slide_in_right`) are not a separate format: they are written as ordinary keyframes, measured from the node's designed pose.

## Transform and effects

> **Status: Implemented** (`crates/core/src/transform.rs`, `od_render::place`, `od_render::layers`). Every field is optional on a node, omitted from the file at its default and means no effect, so a project that uses none of them is unchanged byte for byte.

Any node (frames and groups too) may turn, scale, blur, cast a shadow, clip and sit at a depth; the node and everything in it are drawn under the transform.

| Field (`set` name) | Default | Meaning |
| --- | --- | --- |
| `rotation` | 0 | Degrees clockwise about the pivot (at most 36000 either way) |
| `scale_x`, `scale_y` | 1 | Scale about the pivot along the node's own axes, after the rotation is applied to them (−100 to 100; negative flips). `set scale` sets both; its inverse is `set scale_xy {x, y}` |
| `pivot_x`, `pivot_y` | 0.5 | The point rotation and scale turn about, as fractions of the node's box (−10 to 10). A group without a width and height uses the box around its children's untransformed boxes, so a spinning child does not move the group's pivot |
| `blur` | 0 | Gaussian radius in document units (0 to 500) of the node as drawn, children included |
| `shadow` (`set drop_shadow`) | none | `{dx, dy, blur, color}`, `color` a `#rrggbbaa` or token whose alpha is the shadow's opacity (default `#00000040`). Cast by the node as drawn (blur included), under it, offset in screen space; the node's `opacity` scales it. The name `shadow` already belongs to a 3D device's `set shadow`, so the shadow of a node is set with `drop_shadow`; `shadow_dx`, `shadow_dy`, `shadow_blur` set one number and need an existing shadow |
| `clip` | false | On a group or frame: children are cut at its box and corner radius. A frame always clips, so the flag changes nothing there. A group may now have a `width` and `height` (until now a group could not); without them its box is the one around its children, which clips nothing |
| `depth` | 1 | How far the video camera moves the node (0 to 10): 1 with the scene, 0 pinned to the screen. Depths multiply down the tree |

The map from a node's own coordinates to its parent's is `translate(x, y) · translate(pivot) · rotate · scale · translate(−pivot)` (`Node::local_affine`, f64, `Affine`), composed down the tree. The exported or drawn frame sits at the origin and ignores its own `x`, `y`, rotation and scale.

- **Bounds and hit tests** (`geometry::page_bounds`, `bounds_in_frame`, `hit_test`) use the upright rectangle around the transformed box (a clipping group: its own box) and invert the transform to find what a click lands on. `parent_origin` and the arranging commands (`align`, `distribute`, `group`) still work in untransformed parent offsets `[INFERENCE: they were not audited for turned parents]`.
- **Lint** judges `OD001` to `OD003` by those bounds and adds `OD080` to `OD082` ([Lint rules](./lint-rules.md)).
- **Limits** are in `transform.rs` (`MAX_ROTATION`, `MAX_SCALE`, `MAX_PIVOT`, `MAX_BLUR`, `MAX_SHADOW_OFFSET`, `MAX_PARALLAX_DEPTH`) and enforced by `set`, `create`, loading and keyframes alike.

### Rendering

`od_render` keeps a camera-free matrix per node (`place::node_chain`, a plain translation unless the node is turned or scaled) and applies the camera and, inside an effect layer, the layer's shift on top (`Ctx::world`), so a document without these fields takes the code path it always took. A node that blurs or casts a shadow is drawn on a layer the size of what it paints (`layers.rs`): the layer is blurred with the grade blur (three box passes), the shadow is the tinted silhouette of the blurred layer, shifted by the offset rounded to whole pixels and blurred, and the pair is laid over the picture through the parent's clip. Text under a turned, flipped or unevenly scaled map is drawn upright on a scratch picture (at most 4096 px a side) and placed with the map; a 3D device's picture is placed with it. `Retained` (incremental tiles) gives up on a frame that holds a node that turns, scales, blurs or casts a shadow and the caller draws the frame whole, because such a node paints outside the rows its box covers.

### Camera

`Video` has no camera field: the camera is four keyframed numbers, `camera_x`, `camera_y`, `camera_zoom`, `camera_rotation`, animated with `animate` using the video's own id as `node` (`Video::camera_at(time_ms)`; still, `x` 0, `y` 0, `zoom` 1, `rotation` 0, when there are none; a single key holds a still camera). A node at effective depth `d` is drawn through `translate(c) · rotate(d·rotation) · scale(zoom^d) · translate(−c) · translate(−d·x, −d·y)`, `c` the frame's center, so depth 0 stays, depth 1 follows the camera and depth 0.5 gets half the pan and the square root of the zoom. The frame's own background is not moved. Only frames shown by a clip go through the camera; overlays stay on the screen.

## Template variables

`Document.variables` is a list of typed, document-level slots whose value is written into the properties of the nodes they are bound to. They exist so one design can be rendered many times with different content (`opendesigner export --batch`, the `batch_export` tool) and so an agent knows which slots carry identity. A variable never renders by itself.

```json
{"op": "declare_variable", "variable": {
  "id": "title", "kind": "string", "label": "Headline", "description": "The product name",
  "portrays": "subject_name", "default": "Welcome",
  "binds": [{"node": "n_7", "prop": "text"}]}}
```

| Field | Meaning |
|-------|---------|
| `id` | `[A-Za-z0-9_-]{1,64}`, unique among variables |
| `kind` | `string`, `number`, `color`, `bool`, `enum`, `image` or `font` |
| `label`, `description` | for people and agents (200 and 500 characters at most) |
| `portrays` | optional: `subject_name`, `subject_domain`, `subject_tagline`, `subject_logo` or `host_identity`: the slot carries the identity of what the piece is about (or who makes it), so an agent must get it from the user and never invent it |
| `default`, `value` | the declared default and what the bound nodes show now. A value is a string (also color, enum choice, `media/` reference, font family), a number or a boolean, and must agree with `kind` |
| `enum_values` | the choices of an `enum` (1 to 32, each at most 64 characters, unique); empty for every other kind |
| `binds` | at most 64 `{node, prop}` pairs, no duplicates |

Which node properties a kind may drive: `string` drives `text` and `name`; `number` drives `x`, `y`, `width`, `height`, `opacity`, `corner_radius`, `font_size`, `font_weight`, `line_height` and the 3D device numbers; `color` drives `fill`; `bool` drives `visible`; `enum` drives `text`, `text_align`, `image_fit` and `model`; `image` drives `image`; `font` drives `font_family`. At most 100 variables.

**How a value change works.** `set_variable_value {id, value}` checks the value against the kind, then writes every bound node property through the same validated path as `set` (so bounds, applicability and tokens are checked and a failure rolls the whole transaction back). It is one operation in one transaction: attribution, undo, redo, checkpoints and `restore` all work, and the bound nodes are reported as touched. Its inverse is the history-only `restore_variable`, which carries the variable and the old node values. `declare_variable` writes the default into the bound nodes at once; `set_variable` changes `label`, `description`, `portrays`, `default`, `enum_values` or `binds` (a node newly bound takes the current value); `remove_variable` leaves the nodes as they are. A bound node that was deleted is skipped when a value is written and reported by the linter (`OD060`), so deleting a node never blocks the others. Undoing the delete brings the binding back to life.

**Variables and translations are different things.** `Document.locales` holds a translation of a text node per language (`set_localized_text`): keyed by node, per language, for the same piece in several languages. A variable is keyed by slot, has one value at a time, and is for different pieces (one card per customer). They combine without duplicating: a variable writes the node's own `text` (the default every language falls back to), and in a language that has a translation of that node the translation wins. The linter reports that case (`OD062`) so nobody is surprised; to have a variable drive a translated text, translate the variable's value in each language too. Batch rows may name a `locale` to render a language.

## History and checkpoints

- The log is append-only. A **checkpoint** is a named point in it (automatic at the end of every agent turn and at manual save, user-named on request).
- Restoring a checkpoint appends a restore operation; it never rewrites history. This is what makes "every agent edit is reversible" cheap and auditable.
- The UI diff compares two checkpoints by node id: created, deleted, moved, restyled. Changed nodes are outlined in the accent color while the agent's turn is the latest ([Agent Integration](./agent-integration.md#guidance-context)).

## Interop

| Direction | Format | Notes |
|-----------|--------|-------|
| Export | PNG, JPEG, WebP, SVG, PDF; MP4/WebM/GIF for video | Store presets in [Store Assets](./store-assets.md) |
| Import | SVG, PNG/JPEG/WebP, fonts, video/audio | SVG import maps to nodes where it can, else to an `image` |
| Import (later) | Penpot and Figma exports | `[INFERENCE]` worth doing only if users ask; not in the phases |

## Open questions

- SVG-native storage (every page is a valid SVG with `od:` extensions) versus our own JSON. SVG is readable everywhere; JSON is stricter and carries layout, tokens and components cleanly. Decide in the Phase 0 file-format spike.
- Whether text is stored as runs or as a small rich-text tree.
- Whether history is stored per project folder or per file in a content-addressed store.
