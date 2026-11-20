# Store Assets

> **Status: Partially implemented.** Built: set and panorama export with all-or-nothing validation, generic device frames, swapping a capture by dropping a file on a screen, per-language text and exports (see [What is built](#what-is-built)). Design only: safe areas, text-share and forbidden-word checks, the full web/icon set, capture helpers, the importer. "Today" sections describe the two reference projects as read on 2026-11-19. Store rules change; every rule here carries its source and must be re-checked before the preset ships ([invariant 9](../../AGENTS.md)). The built presets are copied from the reference projects and have **not** been checked against Apple's or Google's own pages (the tools say so).

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

## What is built

| Piece | Where | Behavior |
|-------|-------|----------|
| Presets and validation | `crates/store` | Nine presets: size rule, format, alpha, count. `validate_image`, `validate_set` |
| Export | `crates/agent/src/export.rs` | One entry point, `export_assets(plan)`, for the editor, the MCP tools and the CLI. A plan is one frame, a **set** (several frames, `<prefix>-01.png`, ...) or a **panorama** (one wide frame cut into equal columns). Everything is rendered and validated first, including the count for sets and panoramas; only then are files written, and a failed write removes the ones already written. A refusal names each file and the rule it breaks |
| Panorama | `od_render::slice_columns` | The frame width (times scale) must divide by the slide count; slices are exact pixel columns, so a ribbon or a device placed across a seam lines up |
| Device frames | `crates/store/src/devices.rs` | `phone` (camera island), `phone-punch-hole`, `tablet` (4:3) and `tablet-wide` (16:10): a group of body, `Screen` and camera made of ordinary nodes, so it is editable, undoable and attributed like anything else. These are **neutral shapes with typical proportions, not any manufacturer's artwork**; a screen is a placeholder rectangle until a capture is dropped on it, then an image node with `cover` fit |
| 3D devices | `crates/core/src/device3d.rs`, `od_render::three_d` | A `device3d` node is a phone, tablet, laptop or monitor drawn in perspective inside its own box, with a capture (or a recording) on its screen, turned, tilted and lit. The models are generated from parameters, so they are **neutral shapes with typical proportions, not any manufacturer's artwork**; the box is the viewport, so the same node serves a store screenshot and a video turn ([Document Format](./document-format.md#3d-devices)) |
| Editor | `app` | Device button, Export dialog (frame, set, panorama, with or without a preset), drop a file on the canvas: onto an image it swaps the picture (one undoable edit, layout kept), elsewhere it adds an image; Replace button in the inspector |
| Localization | `core` (`Document::locales`, `set_localized_text`, `localized`), `export_assets(locale)` | Per-language text overrides keyed by text-node id, stored in `od.toml` under `[locales.<tag>]`, changed through history like any edit. A text node without a translation shows its default text. Exports take a `locale` and name set/panorama files `<prefix>-<locale>-NN.png`. The editor has a language switcher (canvas, text field and exports follow it) and a Languages dialog (adding a language copies the current text as a starting point). Only text is translated: images, colors and sizes are shared, so a longer translation can overflow its box. That is detected: `render` reports text that wraps to more rows than its box is tall (`Warning::TextOverflow`, from the layout `cosmic-text` computes), the canvas outlines the box in red, the inspector says by how much, and previews and exports list it under `warnings` (renamed from `not_drawn`; the text is still drawn). A box with no width or no height is not checked, and a word wider than the box breaks into rows (so it shows as height) |
| MCP and CLI | `export_set`, `export_panorama`, `device_template`; `opendesigner export-set`, `export-panorama`, `device` | The same plans and templates |

Videos: `od_store::video` holds app-preview presets (`apple-app-preview-iphone`, `apple-app-preview-ipad`: size, 15-30 s, at most 30 fps, 500 MB), each with its source page and the day it was read (2026-11-19) and what it does not check; `export_video` and `export-video` validate with `--preset` before and after encoding and write nothing on a violation. Google Play's promo video is a YouTube link, so it has no file preset.

Not built: safe areas and the "Art Safe Area" check, text-share and forbidden-word rules, a status-bar overlay, web/social/icon presets, capture helpers (the commands to take a capture yourself are in the `3d` playbook; the app does not run them for you).

## Asset packs: icons, shapes, backgrounds, sounds

> **Status: Built** (`crates/agent/src/assets/`). Read on 2026-11-20; every licence, size and command below comes from the page named, and what could not be read is said so.

The agent used to draw every icon and texture by hand. It now has three sources, chosen so the app and the repository stay small and a project only holds what it uses:

| Source | What | Where it lives | Cost |
|--------|------|----------------|------|
| **Bundled** | Lucide 1.76.0 (ISC; a few icons MIT from Feather): 2,404 line icons with search tags | `crates/agent/assets/lucide.oda`, compiled in | **149,898 bytes** (one zlib stream; 602,873 bytes raw) plus its 3 KB licence text |
| **Generated** | `make_asset`: blob, gradient, aurora, grain, pattern (dots, lines, waves, grid, checker, zigzag, topo), shape (circle, rounded-rect, triangle, star, polygon, ring, burst) | Rust code (`generate.rs`) writes SVG from a seed and the agent's hex colors; `raster.rs` draws it | no data. Deterministic: the same seed, size, colors and variant give the same SVG text |
| **Packs** | Tabler, Phosphor, Heroicons, Iconoir, Bootstrap Icons, and Kenney's Game Icons, Board Game Icons, UI Pack, Shape Characters, Interface Sounds, Impact Sounds, UI Audio | downloaded after the user's yes into `<config>/asset-packs/<id>/`, one compressed container each | 0.17 to 1.4 MB per download (table below); at most 256 MB for all |

**Compact and unloadable.** One container format (`ODA1`: a zlib stream of `name, tags, body` records, `container.rs`) serves the bundled set and every installed pack, so a pack is one file however many thousand icons it holds. A pack keeps only what it names (for example Tabler: the 5,491 `icons/outline/*.svg`; the Kenney sound packs: the `Audio/*.ogg`), which is why Board Game Icons (a 1.04 MB zip full of PNGs) is 56 KB installed. `unload_asset_pack` (or the Unload button) deletes the folder. Nothing goes in a project until an agent imports it: `import_asset` and `make_asset` write one file into `media/` and `media/<name>.asset.json` beside it (source, licence, licence url, whether a credit is required and the credit line; a generated asset's sidecar holds the recipe that remakes it). Nothing is overwritten.

**SVG becomes a PNG at import.** Documents hold png, jpg and webp only, and that did not change. `raster.rs` draws an SVG with `resvg` at the width and height asked for (fitted and centered inside when both are given; the longest side is 512 when neither is; at most 4096 a side and 16 million pixels), after recoloring it: `currentColor` becomes the given hex color, an SVG without `currentColor` has its `fill` and `stroke` colors replaced (`none` and gradients stay), and one that names no color gets a root `fill`. `resvg` is built without text and raster-image support, so an SVG is drawn from its own paths, gradients and filters and nothing it links is ever read.

**Downloads need the user's yes.** `install_asset_pack` from an agent only records a request and answers `waiting_for_user` with the size; the Connect tab ("Asset packs") shows the request and a Download button. The click approves that one download and runs it in the background; the approval is used up when the pack is installed, so installing again after an unload asks again. When a call finds an approved pack it downloads it as a **job** (the job strip) with the editor unlocked. The client is the existing `crates/agent/src/http.rs` (rustls, same-site redirects only, a cap on the answer's size: three times the listed size, between 8 and 64 MB); it gained only an optional `Referer` header. The archive is read by `archive.rs` (zip with stored or deflate members, tar.gz; every size checked before allocating, names that leave their folder dropped). The sha-256 of what came down and the sizes are recorded in `meta.json`. `OPENDESIGNER_ASSETS_URL` points every pack at `<base>/<pack id>.zip|.tgz`; the tests use a local server and never reach the network.

**What was read, and what each source is**

| Source | Is | Licence (read 2026-11-20) | Built in? |
|--------|----|----------------------------|-----------|
| [Lucide](https://lucide.dev) `lucide-static` 1.76.0 | icon set, 2,404 SVGs, npm tarball 7.6 MB (57.5 MB unpacked) | ISC; the Feather-derived icons MIT (`LICENSE` in the package) | **bundled** (inner shapes only, 150 KB) |
| Tabler Icons 3.52.0 (`@tabler/icons`) | 5,491 outline + 1,054 filled | MIT (npm field) | pack, outline only; tarball 1,299,871 B |
| Phosphor `@phosphor-icons/core` 2.1.1 | 1,512 icons x 6 weights | MIT (npm field) | pack, regular weight; 1,443,064 B |
| Heroicons 2.2.0 | 324 icons | MIT (npm field) | pack, 24 px outline; 166,699 B |
| Iconoir 7.12.2 | 1,383 regular + 288 solid | MIT (npm field) | pack, regular; 757,419 B |
| Bootstrap Icons 1.13.2 | ~2,000 icons | MIT (npm field) | pack; 883,452 B |
| [Kenney](https://kenney.nl/assets) | game-art library | CC0, "attribution is not required" ([kenney.nl/support](https://kenney.nl/support)) | packs: Game Icons 1,045,980 B, Board Game Icons 1,039,954 B, UI Pack 1,229,750 B, Shape Characters 546,442 B, Interface Sounds 834,536 B, Impact Sounds 800,850 B, UI Audio 411,949 B |
| Remix Icon, Material Symbols, Simple Icons | icon sets | Apache-2.0, Apache-2.0, CC0 files (the brands' trademarks are not covered) | not offered: more of the same, or brand marks |
| Open Doodles, Open Peeps, Humaaans | CC0 illustrations | distributed as single files / Gumroad downloads, no archive to fetch | not built in |
| WebGradients (MIT), Hero Patterns (CC BY 4.0) | gradient and pattern sets | usable with notice / credit | not built in: `make_asset` gradient and pattern cover them with no credit to show |
| unDraw | illustrations | custom: no redistribution in an app or pack, no AI/ML training use | **link only** |
| Haikei / Blobmaker | generators | terms cover the service and grant nothing over the output | **link only**; `make_asset` blob and gradient replace them |
| SVG Backgrounds | backgrounds | forbids redistribution and use as-is | **link only** |
| [Kitbitz](https://kitbitz.art) | "2,000+ hand-drawn illustrations", Figma plugin | the site says CC0 (its page metadata links the CC0 deed); `/license` is 404 | **not built in**: no terms page and no download interface could be read, so nothing is fetched from it |
| Google Fonts / Fontsource (OFL) | fonts | OFL-1.1 (checked for `@fontsource/inter` only) | not built in: text nodes name a font family; the app does not load font files from packs |
| Pattern Monster, Transparent Textures | patterns | could not be read | not used |

The Kenney download button works by asking `https://kenney.nl/download-link/<hex of "assets/<slug>">` for a signed link. That answers a request that names the pack's page as `Referer` and answers 403 without it, so the client sends it and follows the link (same site). The pages carry `robots: noai, noimageai` and `tdm-reservation: 1`; the packs are CC0 and the download is a single file the user approved, but if Kenney changes the flow the install fails with a message that names the page to download the zip from by hand.

**The MCP servers the user named** are search proxies, not asset libraries, so they are optional external servers (the Connect tab, "Recommended servers", one-click Add; nothing is started or installed by the app):

| Server | What it is | Licence | Key | Command |
|--------|------------|---------|-----|---------|
| `3dassets` ([mcpmarket](https://mcpmarket.com/server/3dassets), repo `3dassets-dev/3dassets`) | thin npm bridge to a hosted catalogue of CC0 GLB models (3D) | CC0-1.0 (README; npm `3dassets-mcp` 1.0.0) | none to search; `THREEDASSETS_API_KEY` only to submit | `npx -y 3dassets-mcp` |
| `sozai` ([sozai-search-mcp](https://github.com/kanaworksai-hue/sozai-search-mcp)) | search index over about 39,000 3D assets (Poly Haven, ambientCG, Kenney, Quaternius, ...); hosts no files; needs `uv` and Python 3.10+; first search downloads about 15 MB; its optional semantic search needs about 600 MB | MIT (code); each library's licence for assets (its `redistributable_only` keeps CC0) | none | `uvx --from git+https://github.com/kanaworksai-hue/sozai-search-mcp sozai` |
| `asset-gate` ([lobehub page](https://lobehub.com/pl/mcp/codepawl-asset-gate) -> `codepawl/asset-gate`) | search proxy over Hugging Face hosted 3D asset packs (0 stars when read) | MIT (code); mixed licences for assets (filter to CC0) | none | `npx -y github:codepawl/asset-gate` |

All three are 3D, which OpenDesigner cannot place yet, so they are recommended for the user's own agent to search, not as a source for designs. **Not verified:** none of the three servers was started by us (the commands, licences and package names were read from their READMEs, `package.json`/`pyproject.toml` and the npm registry); kitbitz.art's MCP could not be read, so it is not in the list.

**Tools** (`crates/agent/src/assets/tools.rs`; read-only where it says so): `list_asset_sources` (read-only), `search_assets {query, kind?, limit?}` (read-only; kinds icon, shape, blob, background, pattern, illustration, font, sound, 3d; font and 3d have nothing built in and say so), `make_asset {generator, width, height, colors?, seed?, variant?, scale?, name?}`, `import_asset {id, width?, height?, color?, name?}`, `list_asset_packs` (read-only), `install_asset_pack {id}` (network), `unload_asset_pack {id}`. Ids are `lucide:<icon>` or `<pack id>:<path in the pack>`. Colors must be hex (`#rgb`, `#rrggbb`, with alpha too): the agent takes them from `list_tokens`.

**Credits.** An import answers `attribution_required` and the credit line. MIT and ISC ask that their notice travel with copies of the icons, so bundled and npm-pack icons are marked `attribution_required: true` and the playbook tells the agent to show the line with the finished work; Kenney and generated assets need none. We do not relicense anything.

**Size accounting** (measured): the repository gains 149,898 bytes of data (+3,208 of licence text); a release CLI binary grew by 1,382,400 bytes (646,246 gzip-compressed) with the bundled set and `resvg`, measured by building `main` and this branch; `list_asset_packs` reports each pack's size on disk, `installed_bytes` and the cap, and the test checks that the reported size is the file's length.

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
