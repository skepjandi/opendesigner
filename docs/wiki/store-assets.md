# Store Assets

> **Status: Design.** "Today" sections describe the two reference projects as read on 2026-11-19. The studio is intent. Store rules change; every rule here carries its source and must be re-checked before the preset ships ([invariant 9](../../AGENTS.md)).

## Today: how GitNotēs and OpenMedia make store art

Both pipelines are scripts a coding agent writes and edits, driven by text instructions:

| | GitNotēs | OpenMedia |
|---|---|---|
| Script | `scripts/store-assets/render.mjs` + `slides.mjs` | `scripts/render-store-assets.mjs` |
| Input | Raw simulator captures `assets/store/raw/{iphone,ipad}/` (8 each) | Captures `assets/store/captures/<family>/<slug>.png` |
| Method | HTML per slide rendered by headless Chrome, post-processed with `sharp`; fonts bundled | One **panorama** per set: a single wide scene sliced into one image per store slot; Chrome headless + ImageMagick |
| Copy and layout | `slides.mjs` (`SLIDES`, `CREATIVES`) | `slides` array with markers (`*accent*`, `|` line break) in the script |
| Outputs | iOS 6.9", 6.3", iPad 13", App Store creative assets (3 sizes), Android phone and 10" tablet, feature graphic | iPhone, iPad, Android phone/tablet, Mac, feature graphic |

What a human does in that loop: type "bigger device, headline on two lines, less purple", wait for a render, open 8+ PNGs, find what is off, type again. Position and spacing live in code the user never sees; a one-pixel margin fix is a prompt. Per the OpenMedia changelog, a redraw of the whole set was needed when the app UI changed, and each set's last-mile check was "by eye on each strip", with App Store Connect's own preview still unverified.

## Store rules the studio encodes

From GitNotēs' `assets/store/README.md` (which cites Apple and Google), `[INFERENCE: re-verify each against the live source before shipping a preset]`:

| Store | Slot | Rule |
|-------|------|------|
| Apple | iPhone 6.9" | portrait 1260×2736, 1290×2796 or **1320×2868** |
| Apple | iPhone 6.3" | portrait **1206×2622** or 1179×2556; the required iPhone slot |
| Apple | iPad 13" | portrait **2064×2752** or 2048×2732; required when the app runs on iPad |
| Apple | all | 1–10 screenshots per size, PNG/JPEG, **no alpha** |
| Apple | creative assets | universal 5244×2950, header 3840×1646, search 3840×2560, with an "Art Safe Area" per placement; no pricing, URLs, awards or other platforms' marks |
| Google | phone | 2–8 screenshots, JPEG or 24-bit PNG (no alpha), each side 320–3840 px, long side ≤ 2× short side; 9:16 recommended |
| Google | 7"/10" tablet | up to 8, width 1080–7680 px |
| Google | feature graphic | exactly **1024×500**, no alpha, device imagery discouraged |
| Google | text | tagline ≤ 20% of the image; no "Best"/"#1"/price/call-to-action text |

Edge cases the existing scripts already learned, which presets must carry: Google's 2:1 limit means an iPhone 17 Pro Max capture (2.17:1) cannot be reused as-is, so Android sets are separate renders with an Android status bar instead of iOS chrome; Apple lists 6.9" as optional-with-fallback and 6.3" as the mandatory slot.

## The studio (intended)

A page type for listing art whose unit is a **set**, not a single frame:

- **Presets.** `store` crate data files: id, store, slot, size, format, alpha rule, count limits, safe areas, text-share limit, `source` URL, `checked` date. Built-ins plus project overrides in `presets/`.
- **Slides as frames.** A screenshot set is N frames in one page sharing a backdrop. **Panorama mode** lays the frames edge to edge on one wide canvas so the background, ribbon and zoom cards cross seams (OpenMedia's technique), and slices on export.
- **Device frames.** `device` nodes with a screen slot; drop a capture in, scale and crop by hand.
- **Copy as nodes.** Headlines are `text` nodes with the accent marker as a style run, not a string template. Localization is a set of per-locale text overrides, so one design exports `en`, `ja`, … without re-prompting.
- **Capture import.** Drag raw simulator or emulator captures in; optional helper commands (`xcrun simctl io booted screenshot …`, as the scripts document) are surfaced, not automated, unless the user asks their agent to.
- **Validation.** `validate_export` checks every output against its preset: size, format, no alpha, count, safe area, text share, forbidden words. A failure names the file, the rule and the fix. The checks run from the GUI, the CLI and MCP identically.
- **Web assets.** Presets for Open Graph and social cards, favicons and touch icons, app icons (the full iOS/Android/desktop icon set from one source, as GitNotēs' `generate-branding.js` does), hero images and short loops.

## What the GUI changes versus the script

| Today (text → script → PNG) | Studio |
|------------------------------|--------|
| "Move the device 40px left" | Drag it, or pin a note and let the agent do it |
| Spacing and sizes live in code | Tokens and auto layout; the agent edits values you can see |
| Whole set re-rendered per tweak | Live canvas; export on demand |
| Eyeballing sizes and alpha | `validate_export` and a per-slot checklist |
| A redraw when the app changes | Swap the capture inside the device frame; layout stays |
| Chrome and ImageMagick required | None; `render` is in-process |

## Migration

An importer (Phase 4) that reads a reference project's `slides` data and captures into a studio page is attractive but optional; the first goal is that the two repositories can adopt OpenDesigner for their next release by pointing their agents at the skill.
