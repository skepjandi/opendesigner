# Style packs

A style pack is a look a piece of work commits to before it is drawn: six colours, a type scale, a
shape posture, a motion personality and the rules that hold them together. OpenDesigner ships eight
of them (`list_style_packs`, `get_style_pack`, `apply_style_pack`). Pick **one**, say why, and stay
inside it.

Without this, every design an agent makes lands on the same indigo gradient, the same centred
headline and the same fade-in. The pack is what makes two pieces from the same agent look like they
came from the same studio — and look different from each other.

## 1. Choose from the brief, not from taste

Read `get_guidance` first: the user's notes, the selection, the workspace's brief (`get_brief`) and
their own words. The format and the audience decide the pack more than your preference does.

| The brief says | Audience | Take |
| --- | --- | --- |
| "explainer", "how it works", "in a nutshell", "teach" | everyone | `sunny-explainer` (light, teaching) or `deep-space-flat` (dark, cinematic) |
| "app store", "landing page", "product shot", "onboarding" | users who will use it | `soft-clay` (friendly) or `swiss-kinetic` (precise) |
| "keynote", "pitch", "investor", "launch" | a room, a screen | `swiss-kinetic` (type-led, fast) or `deep-space-flat` |
| "documentary", "report", "editorial", "print" | readers | `editorial-ink` |
| "brand film", "trailer", "announcement", "30 seconds" | a wide audience | `neon-night` or `deep-space-flat` |
| "developer tool", "terminal", "cli", "log", "dashboard" | technical users | `mono-terminal` |
| "handmade", "craft", "workshop", "paper", "gift" | a small, warm audience | `paper-craft` |
| "9:16 reel", "story", "vertical" | social | any pack, but keep the subject in the upper two thirds |
| "print", "A4", "PDF", "deck" | a reader | `editorial-ink` or `paper-craft` |

If two packs fit, pick the one whose **motion personality** matches the tempo the brief asks for:
`calm` for a slow, careful piece, `snappy` for something that must feel quick, `playful` for
something friendly, `cinematic` for something that should feel produced.

Then say it, in one line: *"deep-space-flat, because the brief asks for a calm, wide, night-time
explainer and the amber accent reads against the deep blue."* An unsaid choice is indistinguishable
from no choice.

## 2. Never mix two packs

One pack per piece of work. Do not take the colours of one and the type of another, and do not
"just add a nicer accent" from a different pack — that is how a piece ends up generic.

The one exception is a **user's own brand colour**: when the user gives you a hex value, pass it as
an override to `apply_style_pack` (`{"accent": "#ff8800"}`) so the pack's structure stays and only
the colour changes.

`apply_style_pack` writes the palette into the document as tokens — `bg`, `surface`, `fg`, `muted`,
`border`, `accent`, plus the tints `accent_2` and `shadow` — and locks it. From then on:

- Paint with the tokens (`{bg}`, `{accent}`), never with literal colours.
- The linter (`validate_document`) flags any colour outside the palette as **OD092**, naming the
  nearest palette colour. Fix it by painting with a token or a blend of two.
- Undo the `apply_style_pack` step and the palette goes with it; it is one attributed, checkpointed,
  undoable transaction.

## 3. The 80/20 rule

**80 percent proven pattern, 20 percent one distinctive choice.** Every pack's rules say this in
their own words, and it is the rule that makes a pack a system instead of a mood:

- The 80 percent is the pattern that already works: a clear hierarchy, a grid, one idea per beat,
  a type scale that jumps.
- The 20 percent is **one** choice that is yours: an unusual crop, a shape that arrives from an odd
  angle, one colour used against the rules.

One distinctive choice reads as intent. Five read as noise.

Two rules every pack repeats:

- **The accent appears at most twice per screen.** The accent is punctuation, not a paint. If it is
  everywhere, it is nowhere.
- **No colour of your own.** The palette is the pack. Everything else comes from the tokens.

## 4. The motion of the pack

Each pack carries a personality, an ease and four timings. Use them as they are; the pack exists so
you do not have to choose a curve per element.

| Personality | Ease | Arrives in | Leaves in | Between arrivals | Holds a word | Camera drift |
| --- | --- | --- | --- | --- | --- | --- |
| `calm` | `power2_out` | about 480 ms | about 320 ms | 120 ms | 150 ms | 3% |
| `snappy` | `power4_out` | about 220 ms | about 170 ms | 60 ms | 90 ms | 2% |
| `playful` | `spring(450,0.3)` | about 350 ms | about 250 ms | 100 ms | 120 ms | 5% |
| `cinematic` | `expo_out` | about 450 ms | about 290 ms | 100 ms | 140 ms | 4% |

- `calm` (paper-craft, soft-clay): shapes settle, nothing bounces. Good for craft and product
  stories.
- `snappy` (swiss-kinetic, mono-terminal): things snap into place. Good for type and readouts.
- `playful` (neon-night, sunny-explainer): a spring with a little bounce. Good for friendly,
  teaching work.
- `cinematic` (deep-space-flat, editorial-ink): a long, smooth ease and a slow camera. Good for
  films and for pages that should feel produced.

What that means in practice:

- **Enter** with the pack's ease and its `enter_ms`; **leave** with `exit_ms` (never the same
  length).
- **Stagger** arrivals by `stagger_ms`. Never let everything start together — that is the single
  most common tell of generated work.
- **Hold** each word for `hold_ms_per_word` before it leaves; a word that vanishes while you are
  still reading it is a mistake.
- **Drift the camera** by at most `camera_drift_pct` of the frame. A camera move that is too slow
  reads as a still; too fast reads as a mistake.
- **Linear easing only for a looping rotation** (a spinning ring, a dashed line that marches). Every
  other move eases.
- Reach for the named moves (`animate_preset`) instead of hand-written keyframes: `float` and
  `breathe` for ambience, `rise_in` for entrances, `hero_reveal` for a turn.

## 5. The anti-slop list

These are the defaults an agent falls into. The pack's `anti_patterns` say the same thing in the
pack's own voice, and they apply to motion as much as to layout:

- **No default indigo-to-purple gradient hero.** One flat ground, one accent.
- **No emoji as icons.** Draw the mark, import a vector set (`import_svg_vector`, `search_assets`)
  or set a real glyph.
- **No invented numbers.** No fake statistics, no made-up counts, no invented testimonials: use the
  user's figures, or leave the number out.
- **No stock-photo backdrop.** A scene is built from vector shapes on depth layers; a photo is a
  picture with text on it (the linter's OD090 says so).
- **Nothing centred by default.** Anchor something to an edge, a grid or a third; centred
  everything reads as a template.
- **No linear easing except a looping rotation.**
- **Never everything starting together.** Stagger, and let the camera do a slow move underneath.

## 6. The order of work

1. `get_guidance` — the notes, the selection, the brief.
2. `list_style_packs` — the eight, with their swatches and their motion.
3. `get_style_pack` — the one you are considering, in full.
4. **Say which pack and why**, in one line, before you build.
5. `apply_style_pack` — the palette, the tokens, the lock. One step, undoable.
6. Build with the tokens, the type scale, the shape posture and the motion of that pack.
7. `validate_document` — fix every OD092 and every other warning.
8. `render_preview` and look. Then `review_frame` / `review_video` for the video.
9. Say what the pack decided and what you would change.

A pack is a commitment, not a suggestion: if the work drifts away from the pack, change the pack
(with `apply_style_pack` again, which is one undoable step) rather than patching the colours.
