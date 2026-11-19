# Document Format

> **Status: Design.** Intent, not a specification. The first PR that implements `core` replaces this page's examples with the real schema and bumps the format version to `1`.

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

## Operations (intended)

```jsonc
{ "op": "set", "id": "n_42", "path": "style.fill", "value": "{brand.gold}" }
{ "op": "create", "parent": "n_7", "index": 2, "node": { "kind": "text", "name": "Headline", "text": "Hold for everything." } }
{ "op": "move", "id": "n_42", "to": { "x": 120, "y": 340 } }
{ "op": "reparent", "id": "n_42", "parent": "n_9", "index": 0 }
{ "op": "delete", "id": "n_42" }
```

Every operation envelope carries `actor` (`user` or an agent name), a `turn` id grouping one agent response, and a timestamp. Operations are the unit of undo. A *transaction* groups many operations (one drag, one agent turn) into one undo step and one checkpoint.

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
