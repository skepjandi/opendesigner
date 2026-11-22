# Stats callout: 9 s, 16:9

- backdrop (sky gradient), a scrolling grid floor along the bottom (below 82% of the height) and dust: whole frame.
- eyebrow (0.06, 0.12, 0.40, 0.07): what the numbers measure.
- stat (0.06, 0.24, 0.42, 0.40): the figure (12 characters at most) and its label; `brand.headline` is the label.
- chart (0.54, 0.12, 0.40, 0.72): a donut with up to six slices; replace its `data` with apply_ops on the group's
  parts, or delete it and add_block donut_chart or bar_chart with your own `data`.
- ring: draws around the stat at 5.6 s and breathes.

One number per scene. If you have two, make two scenes; do not shrink the figure.
