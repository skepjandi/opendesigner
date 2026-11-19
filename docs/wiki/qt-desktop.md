# Qt Desktop

> **Status: Design.** Facts marked as read come from Qt's 6.12 documentation, the `cxx-qt` repository and crates.io page, and KDAB's KDDockWidgets license file, all read on 2026-11-19. API-availability claims from memory are labelled `[INFERENCE]` and are Phase 0 spike items. Nothing is built.

## Why Qt

The shell was first designed on GPUI for raw performance. We switched to Qt 6 (C++/QML) because the shell is not where the risk is: the design canvas and the video preview are custom rendering under either toolkit, and everything around them (text input and IME, accessibility, native dialogs and menus, high-DPI, video playback, packaging) is something Qt already ships and a young Rust toolkit leaves to us. Qt is also the toolkit behind comparable desktop media apps; the point is not a particular name but that the surrounding problems are solved and shipped.

Alternatives considered: GPUI (fast, pre-1.0, small component set, widgets for docking/timeline/rich text all ours to build), iced, egui, Slint (GPLv3 or commercial), Xilem/Vello, Tauri (a webview). The decision is logged in the [Roadmap](./roadmap.md#decision-log). Because the core never imports a UI toolkit ([Architecture](./architecture.md#the-central-rule-the-core-is-ui-agnostic)), the choice is replaceable: only `app/` and the `bridge` crate would change.

Costs, stated plainly:

- **Two languages.** Rust core, C++/QML shell, a bridge between them (below).
- **`cxx-qt` is pre-1.0** (0.10.0, 2026-08-24; MIT or Apache-2.0; MSRV 1.85; needs CMake 3.24+). The churn risk moved from the UI toolkit to the bridge, but the bridge surface is small.
- **Heavier distribution and LGPL obligations** ([Licensing](#licensing)).
- **Qt's own deprecations across minor versions** (it already deprecates media backends, see [Video Editing](./video-editing.md#qt-multimedia-and-ffmpeg)).

## Layers

```
 QML (presentation)  ──►  C++ (QObject glue, custom items)  ──►  bridge crate (cxx-qt)  ──►  core / render / store / media / agent (Rust, no Qt)
```

| Layer | Contents | Rule |
|-------|----------|------|
| QML (`app/qml/`) | Window layout, panels, inspector, layers tree, agent panel, timeline chrome, animations, theme | Bindings and input forwarding only; no document logic |
| C++ (`app/src/`) | Application bootstrap, custom `QQuickItem`s for the canvas and timeline, platform integration (menus, dialogs, file associations), the skill installer's UI | No document logic; thin |
| `bridge` crate | `cxx-qt` QObjects and list models exposing `core` (document, selection, annotations, history, tokens) to QML | The only Rust crate that depends on Qt |
| Rust crates | Everything else | Never depend on Qt |

`cxx-qt` "can be used to integrate Rust into C++ applications using CMake or used to build Rust applications with Cargo", and implements `QObject` subclasses in Rust usable from C++ and QML (read). We start with the CMake-driven C++ application linking the Rust crates; Phase 0 confirms the build story on all three OSes, with a plain C ABI (`cbindgen`) as the fallback if `cxx-qt` blocks us.

## Qt version and modules

Pin an exact Qt 6 minor in Phase 0 (the docs read were 6.12). Use Qt Quick with Qt Quick Controls for chrome; Qt Multimedia for playback; Qt Test and Qt Quick Test for tests. Everything is on the allowlist below. `[INFERENCE]` Whether Qt Quick Controls and the QML tooling we need fall outside the GPL-only list in practice is checked when the allowlist is written; as of the 6.12 licensing page they are not on it.

## Licensing

Qt's open-source licensing (read from Qt 6.12's licensing page): most modules are available under LGPLv3 or GPLv3; some are **GPLv3 only** under the open-source option. LGPLv3 obligations (from Qt's page): provide a re-linking mechanism for the Qt libraries, ship a license copy and acknowledge Qt, make the Qt source available, and accept that modifications to Qt itself are not proprietary. OpenDesigner is MPL-2.0 ([LICENSE](../../LICENSE)), which is compatible with dynamically linking LGPLv3 Qt.

| Rule | Why |
|------|-----|
| Link Qt **dynamically**, never statically | LGPLv3 relinking |
| Ship license texts and a Qt attribution in the app (About dialog, `THIRD_PARTY`) | LGPLv3 |
| Use only modules offered under LGPLv3 | MPL-2.0 project; GPL-only code would force the app to GPL |
| CMake `find_package(Qt6 COMPONENTS …)` list is an allowlist reviewed in PRs | [Invariant 12](../../AGENTS.md) |

**Banned: Qt modules under GPLv3 only** (Qt 6.12 list): Qt Canvas Painter, Qt CoAP, Qt Graphs, Qt GRPC, Qt HTTP Server, Qt Lottie Animation, Qt MQTT, Qt Network Authorization, Qt Qml Compiler, Qt Quick 3D, Qt Quick 3D Physics, Qt Quick Timeline, Qt Virtual Keyboard, Qt Wayland Compositor. Two are tempting here and are out: **Qt Canvas Painter** (a 2D canvas; our canvas renders through our own pipeline) and **Qt Quick Timeline** (a keyframe animation module, not a video timeline; ours is custom). Qt's tools and utilities are GPLv3 with the Qt GPL exception, which covers the programs we run at build time, not the app. `[INFERENCE]` Whether our QML build flow touches the GPL-only *Qt Qml Compiler* module (versus the LGPL QML cache compiler) is verified when the build is written.

**Banned: GPL-only third-party libraries.** KDDockWidgets, the obvious docking library, is GPLv2/GPLv3 (or commercial) per its `LICENSE.txt` (read), so it is out; docking is built in QML with Qt Quick Controls' `SplitView` and our own panel manager.

**Alternative:** a commercial Qt license removes these limits. Not planned; the invariant exists so that choice is never forced by accident.

## Platform requirements

| Platform | Needs |
|----------|-------|
| All | Qt 6 (pinned), CMake ≥ 3.24 (cxx-qt), Ninja, a C++20 compiler, Rust stable ≥ 1.85, `clang-format`, `clang-tidy` |
| macOS | Xcode and command line tools; Metal through Qt's RHI `[INFERENCE]` |
| Windows | MSVC; Direct3D through Qt's RHI `[INFERENCE]` |
| Linux | Qt's X11 and Wayland platform plugin dependencies, Vulkan or OpenGL drivers; deployment is not automated by Qt's tools on Linux/X11 for FFmpeg (read), so packaging needs its own step |

Details and commands: [Development Guide](./development-guide.md).

## Window layout

One editor window; the same panel set OpenCut's desktop shell uses (browser, preview, inspector, timeline) plus ours:

```
┌──────────────────────────────────────────────────────────────┐
│ toolbar: tools · page · zoom · export · agent status         │
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

## Theme and brand

The app UI uses semantic tokens (surface, interactive, feedback, chart, sidebar groups, following OpenCut's `theme.rs` structure), defined in OKLCH in Rust and handed to QML as colors, with light and dark following the OS. The brand comes from `assets/logo.svg`, a stylized antler mark in a warm gold ramp over neutral greys:

| Role | Colors in the logo |
|------|--------------------|
| Gold ramp (highlight → shadow) | `#fcf1d9` `#fadfae` `#f2ca83` `#c7a060` `#a17e47` `#8c6b3c` `#7b5c33` `#573b20` |
| Neutrals | `#b6b6b5` `#959595` `#747474` `#535252` `#313131` |

Accent (selection, the "agent changed this" outline, focus ring) derives from the gold ramp and keeps a 3:1 contrast against both canvas backgrounds; the agent-change outline stays distinguishable from the selection color. `assets/logo.svg` is the source for the app icon (`.icns`, `.ico`, PNG set generated by `xtask`) and the README and wiki header.

## Testing

Qt Test for C++ items and the bridge, Qt Quick Test for QML behavior, run by `ctest`; the renderer is golden-tested in `render` with no window at all ([Development Guide](./development-guide.md#testing)). Offscreen platform (`QT_QPA_PLATFORM=offscreen`) in CI `[INFERENCE]`.
