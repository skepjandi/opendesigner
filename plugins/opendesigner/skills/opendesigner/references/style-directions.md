# Style directions: pick a look before you build

Agents given "make it modern and slick" all produce the same glow, gradient and bounce. A look is a
short list of rules, and the rules are what make a piece feel designed. **Before you build, choose
one direction (or write your own as a rule list), say which one in a line, and follow it.** The
rules below are written for what OpenDesigner can draw: frames, shapes, text, images, device
frames, overlays, eased motion and cuts. Ease names map as: snappy = `ease_out` with short times;
settled = `ease_in_out`; playful = `back`.

## Swiss grid and kinetic type
For brand openers, event titles, bold statements.
- One typeface, two weights, a 12-column grid with 4% margins; every layer's x and width land on it.
- One signal color on a neutral field; exactly one element per frame carries it: the subject.
- Type enters by sliding up in short snaps (250 ms, `ease_out`), never by fading in; things leave by
  cutting (`exit: none`).
- No shadows, no overshoot, no centered-everything layouts.

## Flat vector explainer
For product promos, onboarding, how-it-works.
- Four to five flat colors, no gradients, shapes with at least 3 px outlines or none.
- Enter with a small overshoot (`pop` + `back`, 300 ms); group items stagger by 80 to 120 ms.
- A change of scene is carried by a shape: a rectangle that grows to fill the frame and becomes the
  next background (two clips: the second frame starts with that color) beats a crossfade.
- One analogy for the whole piece (a pipe, a post office, a kitchen); do not mix metaphors.

## Dark tech keynote
For launches, AI and developer products, feature reveals.
- Near-black background token, hairline lines at 10% opacity, one accent color on one thing.
- Slow, long moves: `ease_out` at 600 to 900 ms; a slow push (1.00 to 1.06) on every shot.
- One giant number or phrase at about 30% of the frame height per piece, with its unit in small type.
- No glow, bloom, lens flares or particle confetti: depth comes from contrast and space.

## Living UI screencast
For tutorials, feature tours, changelogs, app previews.
- Use real captures (footage, or screenshots in `device_template` frames), real copy, the app's own
  colors. Never invent features or fake screens.
- One action every 3 to 4 seconds; a push of 1.3 to 1.6 on the detail that matters (a second clip of
  the same footage with a higher `scale_from` and a short `zoom_in` transition), then back out.
- Highlights are overlays: a rounded outline or soft spot over the control (`fade`, 200 ms), a
  one-line caption that names the step, not the click.
- A cursor is a small overlay frame that moves between steps (one overlay per stop, short slides).

## Data story
For metrics, reports, investor updates.
- One reading per screen. Draw bars or lines as shapes grown from the baseline (`slide_up` overlays,
  `ease_out`, 400 ms); finish a number and hold it 1 s.
- Every number must come from data the user gave you or from the repository. Never invent a chart.
- Labels at least 3% of the frame height so they read on a phone; no decorative grid lines; one
  annotation that says why it matters.

## Warm editorial (print feel)
For lifestyle, culture, indie brands, creative tools.
- An off-white or warm paper background token, two or three ink colors, a serif display face with a
  clean sans for small text, generous margins, an asymmetric layout (headline left, image right).
- Flat color blocks and rules instead of shadows; photographs cropped tight; captions in small type.
- Motion is calm: `fade` and slow `slide_up`, long holds. Cuts on beats of the story.

## Soft product (light and friendly)
For consumer apps, wellness, finance for people.
- Light background with one soft brand tint, a large rounded device frame centered or bleeding off
  the bottom, 28 to 40 px corner radii everywhere, friendly rounded type.
- Headlines in sentence case, 2 lines, one highlighted word at most.
- Motion: `pop` for badges, `slide_up` for headlines, `ease_out`, nothing faster than 250 ms.

## Neo-brutalist
For developer tools, creative studios, anything that wants an attitude.
- Hard 3 to 4 px black outlines, flat saturated blocks, offset hard shadows (a second shape behind,
  shifted 8 to 12 px), monospaced or heavy grotesque type, deliberately uneven alignment.
- Motion: hard cuts, `slide_left`, no easing flourishes (`linear` or `ease_out` short); stack, don't float.

## Rules for every direction

- Write the direction as 4 to 6 rules at the top of your reply, then follow them; critique your own
  work against them in the review step.
- Mix two directions only if one is the base (colors and type) and the other supplies motion.
- Your own brand always wins: if the repository has a brand (colors, a logo, a typeface), the
  direction adapts to it, not the other way around.
