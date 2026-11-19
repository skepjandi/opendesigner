# Document Format

> **Status: Partially implemented.** The `core` crate (`crates/core`) implements the node model for frames, groups, rectangles, ellipses, text and image nodes, the operation set below, history with checkpoints, and project I/O at format version `1`. Sections marked *(intended)* are not built. Deviations from the original design are listed under [Implemented](#implemented-cratescore) and in the [roadmap decision log](./roadmap.md#decision-log).

## Implemented (`crates/core`)

| Piece | Where | Notes |
|-------|-------|-------|
| Node model | `model.rs` | `Document` (pages, tokens, id counter), `Page` with an immutable `Group` root, `Node` with kind, geometry, `visible`, `opacity`, `fill`, `stroke`, `corner_radius`, text fields, `image` path, `children`. Ids are `n_<counter>` / `p_<counter>` minted by the document and never reused (`next_id` only grows) |
| Paint | `paint.rs` | `#rrggbb`, `#rrggbbaa` or `{token.name}` resolved against the document's tokens |
| Operations | `ops.rs` | `create`, `set` (one typed `Property`), `move`, `reparent`, `delete`, and the history-only `restore`. Each returns the resolved forward operation, its inverse and the touched ids |
| History | `history.rs` | `Session`: append-only log of `Transaction`s (actor, optional turn id, kind `edit`/`undo`/`redo`/`restore`), atomic apply with rollback, undo/redo as appended transactions, checkpoints as log positions, `restore(checkpoint)` appends the inverse of everything after it, `changed_since(position)` for the agent-change outline |
| Limits | `limits.rs` | Coordinates ±1,000,000; sizes 0..=16,384; 1,000 operations per transaction; 100,000 nodes; nesting 64; name 200 and text 10,000 characters; 64 MB per project file. Values are rounded to four decimals |
| Project I/O | `project.rs` | `od.toml` (format, name, id counter, page ids, tokens), `pages/<id>.json`, `.od/history/log.jsonl`, `.od/history/checkpoints.json`; atomic writes; a newer `format` is refused; unknown fields, duplicate ids, a lagging id counter and ids that are not `[A-Za-z0-9_-]{1,64}` are rejected on load |
| Annotations | `annotations.rs` | Pins on a node or a page region with a note, `open`/`resolved` status, author and resolver (`Actor`), saved in `annotations.json` at the project root. Not part of the undoable history: they are conversation, not content |
| Workspace and guidance | `workspace.rs` | `Workspace` holds the session, annotations, the (ephemeral) selection and the project folder. `guidance()` returns what an agent reads first: pages, the selected nodes with their data, open notes (flagged `target_exists: false` when the node was deleted) and the user's edits since the agent's last turn |

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
- **Tokens live in `od.toml`; there is no `presets/` or `media/` handling yet.** `image` nodes store a project-relative path that is validated for length only; resolving it inside the project root arrives with image rendering.
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
| `text` | Runs with font, size, weight, tracking, line height; shaped at render time |
| `image` | Reference into `media/` plus crop and fit |
| `device` | A device frame (iPhone, iPad, Pixel, Mac window) with a screen slot for an `image` |
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

### Validation

`core` rejects an operation, with a reason an agent can read, when: an id does not exist or is a descendant of its own new parent; a value is outside bounds (size, coordinates, text length, count of nodes per call); a path escapes the project (`media/` references are resolved inside the project root only); a font or token is unknown; the operation would make a frame larger than the export limit.

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
