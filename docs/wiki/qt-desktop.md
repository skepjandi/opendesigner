# Qt Desktop

> **Status: Partially implemented.** The shell exists as the `app` crate (`crates/app`): a Qt Quick window built with Cargo and `cxx-qt`, with a layers panel, a canvas, an inspector and an agent panel. It was built and run on macOS arm64 with Qt 6.12 from Homebrew; Windows and Linux are unverified. Facts marked as read come from Qt's 6.12 documentation, the `cxx-qt` repository and crates.io page, and KDAB's KDDockWidgets license file, all read on 2026-11-19. API-availability claims from memory are labelled `[INFERENCE]`.

## Why Qt

The shell was first designed on GPUI for raw performance. We switched to Qt 6 (C++/QML) because the shell is not where the risk is: the design canvas and the video preview are custom rendering under either toolkit, and everything around them (text input and IME, accessibility, native dialogs and menus, high-DPI, video playback, packaging) is something Qt already ships and a young Rust toolkit leaves to us. Qt is also the toolkit behind comparable desktop media apps; the point is not a particular name but that the surrounding problems are solved and shipped.

Alternatives considered: GPUI (fast, pre-1.0, small component set, widgets for docking/timeline/rich text all ours to build), iced, egui, Slint (GPLv3 or commercial), Xilem/Vello, Tauri (a webview). The decision is logged in the [Roadmap](./roadmap.md#decision-log). Because the core never imports a UI toolkit ([Architecture](./architecture.md#the-central-rule-the-core-is-ui-agnostic)), the choice is replaceable: only `app/` and the `bridge` crate would change.

Costs, stated plainly:

- **Two languages, one build.** Rust core and Rust glue, QML for presentation, and one hand-written C++ file (the image provider that hands the canvas picture to Qt Quick). `cxx-qt` generates the C++ the `QObject` needs; Cargo drives the whole build.
- **`cxx-qt` is pre-1.0** (0.10.0, 2026-08-24; MIT or Apache-2.0; MSRV 1.85), pinned exactly. The churn risk moved from the UI toolkit to the bridge, but the bridge surface is one `QObject`.
- **Heavier distribution and LGPL obligations** ([Licensing](#licensing)).
- **Qt's own deprecations across minor versions** (it already deprecates media backends, see [Video Editing](./video-editing.md#qt-multimedia-and-ffmpeg)).

## Layers

```
 QML (presentation)  ──►  editor.rs (one QObject, cxx-qt)  ──►  state.rs (Qt-free)  ──►  core / render / store / agent
```

| Layer | Where | Rule |
|-------|-------|------|
| QML | `crates/app/qml/` | Window, panels, canvas interaction, theme. Reads JSON the editor publishes and calls its invokables. No document logic |
| The editor object | `crates/app/src/editor.rs` | `Editor` is a `QObject` with 23 properties (JSON strings and flags) and about 40 invokables. Every action calls `EditorState`, reports a rejection as a message, and republishes the snapshot. Agent edits arrive on the Qt thread through `qt_thread().queue` |
| `EditorState` | `crates/app/src/state.rs` | The editor's state and actions without Qt: open/save, selection, hit testing, drawing, move/resize, the inspector's typed `set`, undo/redo, checkpoints, pins, canvas rendering, export, starting the MCP server, the skill installer. Unit-tested without a window |
| Rust crates | `crates/core`, `render`, `store`, `agent` | Never depend on Qt. Geometry (`core::geometry`: bounds and hit tests) and panel views (`core::view`: layers, frames) live in `core` so the shells only present them |

`cxx-qt` is used in its Cargo-only mode (the `qml_module` of `cxx-qt-build`): no CMake, and exactly one hand-written C++ file. The canvas picture is drawn by the Rust core into raw premultiplied RGBA and handed to Qt Quick as `image://pictures/<id>` by a `QQuickImageProvider` (`crates/app/cpp/picture_provider.cpp`, the only C++ in the repository; it is registered in `crates/app/src/frame_bridge.rs` and installed in `run()`). No PNG is written and no base64 travels. Pan and zoom move that picture, and the core re-renders when the zoom settles. The picture is drawn by two `Image` items that take turns (the new drawing loads in the hidden one and replaces the other only when ready; an `Image` that gets a new source drops its old picture first, which showed as a flash), and every list that changes while an agent works goes through `qml/JsonModel.qml`, which updates only the rows that changed so delegates are not rebuilt.

Two cxx-qt/QML pitfalls worth knowing: a QML property named `onFoo` is parsed as a signal handler (the theme's `onAccent` had to become `textOnAccent`), and an invokable `setX` collides with the generated setter of a property `x` (hence `openFrame`, `setNodeProperty`). Multi-word properties need an explicit `cxx_name` to be camelCase in QML.

## Qt version and modules

Qt 6.12 from Homebrew was used. The exact Qt minor is still to be pinned for releases. **Linked modules** (from `cxx-qt-lib`'s `qt_full` plus the ones in `crates/app/build.rs`): Core, Gui, Qml, Quick Controls 2, Network, Quick (the image provider) and Multimedia (the preview's sound). **QML imports at runtime**: `QtQuick`, `QtQuick.Controls` (the Basic style), `QtQuick.Layouts`, `QtQuick.Dialogs`, `QtMultimedia`. None of these is on the GPL-only list below. `crates/app/build.rs` keeps the linked list in one constant (`QT_MODULES`); adding a module there needs a licensing check in the PR. Qt Test and Qt Quick Test are not used yet.

## Licensing

Qt's open-source licensing (read from Qt 6.12's licensing page): most modules are available under LGPLv3 or GPLv3; some are **GPLv3 only** under the open-source option. LGPLv3 obligations (from Qt's page): provide a re-linking mechanism for the Qt libraries, ship a license copy and acknowledge Qt, make the Qt source available, and accept that modifications to Qt itself are not proprietary. OpenDesigner is MPL-2.0 ([LICENSE](../../LICENSE)), which is compatible with dynamically linking LGPLv3 Qt.

| Rule | Why |
|------|-----|
| Link Qt **dynamically**, never statically | LGPLv3 relinking |
| Ship license texts and a Qt attribution in the app (About dialog, `THIRD_PARTY`) | LGPLv3 |
| Use only modules offered under LGPLv3 | MPL-2.0 project; GPL-only code would force the app to GPL |
| The linked-module list in `crates/app/build.rs` and the QML imports above are an allowlist reviewed in PRs | [Invariant 12](../../AGENTS.md) |

**Banned: Qt modules under GPLv3 only** (Qt 6.12 list): Qt Canvas Painter, Qt CoAP, Qt Graphs, Qt GRPC, Qt HTTP Server, Qt Lottie Animation, Qt MQTT, Qt Network Authorization, Qt Qml Compiler, Qt Quick 3D, Qt Quick 3D Physics, Qt Quick Timeline, Qt Virtual Keyboard, Qt Wayland Compositor. Two are tempting here and are out: **Qt Canvas Painter** (a 2D canvas; our canvas renders through our own pipeline) and **Qt Quick Timeline** (a keyframe animation module, not a video timeline; ours is custom). Qt's tools and utilities are GPLv3 with the Qt GPL exception, which covers the programs we run at build time, not the app. `[INFERENCE]` Whether our QML build flow touches the GPL-only *Qt Qml Compiler* module (versus the LGPL QML cache compiler) is verified when the build is written.

**What the bundler does (found on 2026-11-19, macOS, Qt 6.12 from Homebrew).** `macdeployqt` with the app's QML folder copied the *Qt Quick Timeline* and *Qt Virtual Keyboard* plug-ins into the bundle, although nothing in the app imports them (it bundles the plug-ins that sit beside the modules it does import). Linking only the allowlisted modules is therefore not enough for the shipped bundle. `scripts/package-macos.sh` deletes every file or folder whose name matches a banned module, fails if any remain, and then runs the app's scripted session from the bundle to prove it still works. The build-time question about the *Qt Qml Compiler* stays open: the app's QML is compiled by `cxx-qt-build` through Qt's `qmlcachegen`, and `[INFERENCE]` whether that tool's output carries any license obligation beyond the tool exception above has not been checked against Qt's licensing text.

**The modules added after the first release (checked 2026-11-20, Qt 6.12 from Homebrew).** `Quick` (for the `QQuickImageProvider` that serves the canvas picture) and `Multimedia` (for the preview's sound) are both offered under the LGPL: the SPDX header of their own headers reads `LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only`, so the LGPLv3 option is available and neither is on the GPL-only list above. Qt Multimedia's FFmpeg backend is the default on macOS, but nothing in the app depends on it: the sound the preview plays is a WAV that `od_media` mixes with the user's own `ffmpeg`, the same way the export does, and the app runs no decoder of its own through Qt. Qt's bundled FFmpeg would be a GPL build (Homebrew's `ffmpeg` here is `--enable-gpl`), so a bundle that carries Qt's FFmpeg libraries would break invariant 10; `scripts/package-macos.sh` refuses a bundle that contains them.

**Banned: GPL-only third-party libraries.** KDDockWidgets, the obvious docking library, is GPLv2/GPLv3 (or commercial) per its `LICENSE.txt` (read), so it is out; docking is built in QML with Qt Quick Controls' `SplitView` and our own panel manager.

**Alternative:** a commercial Qt license removes these limits. Not planned; the invariant exists so that choice is never forced by accident.

## Platform requirements

| Platform | Needs |
|----------|-------|
| All | Rust stable ≥ 1.85; Qt 6 and its `qmake` (`QMAKE=$(brew --prefix qt)/bin/qmake` on macOS) |
| macOS | Xcode and command line tools; Metal through Qt's RHI `[INFERENCE]` |
| Windows | MSVC; Direct3D through Qt's RHI `[INFERENCE]` |
| Linux | Qt's X11 and Wayland platform plugin dependencies, Vulkan or OpenGL drivers; deployment is not automated by Qt's tools on Linux/X11 for FFmpeg (read), so packaging needs its own step |

**CI.** `.github/workflows/ci.yml` has one editor job per platform, each building the `app` crate and running `cargo clippy -p opendesigner-app` and `cargo test -p opendesigner-app` (the unit tests and the offscreen scripted session): `app-macos` (Homebrew Qt), `app-linux` (Ubuntu 24.04 with the distribution's Qt 6.4 packages, the set `packaging/linux/Dockerfile` uses, `QT_QPA_PLATFORM=offscreen`, `QT_QUICK_BACKEND=software`) and `app-windows` (Qt 6.8.3 for `win64_msvc2022_64` through `jurplel/install-qt-action@v4`). Only the macOS job has been run by hand. **The Linux and Windows jobs are unverified until a CI run is green**: they were written without being run, the Windows one in particular (the Qt arch name, the `QMAKE` path from `QT_ROOT_DIR`, and whether the scripted session works offscreen there are assumptions). They are separate jobs, so a failure in one does not hide another.

Details and commands: [Development Guide](./development-guide.md).

## Window layout

One editor window; the same panel set OpenCut's desktop shell uses (browser, preview, inspector, timeline) plus ours:

```
┌──────────────────────────────────────────────────────────────┐
│ toolbar: workspace · language · format · undo · layout · export│
├───────────┬──────────────────────────────────┬───────────────┤
│ layers /  │                                  │ inspector     │
│ assets /  │   canvas (design) or             │ (properties,  │
│ pages     │   preview (video)                │  tokens)      │
│           │                                  ├───────────────┤
│           │                                  │ agent panel:  │
│           │                                  │ chat, pins,   │
│           │                                  │ turn history  │
├───────────┴──────────────────────────────────┴───────────────┤
│ timeline (video pages) / checkpoints strip (design pages)    │
└──────────────────────────────────────────────────────────────┘
```

The agent panel is the core loop, not a sidebar: pins, open/resolved state, the turn list with per-turn diff, and restore ([Agent Integration](./agent-integration.md#guidance-context)).

## Customizable layout

The diagram above is only the **default**. Every panel (layers, canvas, inspector, notes, history, memory, video, connect, chat, variants, preview) can be moved anywhere: side by side, stacked, grouped as tabs, in a window of its own, or hidden. Built in `crates/app`: `layout.rs` (pure data and rules, tested without a window) and `qml/Dock*.qml` (drawing and dragging).

- **Model.** `Layout {main, floating, hidden}`: `main` and each floating window hold a tree of `Split {dir row|column, weights, children}` and `Group {panels, active}` nodes. Rules checked after every change (a refused change leaves the layout as it was): each panel is in exactly one place; a group is never empty and names an active tab; a split has two or more children and positive weights; the main window keeps a panel. Operations: `move_panel(panel, group, zone)` (zone: center = join the tabs, left/right/top/bottom = split next to the group), `dock_edge`, `float`, `hide`/`show`, `dock_window` (a closed window's panels become tabs of the main window's last group), `reveal`, `set_active`, `set_weights`, `set_geometry`, reset. A same-direction split nested in a split is merged; a one-child split disappears.
- **Default and migration.** `variants` and `preview` start as tabs after Canvas in the canvas group. A `layout.json` saved before they existed keeps its arrangement and gets them as hidden panels (they come back with the Layout menu, or by themselves when the agent offers options or shows a preview). Any other invalid layout falls back to the default.
- **Doing it.** Drag a tab: over a group's middle (or its tab strip) it joins that group's tabs; over an edge it splits next to it (the drop target is outlined and named while you drag); released outside every panel it gets a window of its own where you let go. Right-click a tab or use its `⋯` menu for the same without dragging (own window, hide, dock at an edge, put in tabs with another group). Drag the thin line between panels to resize. The toolbar's **Layout** menu shows or hides each panel and resets everything. Closing a floating window puts its panels back as tabs.
- **Kept.** The arrangement is saved to `layout.json` in the app's config folder after every change (window position and size after you stop moving a window) and restored on launch; a file that is missing, from another version or not valid gives the default.
- **Limits.** Moving a panel between groups recreates it (a half-typed chat message is lost; the chat history is not, it lives in the app). Keyboard shortcuts work in the main window only (a floating window has no shortcuts of its own, so typing in it can never trigger "delete the selection"). Dragging across windows hit-tests the registered groups in creation order, so a window stacked over another may lose a drop to the one underneath. KDDockWidgets would do all of this but is GPL-only, so the dock is ours ([Licensing](#licensing)).

## Transparency on the canvas

A frame with no fill is transparent, not white. The canvas keeps the picture's alpha and draws a
checkerboard behind it, so a frame meant to sit over footage reads as empty instead of as a broken
white screen. Exports flatten onto white unless the export dialog's "keep the transparency" is
ticked, and the export result says how much of the frame was transparent and what the file shows
there.

## Agent-first interface and shortcuts

> **Status: Implemented (macOS), canvas tools added.** The app is still agent-first: an agent creates the frames, shapes, text and videos, and the user points, corrects and, since the canvas overhaul, can also draw and arrange by hand. The **window toolbar** keeps only what is global: logo, workspace picker, language, format, undo/redo, the **Layout** menu and **Export**; the rest of what the old "more" menu held (colors, languages, open another project, save, shortcuts) is in the application [menu bar](#menu-bar). Everything that only means something on the canvas is in the **canvas toolbar**, a bar inside the canvas panel (`qml/CanvasToolbar.qml`, wrapping onto more rows when the panel is narrow): tools **Select**, **Hand**, **Note**, **Rectangle**, **Ellipse**, **Text**, **Frame**; the overlay, snapping, rulers and pixel-grid toggles; zoom out, the percentage (click for 100%), zoom in, fit frame, fit selection; align, distribute and stack buttons (`qml/ArrangeGroup.qml`, also used by the inspector); group, ungroup, duplicate. This reverses the earlier decision that the app has no drawing tools; images and device frames still come from the agent, the menus and drops, not from a tool, and the window shows no size or name prompts. Files dropped in the window go to the agent's memory (the project's `media/` folder) and are never placed on the design.

Keys follow what Figma and Penpot agree on, read on 2026-11-19 from Penpot's shortcut tables (`penpot-docs` `user-guide/introduction/shortcuts.njk`) and Figma's help and community lists. macOS only; Qt writes Command as `Ctrl` in key sequences.

| Action | Keys | Taken from |
|--------|------|-----------|
| Select / Hand / pin a Note | `V` / `H` / `C` | Figma and Penpot: V, H, and C for comments (our pins are notes) |
| Draw a rectangle / ellipse / text / frame | `R` / `O` / `T` / `F` | Figma (`R`, `O`, `T`, `F`); Penpot uses `R`, `E`, `T`, `B` |
| Pan | two-finger scroll, hold `Space` and drag, `H` and drag, middle-drag | Figma, Penpot |
| Zoom | pinch, `⌘` or `⌥` + scroll, `⌘+` `⌘-`, `=` `-` | Figma (`⌘`), Penpot (`⌥` scroll, pinch, `+` `-`) |
| Fit / zoom to selection / 100% | `⇧1` / `⇧2` / `⌘0` | Figma (`⇧1`, `⇧2`), Penpot (`⇧1`, `⇧2`); Figma lists `⌘0` for 100% (Penpot uses `⇧0`) |
| Next / previous frame | `N` / `⇧N` | Figma |
| Next / previous layer | `Tab` / `⇧Tab` | Figma |
| Move the selection | arrows 1 px, `⇧` arrows 10 px | Figma, Penpot |
| Bring forward / back, front / back | `⌘]` `⌘[`, `⌥⌘]` `⌥⌘[` | Figma (Penpot uses `⌘↑` `⌘↓`) |
| Edit the selected text | `Return` | Penpot (start editing) |
| Hide / show the editor overlays | `⇧H` | ours |
| Deselect | `Esc` | both |
| Undo / redo | `⌘Z` / `⇧⌘Z` | both |
| Delete | `⌫` | both |
| Export | `⇧⌘E` | both |
| Talk to the agent | `⌘L` | ours (as in editors with an agent chat) |
| Open / Save | `⌘O` / `⌘S` | macOS |
| Shortcut list | `?` or `⌘/` | Penpot (`?`), Figma (`⌘/`) |

Gestures: two fingers on the trackpad pan, a pinch zooms around the pointer (Qt's `PinchHandler`), `⌘`/`⌥` + scroll zooms around the pointer (2% to 3200%), middle-drag pans. Not done: smart zoom (two-finger double tap) and rotation, which Qt does not deliver to QML as gestures; Magic Mouse swipes were not tried. The keys and gestures are not driven by the window session (it calls the editor's functions, not synthesized key events); they were not checked on a real keyboard here.

## Menu bar

> **Status: Implemented (macOS: built, launched and the native bar read back through System Events; Windows and Linux unverified).** `qml/AppMenus.qml` is the window's `menuBar`: File (new/open/recent/save/export/quit), Edit (undo/redo, cut/copy/paste, duplicate, delete, select all, group/ungroup), Insert (tools, device frames, image, languages, colors), Arrange (z-order, align, distribute, stack), View (zoom, overlays, rulers, snapping, panels, layout, full screen), Agent (chat, connect, variants, preview, history), Window and Help. Items are enabled from editor state; with no project open the File, Window and Help items that need none stay usable.

**Mechanism.** QtQuick.Controls `MenuBar`, `Menu` and `Action`. Since Qt 6.8 Qt Quick Controls menus are shown in the platform's menu bar (on macOS the top-of-screen bar, with the system's Services/Hide/Quit entries and Writing Tools in Edit added by the OS); this was checked with the Basic style the app uses, so no style change and no new Qt module are needed (the licensing allowlist is unchanged). `Qt.labs.platform` was not used: it is the "labs" API in a separate plugin with no stability promise. Where a platform has no native bar the same menus are drawn in the window. A native item gets its shortcut, enabled state and check mark from its `Action`. macOS supplies its own *Enter Full Screen* item, so the app's is shown on other platforms only.

**Shortcut ownership.** Every `Ctrl`/`⌘` shortcut is owned by the menu (no second `Shortcut` for it). Single-key shortcuts (`V`, `H`, `C`, arrows, `Delete`, `Return`, `Tab`, `Esc`, `?`, `N`, `⇧H`, `⇧1`, `⇧2`, `=`, `-`) stay in `main.qml` because they must be off while a text field has focus; the menu shows them as a hint in the label, not as its own key. Undo, Redo, Cut, Copy, Paste and Select All act on the focused text field when one has focus, otherwise on the canvas. Not verified: that a real `⌘C`/`⌘V` keystroke reaches a focused text field before the native menu item.

### Canvas editing

What the canvas does with the pointer (`qml/CanvasView.qml`; every result is an editor call, so each edit is one undo step and reaches the agent as a user edit):

- **Selecting.** Click selects; `⇧`-click adds or removes a layer; dragging on empty space draws a marquee that selects the layers it touches (`⇧` adds). Clicking empty frame space without dragging still selects the frame. The layers panel highlights every selected row and `⇧`/`⌘`-click there adds or removes a layer. Several layers show an outline each plus one box around all; the inspector then shows their count, the union position and size, align/distribute/stack buttons and a stack gap.
- **Moving and resizing.** Dragging a selected layer moves the whole selection; `⌥`-drag duplicates first and moves the copy; `⇧` locks the axis when adding to a selection. Eight handles resize a single layer (not a group); `⇧` on a corner keeps the proportion. The inspector's lock next to W/H keeps the proportion for typed sizes (session only).
- **Snapping.** While moving and resizing, the moved box snaps to sibling edges and centers and to the frame's edges and center within 6 document pixels; the guides are drawn in the overlay. Hold `⌘` (Ctrl) to skip snapping; the magnet button and View > Snapping switch it off for the session.
- **Drawing.** With a drawing tool a drag draws the shape (`⇧` for a square or circle); a click makes one at a default size (rectangle 160 × 120, ellipse 160, text 240 × 60, frame 400 × 300). The new layer is selected and the tool returns to Select.
- **Rulers and grid.** Rulers along the top and left edges show document pixels with ticks that follow the zoom, the pointer position and the extent of the selection; they start off and switch on from the toolbar or the View menu. From 800% zoom a pixel grid is drawn (toggle in the toolbar). Both are session state.
- **Context menu.** Right-click: duplicate, group, ungroup, copy, paste, bring forward, send backward, to front, to back, delete, each enabled by what the editor says is possible.
- **Not done.** Spacing labels while holding `⌥` over another layer; rotation; dragging guides out of the rulers.

## Canvas rendering

The canvas is a picture of the open frame, with overlays (selection handles, guides, snapping, pins, the agent-change outline) as ordinary QML items above it, not part of the document. The pixels come from our own rasterizer, so the canvas and the export are the same drawing; the export is the product, the canvas is a preview of it.

**How the pixels get there (built).** `render` draws the frame into horizontal tiles of 32 rows; a tile's pixels depend only on the document and the tile's row, so a tile drawn on its own is byte for byte the tile of a full draw. The editor keeps the drawn frame (`render::Retained`) and, after an edit, redraws only the tiles the touched nodes reach: the core says which nodes an edit touched (`Session::touched_since`) and each node's rows are recorded at every draw. A frame that cannot be drawn that way (a different size, scale or token set, a root frame whose own look changed, a 3D device, a node that turns, scales, blurs or casts a shadow) is drawn whole. The picture then goes to Qt as raw premultiplied RGBA through the image provider, so nothing is encoded and decoded on the way.

The options that were weighed: a `QQuickItem` that uploads a texture (a second renderer that has to match the export) and `QQuickPaintedItem` (a baseline). Banned: Qt Canvas Painter (GPL-only).

## Text

Design-grade text (kerning, OpenType features, variable fonts, per-run styles) must shape identically on canvas and in export. Whether Qt's text stack or a Rust shaper in `render` (`rustybuzz`/`cosmic-text`) is the single source is a Phase 0 spike; the answer must be one engine for both.

## Icons

UI icons are [Lucide](https://lucide.dev) SVGs (ISC; Feather parts MIT), vendored unchanged in `crates/app/qml/icons/` with `LICENSE`, and listed into the QML module's resources by `build.rs` (`LICENSE`) and generated into `IconSet.qml` (every `*.svg` in the folder). No emoji or Unicode symbols are used as icons (AGENTS.md). QML cannot read its own resource files, so `build.rs` writes the drawings into a generated `IconSet.qml` (git-ignored); `Icons.qml` (singleton) returns one as a `data:image/svg+xml` URL with `currentColor` replaced by the requested color; `Icon.qml` shows it (`name`, `size`, `color`) so icons follow `Theme` and the hover, active and disabled states with no extra Qt module (no Qt Quick Shapes, no effects). `Btn` takes `iconName` and `tip` (tooltip and accessible name for icon-only buttons). To add an icon, copy it from Lucide into the folder; nothing else is needed. Layers rows use `file`, `frame`, `group`, `square`, `circle`, `type`, `image` for the node kinds.

## Overlays, layers folding

The **overlay toggle** (the eye-in-frame icon in the canvas toolbar, `⇧H`, also in the shortcut list) hides everything the editor draws over the picture: the selection box, size label and handles, snapping guides, the blue "agent" outlines, pinned notes, and the unrendered-image and text-overflow markers. Tools still work while hidden (a click selects, keys move); the button shows as on while overlays are hidden. The state lasts for the session and is not saved in the layout file.

Layers with children show a disclosure chevron; the folded layers are remembered by id for the session, the panel header has Expand all and Collapse all, and selecting a layer on the canvas unfolds the ones above it. The rows keep going through `JsonModel`, so folding changes only the rows that appear or vanish.

## Theme and brand

The app UI uses semantic tokens (surface, interactive, feedback, chart, sidebar groups, following OpenCut's `theme.rs` structure), defined in OKLCH in Rust and handed to QML as colors, with light and dark following the OS. The brand comes from `assets/logo.svg`, a mark of two pointers: gold (`#f2ca83`) is the user, blue (`#5ab0ff`) is the agent. `assets/app-icon.svg` is the same mark on a rounded `#161514` tile.

| Role | Color |
|------|-------|
| User pointer | `#f2ca83` |
| Agent pointer | `#5ab0ff` |
| Icon tile | `#161514` |

Accent (selection, the "agent changed this" outline, focus ring) derives from the gold pointer color and keeps a 3:1 contrast against both canvas backgrounds; the agent-change outline stays distinguishable from the selection color. `assets/app-icon.svg` and `assets/app-icon.png` (1024 px) are the source for the app icon (`.icns`); `assets/logo.svg` is used in the app UI, the README and the wiki header.

## Testing

The editor's logic is unit-tested in `state.rs` without Qt. `qml/SelfTest.qml` drives the real window when the binary is started with `--selftest=<empty folder>`: it creates a project, draws three shapes, edits a property (and a rejected one), undoes and redoes, moves and resizes, pins a note, starts and stops the agent connection, takes and restores a checkpoint, imports a picture, exports against two presets (one must succeed, one must be refused without writing), and grabs the window to a PNG. `crates/app/tests/selftest.rs` runs that offscreen (`QT_QPA_PLATFORM=offscreen`) with a temporary `HOME` and config directory, then checks the files it wrote, that the exported PNG has the imported picture's color where the picture was placed, and that the grabbed window is not blank. The renderer is golden-tested in `render` with no window at all ([Development Guide](./development-guide.md#testing)). Not covered: pointer drags, wheel/pinch gestures and dropping files (the self-test calls the same invokables the gestures call, but does not synthesize input), and any visual judgment.

The self-test also fails on any QML warning in the log (a binding that assigns `undefined`, a missing type, a `Connections` handler for a signal that does not exist), which is how mistakes in the QML are caught: there is no `qmllint` step because it cannot resolve the module registered by `cxx-qt-build` and would only add noise.
