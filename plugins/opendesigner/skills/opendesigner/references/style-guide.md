# Store art style guide

Short rules for the art you make with OpenDesigner. The user's own brand and taste win over all of
this; read the existing frames first and match them.

## Copy

- One idea per screenshot. A headline is two short lines at most; a sub line is one.
- Use the words the app itself uses. No superlatives, prices, awards, rankings or calls to action
  ("best", "#1", "free", "download now"): both stores reject them in screenshots.
- Do not show other platforms' marks (an Apple logo on a Google Play image, and the reverse).
- Keep text out of the status bar and the edges. Leave a margin of at least 4 percent of the short
  side.

## Layout

- Check contrast: headline against its background should be readable at thumbnail size.
- Device frames sit fully inside the frame or bleed off a single edge, not both.
- Text takes at most a fifth of a Google Play image. Apple creative assets keep text and focal art
  inside the art safe area of the placement.
- A set reads as one story: same type scale, same margins, one accent color.

## Subject and taste (from Anthropic's frontend-design skill, and screenshot skills on GitHub)

- Ground every design in the subject: what the product is, who it is for, and the one job of this
  asset. Take its words, colors and materials from the product, not from defaults.
- Make deliberate choices of type and palette. One or two type families; a clear scale; avoid the
  tells of generated art: an accent on a single word of every headline, all-caps labels above
  everything, a gradient card with a big number as the default treatment.
- Structure is information: borders, numbers and labels only where the content really is a
  sequence or a group.
- Screenshots are advertisements, not documentation: each sells one feeling, outcome or removed
  pain. Three ways to write the line: paint a moment, state an outcome, kill a pain. Prefer short
  common words, 3 to 5 words a line, the line breaks chosen on purpose.
- A listing is an arc: first slide the main benefit (it may be the only one seen), second what is
  different, then one feature per slide, a trust line near the end, and a closer that is a feature
  wall or a mosaic, not another single feature. Vary the layout of neighbouring slides; break a
  run of device frames with a different hero element every two or three slides.
- The background is a designed surface, never plain white by default. The headline takes roughly
  the top third and must be readable at about 160 px wide (the thumbnail test: render a small
  preview and read it). Put awards and proof on the first slide only.
- Dense real UI inside a device, sparse around it: one headline, one visual, at most one sub line.

## Video (from open ffmpeg and Remotion-style agent skills)

- Plan from real numbers: look at the footage and the frames before choosing lengths, and keep the
  user's originals untouched; write only into `exports/`.
- Plan, then run, then check: render a still of the middle of every transition with
  `render_video_frame`, and `export_video` with the store `preset` so size, length and frame rate
  are verified before encoding.
- One story and one motion language per video: the same easing, the same transition kind, a slow
  zoom rather than random movement; keep the first two seconds on the strongest picture.
- Never invent captions or dialogue; ask when a text is needed and the user has not given it.
- Ask only what changes the output (the destination store or platform sets the size, length and
  frame rate); propose the rest in one line and go.

## Process

- Change one thing per operation list so the user can see and judge it.
- After every visual change, `render_preview` and look. Fix what you see.
- Android screenshots must not show iOS chrome (and the reverse); Google's 2:1 limit means an
  iPhone-proportioned capture needs its own Android layout.
