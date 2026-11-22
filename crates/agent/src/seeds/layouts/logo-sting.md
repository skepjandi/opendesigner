# Logo sting: 6 s, 16:9

- stars (whole frame, far) and sparks: loop behind the logo for the whole video.
- logo (whole frame, centered): wordmark springs in at 0.4 s, the accent rule opens, the tagline rises at 2.0 s.
  `brand.name` is the wordmark and `brand.tagline` the tagline; pass a logo `image` (list_media) through
  apply_ops after the seed, or add_block logo_sting again with `image` on a fresh frame.
- camera: a slow push-in over the first 4 s, then the picture holds.

End on the held logo, nothing else on screen. Sound: a whoosh on the mark and a chime on the tagline
(beat_times in the result gives the milliseconds).
