# Product reveal: 9 s, 16:9

- backdrop (a night sky gradient with a breathing moon) and dust: whole frame, behind everything.
- eyebrow (0.06, 0.20, 0.40, 0.07), title (0.06, 0.29, 0.50, 0.30): the copy column on the left.
- device (x 0.64, y 0.08, height 0.84 of the frame): a 3D phone; it turns in with hero_reveal, then floats.
  Put a capture on it with `set` of its `image` (import_file first) or a recording with `footage`.
- badge (0.06, 0.64, 0.20, 0.07): the "New" pill under the headline.
- cta (0.06, 0.76, 0.30, 0.10): left-aligned button, last to arrive.

Swap: `brand.headline`, `brand.name` (eyebrow), `brand.cta`. For a laptop or tablet, change the device's
`model` with apply_ops and widen the right column (x 0.50, height 0.6).
Keep the device the largest shape; the headline is at most three lines.
