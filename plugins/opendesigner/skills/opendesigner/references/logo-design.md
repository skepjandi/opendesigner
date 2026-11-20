# Logo design: from a brief to production files

Adapted (paraphrased, not copied) from the MIT-licensed logo-design skill by kaankiziltug
(github.com/kaankiziltug/logo-design-skill), fitted to OpenDesigner's tools. A logo is an
**identifier, not an explanation**: simple, distinctive, relevant, and it must work at 16 px and on a
building, in one color, for years. Find one clear idea, build it with craft, prove it works.

## The modes

| The user wants | Do |
|---|---|
| a new logo | the process below |
| feedback on theirs | critique with the checklist at the end; fix nothing unasked |
| a redesign | name what must stay (equity), then the process |
| favicon / app icons from a mark | `export_icon_set` |

Fast track when the user gives little: ask at most five questions in one message (name spelled
exactly, what they do, audience, 3 to 5 adjectives, competitors and constraints), or state your
assumptions and go straight to three concepts. Adjectives are the most valuable input.

## The process

1. **Brief.** Write it in four lines: who, what, why, feel. List your assumptions.
2. **Category research.** List the category's clichés (fintech: blue, arrows, shields, globes; coffee:
   beans, steam, cups; dentist: teeth) and treat them as off-limits unless given a fresh form. Look
   at what exists: `search_assets` for reference shapes, the user's repository for existing marks and colors.
3. **Word map.** Name, offering, adjectives, promise: nouns, metaphors, opposites. Circle intersections.
4. **Concepts.** Write 8 to 12 one-sentence concepts across at least two mark types (wordmark,
   monogram, letterform, pictorial, abstract, emblem, mascot, combination). A sentence that could
   describe a competitor's logo is not a concept. Pick the three strongest and most different.
5. **Build black first** in SVG (below), one file per iteration. Only the three; do not polish what you will drop.
6. **Test, refine, loop twice** (checklist below).
7. **Checkpoint.** Make one frame with the three concepts in grey, large and at 64, 32 and 16 px, with a
   one-line idea each and your recommendation with one honest risk per concept. Show it, then
   **stop and let the user choose** before building the kit. Skip the pause only if they said "just deliver".
8. **The kit** (after they choose): final geometry with optical corrections and a small-size cut;
   color (one or two, accessible, with one-color and reversed versions); type and lockups (horizontal,
   stacked, symbol only, wordmark only; at most two families); a presentation frame with mockups of
   their industry built from frames; `export_icon_set`; a one-page usage guide (clear space from a
   logo element, minimum sizes, color codes, approved backgrounds, misuse).

## Build it in SVG, then in the editor

- Tools: `import_svg` takes the SVG you wrote, checks it, draws it crisp into `media/` and keeps the
  master; place the PNG in an image node; `render_preview` shows it; `export_icon_set` writes the sizes.
  `list_tokens` and the `set_token` operation hold the brand colors; recolor with import_svg's `color`.
- `viewBox="0 0 256 256"` for symbols; lockups keep height 256. Integer or one-decimal coordinates; a
  unit (8 or 16) and everything snapped to multiples of it. Describe the construction in words
  first ("circle r=96 at 128,128; a 45 degree wedge removed top right; a dot r=28 in the gap").
- Prefer `<circle>`, `<rect rx>`, arcs (`A`) for circular geometry, few anchors, anchors at extrema
  with horizontal or vertical handles, exact angles (0, 15, 30, 45, 60, 90), consistent stroke widths and radii.
- Real holes with `fill-rule="evenodd"` for negative space. No `<text>`, no embedded pictures, no
  filters, masks, shadows or gradients in a master; letters are built as paths.
- Name groups (`symbol`, `wordmark`), add a `<title>`. Save every iteration as its own file name.
- **Look at it**: drawing SVG blind fails. After every change `render_preview` the frame holding it, at
  full size and at 16 to 64 px, and judge the picture, not the code.

## Refinement (what separates good from great)

- **Scale**: the idea survives 16 to 24 px; gaps and strokes thick enough, else simplify or add a small-size version.
- **Optical corrections**: overshoot round and pointed forms 1 to 3 percent past the line; fix the
  bone effect on rounded joins; thin horizontals slightly; center optically (a little above geometric center).
- **Balance**: stable, evenly weighted, a near-square footprint for symbols.
- **Craft pass**: every modified letter still reads as the letter; inspect each junction for notches,
  slivers and lumps; put it next to a few well-made marks at the same size; if the concept is just the
  product drawn (a cup for coffee), push further or drop it.
- **Readings**: mirror it, rotate 90 and 180 degrees, view it tiny; look for accidental letters, body
  parts, offensive or sexual shapes, religious or political symbols, hazard signs.
- **Distinction**: would it be confused with a known mark? Familiar and not yours means it is someone else's.

## Color and type

- One or two colors, ownable in the category, with a one-color black, a white and a brand-mono version;
  it must work in greyscale by value, not only hue; check text contrast on the brand background.
- A white-on-dark version looks heavier: thin it slightly. Provide it as its own import.
- Type: a custom or heavily adjusted wordmark beats a stock font; max two families, optical spacing,
  check the pairs (rn vs m, cl vs d). Ask what the wordmark looks like in all caps and lower case.

## The test list (run on every concept, again on the final)

16 px favicon reads; 32 px reads; huge, curves are smooth; one-color black on white works; one-color
white on black works and is not heavier; works on the brand color, on a photo, on a pattern; greyscale
holds; squint test shows a distinctive silhouette; mirror test shows no hidden proportion errors;
letter test; unintended readings; geometry clean; balanced; shelf test next to competitors.

## Red flags: fix before showing anything

Clip-art literalism or a category cliché with no twist; initials in an unmodified stock font;
more than three colors without a concept; gradients or shadows rescuing a weak form; details smaller
than 1/48 of the mark, hairlines, gaps that close small; near-miss angles and lumpy curves; live text;
a concept that needs a paragraph; anything resembling an existing logo. Never trace or copy an
existing mark; study constructions, then make your own.

## Honesty

You cannot clear trademarks. Say so, and say the name and mark should be searched by the owner before launch.
