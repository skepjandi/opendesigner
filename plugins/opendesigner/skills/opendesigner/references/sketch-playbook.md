# Hand-made looks: sketch, watercolor, grain

Seven blocks give a video or image the look of something drawn, painted and photographed on paper:
`sketch`, `boil`, `watercolor_wash`, `paper_grain`, `flow_strokes`, `marker_underline` and
`scribble_circle`. Each one expands into ordinary `path` nodes (and, for the two grains, `image`
nodes) plus ordinary keyframes, in one undoable step; you can edit, recolor or delete the parts like
any other node. Every drawing is deterministic: the same `seed` draws the same lines, another `seed`
draws them again differently.

## When a hand-made look is right

- The subject is human, explanatory or personal: a how-it-works explainer, a sketchnote, a
  storybook, a founder's story, a kids' or education piece, a brand that says "made by people".
- The user asks for sketch, doodle, pencil, ink, watercolor, paper, notebook, hand-drawn, zine or
  "less digital".
- A flat-vector scene feels cold: laying `paper_grain` over it, or giving the hero a `sketch`
  outline, warms it up without redrawing it.

Do not use it for a product UI demo that must look exact, for numbers that must be read precisely
(charts stay crisp; annotate them with `scribble_circle`), or when the brand is glass, chrome and
gradients. A wobbly line is a statement: use it everywhere or nowhere in a piece.

## The blocks

| Block | Makes | Reach for it to |
|-------|-------|-----------------|
| `sketch` | wobbly pen outlines gone over twice, optional fill | redraw a node or a shape by hand; it draws itself on |
| `boil` | 2 to 4 differently seeded drawings that take turns about 12 times a second | make a drawing alive and shivering, like hand-drawn animation |
| `watercolor_wash` | 6 to 12 stacked translucent deformed copies, soft edge, darker pooled rim, bleed | paint a color field behind a subject or text |
| `paper_grain` | paper tint, speckle pictures (light and dark) and a vignette | unify everything; always the last, topmost block |
| `flow_strokes` | tapered ribbons following a seeded field: grass, hair, wind, water, flow | fill empty ground or sky with texture that sways |
| `marker_underline` | marker band, double line, wavy or scribbled underline | emphasize a word or number |
| `scribble_circle` | a scribbled loop, an arrow or a cross | point at or rule out something |

`sketch` and `boil` take either `targets` (ids of nodes in the frame: rectangles keep their corner
radius, ellipses stay round, paths keep their curves, anything else uses its box) or a `shape`:
`{kind: rect | ellipse | circle | polygon | polyline | line | path, ...}` in frame coordinates, or a
list of up to 12. The original nodes stay; hide or delete them when only the drawing should show.

The pen: `roughness` (0 a ruled line, 1 a careful sketch, 3 a scrawl), `bowing`, `stroke_width` (a
percent of the frame's shorter side), `passes` (how often the pen goes over the outline). Fills:
`fill_style` `none`, `solid` (a loose wash), `hachure` (parallel lines at `angle` and `gap`),
`cross_hatch`, `zigzag`, `dots` or `dashed`, drawn in `fill_color` with a thinner `weight`. The fill
lines are clipped to the shape, concave shapes and holes included. Set `draw_on` and `duration_ms` and
the sketch draws itself, outline first and then the fill (the block's `start_ms` times it).

## Combine them

1. Ground: `watercolor_wash` in a pale role, one big shape behind the subject, `layers` 8 or 9.
   A second, smaller wash in another role, offset, makes a two-tone stain.
2. Line: `sketch` over the subject with `passes` 2, `roughness` about 1, `color` foreground. Fill
   with `hachure` for shadow on one part only; hatching everywhere reads as a coloring book.
3. Texture: `flow_strokes` `grass` along the bottom of a landscape, `wind` or `water` in the sky and
   sea, a few strokes only (30 to 70).
4. Emphasis: `marker_underline` under the one word that matters, `scribble_circle` around the one number.
5. Finish: `paper_grain` last, `strength` 0.25 to 0.4, a little tint and vignette.

Order matters in the node tree: washes go behind (they are placed at the back), line work and marks
in front, and `paper_grain` on top of everything. Add it last so nothing is added above it.

## Boil

`boil` is the line boil of hand-drawn animation: the same drawing redrawn with slightly different
wobble a dozen times a second. Use it on the hero and on the outlines of characters, not on every
element: a whole frame boiling is tiring after a few seconds, and a still `sketch` next to a boiling
one reads as "this is the thing that is alive". `variants` 3 is the usual choice, 2 a nervous
shiver, 4 a loose scribble; `fps` 12 is "animating on twos". The drawings are shown one at a time
with hard cuts (opacity keys one frame apart), never cross-faded, which is the whole look. The loop is
a few frames long and repeats forever; a video of any length has no seam.

`paper_grain` boils the same way through `loop_ms` (default 1000; `variants` 2 to 4 different grain
pictures take turns). Set `loop_ms` to 0 for one still grain: a still grain over a still image is
the right choice for an exported poster.

## Palette

The blocks take their colors from the document's tokens (and create `block.*` ones when there are
none); `colors` overrides a role. Paper looks need a warm light `background` (cream, not white, not
`#fff`), a near-black but not pure black `foreground` (ink blue, graphite, sepia), and one or two
accents that are slightly muted: an earthy red, a mustard, a faded teal. Watercolor picks its pigment
from the `color` role and varies it with `variation`; pick a pale accent and let the layers deepen
it. On a dark background use a light `foreground` and `tone: light` for the grain; the vignette
darkens from the background color, so it works on both.

## Budgets and performance

Every block makes at most about 400 nodes, usually far fewer: a `sketch` is a few nodes per shape (a
fill is one node whatever the line count), a `watercolor_wash` about 16 per shape, `flow_strokes`
one node per stroke (at most 140), `boil` one group per variant. Prefer one big wash to ten small
ones, `count` 40 to 70 for strokes, and no more than a handful of `boil` shapes: each variant is a
full drawing the renderer paints while it is up. The grain is a picture per variant (at most 1024 px,
stretched to the frame, written once to `media/` and named by its recipe), not thousands of shapes,
which makes it far cheaper than speckle nodes; keep `variants` at 2 unless the grain visibly repeats.
`watercolor_wash` uses the blur effect, which costs more the larger the blurred group: lower
`softness` before lowering `layers`.

After building, `render_preview` at full size and at 320 px: hand-made lines must still read small.
A `stroke_width` under 0.25 vanishes at thumbnail size; raise it before raising `roughness`.
Run `review_frame` and `review_video` as usual. A boiling or grain loop is judged with
`render_video_frame` at two times a slot apart: the frames should differ.
