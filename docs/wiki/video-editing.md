# Video Editing

> **Status: Partially implemented.** Built (video v1): videos made of **clips that show design frames**, with transitions (cut, fade, slide, dip, zoom_in), eased motion, overlay entrances and exits, a timeline view of what follows what, and a slow zoom, as undoable operations (`crates/core/src/video.rs`); a renderer that draws any moment (`od_render::video`); encoding to H.264 MP4 by running `ffmpeg` (`crates/media`); MCP tools `list_videos`, `render_video_frame`, `export_video`, `list_video_presets`; CLI `videos`, `export-video`, `video-presets`; Apple app-preview rules (15-30 s, at most 30 fps, 886x1920 or 1920x886 for iPhone, 1200x1600 or 1600x1200 for iPad, 500 MB; read from Apple's page on 2026-11-19). Checked on macOS with Homebrew's ffmpeg 9.0.1 and its VideoToolbox H.264 encoder: a 5.4 s 886x1920 30 fps video encodes to a valid MP4 (ffprobe: h264, 162 frames). **Footage (added later the same day):** the user imports MP4/QuickTime files into the agent's memory (`media/`); a clip can play a stretch of one (`footage {file, in_ms}`) with its sound, several clips of a file make cuts, and **overlays** (a design frame with no fill is transparent) are drawn over the whole timeline for a time with fades: titles, lower thirds, call-outs. Footage is decoded by `ffmpeg` (`FootageDecoder`, one process per file playing forward), the sound of every footage clip is cut, delayed and mixed under the picture and muxed with the video copied (AAC), each clip is checked to lie inside its file, and the editor has a Video tab with a playable preview and the clip list. **Not built yet:** animated overlays (they are still pictures that fade), audio mixing controls (volume, music), speed changes, captions from speech, timeline notes pinned to a time range, selecting clips on a timeline in the window, a pinned LGPL ffmpeg bundled in the app. OpenCut and Kino facts come from their public repository and docs as read on 2026-11-19; OpenCut's rewrite is in progress, so its desktop app is a scaffold, not a reference implementation to port. The timeline design is intent.

## What videos are for here

App Store and Google Play app previews, promo and changelog clips, web hero loops, short social cuts of a feature. Short, mostly screen captures and motion graphics, rarely a multi-camera edit. That shapes scope: a precise, scriptable timeline with strong title/overlay tooling beats a full colorist.

## What OpenCut teaches

[OpenCut](https://github.com/opencut-app/opencut) ("a free and open source video editor for web, desktop, and mobile", MIT; the original TypeScript app is archived as `opencut-classic`) is mid-rewrite around ideas we share:

| OpenCut | Us |
|---------|----|
| Business logic moves into a Rust core; apps are UI shells ("the UI framework for any given app is a replaceable detail") | Same rule: [Architecture](./architecture.md#the-central-rule-the-core-is-ui-agnostic) |
| Desktop app in GPUI: `Shell` with `Browser`, `Preview`, `Inspector`, `Timeline` panels (layout 2/3 top row of 25%/50%/25%, 1/3 timeline); semantic OKLCH theme tokens | Same panel set in a Qt shell; we add the agent panel ([Qt Desktop](./qt-desktop.md#window-layout)). OpenCut's GPUI choice is the one thing we do not follow |
| `crates/media`: FFmpeg **8.1.3** pinned in `setup/ffmpeg.json` with SHA-256s and prebuilt targets (Windows/Linux x86_64 and aarch64); a Bun setup script; **shared LGPL only**, `--disable-programs --disable-network --disable-avfilter …`; CI builds prebuilts for six targets and publishes a pre-release | Same *policy*: pinned manifest, checksum-verified, LGPL shared, no network protocols. The *version* is not ours to copy: it must match the FFmpeg major that Qt Multimedia uses ([below](#qt-multimedia-and-ffmpeg)). Write the setup script in Rust (`cargo xtask`) rather than add Bun to the toolchain |
| Plugin-first architecture, an Editor API, MCP server for agents, headless mode, a scripting tab | The operation API plus MCP plus headless CLI are our v1; plugins and scripting are deferred ([Roadmap](./roadmap.md)) |
| moon + proto for monorepo and tool pinning | Plain Cargo workspace first; revisit if a non-Rust toolchain (web docs, store tooling) grows |

Caveats: the OpenCut desktop app is a scaffold (its README says it opens a maximized window with the root `Shell`; the four panels are minimal), and the project says it is not yet taking outside contributions. Treat it as a design reference and a license-compatible source (MIT) for small utilities, not as a dependency.

## What Kino teaches

[Kino](https://kino.ai) is the benchmark for agent-driven video, and its docs show the pattern we want:

- **The agent works on the same timeline the user edits.** "Ask for the cut. The agent reads your footage, makes the edit, and leaves every clip on the timeline for you to move." Clips the agent changed get a **blue outline**. Every agent edit saves a **checkpoint** you can go back to. We copy this exactly ([Document Format: history](./document-format.md#history-and-checkpoints)).
- **Projects are code-like and legible to an agent.** Kino is built on Remotion, so titles and lower thirds are code in your brand, and "Claude Code reads one like a repo". We keep the *legibility* (plain-text project files, tokens for brand) but not the *code*: documents are data ([invariant 3](../../AGENTS.md)).
- **MCP is the integration point,** one local endpoint for many agents, with skills for editing style, and a way to use a Claude or ChatGPT subscription from inside the app ([Agent Integration](./agent-integration.md)).
- **Review and chat as first-class.** Notes on the timeline, references pasted into chat, live progress cards for long jobs. We take the in-app annotation model for video: a note pinned to a time range and a canvas region.

Where we deliberately differ: local-only and open files (Kino stores footage in its cloud and has accounts, plans and credits); a design canvas in the same app, so a store set can contain video and stills; store-aware validation.

## Built: video v1

- **Model.** `Document.videos`: `Video {id, name, fps (1-60), width, height (even, 16-4096), clips}` and `Clip {id, frame, duration_ms (100-60,000), transition {kind, duration_ms, ease}, motion {scale_from, scale_to, ease}}`. Time is whole milliseconds. A transition overlaps the end of the clip before it, so a 500 ms fade makes the video 500 ms shorter than its clips; a video is at most 10 minutes and 500 clips. Stored as `videos/<id>.json` (listed in `od.toml`), like pages.
- **Footage and overlays.** `Clip.footage {file: media/<name>.mp4|mov|m4v, in_ms}` instead of `frame` (exactly one); `Video.overlays [{id (o_..), frame, start_ms, duration_ms, fade_ms, enter, exit, ease}]` drawn over the clips, entering and leaving (at most half the duration each). Operations `add_overlay`, `remove_overlay`, `set_overlay`, and `set_clip` source. Footage files enter only through the user's import (`Workspace::import_footage`, sniffed by the `ftyp`/`moov` box, 8 GB cap, never overwriting, listed apart from images so an image node cannot name a video); `list_footage` shows them to the agent.
- **Operations** (undoable, attributed, checkpointed like any edit): `create_video`, `delete_video`, `set_video` (name, fps, size), `add_clip`, `remove_clip`, `move_clip`, `set_clip` (duration_ms, frame, transition, motion). `restore_video` and `restore_clip` are the history's own and are refused from front ends. A frame that a clip shows cannot be deleted until the clip is removed.
- **Rendering.** Each clip's frame is drawn by `render_frame` once at the scale the output needs (scaled to cover the video and cropped in the middle), then drawn with the zoom, offset and opacity of that moment: fade blends, slides push the old clip out.
- **Encoding.** `od_media` pipes raw RGBA frames to `ffmpeg` (`-f rawvideo ... -c:v <encoder> -pix_fmt yuv420p -movflags +faststart`) and moves the finished `.part` file into `exports/` only when ffmpeg exits cleanly. It prefers an encoder the OS provides: `h264_videotoolbox` (macOS), `h264_mf` (Windows), then `libopenh264`, NVENC, QSV, AMF, VAAPI, and GPL `libx264` last. It looks for `ffmpeg` in `OPENDESIGNER_FFMPEG`, next to the app (and in a bundle's `Resources`), then `PATH`; without one the error says what to install. Release build on an M2 Pro: 5.4 s of 886x1920 in 7.8 s.
- **Store rules.** `od_store::video` presets validate size, length, frame rate (before any encoding) and file size (after); a failure writes nothing and names the rule, the page and the day it was read. Not checked: H.264 profile/level, the 10-12 Mbps target, the audio track.

- **Motion.** Eases (`linear`, `ease_in`, `ease_out`, `ease_in_out` (default), `back` with a small overshoot) shape every transition, every clip zoom and every overlay entrance. Transitions: `cut`, `fade`, `slide_left|right|up|down`, `dip` (through black), `zoom_in`. An overlay has `enter` and `exit` (`none`, `fade` (default), `slide_up|down|left|right`, `pop`, `zoom`), each taking `fade_ms`, and an `ease`. Pose per moment: offset, scale and opacity, so an entrance is the overlay's own frame moved or scaled, not a new frame. There are no free keyframes yet.
- **Sequence view.** `list_videos` returns `story`: one sentence per clip (when it starts, how it arrives and over how long, what it shows, its zoom and ease, when it ends) and per overlay (start, enter, exit, end). The window's Video tab draws the same as a timeline: a seconds ruler, a clip lane (footage and frames coloured apart, the overlapped stretch of each transition marked), overlay lanes (overlapping overlays stack), a playhead you can click or drag, the preview and Play, and the sentences below it.

### Preview in the Video tab

Play in the Video tab plays a **cached preview**, not a live render. When a video is shown (or chosen, or changed) a worker thread draws every frame of it from a detached copy of the project (`od_agent::video_preview`, the same copy `export_video` uses), one frame after the other so footage is decoded as a stream, and keeps each as a PNG in memory. The window shows "Preparing preview N%"; Play starts once the first second is drawn and keeps going while the playhead stays behind the drawn frames (it shows "Buffering" and waits when it catches up). The playhead follows the clock (real elapsed time), and the video loops at its end.

Limits, stated in the panel too:

- **Small and slow:** longest side at most 360 px, at most 15 pictures a second (or the video's own rate when lower). The exported MP4 is full size at the video's rate.
- **Silent:** footage sound is not played (no Qt Multimedia); the export mixes it in.
- **Bounded memory:** about 256 MiB of encoded pictures; a picture that holds still (see `VideoRenderer::signature`) is drawn once and shared. At the cap the preview keeps what it has and the panel says so.
- **Cancelled, not repaired, on change:** any change to the project (the log grows: the user's edit, an agent's, undo) makes the cache stale; the worker stops before its next frame and a new one starts. Switching video or closing the panel cancels it too. While the cache is not there yet, scrubbing draws the one frame asked for on demand.

## Timeline model (intended)

- A **composition** page has an ordered list of tracks (video, audio, overlay, text), each holding clips with `start`, `duration`, `source in/out`, transform, effects and keyframes. Clips are nodes with stable ids, so the operation set is the same family as design operations plus `trim`, `split`, `retime`, `ripple`.
- A composition is also a **frame**: a video page can embed design frames as overlay clips (a device frame around a screen recording, a headline card), rendered by `render`.
- Time is an integer count of ticks per second (rational, never float seconds) so edits do not drift.
- Preview decodes through `media` (or Qt Multimedia for plain playback, see below) to frames shown in the preview panel at the project frame rate; scrubbing prioritizes latency (nearest keyframe plus decode forward) over exactness, with an exact frame on pause.
- Export encodes H.264/HEVC/ProRes where the platform allows, plus VP9/AV1 where the LGPL build includes them; presets for App Store app previews and Play promo video sit beside the image presets ([Store Assets](./store-assets.md)), with their own validation (resolution, frame rate, duration, size limits) `[INFERENCE: rules to be collected from Apple and Google docs in Phase 5]`.

## Qt Multimedia and FFmpeg

Read from Qt 6.12's Qt Multimedia docs: it is LGPLv3 (or GPLv2) and its **default backend is built on FFmpeg** on every platform except WebAssembly and embedded Linux. The binary packages link FFmpeg (9.0.1 in the 6.12 docs) *dynamically*; an application either bundles those libraries or depends on the system's, and may deploy **its own FFmpeg build provided the major version matches the one Qt uses**. Qt's prebuilt FFmpeg includes only features under permissive licenses. Native backends (AVFoundation on macOS; Windows Media Foundation, deprecated since Qt 6.10) are kept with limited support, and new features land only in the FFmpeg backend. Qt's docs warn that codec and hardware-acceleration behavior differs per platform, and that H.264 and similar codecs may carry patent royalties the Qt license does not cover (a distribution decision for the Phase 7 release work).

What follows for us:

- **One FFmpeg in the process** ([invariant 10](../../AGENTS.md)). Our `media` crate and Qt's backend must share the same major version; if we ship our own FFmpeg build, Qt loads it too. OpenCut's pin (8.1.3) is a different major than Qt's, so it is not directly reusable.
- **Use Qt Multimedia for what it is good at** (plain playback, audio output, `QVideoSink` access to decoded frames, `QVideoFrameInput` to feed frames in) and our `media` crate for what an editor needs (frame-accurate decode, export, thumbnails, waveforms). Where they overlap, pick one in the Phase 0 video spike and delete the other path.
- **Do not depend on the deprecated native backends.**

## Open questions

- Hardware decode paths per platform (VideoToolbox, D3D11/MediaFoundation, VA-API/Vulkan video) versus a software-only first cut.
- Can a video preview item take a platform video surface directly, or does every frame go through a texture upload (`QVideoSink` frames versus our own decode)? Phase 0 spike on macOS first.
- Audio: waveform, mixing, and whether speech-to-text for cutting by transcript is in scope (Kino-style) or left to the agent via a tool.
