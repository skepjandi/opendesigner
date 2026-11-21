# Lint rules

> Status: Implemented. The linter is `od_core::lint::lint` (`crates/core/src/lint.rs`); the MCP tool is
> `validate_document`, the CLI is `opendesigner lint`. Codes are stable: a code never changes meaning, a
> retired rule keeps its code unused, and new rules take new codes. Messages and hints may be reworded.

The linter is read-only. It finds things that make a design or a video wrong without making the project
invalid: a node nobody can see, text that cannot be read, a file that is gone. Agents call
`validate_document` before they finish a turn (the generated skill says so); CI can run
`opendesigner lint <project> --fail-on warn`.

## Findings

Each finding is `{code, severity, node?, frame?, video?, message, hint}`. `severity` is `error`, `warn` or
`info`. `hint` says what to do about it.

## Rules

| Code | Name | Severity | What it means |
| --- | --- | --- | --- |
| `OD001` | node-outside-frame | warn | A node lies entirely outside the frame that contains it, so it is never visible. |
| `OD002` | node-clipped-by-frame | info | A node extends past its frame's edge (a bleed); the frame clips the overflow. |
| `OD003` | text-clipped-by-frame | warn | A text node extends past its frame's edge, so some of the words are cut off. |
| `OD010` | text-overflow | warn | Text wraps to more rows than its box is tall. |
| `OD011` | text-empty | warn | A visible text node has no text. |
| `OD012` | text-too-small | warn | Text is smaller than 1.2% of the frame's long side, unreadable at store-listing size. |
| `OD013` | low-contrast | warn | Text contrast against its background fill is below WCAG AA (4.5, or 3 for large text). |
| `OD020` | media-missing | error | An image or screen recording names a file that is not in media/. |
| `OD021` | token-undefined | error | A paint refers to a design token that is not defined. |
| `OD030` | invisible-node-animated | warn | An animated node is hidden, fully transparent or zero-sized and nothing animates that away. |
| `OD031` | invisible-frame-in-video | warn | A frame shown by a clip or overlay is hidden, fully transparent or zero-sized. |
| `OD040` | siblings-overlap | warn | Two texts, or two 3D devices, in the same container cover each other. |
| `OD041` | frame-size-no-target | warn | A top-level frame's size matches none of the chosen export targets. |
| `OD050` | clip-frame-missing | error | A clip shows a frame that does not exist. |
| `OD051` | footage-missing | error | A clip plays footage that is not in media/. |
| `OD052` | overlay-outside-video | warn | An overlay starts after the video ends or runs past its end. |
| `OD053` | animation-beyond-video | warn | An animation has keys after the video ends. |
| `OD054` | sound-missing | error | A video's sound names a file that is not in media/. |
| `OD055` | video-empty | warn | A video has no clips. |
| `OD060` | binding-node-missing | error | A template variable is bound to a node that no longer exists. |
| `OD061` | binding-invalid | error | A template variable's binding cannot be written to its node (wrong property for the node). |
| `OD062` | binding-shadowed-by-translation | info | A text node bound to a variable also has translations; those win in their language. |
| `OD063` | binding-duplicated | warn | Two variables are bound to the same node property; the last one set wins. |
| `OD080` | node-scaled-flat | warn | A node's scale_x or scale_y is 0 and nothing animates it, so the node is never visible. |
| `OD081` | clip-without-area | warn | A group clips its children but has no size and no children to take a box from, so it shows nothing. |
| `OD082` | loop-jumps | info | A looping animation ends on a different value than it starts with, so it jumps once per cycle. |
| `OD090` | image-backdrop-scene | warn | A clip's frame is mostly one raster image (over 60% of it) and no vector shape in it is animated: a picture with text on it, not an animation. |
| `OD091` | text-only-motion | warn | The only things animated in a clip's frame are text and rectangles; shapes, characters and scenery do not move. |

`OD001` to `OD003` judge a turned or scaled node by the upright box around it after its transform and its
parents' (`geometry::bounds_in_frame`), not by its untransformed box.
| `OD070` | path-invisible | warn | A path has no fill and no visible stroke, so nothing of it is drawn. |
| `OD071` | gradient-stops-unsorted | warn | A gradient's stops are not in increasing offset order; they are drawn sorted, which is rarely what was meant. |
| `OD072` | trim-hides-stroke | warn | A stroke's trim start and end are equal, so none of the outline is drawn and nothing animates it. |

## Animation rules

`OD090` and `OD091` judge a clip's frame from the document alone (`od_core::scene::scene_stats`), the same facts `review_video` uses. `OD090` fires when one `image` node covers more than 60% of the frame (by its bounds) and no `path` or `ellipse` (or group holding one) of the frame is animated: a picture with text on it. `OD091` fires when something in the frame is animated but only text, rectangles and groups of them are; it does not fire together with `OD090`. Neither looks at pixels: `review_video` does, and adds what the document cannot say (still stretches, camera, sound). `[INFERENCE]` A full-bleed screenshot in a plain app preview will trigger `OD090`; it is a warning, not an error.

## What the linter does not know

The core has no fonts and does not read files except to see whether a `media/` file exists, so those inputs
are supplied by the caller (`LintOptions`):

- **Text overflow (`OD010`)** needs a measurer. The MCP tool, the CLI and the editor pass the renderer's
  (`od_render::check_text`); without one the rule is skipped.
- **Missing files (`OD020`, `OD051`, `OD054`)** need the project folder; without one they are skipped.
- **Export targets (`OD041`)** are the sizes the caller chose: the workspace brief's format in the MCP tool,
  and `--preset` (repeatable) in the CLI. Without any the rule is skipped. A frame that a video shows is judged
  by the video's size, not by an export target.
- **Contrast (`OD013`)** is WCAG 2 on resolved colors. The background is the nearest fill drawn under the text
  inside its frame (a frame with no fill counts as white, as it exports). Text over an image or a 3D device is not
  judged, because the pixels are unknown. Large text (24 px, or 18.66 px at weight 700 and up) needs 3:1,
  other text 4.5:1.
- **Too small (`OD012`)** compares the font size with the frame's long side (1.2%), so it holds at any export
  scale.
- **Overlap (`OD040`)** flags two texts, or two 3D devices, sharing more than a fifth of the smaller box.
  Text on a shape or an image is how layouts are made and is not flagged.

## Template variables

`OD060` to `OD063` check the bindings of template variables (`docs/wiki/document-format.md#template-variables`).
Translations (`Document.locales`) are keyed by node and language; a variable writes the node's default
text. In a language that has a translation of a bound text node, the translation wins, which `OD062` reports.
