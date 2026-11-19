# Qt Desktop

> **Status: Partially implemented.** The shell exists as the `app` crate (`crates/app`): a Qt Quick window built with Cargo and `cxx-qt`, with a layers panel, a canvas, an inspector and an agent panel. It was built and run on macOS arm64 with Qt 6.12 from Homebrew; Windows and Linux are unverified. Facts marked as read come from Qt's 6.12 documentation, the `cxx-qt` repository and crates.io page, and KDAB's KDDockWidgets license file, all read on 2026-11-19. API-availability claims from memory are labelled `[INFERENCE]`.

## Why Qt

The shell was first designed on GPUI for raw performance. We switched to Qt 6 (C++/QML) because the shell is not where the risk is: the design canvas and the video preview are custom rendering under either toolkit, and everything around them (text input and IME, accessibility, native dialogs and menus, high-DPI, video playback, packaging) is something Qt already ships and a young Rust toolkit leaves to us. Qt is also the toolkit behind comparable desktop media apps; the point is not a particular name but that the surrounding problems are solved and shipped.

Alternatives considered: GPUI (fast, pre-1.0, small component set, widgets for docking/timeline/rich text all ours to build), iced, egui, Slint (GPLv3 or commercial), Xilem/Vello, Tauri (a webview). The decision is logged in the [Roadmap](./roadmap.md#decision-log). Because the core never imports a UI toolkit ([Architecture](./architecture.md#the-central-rule-the-core-is-ui-agnostic)), the choice is replaceable: only `app/` and the `bridge` crate would change.

Costs, stated plainly:

- **Two languages, one build.** Rust core and Rust glue, QML for presentation, no C++ sources (yet). `cxx-qt` generates the C++ the `QObject` needs; Cargo drives the whole build.
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

`cxx-qt` is used in its Cargo-only mode (the `qml_module` of `cxx-qt-build`): no CMake and no C++ sources. A C++ `QQuickItem` for the canvas stays an option if the Phase 0 canvas spike needs it. The canvas today is a PNG rendered by the Rust core and shown by a QML `Image`; pan and zoom move that picture, and the core re-renders when the zoom settles. The picture is drawn by two `Image` items that take turns (the new drawing loads in the hidden one and replaces the other only when ready; an `Image` that gets a new source drops its old picture first, which showed as a flash), and every list that changes while an agent works goes through `qml/JsonModel.qml`, which updates only the rows that changed so delegates are not rebuilt.

Two cxx-qt/QML pitfalls worth knowing: a QML property named `onFoo` is parsed as a signal handler (the theme's `onAccent` had to become `textOnAccent`), and an invokable `setX` collides with the generated setter of a property `x` (hence `openFrame`, `setNodeProperty`). Multi-word properties need an explicit `cxx_name` to be camelCase in QML.

## Qt version and modules

Qt 6.12 from Homebrew was used. The exact Qt minor is still to be pinned for releases. **Linked modules** (from `cxx-qt-lib`'s `qt_full` plus `Network`, which Qt Qml needs on macOS): Core, Gui, Qml, Quick Controls 2, Network. **QML imports at runtime**: `QtQuick`, `QtQuick.Controls` (the Basic style), `QtQuick.Layouts`, `QtQuick.Dialogs`. None of these is on the GPL-only list below. `crates/app/build.rs` keeps the linked list in one constant (`QT_MODULES`); adding a module there needs a licensing check in the PR. Qt Multimedia, Qt Test and Qt Quick Test are not used yet.

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

**Banned: GPL-only third-party libraries.** KDDockWidgets, the obvious docking library, is GPLv2/GPLv3 (or commercial) per its `LICENSE.txt` (read), so it is out; docking is built in QML with Qt Quick Controls' `SplitView` and our own panel manager.

**Alternative:** a commercial Qt license removes these limits. Not planned; the invariant exists so that choice is never forced by accident.

## Platform requirements

| Platform | Needs |
|----------|-------|
| All | Rust stable ≥ 1.85; Qt 6 and its `qmake` (`QMAKE=$(brew --prefix qt)/bin/qmake` on macOS) |
| macOS | Xcode and command line tools; Metal through Qt's RHI `[INFERENCE]` |
| Windows | MSVC; Direct3D through Qt's RHI `[INFERENCE]` |
| Linux | Qt's X11 and Wayland platform plugin dependencies, Vulkan or OpenGL drivers; deployment is not automated by Qt's tools on Linux/X11 for FFmpeg (read), so packaging needs its own step |

Details and commands: [Development Guide](./development-guide.md).

## Window layout

One editor window; the same panel set OpenCut's desktop shell uses (browser, preview, inspector, timeline) plus ours:

```
┌──────────────────────────────────────────────────────────────┐
│ toolbar: select · hand · note · language · zoom · export · ⋯  │
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

The diagram above is only the **default**. Every panel (layers, canvas, inspector, notes, history, memory, video, connect, chat) can be moved anywhere: side by side, stacked, grouped as tabs, in a window of its own, or hidden. Built in `crates/app`: `layout.rs` (pure data and rules, tested without a window) and `qml/Dock*.qml` (drawing and dragging).

- **Model.** `Layout {main, floating, hidden}`: `main` and each floating window hold a tree of `Split {dir row|column, weights, children}` and `Group {panels, active}` nodes. Rules checked after every change (a refused change leaves the layout as it was): each panel is in exactly one place; a group is never empty and names an active tab; a split has two or more children and positive weights; the main window keeps a panel. Operations: `move_panel(panel, group, zone)` (zone: center = join the tabs, left/right/top/bottom = split next to the group), `dock_edge`, `float`, `hide`/`show`, `dock_window` (a closed window's panels become tabs of the main window's last group), `reveal`, `set_active`, `set_weights`, `set_geometry`, reset. A same-direction split nested in a split is merged; a one-child split disappears.
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

> **Status: Implemented (macOS).** The app is for pointing and correcting, not for drawing: an agent creates the frames, shapes, text and videos and decides their names and sizes. The toolbar therefore has **Select**, **Hand** and **Note** (pin) plus undo/redo, the overlay toggle, zoom, language, **Export** and a "more" menu (colors, languages, open another project, save, shortcuts). There are no rectangle, ellipse, text, frame, image or device tools, and the window shows no size or name prompts. What a user can still do by hand: select, move and resize (drag or arrow keys), edit any property in the inspector, delete, pin notes, undo, change colors (tokens) and languages, and drop files in the window, which go to the agent's memory (the project's `media/` folder) and are never placed on the design.

Keys follow what Figma and Penpot agree on, read on 2026-11-19 from Penpot's shortcut tables (`penpot-docs` `user-guide/introduction/shortcuts.njk`) and Figma's help and community lists. macOS only; Qt writes Command as `Ctrl` in key sequences.

| Action | Keys | Taken from |
|--------|------|-----------|
| Select / Hand / pin a Note | `V` / `H` / `C` | Figma and Penpot: V, H, and C for comments (our pins are notes) |
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

Gestures: two fingers on the trackpad pan, a pinch zooms around the pointer (Qt's `PinchHandler`), `⌘`/`⌥` + scroll zooms around the pointer, middle-drag pans. Not done: smart zoom (two-finger double tap) and rotation, which Qt does not deliver to QML as gestures; Magic Mouse swipes were not tried. The keys and gestures are not driven by the window session (it calls the editor's functions, not synthesized key events); they were not checked on a real keyboard here.

## Canvas rendering

The canvas is a custom item in the Qt Quick scene graph, with overlays (selection handles, guides, snapping, pins, the agent-change outline) as ordinary QML items above it, not part of the document. How the canvas pixels get there is **the** Phase 0 spike; candidates, all `[INFERENCE]` on exact API availability per Qt version:

| Option | Idea | Trade-off |
|--------|------|-----------|
| A. `render` output as a texture | Our Rust rasterizer draws visible tiles; the item uploads them | Matches export by construction; update cost on every change |
| B. Custom scene-graph node / `QRhi` item | Draw primitives on the GPU through Qt's RHI | Fast, but a second renderer that must match export |
| C. `QQuickPaintedItem` / `QPainter` | Simplest | Likely too slow at 10k nodes; a baseline for comparison |

Banned: Qt Canvas Painter (GPL-only). If A and B disagree with export, `render` wins: export is the product, the canvas is a preview of it.

## Text

Design-grade text (kerning, OpenType features, variable fonts, per-run styles) must shape identically on canvas and in export. Whether Qt's text stack or a Rust shaper in `render` (`rustybuzz`/`cosmic-text`) is the single source is a Phase 0 spike; the answer must be one engine for both.

## Icons

UI icons are [Lucide](https://lucide.dev) SVGs (ISC; Feather parts MIT), vendored unchanged in `crates/app/qml/icons/` with `LICENSE`, and listed into the QML module's resources by `build.rs` (`LICENSE`) and generated into `IconSet.qml` (every `*.svg` in the folder). No emoji or Unicode symbols are used as icons (AGENTS.md). QML cannot read its own resource files, so `build.rs` writes the drawings into a generated `IconSet.qml` (git-ignored); `Icons.qml` (singleton) returns one as a `data:image/svg+xml` URL with `currentColor` replaced by the requested color; `Icon.qml` shows it (`name`, `size`, `color`) so icons follow `Theme` and the hover, active and disabled states with no extra Qt module (no Qt Quick Shapes, no effects). `Btn` takes `iconName` and `tip` (tooltip and accessible name for icon-only buttons). To add an icon, copy it from Lucide into the folder; nothing else is needed. Layers rows use `file`, `frame`, `group`, `square`, `circle`, `type`, `image` for the node kinds.

## Overlays, layers folding

The **overlay toggle** (toolbar eye-in-frame icon, `⇧H`, also in the shortcut list) hides everything the editor draws over the picture: the selection box, size label and handles, the blue "agent" outlines, pinned notes, and the unrendered-image and text-overflow markers. Tools still work while hidden (a click selects, keys move); the button shows as on while overlays are hidden. The state lasts for the session and is not saved in the layout file.

Layers with children show a disclosure chevron; the folded layers are remembered by id for the session, the panel header has Expand all and Collapse all, and selecting a layer on the canvas unfolds the ones above it. The rows keep going through `JsonModel`, so folding changes only the rows that appear or vanish.

## Theme and brand

The app UI uses semantic tokens (surface, interactive, feedback, chart, sidebar groups, following OpenCut's `theme.rs` structure), defined in OKLCH in Rust and handed to QML as colors, with light and dark following the OS. The brand comes from `assets/logo.svg`, a stylized antler mark in a warm gold ramp over neutral greys:

| Role | Colors in the logo |
|------|--------------------|
| Gold ramp (highlight → shadow) | `#fcf1d9` `#fadfae` `#f2ca83` `#c7a060` `#a17e47` `#8c6b3c` `#7b5c33` `#573b20` |
| Neutrals | `#b6b6b5` `#959595` `#747474` `#535252` `#313131` |

Accent (selection, the "agent changed this" outline, focus ring) derives from the gold ramp and keeps a 3:1 contrast against both canvas backgrounds; the agent-change outline stays distinguishable from the selection color. `assets/logo.svg` is the source for the app icon (`.icns`, `.ico`, PNG set generated by `xtask`) and the README and wiki header.

## Testing

The editor's logic is unit-tested in `state.rs` without Qt. `qml/SelfTest.qml` drives the real window when the binary is started with `--selftest=<empty folder>`: it creates a project, draws three shapes, edits a property (and a rejected one), undoes and redoes, moves and resizes, pins a note, starts and stops the agent connection, takes and restores a checkpoint, imports a picture, exports against two presets (one must succeed, one must be refused without writing), and grabs the window to a PNG. `crates/app/tests/selftest.rs` runs that offscreen (`QT_QPA_PLATFORM=offscreen`) with a temporary `HOME` and config directory, then checks the files it wrote, that the exported PNG has the imported picture's color where the picture was placed, and that the grabbed window is not blank. The renderer is golden-tested in `render` with no window at all ([Development Guide](./development-guide.md#testing)). Not covered: pointer drags, wheel/pinch gestures and dropping files (the self-test calls the same invokables the gestures call, but does not synthesize input), and any visual judgment.

The self-test also fails on any QML warning in the log (a binding that assigns `undefined`, a missing type, a `Connections` handler for a signal that does not exist), which is how mistakes in the QML are caught: there is no `qmllint` step because it cannot resolve the module registered by `cxx-qt-build` and would only add noise.
