# Changelog

All notable changes to OpenDesigner are documented here.

> **Format**: loosely based on [Keep a Changelog](https://keepachangelog.com/), grouped by date descending. Each entry references the originating PR (when available) and names the area from the conventional-commit prefix.
>
> **Wiki**: the [GitHub Wiki](https://github.com/skepjandi/opendesigner/wiki) is the public-facing main wiki — architecture, document format, agent integration, security model, roadmap. Source-controlled editing surface is `docs/wiki/` in this repo; CI (`.github/workflows/sync-wiki.yml`) mirrors it to the GitHub Wiki on every merge to `main`. New single-PR changes belong here, not as new wiki pages.

## 2026-11-19

### fix(app): a frame with no background is transparent, not a white screen

**Area:** `fix(app)`, `fix(mcp)`, `fix(render)`

**Why:** "its a white screen even after a export": the design frames the agent made for the video had no background (they sit over the footage), and the editor flattened them onto white, so the canvas showed a white screen and the exported PNG was white too. **Change:** The canvas keeps the picture's alpha and draws a checkerboard behind it, so a transparent frame reads as empty rather than broken. The export says how much of the frame is transparent and whether the file is white or see-through there, and the export dialog has a "keep the transparency" option.

**Verified:** a render test (a frame with no fill is fully transparent, one with a fill is not, the checkerboard encodes), an MCP test (the export of a frame with no background warns, a filled frame does not), the window session. **Not verified:** by eye (the canvas was never looked at); the export with alpha was not written and inspected.

**PR:** n/a

### feat(app): the chat can change the agent's model and effort, run its commands, and starts from a direction the user picks

**Area:** `feat(acp)`, `feat(app)`, `feat(agent)`

**Why:** "the chat has no way to change models or effort", "add a button with a full selection menu (styles, color palettes) that injects into the prompt", and the agent's words could not be copied. **Change:** (1) The ACP client reads the settings the agent offers (`configOptions`, or the older `modes`/`models`), shows them in the chat (model, mode, effort) and changes them (`session/set_config_option`, `session/set_mode`, `session/set_model`); the agent's slash commands are listed behind `/` and typed into the message. (2) A **Direction** button (chat and start screen) opens a picker of looks, palettes, type pairings, mood, motion, layout and imagery, plus free text and three working switches (look for skills first, show three directions first, remember the choices); the picks become a block in the prompt. (3) The agent's messages are text edits, so they can be selected and copied.

**Verified:** ACP tests (settings and commands arrive, a choice can be changed, a refused setting is reported, an agent with only modes/models gets the same controls), direction tests (ids unique, the looks match the style playbook, the picks become instructions, typed colors beat a listed palette), window session. **Not verified:** against real agents by hand (the protocol was read from omp's own answers over ACP: 3 settings and 116 commands); the picker by eye; whether every agent supports the calls.

**PR:** n/a

### feat(mcp): exports tell the agent when a frame was never reviewed or changed since

**Area:** `feat(mcp)`

**Why:** The quality bar asks agents to check their work; an agent that skips it should be told at the moment it exports. **Change:** `export`, `export_set` and `export_panorama` add `before_you_call_it_done` when a frame was never checked with `review_frame` or anything inside it changed afterwards. The export still goes through.

**Verified:** MCP test (never reviewed, reviewed, unrelated change, change inside the frame, set export). **Not verified:** whether agents act on the note; video exports do not carry it (their export runs on a detached copy).

**PR:** n/a

### feat(app): a fully rearrangeable layout: tabs, splits, windows of their own, hidden panels

**Area:** `feat(app)`

**Why:** The window had one fixed arrangement. **Change:** Layers, canvas, inspector, notes, history, memory, video, connect and chat are separate panels (the agent panel's tabs are now panels of their own) that can sit side by side, stacked, as tabs, in a floating window, or hidden. Drag a tab onto another panel's middle (join its tabs) or edge (split next to it), or off the window (its own window); right-click a tab or use `⋯` for the same by menu; drag the line between panels to resize; toolbar **Layout** shows/hides panels and resets. Closing a floating window puts its panels back. The arrangement is saved in the config folder and restored. The rules are in `crates/app/src/layout.rs` (every operation checked; a refused one changes nothing).

**Verified:** 10 unit tests of the model (joining tabs, edge splits and merging, refusals, floating, the main window keeping a panel, hide/show/reveal, weights, save/load with a bad file, 600 random operations all leaving it valid) and a window session that floats, splits, docks, hides, reveals, closes and resets panels with no QML warning. **Not verified:** the dragging itself (tabs, divider, dropping outside to float) and the floating windows by hand or by eye; keyboard shortcuts inside floating windows (they have none); behavior on a second monitor.

**PR:** n/a

### feat(agent): every agent gets the quality bar, playbooks, a design review tool and optional formats

**Area:** `feat(mcp)`, `feat(app)`, `feat(skill)`

**Why:** "Our agent output is ass": agents did not use our guidance unless the skill was installed and loaded, and the skill had no craft in it. **Change:** (1) The quality bar is sent with the MCP `initialize` instructions and the in-app chat's first prompt, so every agent gets it with or without the skill. (2) `get_playbook` serves five playbooks (design, style: eight named looks, video, motion, formats with safe zones) that are also `references/` in the installed skill; the skill description now covers video and motion. They are rewritten from public skills and platform pages (listed in the wiki). (3) `review_frame`: contrast, small or edge-hugging text, overlaps, overflow, misalignment, fonts, loose colors, size vs. the chosen format. (4) Formats: sections (Video, Store images, Social images, Web and icons) plus a custom size, chosen when starting or from the toolbar, or left to the agent (`list_formats`, `get_brief`, `set_format`); a format the user chose is enforced for videos and changes only through a request shown as Allow / Keep mine.

**Verified:** unit tests (WCAG contrast, each review finding, the approval state machine, formats vs. store preset sizes, playbooks name only real tools), MCP tests (instructions carry the bar, guidance carries the brief, playbooks readable, review finds low contrast, a chosen video size is refused and a change waits for the answer), app test (choose, ask, answer, hand back), window session. **Not verified:** that agents behave better (no model turn compared before and after; only that the text reaches them), the format dialog and approval banner by eye, the safe-zone numbers beyond the pages they came from.

**PR:** n/a

### feat(video): motion that is worth watching, and a timeline that shows what follows what

**Area:** `feat(core)`, `feat(render)`, `feat(app)`, `feat(mcp)`

**Why:** Videos were fades and linear slides over still frames; the user could not see the order of things from the workspace. **Change:** Easing (`linear`, `ease_in`, `ease_out`, `ease_in_out`, `back`) on transitions, zooms and overlays; transitions `slide_up`, `slide_down`, `dip` (through black), `zoom_in`; overlays enter and leave with `fade`, `slide_*`, `pop` or `zoom`. `list_videos` returns a `story` (one sentence per clip and overlay). The Video tab is now a timeline: ruler, clip lane with the overlapped stretch of every transition, stacked overlay lanes, a playhead you can click or drag, the preview and Play, and the same story in words.

**Verified:** core and render tests (ease curves incl. overshoot, slide_up/dip/pop poses at known times, overlay entrance), an MCP test of the story text, window session with the new panel. **Not verified:** the timeline by eye, the preview at playback speed (it is still a stepped still image), real footage with the new transitions.

**PR:** n/a

### fix(app): chat while the agent works, always allow, scrollable panels, and a 20x faster export in dev builds

**Area:** `fix(app)`, `perf(render)`, `build(repo)`

**Change:** (1) You can type while the agent is working: the message is queued (shown "queued"), sent when the turn ends, and dropped if you press Cancel. (2) "Always allow" in the chat answers the agent's permission questions with Allow (always, else once; never a rejection) and notes each one. (3) The Connect and Inspector panels always leave room below their last control (a layout cannot know the height of wrapped text at its final width), and the start panel scrolls. (4) Export: a debug build of the pixel code was about 30 times slower than release, so a 5.4 s 886x1920 video took 3.5 minutes; the rendering and media crates and all dependencies are now optimized in dev builds, and while nothing moves (no zoom, transition or fade) the export draws a frame once and re-sends it. Same video: 11 s in a dev build.

**Verified:** chat unit and scripted-agent tests (queue order, no duplicate, always-allow answers waiting and later questions, `best_allow`), a render test for the held-frame signature, the 5.4 s export timed before and after, window session. **Not verified:** the panels by eye (only reasoned: layouts underestimate wrapped text; the extra room is a safety margin, not a measured fix), footage exports (every frame differs there, so only the build speed helps).

**PR:** n/a

### feat(app): workspaces: several deliverables side by side in one repository

**Area:** `feat(mcp)`, `feat(app)`

**Why:** An agent asked for an app store asset and a reel put both on one page. **Change:** A repository can hold several workspaces, each its own project folder under `opendesigner/workspaces/<name>/` (pages, videos, history, notes, media); a project from before sits in `opendesigner/` and is listed as `main`. The agent gets `list_workspaces`, `create_workspace` and `open_workspace` (the skill says: one per deliverable); the start screen and the toolbar have a workspace picker and "New workspace…"; an agent's switch is announced and the window follows it (view, language and outlines reset). Switching swaps the workspace behind the running connection, so the agent and the chat stay connected.

**Verified:** agent tests (separate frames per workspace, empty new one, listing, switch announced, bad and duplicate names refused), app tests (open/create/recent, switching keeps each one's work), window session. **Not verified:** the pickers by hand; a real agent choosing to split work into workspaces. Media is per workspace, so the same picture added in two workspaces is two copies.

**PR:** n/a

### fix(video): the window no longer freezes during a video export; progress bars and spinners; agents can use pictures from anywhere

**Area:** `fix(mcp)`, `feat(app)`, `feat(mcp)`

**Bug:** The agent's `export_video` call held the editor's state for the whole export (minutes), and the window reads that same state, so it hung until the export ended. **Fix:** the export now works on a copy of the project, reports progress, and can be stopped; a strip at the bottom-left of the window shows a bar with a percentage and a Stop button. Copying a large video into the agent's memory now happens on a thread with a spinner, the chat shows a spinner while the agent works or starts.

**New:** `find_files` and `import_file` let the agent search the home folder for pictures and videos and copy one into the project (a switch in the Connect tab turns it off). Invariant 5 is reworded and the decision and risks are written down (security.md, roadmap).

**Verified:** an MCP test with a real ffmpeg that the editor's state is free at every progress report, progress reaches the total, and a stopped export leaves no file; app tests for the job rows; device tools tests (copy, refuse a fake picture, refuse paths, search limited to home). **Not verified:** the bar and spinners by eye, a very long export, the Stop button by hand, the search over a real home folder.

**PR:** n/a

### feat(video): footage clips with sound, overlays, and a Video tab

**Area:** `feat(core)`, `feat(render)`, `feat(media)`, `feat(mcp)`, `feat(app)`

**Change:** The user imports video files into the agent's memory; the agent cuts them into a video (`add_clip` with `footage`), lays titles and call-outs over it (`add_overlay`: a transparent design frame that fades in and out) and exports an MP4 with the footage's sound mixed under the picture. New tool `list_footage`; `export_video` refuses a clip that runs past its file. The editor has a Video tab: preview, play, scrub and the clip list. The skill teaches the flow.

**Verified (macOS, Homebrew ffmpeg 9.0.1):** core tests (source rules, overlays, undo, history-only ops refused, footage naming/symlinks/sniffing), render tests with a fake footage source (in point, fades, timing), media tests (log parsing, paths), an MCP end-to-end with real ffmpeg that makes a 3 s test video with a tone, imports it, cuts two clips and an overlay and exports 200x400 10 fps: audio present, ~2 s, length error for a clip past the end; app tests (import to memory as footage, preview). **Not verified:** real screen recordings or phone footage (codecs, variable frame rate, rotation metadata, long files and speed), more than a few clips, the Video tab by hand, Windows/Linux. **Not built:** animated overlays, volume/music, speed, captions from speech, notes pinned to a time range, clip selection on a timeline.

**PR:** n/a

### feat(app): the agent's memory tab; the skill learns from public agent skills

**Area:** `feat(app)`, `feat(skill)`

**Change:** A Memory tab lists the pictures in the project's `media/` folder with thumbnails and whether the design uses them; "Add pictures" (or dropping on the window) copies files there for the agent (`list_media`). The generated skill gained "You decide the shape" (the agent names and sizes everything), a subject-and-taste section and a video section. Sources read on 2026-11-19: anthropics/skills `frontend-design` (ground in the subject, avoid generated-art tells), ParthJadhav/app-store-screenshots (screenshots are ads; copy rules; deck arc; thumbnail test), and the video-editing skills affaan-m/ECC `video-editing`, kajisho5/ffmpeg-skill and arhen/motion-film (probe first, keep originals, plan-run-check, one motion language, never invent captions). Advice was paraphrased, no text copied.

**Verified:** an app test for the memory list; skill tests pass. **Not verified:** the tab by hand; whether agents behave better with the new text (no agent turn was run on it). Footage (video files) cannot be imported yet: `media/` takes pictures only.

**PR:** n/a

### feat(app): start screen: choose the working folder, the agent and the first task

**Area:** `feat(app)`

**Change:** The launch screen (and "Open another project") now asks where you are working (usually a repository), which agent to start (installed ones marked) and what it should make first. The agent starts in that folder, so it can run your app and take screenshots; the design lives in `opendesigner/` inside it, opened if present or set up empty (no frame is made: the agent decides). The task you type is sent once the agent is connected. Recent folders are remembered in the config folder. The "new project with a first frame and size" dialog is gone. When the agent makes the first frame, it becomes the open one.

**Verified:** app tests (inspect, refuse without a yes, create empty, reopen from folder or project, recent list, first frame adoption, the first task reaching the scripted agent) and the window session loading the panel with no QML warnings. **Not verified:** the panel by hand (folder dialog, clicks), a real agent receiving the task.

**PR:** n/a

### feat(ui): an agent-first window: pointing tools only, macOS shortcuts and gestures, drops go to the agent's memory

**Area:** `feat(ui)`

**Change:** The toolbar loses the rectangle, ellipse, text, frame, image and device tools and the colors/languages/open/save buttons (the last four are in a "⋯" menu): the agent creates things and decides their names and sizes; the user selects, moves, edits properties, pins notes and corrects. Added the Hand tool, a shortcut set taken from Figma and Penpot (`V` `H` `C`, `⇧1` fit, `⇧2` selection, `⌘0` 100%, `N` frames, `Tab` layers, arrows to nudge, `⌘]` `⌘[` order, `Return` to edit text, `⌘L` agent chat, `⇧⌘E`, `?` list) and a shortcut dialog, ⌘/⌥-scroll zoom. Files dropped on the window are copied to the project's `media/` folder (the agent's memory) and not placed on the design.

**Verified:** the window session loads the new QML with no warnings (it fails on any) and the existing scripted session passes; app unit tests pass. **Not verified:** the keys and gestures themselves (no synthesized key events; not tried on a real keyboard or trackpad here), Magic Mouse.

**PR:** n/a

### fix(ui): the canvas and the panels update without flashing or jumping

**Area:** `fix(ui)`

**Problem:** While an agent edited (or the user did), the canvas flashed, the layers list, chat and notes were rebuilt on every change (losing scroll position and flickering), and the view jumped when an edit changed the frame's size.

**Cause and fix:** (1) A QML `Image` given a new `source` drops its old picture while the new one loads (a `data:` URL always loads asynchronously), so the canvas went blank for a moment each redraw. The picture is now drawn by two images that take turns: the new drawing loads in the hidden one and replaces the other only when ready. (2) Every list was a fresh JavaScript array each time its JSON changed, which rebuilds every delegate. `JsonModel` (new) syncs an array into a `ListModel` and touches only the rows that changed, moved, appeared or went; layers, notes, activity, restore points, chat lines, plan, permission buttons, frame tabs and the canvas overlays use it. (3) The view now fits only when a different frame is opened, not when a frame is resized, and redraws are throttled (at most every 90 ms during a burst of changes) instead of debounced until the burst ends.

**Verified:** the window session now checks that a changed list builds only its new rows (changed and moved rows keep their delegates) and that the picture in front stays ready while a new one loads (three redraws); app tests pass on macOS. **Not verified:** by eye (no screenshots are used as evidence here); the user's report is the reference, so please say if any flash remains.

**PR:** n/a

### feat(video): videos of design frames, an ffmpeg export, Apple app-preview rules

**Area:** `feat(core)`, `feat(render)`, `feat(media)`, `feat(mcp)`, `feat(cli)`, `feat(store)`

**Change:** A video is a timeline of clips that show design frames, with transitions (cut, fade, slide) and a zoom, changed only through undoable operations (`create_video`, `add_clip`, `set_clip`, ...). `od_render::video` draws any moment; the new `crates/media` pipes frames to `ffmpeg` for an H.264 MP4 (OS encoder first). MCP: `list_videos`, `render_video_frame`, `export_video`, `list_video_presets`; CLI: `videos`, `export-video`, `video-presets`; the skill teaches it. `od_store::video` checks Apple's app-preview rules (read 2026-11-19). Invariant 10 is reworded and the decision logged: ffmpeg runs as a separate process.

**Verified (macOS arm64):** core tests (overlap maths, bounds, undo/redo, persistence, a clip on a missing frame refused to load), render tests (cut, fade, slide, zoom, cover-and-crop), media tests with a stand-in ffmpeg and with Homebrew's real ffmpeg 9.0.1, MCP and CLI end-to-end encodes, preset refusals before encoding, and a real 886x1920 30 fps export probed with ffprobe (h264, 162 frames). **Not verified:** other encoders (Media Foundation, NVENC, VAAPI), Windows/Linux, a bundled LGPL ffmpeg, playback in the editor (not built), audio (none is written), footage clips (not built).

### build(repo): a Linux build-and-test container; the ACP test no longer guesses where `target/` puts the mock agent

**Area:** `build(repo)`, `test(agent)`

**Change:** `scripts/test-linux.sh` builds `packaging/linux/Dockerfile` (Ubuntu 24.04, distribution Qt 6.4, lld) and runs the AGENTS.md checks inside it. Running it found two real problems, both fixed: the ACP tests located the mock agent by walking up from the test executable, which breaks with the newer Cargo `target/` layout (they now ask Cargo for the path), and the Qt shell did not link on Linux because the cxx-qt initializers were never referenced from Rust (`run()` now calls `init_crate!`/`init_qml_module!`; the container links with lld, which resolves archives in any order).

**Verified (Ubuntu 24.04 aarch64 in Docker, not Windows, not x86_64):** `cargo fmt --check`, `clippy -D warnings` and the 123 default-member tests pass; the editor crate builds and links and 24 of its 25 unit tests pass. **Not verified / failing:** `the_chat_runs_a_scripted_agent_through_the_editor_and_hands_it_the_local_server` fails there (not investigated), the window session was not reached, and nothing was run on a real Linux desktop. macOS stays the supported platform for now.

**PR:** n/a

### feat(app): first-run skill offer and the agent's plan in the chat

**Area:** `feat(app)`, `feat(agent)`

**Change:** On the first start with an agent that reads skills present and ours not installed, a dialog lists every file it would write and offers Install, Not now or Don't ask again (a marker in the config folder; nothing is written without a click). The chat shows ACP `plan` updates as a checklist (replaced whole by each update, cleared at the next user message). The session list stays unbuilt.

**Verified:** app tests (offer once, no agent nothing to offer, don't-ask-again installs nothing, installed is not offered), a chat test for plans, the ACP test and the window session (the mock agent's plan, the offer's API), and a real omp turn whose `todo` tool arrived as two `plan` updates. **Not verified:** the dialog itself (the window session drives the API, not clicks; it is not opened in the session), plans from Claude Code, Codex or Gemini, Windows.

**PR:** n/a

### feat(render): text that does not fit its box is detected and shown

**Area:** `feat(render)`, `feat(app)`, `feat(mcp)`

**Problem:** A longer translation or an agent's long headline could run past its box and nothing said so.

**Change:** `od_render::check_text` measures each text node's layout (the cached `cosmic-text` layout, no drawing) and reports `Warning::TextOverflow` when it wraps to more rows than the box is tall; `render_frame` includes it. The MCP preview note and the export tools, the CLI and the Export result carry it as `warnings` (the key `not_drawn` is renamed, since the text is drawn). The canvas outlines the box in red with the numbers; the inspector explains it; the skill text tells agents to fix it.

**Verified:** render tests (overflowing, wrapped word, fitting, unboxed text), an MCP test (preview note and export `warnings`), an app test (the snapshot lists the box and the numbers, and clears after resizing), and the window session. **Not verified:** boxes narrower than one glyph, right-to-left or vertical text, other fonts than the bundled Lato.

**PR:** n/a

### perf(render): frames with many nodes are drawn in parallel bands; laid-out text is cached

**Area:** `perf(render)`

**Problem:** Redrawing the canvas took 64 ms at 10,000 nodes (budget 16 ms) and an export at scale 1 took 231 ms.

**Change:** A frame of 256 or more nodes is drawn as up to 8 horizontal bands on `std::thread::scope` threads (4 rows of overlap per side so seams match), each band skipping nodes wholly outside it (a frame's subtree goes with it). Text layouts (shaping) are kept between renders in a 4096-entry cache keyed by text, size, family, weight, alignment and wrap width. No new dependency.

**Verified:** `cargo run --release -p opendesigner-render --example bench` on an M2 Pro: canvas at 10,000 nodes 64 ms to about 31-34 ms (still over the 16 ms budget), 1,000 nodes 9 ms, export at scale 1 and 10,000 nodes 230 ms to 133 ms. A test compares 1, 2, 3 and 8 bands on a busy frame (strokes, rounded corners, nested clipping frame, wrapped text across seams, nodes outside the frame) at three scales: not bit-identical (anti-aliased edge samples differ by float rounding: fewer than 0.02% of values, by at most 64/255). **Not verified:** other machines (the speed-up depends on core count; a 1-core machine renders in one band as before), Windows and Linux.

**PR:** n/a

### feat(agent): agent-agnostic connection (omp, opencode, Codex, Gemini, Cursor, any MCP client) and a real omp turn

**Area:** `feat(agent)`, `feat(app)`, `feat(cli)`

**Problem:** Connection help covered Claude Code only, the skill had no omp location, and the chat offered three presets.

**Change:** `od_agent::connect::recipes` gives the exact setup per agent (omp, Claude Code, Codex CLI, Gemini CLI, opencode, Cursor, any client); none contains the token. The Connect tab has an agent picker; `serve-mcp --agent <id>` prints one. New skill target `omp` (`~/.omp/agent/skills`). The chat offers `omp acp` and `opencode acp` besides the Claude, Codex and Gemini ones and marks the installed ones. `acp_probe --prompt=` runs a turn.

**Verified:** with omp 19.0.0 (signed in) a turn through the chat's ACP client called `get_guidance`, `list_pages` and `apply_ops` on a served project, created a frame and a text node, and the history attributes it to `agent:omp-coding-agent`; `omp -p` read the same server from a project `.omp/mcp.json`; `opencode mcp list` showed it connected and `opencode acp` opened a session; unit tests for the recipes; app tests and the window session. **Not verified:** a model turn with Claude Code, Codex or Gemini CLI (not signed in), Codex/Gemini/Cursor connecting to MCP, the omp skill location with a running omp, Windows shells.

**PR:** n/a

### test(agent): the chat's ACP setup checked against real agents; MCP checked with real Claude Code

**Area:** `test(agent)`, `docs(wiki)`

**Problem:** The earlier entries said no real agent was installed. That was wrong: I had not looked. Claude Code, Codex, Gemini CLI and opencode are installed on this machine (none is signed in).

**Change:** Added `examples/acp_probe` (starts a real ACP agent and runs the chat's setup, no prompt). Findings: the first run of `npx -y @agentclientprotocol/claude-agent-acp` takes over a minute (it downloads the adapter), so the 60-second setup timeout failed on a cold cache even though the adapter was fine; it is now 4 minutes. Real Claude Code 2.1.318 connected to `serve-mcp` over HTTP with the bearer token (`claude -p --mcp-config --strict-mcp-config --debug mcp`, which does not touch your Claude config): "Successfully connected", tools advertised, protocol 2025-06-18.

**Verified:** the probes above, run once each. **Not verified:** any prompt turn or MCP tool call by a real agent (all three agents' model calls fail with an expired sign-in), Codex and opencode against MCP, ACP v2.

**PR:** n/a

### docs(repo): status banners and README match what is built

**Area:** `docs(repo)`

**Change:** The roadmap, agent-integration, security and architecture banners and the README still described the editor, the chat and device frames as not built. They now say what exists and, in the same places, what has not been verified (no real agent, one OS, no video, no first-run skill offer).

**Verified:** read against the code and the changelog entries above; no code changed. **PR:** n/a

### feat(agent): in-app chat over the Agent Client Protocol

**Area:** `feat(agent)`, `feat(ui)`, `docs(wiki)`

**Problem:** Using the agent meant leaving the editor for a terminal and pasting a connect command.

**Change:** `od_agent::acp` is a client for ACP v1 (JSON-RPC over the child's stdio): it starts the agent the user configures, negotiates the version, opens a session in the project folder with the editor's MCP server (HTTP, bearer token) as its tool, streams text, thoughts and tool calls, surfaces permission requests, and cancels. It serves no file or terminal capability. The editor gets a Chat tab: presets or a custom command (split without a shell), Connect (which also turns on the agent connection), a conversation view, permission buttons, Cancel and Disconnect. A first-prompt preamble points the agent at `get_guidance`, so the user's selection and notes travel with every request.

**Verified:** `fmt`, `clippy -D warnings`, tests against a scripted agent (`examples/mock_acp_agent`): streaming and ordering, permission allow/reject/cancel, a cancelled turn, refusing a file request, the MCP server and token handed over, a wrong protocol version, no HTTP support, an authentication error with the agent's own hint, a missing program, an agent that dies mid-turn; the editor's state tests and the offscreen window session run a whole conversation. **Not verified:** any real agent (Claude Code, Codex and Gemini adapters were not installed or signed in), the preset commands and package names (taken from published names, which have changed before), ACP v2, Windows (`.exe` lookup in the tests is unchecked), long conversations and resizing of the chat view.

**PR:** n/a

### build(app): a macOS app bundle script with a licensing guard

**Area:** `build(app)`, `docs(repo)`

**Problem:** The editor could only be started with `cargo run`; there was no installable app, and the "no GPL-only Qt module" rule (invariant 12) was a convention, not a check.

**Change:** `scripts/package-macos.sh` produces `dist/OpenDesigner.app`: release build, Info.plist, icon from the logo, license notices (`packaging/THIRD_PARTY.txt`, the font license, the project license), `macdeployqt` with the QML folder, a licensing step, ad-hoc signing, and a smoke test that runs the bundled app's scripted session offscreen. The licensing step exists because the first bundle contained two GPL-only Qt parts (the Qt Quick Timeline and Qt Virtual Keyboard plug-ins, copied by `macdeployqt` though the app never imports them): the script now deletes anything matching a banned module and fails if any remains. It also removes the Homebrew rpath so Qt cannot load twice. Also removes leftover CMake/C++/`bridge` wording from AGENTS.md, CONTRIBUTING.md, README and the wiki index.

**Verified:** the script ran to the end on macOS arm64 with Qt 6.12 from Homebrew and the bundle printed `SELFTEST OK`; the frameworks list contains no banned module. **Not verified:** the bundle on another Mac (no other machine), a Developer ID signature or notarization (not done), Windows and Linux packaging (none), drag-to-Applications install and first-run behavior from Finder.

**PR:** n/a

### feat(core): per-language text and exports

**Area:** `feat(core)`, `feat(render)`, `feat(mcp)`, `feat(cli)`, `feat(ui)`, `feat(skill)`

**Problem:** A store listing ships in many languages, and re-asking an agent to redraw each one is the cost this studio exists to remove.

**Change:** `Document::locales` holds translations of text nodes (language tag, then node id to text) in `od.toml`; the new operation `set_localized_text` sets or removes one (undoable, attributed, checkpointed) and a language with no translations disappears. `Document::localized(tag)` gives the document as it reads in a language, which `export_assets` renders: `export`, `export_set` and `export_panorama` take a `locale`, and set and panorama files are named `<prefix>-<locale>-NN.png`. New tool `list_locales` (languages, what each lacks, every text node) and CLI `locales`, `--locale` on the export commands. The editor has a language switcher (the canvas, the text field and exports follow it) and a Languages dialog; adding a language copies the current text of every text node as a starting point, in one undoable step.

**Verified:** `fmt`, `clippy -D warnings`, tests (core: operation, undo/redo, removal, refusals, persistence and restore after a reopen; MCP: translate then export both languages with different pixels and unknown-language refusal; app unit test; the window session adds a language, edits a translation, exports a set in it and checks the saved manifest). **Not verified:** the toolbar combo box and the Languages dialog controls (only the calls behind them run); right-to-left text and complex scripts (shaping uses the bundled Lato plus system fonts, unchecked); that a longer translation fits its box (nothing detects overflow).

**PR:** n/a

### perf(render): a benchmark for the render path, and the first measured numbers

**Area:** `perf(render)`

**Problem:** The roadmap's performance budgets (invariant 11) had never been checked; the canvas design (re-render a PNG per change) was known to be a risk but unmeasured.

**Change:** `cargo run --release -p opendesigner-render --example bench` times render and PNG encode at canvas and export scale, and a 100-operation transaction with a checkpoint, for 10 to 10,000 nodes. Results and the one budget that is not met (canvas at 10,000 nodes: 64 ms against 16 ms) are in the roadmap.

**Verified:** the example runs and its numbers were recorded from that run (Apple M2 Pro). **Not verified:** other machines, cold start, the Qt-side cost of the canvas, scrub latency (no video yet). The benchmark is not a CI gate.

**PR:** n/a

### feat(core): design tokens change through history, with a Colors dialog

**Area:** `feat(core)`, `feat(mcp)`, `feat(skill)`, `feat(ui)`

**Problem:** Tokens (named colors) were setup-only: neither the user nor an agent could change one, and a change would not have been undoable or attributed, so "change the brand color in one place" did not work.

**Change:** New operation `set_token {name, value}` (value `null` removes). It stores the canonical lowercase hex, reports the nodes that use the token as touched (so an agent's recolor is outlined), refuses to remove a token that a node still uses, and carries its inverse, so undo, redo, checkpoint restore and a restore after reopening all work. The editor gets a Colors dialog (list, recolor, add, remove unused); `apply_ops` and the skill text describe the operation; `core::view::tokens` feeds the panel.

**Verified:** `fmt`, `clippy -D warnings`, tests (core: change/undo/redo/remove/refusals/transaction rollback/checkpoint restore, persistence across a reopen; app window session sets a token, uses it in a fill, is refused when removing it, and the saved files contain it). **Not verified:** the dialog's own controls (only the calls behind them run); token types other than color (type scale, spacing) are not built.

**PR:** n/a

### feat(store): panorama and set export, generic device frames, drop-to-swap captures

**Area:** `feat(store)`, `feat(render)`, `feat(mcp)`, `feat(cli)`, `feat(ui)`

**Problem:** A store listing is a set of slides, but the editor could only export one frame at a time, there was no way to make a background cross the seams between screenshots, and putting a screenshot in a device frame was manual geometry.

**Change:** `export_assets` (moved from the MCP tools into `agent::export`) renders a frame, a set of frames, or one wide frame cut into equal columns, validates every image and the count against the store preset, and only then writes, so a set never ends half-exported. New tools `export_set`, `export_panorama`, `device_template`, CLI commands `export-set`, `export-panorama`, `device`. `od_store::devices` builds generic phone and tablet frames (body, `Screen`, camera) from ordinary nodes. The editor gets a Device button, an Export dialog with the three modes, a Replace button for images, and dropping a file on an image swaps its picture (one undoable edit, layout kept). `store` now depends on `core`; `od_render` re-exports `Pixmap` and adds `slice_columns`.

**Verified:** `fmt`, `clippy -D warnings`, default-member tests (new: slicing, every device validates and applies, MCP panorama/set/device with refusals leaving no files, CLI flow from device to validated panorama), the app's unit tests (set/panorama/device/drop-to-swap) and the offscreen window session (set, panorama and a device added). **Not verified:** that the presets match Apple's and Google's current pages (still copied from the reference projects); that real captures look right inside the frames; the Export dialog's radio buttons and the Replace file dialog (only the calls behind them run).

**PR:** n/a

### feat(render): images are imported, validated and drawn

**Area:** `feat(render)`, `feat(core)`, `feat(ui)`, `feat(mcp)`, `feat(skill)`

**Problem:** Image nodes existed in the model but were placeholders: no way to bring a screenshot in, nothing validated the path, and exports warned that they were not drawn.

**Change:** `core::media` defines the only valid reference (`media/<plain name>.png|jpg|jpeg|webp`), checked on every create, set and load, and imports a user-chosen file into `<project>/media/` after sniffing its first bytes (safe unique name, never overwrites, 64 MB cap, symlinks never listed or resolved). `render` decodes with the `image` crate (limits on size and allocation; decoded pixels cached by path, size and modification time) and draws images with `cover` (default), `contain` or `fill`, rounded corners, opacity and scale; a large picture drawn small is box-filtered first so it does not alias; a missing or unreadable image is reported with the reason (`Warning` now has `Display`). New `image_fit` property; `corner_radius` now applies to images. The editor gets an Image button (file dialog) and drop-onto-canvas, which copy the file, size the node to at most 80% of the frame and select it. New MCP tool `list_media` (paths, bytes, pixel sizes); there is deliberately no agent import. `render_frame` takes the project folder as a fourth argument.

**Verified:** `fmt`, `clippy -D warnings` (default members and the app), 94 default-member tests (new: media references and import, image fit/cover/contain/fill/radius/opacity/scale, downscale averaging, symlink refusal, warnings), the app's unit test for import and fitting, and the offscreen window session, which imports a picture and then checks that the exported PNG has its color at the expected pixel; `cargo deny`. **Not verified:** dropping files (the drop handler is untested, only the invokable it calls), large photos for speed (decode is cached but a 20-megapixel image is not benchmarked), CMYK or 16-bit JPEG/PNG variants, animated WebP, color profiles, fuzzing of the decoders.

**PR:** n/a

### feat(render): text is shaped, wrapped and drawn into the canvas and exports

**Area:** `feat(render)`, `feat(core)`, `feat(ui)`, `feat(skill)`

**Problem:** Text nodes were only previewed by a QML overlay and exports warned that they were not drawn, so the main ingredient of a store screenshot (the headline) could not be exported.

**Change:** `render` draws text with `cosmic-text` (shaping with kerning and ligatures, wrapping at the node's width, alignment, line height, weight, any installed family), blending glyph coverage through the frame's clip and the node's opacity. The bundled Lato Regular and Bold (SIL OFL) are the default and the fallback, so a document renders the same on every machine; system fonts are loaded only when a document names a family the bundle lacks or uses characters beyond Latin. `core` gains `font_family`, `font_weight`, `text_align` and `line_height` as validated, undoable properties (and on `create`), the inspector edits them, and the agent tool schema and skill text describe them. The QML text overlay is gone; images are still marked by a placeholder. Dependency: `cosmic-text` 0.19; `cargo deny` ignores the `ttf-parser` unmaintained advisory (RUSTSEC-2026-0192, no vulnerability), logged in the roadmap decision log.

**Verified:** `cargo clippy -D warnings`, `cargo test` (new render tests: drawn at the node's position and byte-identical across runs, weight and size change the ink, alignment and wrapping stay inside the box, opacity, frame clip, unknown family falls back), the shell's tests and scripted window session, `cargo deny check`. **Not verified:** other platforms' system-font lookup; complex scripts and emoji (system fonts are loaded for them but nothing checks the result); per-run styles (one style per text node), letter spacing and text on a path are not built.

**PR:** n/a

### feat(ui): the Qt Quick editor, with a canvas, layers, inspector and the agent panel

**Area:** `feat(ui)`, `feat(core)`, `feat(agent)`

**Problem:** Everything so far worked from a terminal. The product is the GUI where you guide an agent by pointing.

**Change:** New `app` crate: a Qt Quick editor built with Cargo and `cxx-qt` (pinned 0.10.0; no CMake, no C++ sources). One `Editor` QObject forwards to a Qt-free, unit-tested `EditorState`; QML presents it. The window has a toolbar with tools, a layers panel, a canvas that shows the frame rendered by `render` (pan, wheel/pinch zoom, fit; select, drag-move, 8 resize handles; rubber-band drawing of rectangles, ellipses, text and frames; a pin tool), an inspector with typed property edits (a rejected value shows the core's reason), and an agent panel: your pinned notes (resolve, reopen, delete), the history with a restore point before each agent turn, and a Connect tab that starts the MCP server in-process, prints the Claude Code command, and previews then installs the skill. What an agent changes is outlined in blue on the canvas as it happens. New dialogs create a project from a store size preset, pin a note, and export against a store preset. `core` gained `geometry` (bounds and hit testing) and `view` (layers and frames); `agent` exposes `export_frame` shared by the MCP tool and the editor. CI gained a macOS job for the shell.

**Verified:** on macOS arm64 with Qt 6.12 (Homebrew): `cargo fmt --check`, `clippy -D warnings` (default members and the app), `cargo test` (default members and `-p opendesigner-app`: 12 shell unit tests and a scripted session of the real window run offscreen that draws, edits, undoes, pins, starts the agent connection, restores a checkpoint, exports against two presets and grabs the window), `cargo deny check`. **Not verified:** pointer drags, wheel and pinch (the session calls the same invokables, it does not synthesize input); Windows and Linux; a real agent connecting; performance (the canvas re-renders a PNG on the UI thread, so large documents will be slow). Text is previewed by a QML overlay and is not drawn into exports yet.

**PR:** n/a

### feat(skill): generated Agent Skill and a previewed, reversible installer

**Area:** `feat(skill)`, `feat(cli)`

**Problem:** The whole premise is that installing the app teaches the agents on the machine how to use it, and that must not be able to damage anything.

**Change:** `skill.rs` generates the `opendesigner` Agent Skill (`SKILL.md` plus references for the tools, the store presets and a style guide) from the live tool definitions and presets, and installs it into `~/.agents/skills/` (Codex CLI, Gemini CLI, opencode) and `~/.claude/skills/` (Claude Code), both verified against those agents' docs. The installer records files and SHA-256 hashes in a manifest inside the skill folder, keeps files the user edited, refuses a folder it did not create and any symlink, and uninstalls exactly what it wrote. `opendesigner skill status|install|uninstall` previews every path and writes only with `--yes`. Dependency: `sha2`.

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (tests snapshot a temp home and prove only the skill folders change), on macOS arm64. **Not verified:** that Claude Code, Codex, Gemini CLI or opencode actually load and follow the skill (run by hand); Windows and Linux paths; Antigravity CLI's skills location; Cursor.

**PR:** n/a

### feat(mcp): local MCP server, tools and token for coding agents

**Area:** `feat(mcp)`, `feat(cli)`

**Problem:** The guidance model had no way to reach an agent. Without a tool surface an agent could only use the CLI.

**Change:** New `agent` crate. `mcp.rs` speaks MCP (JSON-RPC 2.0, Streamable HTTP, protocol versions 2025-06-18, 2025-03-26, 2024-11-05) with sessions and per-agent attribution; `tools.rs` exposes 17 tools (`get_guidance`, `apply_ops` with a checkpoint before each turn, `render_preview` returning an image, store-validated `export`, checkpoints and restore, and more); `server.rs` is the loopback transport with token, `Host` and `Origin` checks and size limits; `token.rs` creates the 0600 token file. `opendesigner serve-mcp <project>` runs it and prints the Claude Code connect command. Dependencies: axum, tokio, base64, getrandom (evidence in the roadmap decision log).

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (63 tests, including socket-level security tests and a test that starts the real binary, connects as an agent and checks the edit landed on disk with the agent's name in the history), `cargo deny check`, on macOS arm64. **Not verified:** Claude Code or Codex actually connecting (run by hand); Windows and Linux; token file permissions on Windows; behavior under concurrent agents beyond the mutex that serializes calls.

**PR:** n/a

### feat(core): selection, annotations and the guidance context

**Area:** `feat(core)`, `feat(cli)`

**Problem:** The point of the editor is that the user guides an agent by pointing, but the core had no model for what the user points at.

**Change:** `core` gains `Annotations` (pins on a node or a page region, open/resolved, attributed, persisted in `annotations.json`) and `Workspace` (session + annotations + selection + project folder) with `guidance()`: pages, selected nodes, open notes with an orphan flag, and the user's edits since the agent's last turn. The CLI gains `annotate`, `annotations`, `resolve` and `guidance`.

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (43 tests, including a CLI test of pin → read → act → resolve). **Not verified:** Windows and Linux; selection is in memory only (a front end holds it); the MCP and Qt front ends that will use it are not built yet.

**PR:** n/a

### feat(core): headless core, renderer, store presets and CLI

**Area:** `feat(core)`, `feat(render)`, `feat(store)`, `feat(cli)`

**Problem:** The project had rules and a design but no code, and the central claim (agents edit through validated, reversible operations) was unproven.

**Change:** A Cargo workspace with four crates. `core`: node model (frames, groups, rectangles, ellipses, text, image), the operations `create`/`set`/`move`/`reparent`/`delete` with bounds validation and inverses, an append-only history with atomic transactions, undo/redo, checkpoints and `restore`, and project folders (`od.toml`, `pages/*.json`, history log) that save deterministically and reject hostile input. `render`: CPU rasterization of frames, groups, rectangles and ellipses (fills, strokes, rounded corners, clipping, opacity) and PNG export as RGB or RGBA. `store`: nine App Store and Google Play presets with size, format, alpha and count validation. `cli`: the `opendesigner` binary (`new`, `show`, `apply`, `checkpoint`, `checkpoints`, `restore`, `export`, `validate`, `presets`). Also `deny.toml`, `clippy.toml` (no `unwrap`/`expect`/`panic` outside tests), `rust-toolchain.toml`, a Rust CI workflow, and wiki updates marking what is implemented. Dependencies (all popular and maintained, licenses checked by `cargo deny`): serde, serde_json, thiserror, toml, clap, tiny-skia, png, tempfile (tests).

**Verified:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (36 tests, including an end-to-end CLI test that applies an agent edit, exports a 1320x2868 App Store PNG, validates it, rejects a wrong preset without writing a file, and restores a checkpoint), `cargo deny check`, all on macOS arm64. **Not verified:** Windows and Linux; the CI workflow (never run); text and image rendering (not implemented, reported as warnings); the store preset rules against Apple's and Google's own pages (copied from a reference project); performance against the roadmap budgets (no benchmarks yet).

**PR:** n/a

### docs(repo): project rules, wiki and design for the Qt + Rust editor

**Area:** `docs(repo)`

**Problem:** GitNotēs and OpenMedia produce their App Store, Google Play and web assets by describing every detail to an agent in a terminal and re-reading the output. There is no surface to select, annotate or adjust, so the agent is guided by text alone.

**Change:** Repository bootstrap with no application code. `AGENTS.md` (wiki-first rule, worktrees, a dependency policy limiting the project to popular, well-maintained packages, and 12 architecture invariants for a local-first editor whose agents work through validated, checkpointed operations, including that Qt stays LGPL), this changelog, the wiki under `docs/wiki/` (architecture, document format, agent integration, Qt desktop, store assets, video editing, security, prior art, roadmap, development guide) with its sync workflow, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, issue and PR templates, `.editorconfig`, `.clang-format`, and a Rust/Qt/CMake `.gitignore`. `assets/logo.svg` is the app logo. Toolkit: Qt 6 (C++/QML) over a Rust core, chosen over GPUI; see the roadmap decision log.

**Verified:** nothing run; documentation only. Research inputs: the two reference repositories' own docs and store-asset scripts, Kino's public docs, OpenCut's repository and DeepWiki pages, Qt 6.12's licensing and Qt Multimedia docs, `cxx-qt`'s repository and crates.io page, KDDockWidgets' license, the Agent Skills specification. **Not verified:** canvas and video-preview performance on Qt, the `cxx-qt` build on all three OSes, the per-agent skill directories other than the ones present on the maintainer's machine, whether the QML build flow touches the GPL-only Qt Qml Compiler module, and every `[INFERENCE]` on the wiki pages; these are the Phase 0 spikes in the [roadmap](docs/wiki/roadmap.md).

**PR:** n/a
