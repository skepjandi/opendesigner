# Blocks: ready-made titles, overlays, backgrounds, charts and motion

A block is a named, parameterized generator. `add_block` expands it into ordinary nodes (frames,
rectangles, ellipses, text, images) and, when you give it a video, ordinary keyframes made from the
same named moves as `animate_preset`. It is one undoable step, attributed to you and checkpointed
like any other edit. Nothing in the document marks it as special except the name `block:<id>#<n>`
on its group, so you can find it, edit its parts with `apply_ops`, or delete the group to remove it.

## When to reach for a block, and when not to

Reach for a block when the thing is a known pattern and the craft is in the details you would
otherwise redo every time: a lower third that slides in and out, a title card that fits its text, a
bar chart that grows from its baseline, a store headline sized for the slide, a spotlight that dims
everything but one region, a cursor that clicks. The block already handles the parts that go wrong
by hand: text that overflows, margins, contrast, one font family, scale across sizes, and the
timing of the entrance.

Hand-build when the design is the point: a hero composition, a custom illustration, a layout the
catalog does not have. A block is a start, never the finish. Mix them: a block for the chart, your
own composition around it.

## The loop

1. `list_blocks` with a `query` (words for what you need: "lower third", "chart", "dim") and/or a
   `category` (titles, overlays, backgrounds, data, store, motion). Then `get_block` for the
   parameters: each has a type, a range or options and a default.
2. Know the target first. `add_block` needs a `frame` (optional when the document has one) and, for
   motion, a `video` (optional when there is one). `start_ms` is from the start of the video;
   `hold_ms` fades the block out that long after it starts. Without a video the block is a static
   layout, which is right for store images.
3. Customize it. The defaults are placeholders: pass your own copy in `params`, and your brand in
   `colors` or, better, define tokens first with `set_token` so the block picks them up
   (`brand.background`, `brand.text`, `brand.accent`, `brand.muted`, or the plain `background`,
   `text`, `accent`, `muted`). If there are none, the block creates `block.background`,
   `block.foreground`, `block.accent` and `block.muted` once; changing those recolors every block.
   Set one `font` for the whole design. A block shipped with its defaults is not finished work.
4. Check it. `render_preview` the frame (also small, 160 to 320 px), `review_frame` and fix what it
   finds, and for motion `render_video_frame` at the entrance, the hold and the exit, or
   `video_contact_sheet` to see the whole move. A block never overflows its own box, but your copy
   can be too long: the call then fails and says so; shorten the text or give it a bigger `box`.
5. Edit like any other nodes. The result lists the group and its parts by name (`title`, `plate`,
   `bar-3`). Change a part with `apply_ops` `set`, re-time it with `animate`, or delete the group and
   add the block again with different parameters. Undo removes the whole block in one step.

## Placement, sizes and safe areas

Every block lays itself out in a box: its own default (usually the frame inside its safe margin, or
the lower third, or the top band) unless you pass `box` as `{x, y, width, height}` in frame
coordinates; omit sides to keep the default. Sizes are shares of the frame's shorter side, so the
same block works in a 1290 x 2796 store slide, a 1920 x 1080 video and a square post. Text keeps at
least 6 percent of the short side from the edges and never drops below a readable size.

## The catalog by category

- Titles and text: a title card, a kinetic headline whose words land one by one, a subtitle plate, a
  quote card, a big-number callout, a badge pill, an eyebrow label.
- Lower thirds and overlays: lower third (bar or card), name tag, call-to-action button, tooltip
  callout with an arrow, highlight box, spotlight dim, cursor click, notification toast.
- Backgrounds: gradient wash, aurora, grid and noise backdrops. They are pictures written to
  `media/` from the generators, with your colors baked in; they sit behind everything.
- Data: bar chart, line chart, donut chart, progress bar, comparison table. Data is JSON, validated
  and capped (12 bars, 24 points, 3 series, 6 slices, 6 columns by 8 rows). Use real numbers from
  the user; never invent data. The document has no path node, so lines and rings are chains of dots.
- Social and store: the app store headline pair, a feature list with Lucide icons (their credit
  line comes back in the result: show it), rating stars, a review card. Use real ratings and
  reviews only.
- Motion: `stagger_in` brings nodes you already made in one after another (it makes no nodes and
  needs a video), a logo sting, an end card.

## Rules of thumb

- One accent color, one font family. Do not stack five blocks that each shout; a frame sells one idea.
- Backdrops go in first and stay behind; overlays go over footage and carry their own plates so they
  read on any picture.
- A block with `hold_ms` must hold at least as long as its entrance takes; the call tells you.
- Several blocks in one frame get distinct numbers (`#1`, `#2`); they do not overlap by themselves,
  so give each its own `box`.
- Use `dry_run` to see the operations first when you are unsure; nothing is applied.
