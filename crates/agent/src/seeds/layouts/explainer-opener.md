# Explainer opener: 9 s, 16:9

Boxes are fractions of the frame (x, y, width, height).

- sky, clouds, hills, dust: whole frame, far to near; they loop for the whole video.
- eyebrow (0.06, 0.15, 0.50, 0.07): the series or brand name, small capitals.
- title (0.06, 0.24, 0.52, 0.34): the headline, left column; the last two words take the accent.
- hero (0.62, 0.34, 0.26, 0.48): the character, right of the headline, feet on the hills.
- bubble (0.50, 0.12, 0.32, 0.18): points at the hero; its tail follows the hero's box.

Swap: `brand.headline` for the title, `brand.tagline` for the bubble, `brand.name` for the eyebrow.
Change the character with apply_ops on `block:character#1` parts; the expression changes with `expression`.
Keep the left column for words and the right for the picture; do not move the bubble above the title.
