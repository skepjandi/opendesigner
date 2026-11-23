# Assets that get users: conversion craft for store listings and launch

A store asset is not decoration. It is the only thing a stranger sees before deciding to install, so
every pixel is judged by one question: **does it make the right person install, and the wrong person
scroll on?** Prettier is not the goal. This playbook is what is known, how well it is known, and the
process to follow. Evidence was read on 2026-11-22; store rules drift, so re-check a rule against the
store's own page before you rely on it (`list_export_presets` carries the checked size and format rules).

## How people decide (the funnel your assets sit in)

1. **Impression** in search, a chart, a feature or an ad: the icon, the name, and on the App Store
   one to three screenshots, or the preview video, or a search-result creative asset (Apple: "the
   first one to three images will appear in search results when no app preview is available").
2. **Tap** to the product page: decided by the icon plus those first frames.
3. **Page view**: people skim the first one or two frames, decide whether the app looks relevant and
   credible, and either swipe or leave. The rest is read only by people already leaning in.
4. **Install**: the screenshots after the first three are for reassurance, not for the headline.

So the order of spend is: icon, frame 1, frames 2 and 3, then the rest, then video and graphics.

## What is measured, and what is only convention

Be honest with the user about this. Never promise a lift.

| Claim | Evidence | Strength |
|-------|----------|----------|
| A different icon concept moved conversion +8 % (a brain beat robots and orange variants, which lost 2.9 % and 3.5 %) | Apple, Product Page Optimization page, Peak Brain Training, 44 days, ~154k impressions per treatment | measured, single case |
| Removing the preview video and leading with a screenshot won by 3 % (Simply Piano) | Apple PPO page and Tech Talk, ~430k impressions per treatment | measured, single case; a video is **not** automatically better |
| A seasonal first screenshot lost to the evergreen one by 1.5 % (Angry Birds 2, 2M impressions per treatment) | Apple Tech Talk | measured |
| Targeted custom product pages convert 2.5 points higher on average than the 1.6 % default | Apple, Custom Product Pages page | audience and creative bundled; not a screenshot-only effect |
| Redesigned screenshot sets lifted conversion 4 to 32 % | vendor case studies (SplitMetrics, Marketing for Apps) | bundles of several changes, vendor-reported; one lifted only 4 % after every earlier test failed |
| In one agency's 40 listing tests the icon won more tests than every other element combined | Aghastya write-up | anecdotal, one marketplace app |
| Caption present, short, on top; device frame; dark background; panoramic set | **no controlled published result found** | convention, not evidence |
| "60 % never scroll past the first screen" and similar statistics | a 2021 vendor post, no method | do not cite |

Read it this way: real lifts are single digits to low tens of percent, they are specific to that
app, and the most reliable lever is what the first frame **claims**, not how it is styled. Treat
styling conventions as defaults and spend variation on the claim.

## Process: brief, hypotheses, build, check, test

1. **Brief.** `get_guidance`, then the workspace brief and memory. Learn the app from its own
   repository, screens, store listing and reviews, never from a guess. Write down:
   - the **why** in one sentence a stranger could repeat (if it lists features, it is not the why),
   - the **audience** (one primary person, one situation),
   - the **three best reasons to install**, ranked by how strongly the audience feels them,
   - the **proof you genuinely have** (rating count, a user's attributed sentence, a real number),
   - the **nearest alternatives** on the store and how the app differs.
   If the app has no proof yet, say so; do not invent reviews, awards, user counts or numbers.
2. **Look at the competition.** Search the store for the primary keyword and read the top results at
   thumbnail size. Note what everyone does (colors, captions, device treatment). Differentiate where
   it costs nothing (a color family, a claim nobody makes); match where deviating confuses people
   (the category's icon vocabulary).
3. **Write hypotheses before pixels.** Each variant is a named bet: "frame 1 says the outcome
   (X), not the feature (Y)". Vary the claim, not only the style.
4. **Hero frame first** (frame 1 at full size and at 160 px), then the system, then the set.
5. **Check at the size people see**: `render_preview` at 160 to 320 px wide; if the claim or the icon
   motif is not readable there, it does not exist. `review_frame` every frame; `validate_document`.
6. **Export with the preset** (`export_set`, `export_panorama`; check `list_export_presets` first).
   A refusal names the rule; fix the design, do not work around the check.
7. **Hand the user a test plan** (below) and `write_memory` what you decided and why.

When the user asks for options, build each as its own named frame, `offer_variants`, then stop until
they pick; `get_variants` for the pick.

## The icon

The icon is seen first and everywhere (search, charts, ads, the home screen) and is often the only
asset in the impression. Rules that hold across Apple's guidance and the published tests:

- **One idea, one shape, readable in about 100 ms** at 60 px. A single strong metaphor beats three
  symbols. Test by rendering at 60, 120 and 180 px and squinting.
- **Express what the app does or is for** (the brain beat the robots because it said "thinking").
  A logo with no meaning is fine only when the brand is already known.
- **High contrast against light and dark store backgrounds**; avoid mid-gray, avoid thin strokes,
  avoid text unless it is one letter or a short, large wordmark. Illegible small text is the most common failure.
- **No iOS transparency; keep the motif in the central 80 %.** Play wants a 512 px 32-bit PNG;
  never reuse another product's marks or a look so close to one that it could mislead (Play policy).
- **Differentiate from the category without leaving it.** Look at the top ten; be recognizable as the
  same kind of app and different at a glance.
- Show the user 2 to 3 genuinely different concepts (not recolors) with `offer_variants`; icons
  are the one place where a test on 3 alternates is cheap and well supported by Apple's tooling.
  Details: `get_playbook` `logo`.

## Screenshots: the conversion sequence

Treat the set as a short argument, not a gallery.

- **Frame 1 states what the app is for, as a point of view or an outcome** ("Talk to them. Not to
  yourself.", "Your money finally has a copilot.") and shows the part of the app that proves it.
  Feature-first openers ("Advanced filters") are weaker by convention; in a 37-app read, 23 of 31 captioned
  sets opened with a point of view.
- **Frame 2 proves it** with the real product doing the thing. **Frame 3 gives a second, different
  reason** (the outcome, the speed, the unique mechanism). Frames 1 to 3 must work alone because
  search shows them and many people never swipe further.
- **Order by how the user feels it**, then: problem, mechanism, feeling, outcome, trust. Put the most
  loved or most unusual feature early, not the settings screen.
- **One claim per frame. Never repeat a claim.** Two frames on the same screen with different
  captions is a flaw.
- **Captions: a short phrase that adds to the picture, not describes it** (Apple's wording). Convention
  that works: eight words or fewer, second person, a verb, a full stop, top of the frame, large enough
  to read at 160 px wide. Nothing the screen already says.
- **Show real UI.** App Review requires that more than half of the screenshots show the app in use,
  and Play forbids graphics that misrepresent functionality. Illustrations and brand frames are
  extras, never the majority.
- **Composition rhythm.** Five identical tiles with a caption each is the weakest and most common
  pattern. Vary scale, crop of the product, ground color and focal point from frame to frame while
  the type, grid and palette stay the same. Panoramas (`export_panorama`) give continuity across
  frames and a reason to swipe; make sure each slice still carries its own claim.
- **Landscape vs portrait.** Portrait is the default for phone apps; landscape only when the app is
  landscape (some games), and then test it.
- **Social proof** (rating, count of users, an attributed quote) belongs inside a frame that still
  shows the app, never alone, and only if true and verifiable.
- **Use the latest device frame and a current UI.** Apple asks for the latest UI; stale chrome
  signals an abandoned app. `device_template` builds neutral frames; do not imitate a manufacturer's
  artwork. If you use Apple's own bezels, they are to be used whole and straight (no tilt, crop,
  shadow or elements leaving the screen) per Apple's Marketing Guidelines.
- **Text is localized text.** Per language, use `set_localized_text`, and re-run `review_frame`:
  longer translations overflow. Localize the screenshots for the biggest markets first; a layout that
  follows local conventions (one case study saw +36 % in Japan from redesigning, not translating) beats a literal translation.

## Policy rails (violations get assets rejected or listings removed)

| | Apple App Store | Google Play |
|---|---|---|
| Price, discounts, URLs | not in assets | not in assets ("free", "10 % off") |
| Awards, rankings | no Apple-designated recognitions (Editor's Choice, Design Award...), no unverifiable claims | no "#1", "Best", "App of the year", award icons |
| Other platforms | no other stores' logos or references | no comparisons to other apps, no endorsement implied |
| Content | must suit a 4+ audience even for an older-rated app | suitable for a general audience |
| Text | short phrase that enhances, localized | "minimize your use of text"; the existing preset treats a fifth of the area as the limit (re-verify) |
| Misrepresentation | screenshots show the app in use (more than half) | no graphics that misrepresent function |

Anything in a claim must survive the next build: do not write "No servers" on a frame if a crash
reporter ships next month. Every figure on a frame must come from the user's data; ask if you
cannot find it.

## App preview and promo video

- **Not automatically a win.** The one published Apple test of video vs screenshot-only favored
  screenshot-only (+3 %). Make the video only if you can make it better than frame 1; otherwise
  spend the effort on screenshots. Test with and without.
- **It autoplays muted and loops.** It must work silently; start with the value proposition in the
  first seconds, not a logo sting; avoid fast cuts; loop without a jump; choose an impactful
  **poster frame** (the first image before it plays).
- **Show the real app.** Apple wants real UI or gameplay and a beginning, middle and end; 15 to 30 s
  for App Store previews (`export_video` with the preview preset validates it).
- Play's promo video is a YouTube link; the feature graphic overlays it, so design the graphic to
  stand alone.
- Direction: `get_playbook` `video` and `motion`.

## Google Play specifics

- **Feature graphic** (1024x500): the banner above the screenshots and behind the video play button.
  One idea, the icon or brand mark plus a three-to-six-word promise, minimal text, important elements
  toward the center (it is cropped on different devices). Device imagery is discouraged.
- **Icon** 512 px; **short description** 80 characters is the second thing read in search: it should
  state the biggest benefit (it is text, so tell the user it is theirs to write).
- Screenshots for every supported form factor (tablets, foldables) increase reach; do not stretch a
  phone shot onto a tablet. Separate renders per language when the shots carry text.
- Play's own store listing experiments are free; use them (below).

## Custom product pages and paid traffic

When users arrive from an ad, a link, a keyword, a creator or a landing page, **the first three
screenshots must say what the source promised** (message match). Apple allows up to 70 custom product
pages, each with its own screenshots, previews and promo text. For each audience:

- frame 1 mirrors the ad/keyword/audience promise,
- frame 2 proves it with real UI,
- frame 3 shows the outcome or the speed to first value.

Ask the user for the audiences, keywords or campaigns. If none, start with the two or three
different "reasons to install" from the brief and make one page for each.

## Off-store assets (launch and discovery)

[INFERENCE: no controlled data was found for these; the rules are craft convention.]

- **Open Graph / link card** (1200x630, `web-og`): readable at 300 px wide; a claim plus the product in
  use plus the name; keep text inside the center. It is the first impression on every shared link.
- **Landing page hero**: the product in use, not an abstract shape; one claim; the store badge.
- **Social posts and short video**: first second carries the claim; vertical 9:16 with text inside the
  platform's safe zone (`formats`); captions burned in because most play muted.
- **Launch directories and press kit**: the same icon, the same three claims, one clean hero image, one
  30 s video; export at the stated size. One look across all of it so the product is recognized.

## Test plan to hand over

Store tools measure; you propose. After you finish a set, give the user:

1. The **hypothesis** of each variant in one line.
2. **One change per test** (icon, or frame 1 claim, or screenshot order, or video yes/no). Apple: test
   one asset at a time; Google: the same.
3. **Run length**: at least a week and in full seven-day weeks, until the store shows at least 90 %
   confidence; do not stop on a good day. Apple tests run up to 90 days, up to three treatments, and
   a test cannot be changed once started. Google: run on the main listing and each key language.
4. **Priority order**: icon and frame 1 claim first, then screenshot order, then the video,
   then captions and style.
5. **Metrics**: product page conversion from search and from browse separately, then downloads;
   for subscriptions, look at retention and paying users, not only installs.
6. **Expect single digits.** Apply a winner only when the tool says it is one; a loss is a result.

## Anti-patterns (what gets scrolled past)

- A first frame that is a splash, a login, a settings list or a logo.
- Feature-list captions ("Filters. Tools. Sharing."), long captions, captions smaller than 28 px at
  1320 px wide, light-gray text on a pastel ground.
- Five identical tiles; the same screen twice; the whole set being illustrations.
- Fake or stock social proof; awards that do not exist; an Apple recognition badge; price or "free".
- Stale UI, an old device, a status bar from another platform (Android sets are their own renders).
- A tiny device with a huge empty background: the product is the reason, make it the hero.
- Copy that only makes sense to the maker's internal vocabulary.
- An icon that needs a caption, or that looks like another app.

## Self-check before you say done

- Can a stranger say in five seconds what the app is for, from frame 1 alone, at 160 px wide?
- Do frames 1 to 3 each stand alone and say three different things?
- Are more than half of the frames the real app in use? Is every claim true today?
- Is anything an award, a price, a URL, another store's mark, or a "best"? Remove it.
- Is every caption inside the safe area and legible at thumbnail size, in every language?
- Does the icon survive at 60 px in both light and dark mode?
- Did you list the hypotheses and the one-change-per-test plan for the user?
