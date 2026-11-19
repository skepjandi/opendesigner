# Video direction: from a request to a finished video

## Which kind of video is it?

| kind | length | shape | notes |
|---|---|---|---|
| App Store app preview | 15 to 30 s | the device's size from `list_video_presets` | autoplays muted: must work with the sound off; screen captures of the app only, words and overlays allowed |
| Reel / TikTok / Short | 7 to 30 s (up to 60) | 1080x1920, 9:16, 30 fps | hook in the first 1.5 s; captions inside the safe zone |
| Explainer / product demo | 45 to 90 s | 1920x1080, 16:9 | one idea; script first |
| Teaser / trailer | 15 to 45 s | any | build to one payoff |
| Edit of the user's footage | their length | their footage's shape | cut, tighten, title, caption |

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
   tokens, type scale, margins) and show real captures inside `device_template` frames.
6. **Build the structure** with one `apply_ops` per beat group (`create_video`, `add_clip`,
   transitions, motion, then overlays). Name clips and overlay frames by what they are.
7. **Look.** `render_video_frame` at each beat and at the middle of every transition; fix clipped
   text, overlaps, empty frames, weak first and last frames. Read `story` in `list_videos`.
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
`set_video` prop audio {file, start_ms, gain, fade_in_ms, fade_out_ms}; `list_sounds` shows what
`media/` holds. The `motion` playbook has the details.

## Quality gate

First frame: strongest picture? Every shot 1 to 6 s? Anything still for more than 4 s without
life? Text readable at phone size (the thumbnail test at 360 px wide)? Nothing important in the
platform UI zones? Ending held, with one clear thing? Sound set, quiet under any voice, faded out? The same transition vocabulary throughout?
If any answer is no, fix it before exporting.

## Memory across tasks

Start from what earlier runs learned: `get_guidance` lists the notes in `.opendesigner/memory/`;
`read_memory` the relevant ones (`brand.md`, `style.md`, `lessons.md`, `decisions.md`). When you are
done, `write_memory` what the next video needs: the brand facts you used and their source, the look
and pacing the user approved, what they cut or corrected, which transitions and caption styles
worked, decisions and mistakes to avoid. Short, factual, never secrets or tokens.
