# Document Format

> **Status: Partially implemented.** The `core` crate (`crates/core`) implements the node model for frames, groups, rectangles, ellipses, text, image and 3D device nodes, the operation set below, keyframe animations, history with checkpoints, and project I/O at format version `2` (version 1 projects still load). Sections marked *(intended)* are not built. Deviations from the original design are listed under [Implemented](#implemented-cratescore) and in the [roadmap decision log](./roadmap.md#decision-log).

## Implemented (`crates/core`)

| Piece | Where | Notes |
|-------|-------|-------|
| Node model | `model.rs` | `Document` (pages, tokens, id counter), `Page` with an immutable `Group` root, `Node` with kind, geometry, `visible`, `opacity`, `fill`, `stroke`, `corner_radius`, text fields (`text`, `font_size`, `font_family`, `font_weight` 100 to 900, `text_align` left/center/right, `line_height` as a multiple of the size; all optional with the defaults Lato, 400, left, 1.2), `image` path, `children`. A text node's `width` wraps its lines (0 means no wrapping); its `height` is not used for layout. Text is one run in one style: per-run styles, tracking and fonts per range are not built. Ids are `n_<counter>` / `p_<counter>` minted by the document and never reused (`next_id` only grows) |
| Paint | `paint.rs` | `#rrggbb`, `#rrggbbaa` or `{token.name}` resolved against the document's tokens |
| Operations | `ops.rs` | `create`, `set` (one typed `Property`), `move`, `reparent`, `delete`, `set_localized_text` (a translation of a text node in a language; `text: null` removes it, and a language with no translations disappears), `set_token` (define, recolor or, with `value: null`, remove a design token; removal is refused while a node uses it; users of the token are reported as touched), and the history-only `restore`. Each returns the resolved forward operation, its inverse and the touched ids |
| History | `history.rs` | `Session`: append-only log of `Transaction`s (actor, optional turn id, kind `edit`/`undo`/`redo`/`restore`), atomic apply with rollback, undo/redo as appended transactions, checkpoints as log positions, `restore(checkpoint)` appends the inverse of everything after it, `changed_since(position)` for the agent-change outline |
| Limits | `limits.rs` | Coordinates ±1,000,000; sizes 0..=16,384; 1,000 operations per transaction; 100,000 nodes; nesting 64; name 200 and text 10,000 characters; 64 MB per project file. Values are rounded to four decimals |
| Project I/O | `project.rs` | `od.toml` (format, name, id counter, page ids, tokens), `pages/<id>.json`, `.od/history/log.jsonl`, `.od/history/checkpoints.json`; atomic writes; a newer `format` is refused; unknown fields, duplicate ids, a lagging id counter and ids that are not `[A-Za-z0-9_-]{1,64}` are rejected on load |
| Annotations | `annotations.rs` | Pins on a node or a page region with a note, `open`/`resolved` status, author and resolver (`Actor`), saved in `annotations.json` at the project root. Not part of the undoable history: they are conversation, not content |
| Workspace and guidance | `workspace.rs` | `Workspace` holds the session, annotations, the (ephemeral) selection and the project folder. `guidance()` returns what an agent reads first: pages, the selected nodes with their data, open notes (flagged `target_exists: false` when the node was deleted) and the user's edits since the agent's last turn |
| Videos | `video.rs` | `Video` and `Clip` (a clip shows a design frame for `duration_ms`, with a transition into it and a zoom, both eased; overlays with `enter`/`exit`/`ease`; one older sound track `audio {file, start_ms, gain, fade_in_ms, fade_out_ms}`; optional `tracks [SoundTrack]` (`audio.rs`) and `captions {style, cues}` (`captions.rs`), both omitted from the file while empty, so a project written before them loads and saves byte-identically; they change through `set_video` props `tracks` and `captions`), stored in `videos/<id>.json` and listed in `od.toml` (`videos = [...]`); operations `create_video`, `delete_video`, `set_video`, `add_clip`, `remove_clip`, `move_clip`, `set_clip`; limits in `video.rs` (100 videos, 500 clips, 10 minutes). A video file without `audio` still loads: the field is optional and missing means a silent video. See [Video Editing](./video-editing.md#built-video-v1) |
| Media | `media.rs` | `media/<plain name>` references (png, jpg, jpeg, webp), import by the user with magic-byte sniffing and unique names, listing and resolving without following symlinks. `Workspace::import_media`, `media`, `media_path` |
| 3D devices | `device3d.rs` | `Device3d`: the model (`phone`, `phone_punch_hole`, `tablet`, `laptop`, `monitor`), the turn (`rotate_x`, `rotate_y`, `rotate_z`), `zoom`, the lens (`fov`), `light_angle`, `shadow`, `glare`, `screen_scroll` and `screen_footage`. Stored on a `device3d` node as `device3d`; the node's box is the viewport, its `fill` the body's finish and its `image` the capture on the screen. Set with `set` props `model`, `rotate_x`, `rotate_y`, `rotate_z`, `zoom`, `fov`, `light_angle`, `shadow`, `glare`, `screen_scroll`, `screen_footage` |
| Animations | `animation.rs` | `Video.animations`: keyframes of any number of a node over the video's time (`AnimProp`, `Key`), the operation `animate` (empty keys removes an animation), `Ease::Spring`, `Video.motion_blur` (2 to 16 pictures blended while something moves), `Document::at_time` (the document as it looks at a moment) and the named moves (`Preset`, `preset_ops`) that write ordinary keyframes |

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

The value holds before the first key and after the last, and eases in between. The animatable properties are `x`, `y`, `width`, `height`, `opacity`, `corner_radius` and, on a 3D device, `rotate_x`, `rotate_y`, `rotate_z`, `zoom`, `fov`, `light_angle`, `shadow`, `glare` and `screen_scroll`. Keys with no animation (an empty list) remove it; a node with an animation cannot be deleted until the animation is removed. `Video.motion_blur` (2 to 16 pictures) blends the moments across a half-frame shutter while something moves.

The named moves (`spin`, `turntable`, `swing`, `flip_in`, `hero_reveal`, `rise_in`, `drop_in`, `float`, `push_in`, `pull_back`, `orbit`, `scroll_screen`, `fade_in`, `fade_out`, `slide_in_left`, `slide_in_right`) are not a separate format: they are written as ordinary keyframes, measured from the node's designed pose.

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
