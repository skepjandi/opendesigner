# Design direction: from brief to art that is not mediocre

The bar is a design a studio would ship, not "a rectangle with a headline". Before you finish any
piece, ask: would someone stop scrolling for this? If the honest answer is "it is fine", it is not
done. You have every tool below; use them. A design made with only `apply_ops` and one preview is
a first draft.

## The process (do not skip steps)

1. **Know the subject.** Read the repository you were started in: the README, the app's own
   colors, fonts, screens and copy. Run the app and capture real screens when you can. Call
   `list_media`, `find_files` and `import_file` for logos, icons and photos that already exist.
   Never invent facts: no ratings, user counts, awards, prices or testimonials.
2. **Pick the format.** `get_brief` says what the user chose (size, aspect ratio, fps). Where they
   chose nothing you decide, from `list_export_presets` and `list_video_presets`, and say what you
   chose in one line. Changing something the user chose needs `change_format`, which asks them.
3. **One concept.** One sentence: the feeling or outcome the set sells. Everything serves it.
4. **A system before pictures.** Define colors with `set_token` (brand, background, text, accent,
   plus one muted), pick one or two type families and a scale (below), a grid and margins. Name
   everything you create (`Hero`, `Headline`, `Screen`, `Badge`), never "Rectangle 14".
5. **Build the first frame fully, look, then repeat it.** `render_preview` at export scale and at
   thumbnail scale (`max_side` 160 to 320). Fix what you see. Only then make the other frames, by
   copying the same structure so the set reads as one story.
6. **Critique with the tools.** `review_frame` measures contrast, text size, overflow, margins and
   alignment; fix every finding or say why it stays. `render_preview` again.
7. **Ship.** `export_set` / `export_panorama` with the preset (nothing is written if a rule breaks),
   `list_locales` and translations for each language, then tell the user what you made, what you
   decided for them and what you would try next.

## What makes it look professional

- **Hierarchy.** One thing is biggest (the headline or the device), one is second (a sub line or
  badge), the rest is quiet. If everything is bold nothing is. Squint: you should see exactly one
  focal point.
- **Space.** Margins of at least 6% of the short side; more is better. Generous space around the
  headline. Do not fill every corner. Keep text out of the status-bar area and the bottom
  gesture area.
- **Type.** One family, two weights, or two clearly different families. A scale with a ratio of
  about 1.5 (for example 160 / 96 / 56 / 36 on a 1320 wide canvas). Headline 2 to 3 short lines,
  3 to 5 words a line, line height 1.0 to 1.15 for headlines (`line_height`), 1.3 for body. Left or
  centered, not both. Never body text under 3.5% of the canvas height on a store image.
- **Color.** A background that is a designed surface (a deep brand color, a warm off-white, a
  gradient made from two tokens), never plain white by default. One accent color, used for one thing.
  Check contrast: text against its background at least 4.5:1 (7:1 for small text). Use tokens so a
  palette change is one operation.
- **Devices.** Use `device_template` for a phone or tablet around a real capture. Let a device bleed
  off one edge for energy, or float it fully inside; never half-cut on two edges. Mix framings
  across a set (straight, cropped, tilted is not available: use crop and scale instead).
- **Rhythm across a set.** The first slide sells the main benefit; neighbours differ in layout;
  break a run of devices with a type-only or detail slide; end on a feature wall or a closer, not
  one more feature.
- **Detail.** Align to the grid; equal gaps; round numbers; consistent corner radii (one value
  everywhere); real shadows are not available, so use a darker shape under a card with lower
  opacity. Add one accent element per slide (a badge, a highlighted word color, a rule), not five.

## Banned defaults (they read as generated)

A centered title on a gradient with nothing else. A glowing card. Five equal feature boxes. Emoji
as icons. Lorem ipsum. A stock gradient from purple to blue. One accented word in every headline.
All-caps tiny labels above everything. Plain white. A device frame floating in empty space.
Anything you cannot say why it is there.

## Layout recipes (canvas 1320 x 2868, portrait store slide; scale proportionally)

- **Hero:** background token, headline top-aligned at y 200, x 90, width 1140, 150 px, 2 lines;
  sub line 52 px, 60 px below; device frame 1000 wide centered, top at y 820, bleeding off the
  bottom by 200 px. Badge or accent bar 12 px high above the headline.
- **Detail:** device cropped large (1500 wide) so one UI element fills the frame, headline at the
  bottom third, one callout shape pointing at the element.
- **Type slide:** headline 220 px over three lines, one accent color for the line that matters,
  nothing else.
- **Feature wall:** 6 to 8 one-word features as 120 px type in two tones.

## Using every tool

`get_guidance` (selection and notes first) · `get_tree` / `get_node` / `find_nodes` · `apply_ops`
with `set_token`, `create`, `reparent` for structure · `device_template` · `list_media`,
`find_files`, `import_file` · `render_preview` (several scales) · `review_frame` · `list_locales`
+ `set_localized_text` · `export` / `export_set` / `export_panorama` · `resolve_annotation` for
every note you acted on · `list_workspaces` / `create_workspace` for another deliverable ·
`get_playbook` for the other guides. If a tool could improve the result and you did not use it, use
it.
