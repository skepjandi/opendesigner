# Video direction: from a request to a finished video

## Animated, illustrated or explainer? Read `explainer` first

If the request says animated, animation, scene, explainer, motion graphics, cartoon, 2D, illustration,
Kurzgesagt or In a Nutshell, call `get_playbook` with `explainer` before anything else and build the
film that page describes. **An image or JPG backdrop with text animated on top is never an acceptable
animated scene**: the world is vector shapes in depth layers (`add_block` scenery and props, `path`
nodes, `import_svg_vector`) that move, with a camera and ambient loops. Before `export_video`, call
`review_video` and fix its findings (the `explainer` playbook has the gate).

## Which kind of video is it?

| kind | length | shape | notes |
|---|---|---|---|
| App Store app preview | 15 to 30 s | the device's size from `list_video_presets` | autoplays muted: must work with the sound off; screen captures of the app only, words and overlays allowed |
| Reel / TikTok / Short | 7 to 30 s (up to 60) | 1080x1920, 9:16, 30 fps | hook in the first 1.5 s; captions inside the safe zone |
| Explainer / product demo | 45 to 90 s | 1920x1080, 16:9 | one idea; script first; an animated one follows the `explainer` playbook |
| Teaser / trailer | 15 to 45 s | any | build to one payoff |
| Edit of the user's footage | their length | their footage's shape | cut, tighten, title, caption |

## First: take inventory

Before the brief or any `apply_ops`, see what you can already use, then say in one line what you
will use:

1. `list_media` (pictures, including earlier `import_asset` and `make_asset` results),
   `list_footage` (videos, with length and sound), `list_sounds` (music and effects).
2. `list_asset_sources` once, then `search_assets` for what the story needs: icons for each
   feature (`lucide:...`), shapes, blobs, `background` and `pattern` pictures, illustrations from
   installed packs, sounds. `list_asset_packs` shows packs `install_asset_pack` can add (the user
   must approve the download).
3. `get_guidance` and `list_tokens` for the brand colors the assets must match.

Do not fill a video with flat color rectangles and plain text while icons and generators sit
unused.

## Using assets in a video

Every picture reaches the timeline the same way: a file in `media/` becomes an `image` node
inside a design frame, and the frame is a clip or an overlay.

| asset | how |
|---|---|
| icon, shape, logo (`import_asset`, `width`/`height` at least the size it is drawn, `color` a token) | `create` an `image` node in a transparent overlay frame (`add_overlay`, `enter` pop or fade) or inside a scene frame next to its caption |
| background, pattern, aurora, grain, blob (`make_asset`, the video's width and height, token colors, a seed) | for a slideshow or a text video only: an `image` node that fills a scene frame (`image_fit` cover) under the text; grain over footage as an overlay with `opacity` 0.1 to 0.3. Never the world of an animated scene: use vector scenery there |
| picture or screenshot (`list_media`) | an `image` node in a scene frame, or inside a `device_template` screen; give it `motion` on the clip |
| footage (`list_footage`) | `add_clip` with `footage:{file, in_ms}`, then overlays above it |
| sound (`make_sound`, `import_asset`, `import_sound`, `import_file`) | `add_sound_track` (role music, voice or sfx; up to 8, each with start, trim, gain, fades, pan; music can `duck` under the voice) or the older single `set_video` prop `audio` |

The frames must be the video's shape. Credit lines that `import_asset` returned are shown in your
closing message.

## The process

1. **Brief, once.** Ask only what changes the result and is not obvious from the repository or the
   request: where it will be shown (this sets size, length, safe zone), the one thing to remember,
   and the footage or captures to use. Propose the rest in one line and go. If a format was
   chosen in the workspace (`get_brief`) use it.
2. **A workspace per video** (`create_workspace`, for example "instagram reel"), so images and
   videos for the same product do not share a page.
3. **Script and beat sheet first**, as text in your reply and as the video's name and clip names:
   one row per beat with its time, what is on screen and what text. At 2.3 words a second a 20 s
   piece holds about 45 words of on-screen text at most; 3 to 7 words per screen.
4. **The arc.** Hook (0 to 1.5 s: the most interesting picture, the claim or the result) → the
   problem or the ask (a beat) → the proof or how (2 to 3 moments, each one idea) → the payoff and
   a close (held end card). For an app preview: benefit first, show the job being done in the
   real UI, then the second feature; no logo or loading screen at the start.
5. **Source material.** Footage: `list_footage` for length and sound; plan in and out points from
   what you know; cut dead air by using several clips of the same file with different `in_ms`.
   Design frames: build each scene as a frame of the video's size (same system as the stills:
   tokens, type scale, margins) and show real captures inside `device_template` frames. Fill
   frames with the inventory: icons, generated backgrounds and pack art (see "Using assets").
6. **Build the structure** with one `apply_ops` per beat group (`create_video`, `add_clip`,
   transitions, motion, then overlays). Name clips and overlay frames by what they are.
7. **Look.** `render_video_frame` at each beat and at the middle of every transition; fix clipped
   text, overlaps, empty frames, weak first and last frames. Read `story` in `list_videos`.
   Asked for directions first? Build each as its own named video, `offer_variants`, tell the user to
   pick in the Variants panel and wait; `get_variants` has the pick. Call `show_preview` with the
   video once it plays well: the Preview panel is where the user watches it.
8. **Export** with `export_video` and the right `preset`; it shows progress to the user and can be
   stopped. Report the file path, length and what you decided.

## Captions and text on video

- Inside the safe zone (see `formats`); at least 5% of the video's height for the main line; a dark
  semi-transparent band or a drop shape behind text on a busy picture; max two lines.
- Never invent dialogue or captions: if footage has speech and the user wants captions, ask for the
  text or the transcript. Keep words the user's own.
- Muted by default: every important idea is readable without sound.

## Editing the user's footage

Keep their originals untouched (you only ever read `media/`). Remove the weak start and the dead
air; cut on action and at the end of a sentence, never in the middle of a word you cannot hear;
tighten pauses over 0.7 s; use a cut for energy and a short fade only to mark a jump in time. Add
titles only where they help. Do not add effects for their own sake. Say in your reply which parts
you cut and why, so the user can overrule.

## Sound

Plan the sound with the script, not after it. Unless the user asked for silence, the video has a
sound track: a music bed under a voice at `gain` 0.15 to 0.3 (about -18 to -24 dB), fades of 300 to
800 ms, accents on cuts and impacts, and silence before a reveal. Sources: `make_sound` (clicks,
whooshes, risers, hits, chimes, a drone, a beat, a stinger: synthesized, no licence), `search_sounds`
and `import_sound` (Freesound; needs the user's token, show the attribution), `import_file` with
kind audio (the user's own music or recording), a voice-over from a speech MCP server such as
minimax-mcp when `get_guidance` lists it (ask before spending credits on long text). Set it with
`add_sound_track` {file, role, start_ms, gain, fade_in_ms, fade_out_ms, duck} (or `set_video` prop
audio for the one older track); `list_sounds` shows what `media/` holds. A voice-over is a `voice`
track and the bed a `music` track with `duck`, so the bed lowers while the voice speaks. The `motion`
playbook has the details.

Sound must be about what is on screen. Before any `make_sound`, write a cue sheet: one row per
visible event (the time, what appears or moves, the sound that event would make). Cut and slide:
a whoosh (`swoosh_up` for upward or rightward motion, `swoosh_down` for downward) timed to the
motion's peak speed. An object or card landing: `hit`, or `impact` only for the one big moment.
Text or a UI element appearing: `pop` or `blip`; typing: `key` with a different seed per press. A
number counting: `tick` per step. A tension build before a reveal: `riser`, ending on the reveal
with silence for a beat before it. Success or the final title: `chime` or `stinger`, once. Never
the same file for different events: another kind, or the same kind with its own `seed` and
`pitch_hz`, and a file name that says what it is for (`reveal-chime`, `cut-2-whoosh`). A beat
without an event gets no sound. Match the bed to the subject and mood (calm product film: soft
`pad`; energetic: `beat` at the cut rhythm; or a `search_sounds` piece whose tags fit), not to
whatever came first. Then check: play each cue against `render_video_frame` at its start time and
drop any cue whose frame shows nothing happening.

## Captions

Captions are data on the video (`captions`: a style and timed cues), drawn in the preview and burned
into `export_video`. OpenDesigner has no speech-to-text: a transcript comes from the user's own
speech MCP server (or the user), and you bring it in with `import_captions` (`srt`, `vtt` or
`words_json`, the last giving a highlighted active word). With no transcript, `captions_from_script`
spreads the script over a time range by word length: an estimate, so say it is one and prefer real
timings when they exist. `add_caption_cue`, `remove_caption_cue` and `clear_captions` edit cues;
`set_caption_style` takes a preset (`clean`, `bold_pop`, `karaoke`, `boxed`) and overrides (size is a
percent of the video's height, colours are hex). Keep a cue to two lines, check it with
`render_video_frame` inside a cue and in a word of another, and keep captions off the platform's UI zones.

## Quality gate

An animated or illustrated video: `review_video` scores it and lists findings with stable codes (an image
backdrop, only text moving, a still stretch, no ambient loops, no camera move, thin scenes, no sound);
fix every finding or have a reason, until the score is 70 or more. Then:

First frame: strongest picture? Every shot 1 to 6 s? Anything still for more than 4 s without
life? Text readable at phone size (the thumbnail test at 360 px wide)? Nothing important in the
platform UI zones? Ending held, with one clear thing? Sound set, quiet under any voice, faded out? The same transition vocabulary throughout?
If any answer is no, fix it before exporting. For a harsh look at your own animated video (strips of every key motion, crops at 1:1, overlaps), read `get_playbook review`.

## Memory across tasks

Start from what earlier runs learned: `get_guidance` lists the notes in `.opendesigner/memory/`;
`read_memory` the relevant ones (`brand.md`, `style.md`, `lessons.md`, `decisions.md`). When you are
done, `write_memory` what the next video needs: the brand facts you used and their source, the look
and pacing the user approved, what they cut or corrected, which transitions and caption styles
worked, decisions and mistakes to avoid. Short, factual, never secrets or tokens.
