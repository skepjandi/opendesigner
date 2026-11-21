# OpenDesigner MCP tools

Generated from the server's tool definitions. Arguments not marked required are optional. A rejected call returns `isError: true` with the reason and changes nothing.

## `get_guidance`

Read this first. Returns the pages, the nodes the user has selected (with their data), the open notes the user pinned, and the edits the user made after your last turn. 'Make this bigger' means the selection.

Read-only.

No arguments.

## `get_playbook`

Craft guidance for making work that is not mediocre. Topics: design (images), style (named looks with rules), video (start with the inventory of media, footage, sounds and assets), motion (timings, easing, recipes), 3d (devices in 3D, captures from a simulator, moves), formats (sizes and safe zones), logo (identity design), blocks (the catalog of ready-made titles, overlays, backgrounds, charts and motion, and when to use one), scenery (layered flat-vector scenes: scenery blocks, parallax, idle loops, scene moves and the camera), characters (flat-vector characters, bubbles and animated props), explainer (anything animated: animation, scenes, explainers, motion graphics, cartoons, 2D illustration, Kurzgesagt or In a Nutshell style; read it FIRST, a jpg backdrop with moving text is never an animated scene). Call it with no topic for the list; read the ones that apply before you build, not after.

Read-only.

Arguments:

- `topic` (string) 

## `review_frame`

An art director's check of one frame, in numbers: text contrast against what is under it, text too small to read at thumbnail size, text too close to or past an edge, overlapping text, text that overflows its box, misaligned left edges, too many fonts, hard-coded colors that should be tokens, and a size that differs from what the user chose. Fix every `error`, and every `warn` unless you have a reason. Does not change the design.

Read-only.

Arguments:

- `frame` (string, required) Id of a frame from list_pages or get_tree.

## `list_formats`

Sizes to make things in, in sections (Video, Store images, Social images, Web and icons): id, label, width, height, frame rate. Use when the workspace has no format yet; then record your pick with set_format.

Read-only.

No arguments.

## `get_brief`

What this workspace is for: its format (size, aspect ratio, frame rate), who chose it (the user or you), a request waiting for the user's answer, and the user's last answer.

Read-only.

No arguments.

## `set_format`

Set this workspace's format: a `format` id from list_formats, or a custom size (`kind` video or image, `width`, `height`, `fps` for video). If the user chose the current format this does NOT change it: the user is asked, and you must stop and wait (call get_brief later to read the answer). If they chose nothing it applies at once. A video cannot be created or resized away from a format the user chose.

Arguments:

- `format` (string) An id from list_formats.
- `fps` (integer) 
- `height` (integer) 
- `kind` (string) 
- `reason` (string, required) One line for the user: why this format.
- `width` (integer) 

## `get_selection`

The nodes the user has selected, with their data.

Read-only.

No arguments.

## `get_annotations`

The notes the user pinned on nodes or page regions.

Read-only.

Arguments:

- `include_resolved` (boolean) Also list resolved notes.

## `resolve_annotation`

Mark a note resolved once you have acted on it. Say what you did in the user's terms.

Arguments:

- `id` (integer, required) 
- `note` (string) What you changed.

## `list_pages`

Pages and their top-level nodes.

Read-only.

No arguments.

## `get_node`

One node with its properties and, to `depth` levels, its children.

Read-only.

Arguments:

- `depth` (integer) 
- `id` (string, required) Node id, for example n_7.

## `get_tree`

A page as an indented outline: id, kind, name, position and size.

Read-only.

Arguments:

- `depth` (integer) 
- `page` (string) Page id; defaults to the first page.

## `find_nodes`

Find nodes by name and kind.

Read-only.

Arguments:

- `kind` (string) 
- `limit` (integer) 
- `name_contains` (string) 

## `list_tokens`

Design tokens: named colors usable as paints like {brand.gold}.

Read-only.

No arguments.

## `apply_ops`

Change the design with a list of operations, applied together or not at all. Operations: {op:'create', parent, index?, node:{kind, name?, x, y, width, height, fill?, stroke?, corner_radius?, opacity?, visible?, text?, font_size?, font_family?, font_weight?, text_align?, line_height?, image?, image_fit?, device3d?, path?, view_box?, fill_rule?, rotation?, scale_x?, scale_y?, pivot_x?, pivot_y?, blur?, shadow?, clip?, depth?, children?}}, {op:'set', id, prop:{name, value}} with name one of name, x, y, width, height, visible, opacity, fill, stroke, corner_radius, text, font_size, font_family, font_weight, text_align, line_height, image (a media/ path from list_media), image_fit (cover, contain or fill), path, view_box, fill_rule, stroke_width, trim_start, trim_end, rotation, scale_x, scale_y, scale (both), scale_xy (value {x, y}), pivot_x, pivot_y, blur, drop_shadow, shadow_dx, shadow_dy, shadow_blur, clip, depth (see Transform and effects below), {op:'move', id, x, y}, {op:'reparent', id, parent, index?}, {op:'delete', id}, {op:'set_token', name, value} (a named color '#rrggbb'; value null removes an unused token), {op:'set_localized_text', locale, id, text} (a translation of a text node; text null removes it). Vector shapes: kind 'path' draws SVG path data in its box: path:'M0 0 C10 20 30 20 40 0 Z' (commands M L H V C S Q T A Z, absolute or relative, at most 4000 segments and 64 KB). The path's own bounds are stretched over the node's width and height, so resizing scales it; give no width and height and the box takes the path's size. view_box:{x, y, width, height} maps that rectangle of path coordinates onto the box instead (exact placement for imported art); fill_rule is nonzero (default) or evenodd. Any shape (frame, rect, ellipse, path) takes a gradient as fill or as stroke paint: {type:'linear', angle?: degrees, 0 left to right, 90 top to bottom (default), stops:[{offset: 0 to 1, color}]} or {type:'radial', cx?, cy?: fractions of the box (0.5), r?: fraction of the longer side (0.5), stops}, 2 to 8 stops whose colors are '#rrggbb[aa]' or '{token}'. A stroke is {paint, width, cap?: butt|round|square, join?: miter|round|bevel, dash?: [dash, gap, ...], trim_start?, trim_end?: 0 to 1 along the outline, only the stretch between them is drawn}; animate trim_end from 0 to 1 to draw a line on. A stroke width of 0 draws nothing. import_svg_vector turns a whole SVG illustration into path nodes. 3D devices: kind 'device3d' is a phone, tablet, laptop or monitor drawn in 3D inside its box (the box is the viewport; 1 zoom fits the device with a margin at any turn); its fill is the body finish, its image (with image_fit) is the capture on the screen; create it with device3d:{model: phone|phone_punch_hole|tablet|laptop|monitor, rotate_x?, rotate_y?, rotate_z?, zoom?, fov?, light_angle?, shadow?, glare?, screen_scroll?, screen_footage?:{file:'media/x.mov', in_ms}} and change it with set props model, rotate_x (tilt, + leans the top away), rotate_y (turn, + turns the right edge away), rotate_z (roll, + clockwise), zoom (0.05 to 10), fov (1 to 100 degrees, 30 natural, low is flat), light_angle (degrees, 0 from above, 90 from the right), shadow (0 to 1), glare (0 to 1, screen reflection), screen_scroll (0 top to 1 bottom of a long capture; null centres), screen_footage (value {file, in_ms}: a recording on the screen that plays while the frame is shown; null shows the image). Transform and effects (any node, drawn together with everything inside it; defaults mean no effect): rotation (degrees, clockwise, up to 36000), scale_x and scale_y (default 1, negative flips; scale sets both), pivot_x and pivot_y (fractions of the node's own box that rotation and scale turn about, default 0.5 the center; a group without a size uses the box around its children), blur (Gaussian radius in px, 0 to 500), shadow (create) or drop_shadow (set) = {dx, dy, blur, color} with the color's alpha as its opacity (null removes it; shadow_dx, shadow_dy and shadow_blur change one number of an existing shadow and can be keyframed), clip (true on a group or frame: children are cut at its box; a group needs a width and height for that, a frame always clips) and depth (0 to 10, default 1: how strongly the video's camera moves the node, 0 pins it to the screen, 0.5 moves half as far for parallax; depths multiply down the tree). The exported frame ignores its own transform. A node's bounds (and what a click hits) are the box around it after its transform. Videos (see list_videos): {op:'create_video', name, fps, width, height} (even sizes), {op:'delete_video', id}, {op:'set_video', id, prop:{name, value}} with name one of name, fps, size (value {width, height}), audio (value {file:'media/x.mp3', start_ms?, gain? (0 to 2), fade_in_ms?, fade_out_ms?} for the sound under the picture; null takes it away), motion_blur (0 off, or 2 to 16 pictures blended while anything moves), hold_frames (1 to 6, default 1: every drawing is held for this many frames, 2 is animation on twos, 3 on threes; audio is unaffected), {op:'add_clip', video, frame | footage:{file:'media/x.mp4', in_ms}, duration_ms, index?, transition?, motion?} (a clip shows a design frame or plays footage from list_footage), {op:'remove_clip', video, clip}, {op:'move_clip', video, clip, index}, {op:'set_clip', video, clip, prop:{name, value}} with name one of duration_ms, frame, source (value {frame} or {footage:{file,in_ms}}), transition (value {kind: cut|fade|slide_left|slide_right|slide_up|slide_down|dip|zoom_in|wipe_left|wipe_right|wipe_up|wipe_down|iris|flash|blur_dissolve|glitch_slice, duration_ms, ease?}), motion (value {scale_from, scale_to, ease?}, 1 to 3), speed (footage only, 0.25 to 4; duration_ms is the length on the timeline, the clip plays duration_ms*speed of the file), speed_ramp (footage only, value [{at_ms, rate 0.25 to 4, ease?}] in clip time, needs speed 1; [] removes it), grade (value {exposure?(-3..3 stops), contrast?, saturation?, temperature?, tint? (all -1..1), vignette? (0..1), blur? (0..50), look?: warm|cool|mono|faded|punchy}, or null; a look fills the numbers and the numbers you give override it; also set on an image node with {op:'set', id, prop:{name:'grade', value}}). Times (durations, starts, in points, transitions, keys) are rounded to the video's frame grid; the receipt shows the value kept. {op:'add_overlay', video, frame, start_ms, duration_ms, fade_ms?, enter?, exit?, ease?} (a design frame drawn over the whole video for a while: titles, lower thirds, call-outs; a frame with no fill is transparent; enter and exit are none, fade, slide_up, slide_down, slide_left, slide_right, pop or zoom, taking fade_ms each), {op:'remove_overlay', video, overlay}, {op:'set_overlay', video, overlay, prop:{name, value}} with name one of frame, start_ms, duration_ms, fade_ms, enter, exit, ease, {op:'animate', video, node, prop, keys:[{at_ms, value, ease?}], repeat?} (keyframes one number of a node over the video's time, at_ms from the start of the video, strictly increasing; the value holds before the first key and after the last unless repeat is loop (start again from the first key, for as long as the video plays; end on the first key's value for a seamless cycle) or pingpong (play back and forth), once being the default and what an animate without repeat sets, so send repeat again when you change the keys of a looping track; prop one of x, y, width, height, opacity, corner_radius, rotation, scale_x, scale_y, scale (both), blur, shadow_dx, shadow_dy, shadow_blur (the node needs a shadow), rotate_x, rotate_y, rotate_z, zoom, fov, light_angle, shadow, glare, screen_scroll, trim_start, trim_end, stroke_width (the node needs a stroke), fill_color (key value '#rrggbb[aa]', mixed in sRGB and listed back as the number 0xRRGGBBAA; the node needs a fill), or on an image node grade_exposure, grade_contrast, grade_saturation, grade_temperature, grade_tint, grade_vignette, grade_blur; the camera props camera_x, camera_y (pan in px: the scene moves the other way), camera_zoom (0.1 to 20, about the frame's center) and camera_rotation (degrees the scene turns clockwise about the center) take the VIDEO's id as node and move every node of a clip's frame by its depth (nodes at depth 0 and overlays stay); a single key holds a camera still; keys:[] removes it; animate_preset writes some of these for you). Eases: linear, ease_in, ease_out, ease_in_out (also ease_in_out_cubic), back (overshoots; also ease_out_back), spring (overshoots and settles), elastic (rings past the end) or bounce (bounces off the end); power2_in|out|in_out and power4_in|out|in_out (stronger cubic-like curves), expo_in|out|in_out, circ_in|out|in_out, sine_in|out|in_out (the gentlest); steps_2, steps_3, steps_4, steps_6, steps_8 and steps_12 (no in-between: the value holds each of that many poses, jumping between them, for the stepped feel of drawn animation). Colors are '#rrggbb', '#rrggbbaa' or '{token.name}'. Template variables (see list_variables): {op:'declare_variable', variable:{id, kind, label, description?, portrays?, default, enum_values?, binds?:[{node, prop}]}} with kind one of string, number, color, bool, enum, image, font (enum_values only for enum) and portrays one of subject_name, subject_domain, subject_tagline, subject_logo, host_identity, {op:'set_variable', id, prop:{name, value}} with name one of label, description, portrays, default, enum_values, binds, {op:'remove_variable', id}, {op:'set_variable_value', id, value} (writes every bound node property through the same validated path as set, so it is attributed and undoable). Ids are minted for you; the result lists the created ids. A checkpoint is taken before your first change in a turn so the user can restore it. Set dry_run true to check the operations without applying them: you get ok, what each operation would do, the node ids that would change, and for a rejection a stable code (E_NODE_NOT_FOUND, E_OUT_OF_BOUNDS, E_LIMIT, ...) with a hint; nothing is changed, checkpointed or saved. An applied result has stage 'applied', the checkpoint taken before the turn, the transaction, the changed ids and whether the project was saved.

Arguments:

- `dry_run` (boolean) Check the operations on a scratch copy and report what would happen; changes nothing.
- `ops` (array, required) 
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.

## `list_checkpoints`

Named points the design can be restored to.

Read-only.

No arguments.

## `diff_checkpoint`

The nodes changed since a checkpoint (those that still exist).

Read-only.

Arguments:

- `checkpoint` (integer, required) 

## `restore_checkpoint`

Return the design to a checkpoint. This appends an undoable change; nothing is lost.

Arguments:

- `checkpoint` (integer, required) 

## `list_export_presets`

Store export presets: sizes, formats and rules.

Read-only.

No arguments.

## `list_media`

Images the user has imported into the project (media/), with their size in pixels; pictures made by make_asset and import_asset land here too. Use a path as the `image` of an image node (in a video, in a frame that is a clip or an overlay). If the one you need is not here, search_assets, or use find_files and import_file (when the user allows it) or ask the user to add it in the app.

Read-only.

No arguments.

## `list_locales`

The languages of the design and what each still lacks, plus every text node with its default text. Translate with apply_ops set_localized_text; export a language with the `locale` argument of export, export_set and export_panorama.

Read-only.

No arguments.

## `render_preview`

Render a frame to an image so you can look at the result.

Read-only.

Arguments:

- `frame` (string, required) Id of a frame node.
- `max_side` (integer) 
- `scale` (number) 

## `export`

Render a frame to exports/<file> in the project, validated against a store preset. Nothing is written when the image does not meet the preset.

Arguments:

- `alpha` (boolean) Keep transparency (ignored when the preset forbids it).
- `file` (string, required) File name ending in .png, for example shot-01.png.
- `frame` (string, required) Id of a frame node.
- `locale` (string) A language from list_locales: text nodes show that language; set files are named <prefix>-<locale>-NN.png.
- `preset` (string) A preset id from list_export_presets.
- `scale` (number) 

## `export_set`

Render several frames to exports/<prefix>-01.png, -02, ... in the order given, validated as a set against a store preset (every image, and the count of images). Nothing is written unless every image and the count meet the preset.

Arguments:

- `alpha` (boolean) Keep transparency (ignored when the preset forbids it).
- `frames` (array, required) 
- `locale` (string) A language from list_locales: text nodes show that language; set files are named <prefix>-<locale>-NN.png.
- `prefix` (string, required) File name stem, for example shot (gives shot-01.png).
- `preset` (string) A preset id from list_export_presets.
- `scale` (number) 

## `export_panorama`

Render one wide frame and cut it into `slices` equal columns, exports/<prefix>-01.png, -02, ... so a background, ribbon or device can cross the seams between store screenshots. The frame width (times scale) must divide evenly; each slice and the count are validated against the preset before anything is written.

Arguments:

- `alpha` (boolean) Keep transparency (ignored when the preset forbids it).
- `frame` (string, required) Id of the wide frame.
- `locale` (string) A language from list_locales: text nodes show that language; set files are named <prefix>-<locale>-NN.png.
- `prefix` (string, required) File name stem, for example shot (gives shot-01.png).
- `preset` (string) A preset id from list_export_presets.
- `scale` (number) 
- `slices` (integer, required) 

## `device_template`

A generic phone or tablet outline (body, screen, camera) as a node you can put in an apply_ops create operation. The node named Screen is where a capture goes: pass an `image` from list_media, or set `image` on Screen later.

Read-only.

Arguments:

- `image` (string) A media/ path from list_media.
- `kind` (string, required) 
- `screen_width` (number, required) Width of the screen in document units.

## `list_videos`

The project's videos: each is a timeline of clips that show design frames, with the time every clip starts, its transition (which overlaps the clip before it) and its zoom, the overlays, the keyframe animations (node, property, keys) and motion blur. A video may also have one sound track (the file, when it starts and its volume). Start a video with list_media, list_footage, list_sounds and search_assets to see what you can use. `story` says all of it in sentences. Change them with apply_ops (create_video, add_clip, add_overlay, set_clip, animate, ...) or animate_preset; look at one moment with render_video_frame, at the motion with video_contact_sheet, and write the MP4 with export_video.

Read-only.

No arguments.

## `list_memory`

The agent memory of this repository: small text notes in .opendesigner/memory/ that earlier runs left for the next one (brand facts, what the user corrected or liked, lessons, decisions). Returns name, bytes, title (first line) and modified time. Read the relevant ones with read_memory before you build.

Read-only.

No arguments.

## `read_memory`

Read one memory note by name (for example brand.md or lessons.md).

Read-only.

Arguments:

- `name` (string, required) A note name from list_memory: lowercase letters, digits, . _ -, ending .md or .txt.

## `write_memory`

Leave a note for the next run, in .opendesigner/memory/ (a folder the user tracks in git). Write what future work should know: brand facts (colors, fonts, copy) and where they came from, what the user corrected or liked, what worked, mistakes to avoid, decisions. Files like brand.md, lessons.md, decisions.md, style.md. Short, factual, Markdown. NEVER store secrets, tokens or passwords. mode append (default) adds a dated entry; replace rewrites the file (use it to merge and tidy). Limits: 64 KB per note, 200 notes, 2 MB total.

Arguments:

- `mode` (string) append (default) or replace.
- `name` (string, required) Note name: lowercase letters, digits, . _ -, ending .md or .txt.
- `text` (string, required) The note text (Markdown).

## `list_workspaces`

The workspaces of this repository: separate designs side by side (an app store set, a reel, a landing page), each with its own pages, videos, notes and media. Says which one is open; every other tool works on the open one.

Read-only.

No arguments.

## `create_workspace`

Make a new, empty workspace and open it. Use one workspace per deliverable (for example "app store" and "instagram reel") instead of putting different things on the same page. Names are up to 40 letters, digits, spaces, - or _.

Arguments:

- `name` (string, required) The workspace name.

## `open_workspace`

Switch to another workspace of this repository (from list_workspaces). The open one is saved first; the user sees the switch.

Arguments:

- `name` (string, required) A name from list_workspaces.

## `find_files`

Look for pictures, videos and sounds (.mp3 .wav .m4a .aac .ogg) anywhere under the user's home folder (the user allows this; hidden folders and caches are skipped). Returns paths, sizes and dates, newest first, at most 200. Use `import_file` to bring one into the project.

Read-only.

Arguments:

- `folder` (string) A folder to search instead of the whole home folder, for example ~/Pictures or an absolute path under your home folder.
- `kind` (string) Default any.
- `name_contains` (string) Part of the file name.

## `import_file`

Copy one picture, video or sound from the user's computer into this project's media/ folder (it is checked to be a real PNG, JPEG, WebP, MP4/MOV, or MP3, WAV, M4A, AAC or OGG first) and return its media/ reference. The original is never changed. An imported sound can be the video's sound track: set_video prop {name: "audio", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}.

Arguments:

- `path` (string, required) Absolute path (or ~/...) of the file, from find_files.

## `list_footage`

Videos the user imported into the project (media/): reference, size on disk and, when ffmpeg is installed, length, picture size, frame rate and whether it has sound. Play one in a video with add_clip footage:{file, in_ms}. Bring in another with find_files and import_file.

Read-only.

No arguments.

## `search_sounds`

Find music and sound effects on Freesound (https://freesound.org) for a video. Returns each sound's id, name, length, license, author and a preview to listen to. Freesound sounds are licensed by their authors, not by us: the license in the result is the license the user must obey, and you must show the attribution the result carries (title, author, license, link) wherever the sound is used. Prefer CC0. Needs the user's Freesound token (the Connect tab); without one the call says so.

Read-only.

Arguments:

- `duration_max_ms` (integer) Keep only sounds no longer than this.
- `page` (integer) Which page of results (1 is the first).
- `query` (string, required) Words to search for, for example "soft piano loop".

## `import_sound`

Download one Freesound sound (the preview recording, not the original file) into this project's media/ folder and write what it is next to it. Returns the media/ reference and the one line the user must show: "Title by Author, License, link". The license must be honoured: say who made it, under which license, and where it came from. The file is never overwritten. Then put it under a video with set_video prop {name: "audio", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}.

Arguments:

- `id` (integer, required) The id of a sound from search_sounds.
- `name` (string) A plain file name for the sound, without the .mp3 ending.

## `list_sounds`

All the sounds in this project's media/ folder (.mp3 .wav .m4a .aac .ogg), whether made with make_sound, downloaded with import_sound or imported with import_file: the file, size, length when ffmpeg is installed, where it came from (synthesized, freesound or imported) and, for a Freesound sound, the attribution line the user must show. Any of them can be the video's sound track (set_video audio).

Read-only.

No arguments.

## `make_sound`

Make a sound effect or a simple loop yourself, with no download and no licence to honour (it is synthesized and written to media/<name>.wav, normalized, with short fades so it never clicks). Use it for the basics: click, pop, tick, whoosh, swoosh_up, swoosh_down, riser (builds into a cut), hit (impact: put it on a cut or a reveal), chime (a notification or a finish), stinger (a 2 to 3 note sting), drone (a soft pad that loops) and beat (4-on-the-floor kick and hat at `bpm`, `bars` bars). It cannot make real music or speech: for a music bed use search_sounds (Freesound, needs the user's token) or ask the user; for a voice-over use a speech MCP server if the user added one (get_guidance lists them). The same kind, duration and seed give the same sound. Then set it as the track: set_video prop {name: "audio", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}.

Arguments:

- `bars` (integer) Beat only: bars of four beats.
- `bpm` (number) Beat only: tempo, default 120.
- `duration_ms` (integer) How long. Optional: each kind has a fitting default. A beat takes its length from bpm and bars.
- `kind` (string, required) Which sound.
- `name` (string) A plain file name without the .wav ending; default is the kind. An existing file is never overwritten.
- `pitch_hz` (number) The base pitch of chime, drone, tick, pop, riser, hit and stinger.
- `seed` (integer) Another seed gives another variation (noise, the notes of a stinger, and the pitch or tempo when you give none). Default: a new random seed each call, returned as `seed` and kept in the sidecar. Pass the same seed to make the same sound again.
- `stereo` (boolean) Two channels (default one).

## `list_video_presets`

Store rules for videos (app previews): sizes, length, frame rate and file size, with the page each was read from and what is not checked.

Read-only.

No arguments.

## `render_video_frame`

Render one moment of a video to an image so you can look at the result.

Read-only.

Arguments:

- `locale` (string) A language from list_locales.
- `max_side` (integer) 
- `time_ms` (integer, required) Milliseconds from the start.
- `video` (string, required) Id of a video from list_videos.

## `export_video`

Draw every frame of a video and write it as an H.264 MP4 to exports/<file>. Needs ffmpeg on the user's computer (the error says what to install). This can take a while. For an animated or illustrated video, run review_video first and fix its findings.

Arguments:

- `file` (string, required) File name ending in .mp4, for example preview.mp4.
- `locale` (string) A language from list_locales: text nodes show that language.
- `loudness` (integer) Bring the final mix to this integrated loudness in LUFS: -14 social, -16 podcast, -23 broadcast. Default: leave the level as mixed. Measured first, then applied as a gain (ffmpeg loudnorm); a video with no sound is unaffected.
- `preset` (string) A preset id from list_video_presets: the video is checked against the store's rules and nothing is written if it breaks one.
- `video` (string, required) Id of a video from list_videos.

## `validate_export`

Check exported PNGs in exports/ as one set against a store preset.

Read-only.

Arguments:

- `files` (array, required) 
- `preset` (string, required) A preset id.

## `animate_preset`

Animate a node with a named move, measured from its designed pose, as keyframes you can then inspect in list_videos and change with apply_ops animate. 3D device moves: spin (a full turn; strength 2 is two), turntable (slow side to side), swing (away one way, then the other, back), flip_in (turns from its back to face the viewer, springing), hero_reveal (starts flat and square, settles into the designed tilt and turn), push_in and pull_back (camera closer or settling back), orbit (the view swings around the device), scroll_screen (scrolls a long capture top to bottom). Any node: rise_in, drop_in, float (gentle drift; a 3D device also sways), fade_in, fade_out, slide_in_left, slide_in_right. Keys already inside start_ms..start_ms+duration_ms for the same properties are replaced and the rest kept, so moves chain. Times are from the start of the video; the node should be in a frame the video shows at that time. Scene moves work on any node, not only 3D devices, and write ordinary keyframes. Ambient (loop by default, end where they start, `seed` shifts the phase so nodes do not move in lockstep; duration_ms is one cycle): float (lazy drift), bob (up and down with a little tilt), breathe (squash and stretch), pulse (a beat of scale), spin (intensity = turns), wobble, swing (a pendulum tilt), orbit (the node's middle circles `center_x`/`center_y`, or a point to its left; `radius`). Entrances and impacts (play once): pop_in (scale 0 to 1 with a small overshoot), bounce_in (drops in and bounces), elastic_in (rings in), slide_in_left|right|up|down (with overshoot), fade_up, zoom_in, shake (decays), draw_on (a stroked node's trim_end 0 to 1). Camera moves write the video's camera (the node is the video; layers with a `depth` follow it by that much): camera_push_in, camera_pull_back, camera_pan (`direction` left|right|up|down, `amount` in document units), camera_drift (slow loop), camera_shake (decays), camera_whip (fast pan, `direction`), camera_burst (a kick that is fastest on its first frame, decays over about nine frames with power4_out and then holds; `direction`, `amount`, `start_ms`). animate_along_path: the `path` node's curve is sampled into x/y keys (and, with `orient`, rotation) so `node` follows it over the time range with `ease`. stagger: apply the move named in `of` to every id in `nodes`, `step_ms` apart, starting `from` first|last| center|edges|random (seeded); with `columns` the nodes are a grid read row by row and each is delayed by its distance (`metric` manhattan or euclid, default euclid; `step_ms` per unit) from the start cell: `start_index` (an index into `nodes`) or the cell `from` names. `intensity` scales how far a move goes (1 as designed), `repeat` is once|loop|pingpong, `ease` overrides the move's own.

Arguments:

- `amount` (number) camera_pan, camera_whip and camera_burst: how far the camera goes, in document units.
- `center_x` (number) orbit: the middle of the circle, in the node's parent coordinates.
- `center_y` (number) orbit: the middle of the circle, in the node's parent coordinates.
- `columns` (integer) stagger: lay the nodes out as a grid this many columns wide, row by row, and delay by distance from the start cell.
- `direction` (string) camera_pan, camera_whip and camera_burst.
- `duration_ms` (integer) Needed by the older moves; a scene move has its own default (one cycle, for a loop).
- `ease` (string) Overrides the preset's own ease.
- `from` (string) stagger: which node goes first (edges: both ends together, then inward; in a grid, the outer ring first).
- `intensity` (number) How far the move goes; 1 is as designed (the same as strength).
- `metric` (string) stagger with columns: how distance is measured (default euclid).
- `node` (string) Id of the node to move (a device3d for the 3D moves); the video's camera moves need none.
- `nodes` (array) stagger: ids of the nodes, in order.
- `of` (string) stagger: the move to apply to each node (any move name except stagger).
- `orient` (boolean) animate_path: turn the node to face along the path.
- `path` (string) animate_path: id of the path node to follow.
- `preset` (string, required) 
- `radius` (number) orbit: the radius when there is no center.
- `repeat` (string) What the track does after its last key (scene moves; ambient ones loop by default).
- `seed` (integer) Shifts an ambient move's phase and orders a random stagger.
- `start_index` (integer) stagger with columns: index in nodes of the cell the wave starts from (overrides from).
- `start_ms` (integer, required) 
- `step_ms` (integer) stagger: delay between one node and the next.
- `strength` (number) How far the move goes; 1 is as designed.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `device3d_template`

A device in 3D (phone, tablet, laptop or monitor) as a node you can put in an apply_ops create operation. The node's box is the viewport the device is drawn in; the device is fitted into it with a margin, so it never runs out of the frame when it turns. Give it the capture to show on the screen. Change it afterwards with apply_ops set (model, rotate_x, rotate_y, rotate_z, zoom, fov, light_angle, shadow, glare, screen_scroll, screen_footage).

Read-only.

Arguments:

- `image` (string) A media/ path from list_media: the capture on the screen.
- `model` (string, required) 
- `rotate_x` (number) Tilt in degrees; positive leans the top away.
- `rotate_y` (number) Turn in degrees; positive turns the right edge away.
- `screen_width` (number, required) Width of the viewport in document units.

## `video_contact_sheet`

Several moments of a video in one image, in a grid read left to right, top to bottom: the way to judge a move (its arc, its timing, where it settles) instead of one still. Give `times` or let it spread `count` moments evenly from `from_ms` to `to_ms` (the whole video by default).

Read-only.

Arguments:

- `cell_side` (integer) Longest side of each picture (default 256).
- `count` (integer) 
- `from_ms` (integer) 
- `locale` (string) A language from list_locales.
- `times` (array) 
- `to_ms` (integer) 
- `video` (string, required) Id of a video from list_videos.

## `list_blocks`

The block catalog: ready-made titles, lower thirds, overlays, backgrounds, charts, store headlines, motion wrappers, rigged flat-vector characters, speech and thought bubbles and animated props that expand into ordinary nodes (and keyframes on a video). Search by what you need (`query`: words matched against names, tags and descriptions) and/or `category`. Each result has the block id, its category and what it is for. Then get_block for its parameters and add_block to use it. Scenery blocks (sky_gradient, parallax_hills, starfield, clouds, sun_glow, ocean_waves, city_skyline, cells_backdrop, particles, grid_floor, connector, highlight_ring) build layered, always-moving flat-vector scenes: read get_playbook `scenery` first.

Read-only.

Arguments:

- `category` (string) 
- `query` (string) Words describing what you need, e.g. 'lower third name' or 'bar chart'.

## `get_block`

One block's full description: what it makes, every parameter (type, range or options, default), the default box it lays itself out in, whether it moves, and the standard parameters every block takes.

Read-only.

Arguments:

- `block` (string, required) A block id from list_blocks.

## `add_block`

Add a block to a frame as one undoable step. It makes a group named `block:<id>#<n>` with named parts inside (find them with get_tree, change them with apply_ops, delete the group to remove the block); with a `video` it also keyframes the parts using the same animations as animate_preset. Colors come from the document's tokens (or tokens the block creates once, named block.background/foreground/accent/muted; set_token changes them everywhere) unless you pass `colors`. The block's defaults are placeholders: write your own copy and colors, then check the result with render_preview and review_frame. The result lists the created root and its parts. Use `dry_run` to see the operations without applying them.

Arguments:

- `block` (string, required) A block id from list_blocks.
- `box` (object) Where it lays itself out, in frame coordinates; omitted sides use the block's default box.
- `colors` (object) Override the roles with #rrggbb or a {token}.
- `dry_run` (boolean) Return the operations without applying them.
- `font` (string) One font family for all of the block's text (default: the bundled one).
- `frame` (string) Id of the frame to put it in (optional when the document has one frame).
- `hold_ms` (integer) Fade the block out this long after start_ms (needs a video; at least as long as its entrance). Omit to keep it on screen.
- `params` (object) The block's own parameters (see get_block).
- `start_ms` (integer) When it starts, from the start of the video (default 0).
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string) Id of a video from list_videos to keyframe in (optional when there is one video; without one the block is static).

## `validate_document`

Lint the whole document and its videos and get findings with stable rule codes (OD001 ...): nodes outside their frame, clipped or overflowing or tiny or empty text, low contrast, missing images or footage or sound, undefined tokens, hidden nodes a video animates, overlapping siblings, frames that match no export target, video timing problems, and broken template variable bindings. Each finding says what is wrong, where (node, frame or video) and how to fix it. Read-only. Call it before you finish a turn and fix every error and warning, or say why you left one.

Read-only.

Arguments:

- `min_severity` (string) Only list findings at or above this severity (default info). Counts always cover everything.

## `list_variables`

The document's template variables: id, kind (string, number, color, bool, enum, image, font), label, description, what it portrays (subject_name, subject_domain, subject_tagline, subject_logo, host_identity: slots that carry the identity of what the piece is about, which you must take from the user and never invent), default, current value, enum_values and the node properties it is bound to. Declare and change them with apply_ops declare_variable, set_variable, set_variable_value and remove_variable.

Read-only.

No arguments.

## `batch_export`

Render the project once per row of template-variable values (see list_variables), for personalized sets such as one card per customer or one video per language. Each row is {name, variables:{<variable id>: value}, locale?}; the project is never changed (values are applied to a scratch copy through set_variable_value). Pick exactly one of frame (a PNG per row) or video (an MP4 per row, needs ffmpeg). A row either exports completely and validates against preset, or fails with its reason and writes nothing; other rows still run. Files go to exports/ only: out_template (default exports/{name}.png or .mp4) may use {name} and {index}. Unknown or mistyped variables are skipped with a warning unless strict_variables is true, which fails the row. At most 100 rows (20 for video).

Arguments:

- `frame` (string) Id of the frame to render for each row (a PNG).
- `out_template` (string) exports/{name}.png (or .mp4 for a video); {name} and {index} are replaced.
- `preset` (string) A store preset id (list_export_presets or list_video_presets) every output must meet.
- `rows` (array, required) Rows {name: letters/digits/_/-, variables: {id: string|number|boolean}, locale?}.
- `scale` (number) Pixels per document unit for frames (default 1).
- `strict_variables` (boolean) Fail a row that names an unknown variable or gives a value of the wrong type.
- `video` (string) Id of the video to render for each row (an MP4).

## `add_sound_track`

Add a sound track to a video, up to 8 besides the older single `audio`: music, a voice-over or effects, each with its own start, trim (in_ms/out_ms), gain, fades, pan and, for music, `duck`: the music lowers while a voice track speaks (a side-chain compressor in the export's one ffmpeg pass; the picture is copied, not re-encoded; footage sound is still mixed). Put a voice-over in with role voice, then music with `duck: {}` and gain 0.3 to 0.6. Returns the track id for set_sound_track and remove_sound_track. Speech is not made here: for a voice-over use a speech MCP server the user added (get_guidance lists them) and import the file with import_file.

Arguments:

- `duck` (any) Music only: lower under the voice tracks. {depth 0-1 (default 0.7), attack_ms (default 40), release_ms (default 500)}; null turns it off.
- `fade_in_ms` (integer) 
- `fade_out_ms` (integer) 
- `file` (string, required) An audio file in media/ from list_sounds (media/<name>.mp3|wav|m4a|aac|ogg).
- `gain` (number) 1 is as recorded. A music bed under a voice: 0.15 to 0.3.
- `in_ms` (integer) Trim in: where in the file it starts playing.
- `out_ms` (any) Trim out: where in the file it stops (null or omitted plays to the end).
- `pan` (number) -1 all left, 1 all right.
- `role` (string) music (default; may duck), voice (a voice-over: music ducks under it) or sfx.
- `start_ms` (integer) When it starts, from the start of the video.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `set_sound_track`

Change one sound track of a video: only the fields you give change; `out_ms: null` and `duck: null` clear them. The older single track (id `audio`) is changed with set_video audio.

Arguments:

- `duck` (any) Music only: lower under the voice tracks. {depth 0-1 (default 0.7), attack_ms (default 40), release_ms (default 500)}; null turns it off.
- `fade_in_ms` (integer) 
- `fade_out_ms` (integer) 
- `file` (string) An audio file in media/ from list_sounds (media/<name>.mp3|wav|m4a|aac|ogg).
- `gain` (number) 1 is as recorded. A music bed under a voice: 0.15 to 0.3.
- `in_ms` (integer) Trim in: where in the file it starts playing.
- `out_ms` (any) Trim out: where in the file it stops (null or omitted plays to the end).
- `pan` (number) -1 all left, 1 all right.
- `role` (string) music (default; may duck), voice (a voice-over: music ducks under it) or sfx.
- `start_ms` (integer) When it starts, from the start of the video.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `track` (string, required) A track id from list_videos (a1, a2, ...).
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `remove_sound_track`

Remove one sound track from a video.

Arguments:

- `summary` (string) One line, in the user's terms, naming what this turn does.
- `track` (string, required) A track id from list_videos.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `detect_pauses`

Find the pauses in the sound of a file in media/ (audio or footage): stretches at least `min_ms` long quieter than `noise_db`, by ffmpeg's silencedetect. Returns the pauses and the stretches of sound between them, in the file's own time. Needs ffmpeg. Use it to plan a cut with remove_pauses or to time captions to speech.

Read-only.

Arguments:

- `file` (string, required) A media/ file: audio or footage.
- `min_ms` (integer) A pause is at least this long (default 500).
- `noise_db` (number) Quieter than this is a pause (default -30).

## `remove_pauses`

Cut the pauses out of a footage clip: the clip becomes several clips that play only the stretches with sound, one after the other, in one undoable step (a zoom on the clip carries on across the cuts). `keep_ms` of each pause is left at both ends so speech is not clipped. The video gets shorter; overlays, captions and sound tracks keep their times, so move them after, and read the new story in list_videos. Needs ffmpeg.

Arguments:

- `clip` (string, required) A footage clip id from list_videos.
- `keep_ms` (integer) Pause kept at each end (default 120).
- `min_ms` (integer) Shortest pause to cut (default 400).
- `noise_db` (number) Default -30.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `get_waveform`

Downsampled peaks (0 to 1) of the sound in a media/ file (audio or footage), decoded by ffmpeg: `buckets` numbers from 50 to 2000 across the file's whole length. The window's Sound tracks draw the same. Needs ffmpeg.

Read-only.

Arguments:

- `buckets` (integer) Default 400.
- `file` (string, required) A media/ file: audio or footage.

## `import_captions`

Set a video's captions from a transcript the user (or their speech MCP server) produced. format `srt` or `vtt` (WebVTT inline <00:00:01.500> word times become word timings), or `words_json`: a list of {text|word, start_ms|start (seconds), end_ms|end}, which is what a speech-to-text server gives, grouped into cues at sentence ends and pauses. This tool does not recognize speech: OpenDesigner has no speech-to-text built in, so a transcript must come from the user's own speech server. Replaces every cue; keeps the style. `offset_ms` shifts all times; cues outside the video are dropped and counted. Captions are drawn in the preview and burned into export_video.

Arguments:

- `format` (string, required) 
- `offset_ms` (integer) 
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `text` (string, required) The file's text (at most 2 MB).
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `captions_from_script`

Set captions from a plain script spread over a time range in proportion to word length. This is an ESTIMATE, not speech recognition: it knows nothing of how the words are spoken, so it fits a voice-over only roughly; prefer import_captions with real timings when there is a transcript. Cues break after sentence ends and at `words_per_cue`. Replaces every cue; keeps the style.

Arguments:

- `end_ms` (integer) Default the end of the video.
- `script` (string, required) The words to show.
- `start_ms` (integer) Default 0.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.
- `words_per_cue` (integer) Default 7.

## `add_caption_cue`

Add one caption cue. Cues are in time order and may not overlap; they end inside the video. Give `words` ([{text, start_ms, end_ms}] spelling the text) to highlight the spoken word.

Arguments:

- `end_ms` (integer, required) 
- `start_ms` (integer, required) 
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `text` (string, required) At most 400 characters.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.
- `words` (array) 

## `remove_caption_cue`

Remove one caption cue (ids are in list_videos under captions.cues).

Arguments:

- `cue` (string, required) A cue id (q1, q2, ...).
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `clear_captions`

Remove every caption cue of a video (the style stays).

Arguments:

- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `set_caption_style`

Set how captions look: start from a `preset` and/or change single fields; fields you leave out keep their value (`highlight: null` and `background: null` remove them). Sizes are a percent of the video's height so one style fits every output size.

Arguments:

- `align` (string) 
- `background` (any) A box behind each line, #rrggbbaa; null removes it.
- `color` (string) #rrggbb or #rrggbbaa.
- `font_family` (string) A font family; unknown ones fall back to Lato.
- `font_weight` (integer) 
- `highlight` (any) Colour of the word being spoken (needs word timings, or the estimate by word length); null removes it.
- `max_lines` (integer) 
- `position` (string) 
- `preset` (string) Start from a named look: clean, bold_pop (big, uppercase, yellow active word), karaoke (mid-screen, green active word), boxed (dark box).
- `size_pct` (number) Text size as a percent of the video's height.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `uppercase` (boolean) 
- `video` (string, required) Id of a video from list_videos.

## `list_asset_sources`

What assets exist and under which licence: the bundled Lucide icon set, the generators make_asset runs, the packs install_asset_pack can download (with sizes and whether installed), optional external MCP servers for 3D assets, and sources that are only a link and why. Call it once before you draw an icon, blob, background or pattern by hand, and at the start of every video (with list_media, list_footage and list_sounds).

Read-only.

No arguments.

## `search_assets`

Search icons, shapes, blobs, backgrounds, patterns, illustrations and sounds by name and tag across the bundled icon set, the generators and every installed pack. Each result has an id and says whether to use import_asset (a stored file) or make_asset (a generator). Use this before drawing an icon or decoration by hand, and when planning a video (sounds are listed too). Needs no network.

Read-only.

Arguments:

- `kind` (string) Narrow to one kind. font and 3d have nothing built in; the answer says where to look.
- `limit` (integer) How many results (default 20).
- `query` (string, required) Words, e.g. "arrow right", "camera", "click". Empty lists everything of the kind.

## `make_asset`

Generate a picture from a seed and your colors and save it as a PNG in media/ (with a sidecar that holds the recipe). generator: blob (organic shape), gradient (linear or radial), aurora (soft mesh-gradient clouds on a base color), grain (film/paper noise), pattern (dots, lines, waves, grid, checker, zigzag, topo) or shape (circle, rounded-rect, triangle, star, polygon, ring, burst). The same seed always gives the same picture. Pass hex colors from list_tokens so it matches the design. Then use the returned file in an image node. No licence conditions. For a video: make the background or texture the video's size, put it in an image node of a frame, and use that frame as a clip or overlay.

Arguments:

- `colors` (array) Hex colors (#rrggbb). Meaning per generator: blob/shape one fill (two = gradient); gradient 2 to 8 stops; aurora the base color then the clouds; grain base then speck color; pattern background then ink.
- `generator` (string, required) 
- `height` (integer, required) Pixels.
- `name` (string) File name without ending; a free one is chosen.
- `scale` (integer) A pattern's cell size in pixels, or the grain's speck size (1 to 16).
- `seed` (integer) Another seed, another picture. Default 0.
- `variant` (string) Which kind: pattern dots|lines|waves|grid|checker|zigzag|topo; shape circle|rounded-rect|triangle|star[-N]|polygon[-N]|ring|burst; gradient linear|radial; blob[-N points].
- `width` (integer, required) Pixels.

## `import_asset`

Put one asset from search_assets into the project: an icon or other SVG is recolored and drawn as a crisp PNG at the size you ask for in media/; a PNG or sound is copied as is. A sidecar next to it names the source and licence. Returns the media/ file and, when the licence asks for it, the credit line you must show with the work (say it in your closing message and keep it in the credits). Keep one icon set per design. For a video: create an image node with this file as `image` in a clip frame or a transparent overlay frame (apply_ops); an imported sound goes to set_video audio.

Arguments:

- `color` (string) Hex color to recolor an SVG icon (replaces currentColor or its fills); use a token color.
- `height` (integer) Draw this tall.
- `id` (string, required) An id from search_assets, e.g. lucide:camera.
- `name` (string) File name without ending; a free one is chosen.
- `width` (integer) Draw this wide (the icon is fitted inside, centered).

## `list_asset_packs`

The downloadable packs (icon sets, Kenney CC0 packs, sounds): state, download size, size on disk when installed, hash and licence, the cap on what packs may take, and any pack an agent asked for that the user has not answered yet.

Read-only.

No arguments.

## `install_asset_pack`

Download a pack into the user's cache (not the project) so search_assets and import_asset can use it. The user must agree first: the first call only asks (the Connect tab shows the request with the size); tell the user, stop, and call again after they approved. The download shows as a job. Install only what the design needs.

Arguments:

- `id` (string, required) A pack id from list_asset_packs.

## `unload_asset_pack`

Delete an installed pack from the user's cache to free its space. Files already imported into projects stay. Do this when the work that needed the pack is done.

Arguments:

- `id` (string, required) A pack id from list_asset_packs.

## `import_svg`

Put a logo or any vector drawing you wrote as SVG into the project: it is checked (live text, embedded pictures, filters, masks, many colors, gradients, no viewBox are reported), drawn crisp as a PNG in media/ for an image node, and the SVG itself is kept as media/<name>.svg for export_icon_set. Read get_playbook logo first: construct the mark from simple shapes, no <text>, black first, then color. Look at the result with render_preview afterwards, also very small.

Arguments:

- `background` (string) Hex color drawn behind the mark (the PNG is then opaque).
- `color` (string) Hex color to replace currentColor and the fills/strokes with (a one-color version).
- `height` (integer) 
- `name` (string) File name without ending.
- `svg` (string, required) The whole SVG document, with a viewBox.
- `width` (integer) 

## `export_icon_set`

Write the PNG sizes a brand needs from a mark saved by import_svg (media/<name>.svg) into exports/<name>-icons/: 1024, 512, 256, 180, 167, 152, 120, 64, 48, 32 and 16 px. With `background` the icons are opaque (the App Store rejects transparency in the 1024 icon); without it they keep their transparency. For a tiny size that needs a simpler drawing, import that drawing as its own svg and export it with `only_sizes`.

Arguments:

- `background` (string) Hex color behind the mark.
- `name` (string) Folder name stem; default the file's name.
- `only_sizes` (array) Only these sizes (from the list above).
- `padding_percent` (integer) Empty margin around the mark, as a percent of the icon (default 0).
- `svg_file` (string, required) media/<name>.svg from import_svg.

## `import_svg_vector`

Bring an SVG illustration in as editable vector nodes: one node per shape (a path, rect or ellipse with its fill, gradient, stroke and opacity), placed exactly as drawn, inside one group, as a single undoable change. Parts named by SVG ids become node names, and an SVG group with an id or an opacity stays a group, so an arm, an eye or a wheel can be found with find_nodes and moved, recolored or animated on its own. Give exactly one source: `svg` (the whole SVG text), `file` (media/<name>.svg) or `asset` (an id from search_assets, an installed pack's illustration or a Lucide icon; its credit line is kept). Pictures, scripts, outside references and drawings over 500 shapes are refused; text, clip paths, masks, filters and pattern fills are not imported and are listed in `notes`. `width` scales the whole drawing; the result gives its size and the id of every part. Use import_svg instead for a logo or icon that should stay one picture.

Arguments:

- `asset` (string) An asset id from search_assets (lucide:<icon> or <pack>:<file>).
- `file` (string) media/<name>.svg of the project.
- `index` (integer) Position among the parent's children (default: on top).
- `name` (string) Name of the group (default: the SVG's own id, else 'illustration').
- `parent` (string, required) Id of the frame or group to put the drawing in.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `svg` (string) The whole SVG document, with a viewBox.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `width` (number) Width of the whole drawing in document units; the height follows (default: the SVG's own size).
- `x` (number) Where the drawing's top-left goes in the parent (default 0).
- `y` (number) Where the drawing's top-left goes in the parent (default 0).

## `review_video`

An animation director's check of a whole video, in numbers, without changing anything. Call it before export_video on any animated or illustrated video and fix what it finds. It reads each scene's nodes and tracks and renders a few small pictures across the video to measure motion. Findings have stable codes: RV001 a scene is one image with text on it, RV002 only text and rectangles move, RV003 a still stretch over 2.5 s, RV004 no looping ambient motion, RV005 no camera move in a video over 15 s, RV006 too few vector shapes, RV007 repeated scene compositions, RV008 captions cover too much, RV009 no sound, RV010 the sound is the same in every scene, RV011 colors are not from tokens, RV012 too few layers moving. It returns per-scene and whole-video numbers, a score from 0 to 100 and the three fixes that would help most. A score under 70 is not ready.

Read-only.

Arguments:

- `samples` (integer) How many moments to render (default 12).
- `video` (string) Id of a video from list_videos; defaults to the first.

## `show_preview`

Show the user the end product, in the Preview panel: the clean frame, video or image they asked for, without editor handles. You decide what that is: call it as soon as there is a deliverable (the hero frame, the finished video) and again whenever you replace it with a better one. The user can look at something else; their choice is theirs and your call stays the default. The canvas is for working; this is for the result.

Arguments:

- `kind` (string, required) frame: a frame id from list_pages or get_tree. video: a video id from list_videos. image: a media path from list_media.
- `note` (string) One line for the user: what they are looking at, or what changed.
- `target` (string, required) The frame id, video id or media path.
- `title` (string) A short name for it, shown above the picture.

## `offer_variants`

Offer the user 2 to 8 options to choose from, in the Variants panel (not on the canvas). Build each option first as its own clearly named frame (or video), then call this with one entry per option. It replaces any open set. Then tell the user to pick in the Variants panel and STOP: do not build the full set until get_variants shows their pick. A frame, video or image can appear in one option only.

Arguments:

- `options` (array, required) 
- `question` (string) Optional: what you want the user to decide on.
- `title` (string, required) What is being chosen, for example "Three directions for the hero".

## `get_variants`

Read the open set of options and which one the user picked (`status`: none, waiting or picked). Call it after offer_variants and before you build on a choice; if it says waiting, tell the user you are waiting for their pick and stop.

Read-only.

No arguments.

## `clear_variants`

Close the open set of options once the choice is made and used.

No arguments.

## `apply_ops` examples

Add a headline under a frame, then recolor it with a token:

```json
{"turn": "headline", "summary": "add the headline",
 "ops": [{"op": "create", "parent": "n_3",
          "node": {"kind": "text", "name": "Headline", "x": 80, "y": 160,
                   "text": "Hold for everything.", "font_size": 96, "fill": "{brand.gold}"}}]}
```

Resize and move an existing node, then reorder it under another parent:

```json
{"ops": [{"op": "set", "id": "n_7", "prop": {"name": "width", "value": 1120}},
         {"op": "move", "id": "n_7", "x": 100, "y": 220},
         {"op": "reparent", "id": "n_7", "parent": "n_4", "index": 0}]}
```

Properties for `set`: `name`, `x`, `y`, `width`, `height`, `visible`, `opacity` (0 to 1), `fill`
(a color or null), `stroke` (`{"paint": "#000000", "width": 2}` or null), `corner_radius`, `text`,
`font_size`, `font_family` (a name or null), `font_weight` (100 to 900 or null), `text_align`
(`left`, `center`, `right` or null), `line_height` (a multiple of the font size, 0.5 to 4, or
null), `image`, and on paths `path`, `view_box`, `fill_rule`. A stroke's `stroke_width`, `trim_start`
and `trim_end` are properties too. Not every property applies to every kind of node; a rejection says
which.

Vector shapes: a `path` node draws SVG path data in its box (the path's own bounds are stretched over
the box, so resizing scales it), and any shape can be filled or stroked with a gradient. A stroke
has `cap`, `join`, a `dash` array and `trim_start` / `trim_end` (0 to 1 along the outline), so a line
draws on when `trim_end` is animated from 0 to 1:

```json
{"ops": [{"op": "create", "parent": "n_3",
          "node": {"kind": "path", "name": "Wave", "x": 80, "y": 600,
                   "path": "M0 40 C60 0 120 80 180 40 S300 0 360 40",
                   "stroke": {"paint": {"type": "linear", "angle": 0,
                                        "stops": [{"offset": 0, "color": "#ff7a59"},
                                                  {"offset": 1, "color": "#ffd166"}]},
                           "width": 10, "cap": "round", "trim_end": 0}}]}
```

Then, with the created id:

```json
{"ops": [{"op": "animate", "video": "v_20", "node": "n_9", "prop": "trim_end",
          "keys": [{"at_ms": 0, "value": 0}, {"at_ms": 1200, "value": 1, "ease": "ease_out"}]}]}
```

`import_svg_vector` brings a whole SVG illustration in as one group of shape nodes named after the
SVG's ids (one undo removes it); `fill_color` keys (`"value": "#rrggbb"`) animate a shape's color.

Transform and effects work on any node and on everything inside it: `rotation` (degrees, clockwise),
`scale_x` and `scale_y` (negative flips; `scale` sets both), `pivot_x` and `pivot_y` (fractions of the
node's own box the turn and scale happen about, 0.5 is the middle; a group without a size uses the box
around its children), `blur` (px), `drop_shadow` (`{"dx": 0, "dy": 12, "blur": 24, "color": "#00000066"}`
or null; `shadow_dx`, `shadow_dy` and `shadow_blur` change one number), `clip` (a group or frame cuts
its children at its box; give a group a `width` and `height`) and `depth` (how far a video's camera
moves it: 1 with the scene, 0 pinned to the screen, 0.5 for parallax). `create` takes the same names
(`shadow` for the shadow). Bounds and hit tests follow the transform.

Design tokens are named colors. Define or recolor one with `{"op": "set_token", "name":
"brand.gold", "value": "#f2ca83"}`; every node that fills or strokes with `{brand.gold}` changes
with it, and those nodes are outlined for the user. `{"op": "set_token", "name": "brand.old",
"value": null}` removes a token, and is refused while a node still uses it. Prefer changing a token
to recoloring many nodes one by one.

Translations: a text node's own `text` is the default language. `list_locales` shows the languages
and which text nodes each still lacks. `{"op": "set_localized_text", "locale": "ja", "id": "n_7",
"text": "..."}` sets one translation (`text: null` removes it); a language tag looks like `ja` or
`pt-BR`. Pass `locale` to `export`, `export_set` or `export_panorama` to render a language; set and
panorama files are named `<prefix>-<locale>-01.png`. Translated text can be longer: check
`render_preview` of each language for overflow.

Videos: a video is a timeline of clips, and each clip shows one design frame for a while, so an
app preview is made from the same frames as the store screenshots. `list_videos` shows them with
the time each clip starts. Build one with operations:

```json
{"turn": "preview", "summary": "a 15 second app preview",
 "ops": [{"op": "create_video", "name": "Preview", "fps": 30, "width": 886, "height": 1920},
         {"op": "add_clip", "video": "v_20", "frame": "n_3", "duration_ms": 4000,
          "motion": {"scale_from": 1.0, "scale_to": 1.12}},
         {"op": "add_clip", "video": "v_20", "frame": "n_9", "duration_ms": 4000,
          "transition": {"kind": "slide_left", "duration_ms": 500}}]}
```

Footage: `list_footage` shows the videos the user imported (length, size, whether they have
sound). A clip can play one instead of showing a frame: `{"op": "add_clip", "video": "v_20",
"footage": {"file": "media/demo.mp4", "in_ms": 12000}, "duration_ms": 5000}` plays 5 seconds from
12 s into the file, with its sound. Cut by adding several clips of the same file with different
`in_ms` (a cut removes the part between them); use `transition` to blend them. `export_video`
checks every clip lies inside its file. To add titles, lower thirds or call-outs over footage, make
a design frame with no fill (it is transparent) holding only the text and shapes, then
`{"op": "add_overlay", "video": "v_20", "frame": "n_30", "start_ms": 1000, "duration_ms": 3000,
"fade_ms": 400}`; overlays sit above every clip and fade in and out. Overlays are static pictures:
for motion, use several overlays or `set_overlay` over time. Match the video's size and shape.

The ids of a created video and clips come back in `created` (`v_...`, `c_...`). Width and height
are even numbers; a frame of another shape is scaled to cover the video and cropped in the middle,
so use frames of the video's shape. A transition (`cut`, `fade`, `slide_left`, `slide_right`, `slide_up`, `slide_down`, `dip`, `zoom_in`,
`wipe_left`, `wipe_right`, `wipe_up`, `wipe_down`, `iris`, `flash`, `blur_dissolve`, `glitch_slice`)
overlaps the end of the clip before it, so it makes the video shorter by its length; the first
clip's transition is not shown. `motion` zooms slowly from `scale_from` to `scale_to` (1 to 3) over
the clip. Change a clip with `set_clip` (`duration_ms`, `frame`, `source`, `transition`, `motion`, and for footage
`speed`, `speed_ramp`; for any clip `grade`), reorder
with `move_clip`, remove with `remove_clip`. A frame that a clip shows cannot be deleted until the
clip is removed. Look at a moment with `render_video_frame` (try the middle of a transition) and
write the file with `export_video`, which needs `ffmpeg` on the user's computer and tells you if it
is missing. For the App Store pass `preset` to `export_video` (see `list_video_presets`): sizes, 15 to 30
seconds and 30 fps are checked before anything is encoded, and the preset says what it does not
check (the audio track, the H.264 profile).
Pass `loudness` (-14 social, -16 podcast, -23 broadcast LUFS) to bring the sound to a platform level. For a hand-drawn rhythm set the video's `hold_frames` to 2 (on twos) or 3 and use `steps_4`-style eases.

Sound: a video can carry one sound track under its picture, a file in the project's `media/`
folder. `{"op": "set_video", "video": "v_20", "prop": {"name": "audio", "value": {"file":
"media/song.mp3", "start_ms": 0, "gain": 1.0, "fade_in_ms": 500, "fade_out_ms": 500}}}` sets it
(`gain` is 0 to 2, the fades are milliseconds; `"value": null` takes the track away). The track
never makes the video longer: it is cut at the end. To find music or sound effects, use
`make_sound` (synthesized in the project: click, pop, tick, whoosh, swoosh_up, swoosh_down, riser,
hit, chime, drone, beat, stinger; no licence, but no music), `search_sounds` (Freesound; it needs
the user's token, and the answer says so if there is none), `import_sound` to bring one into
`media/`, or `import_file` with a sound file of the user's (`find_files` kind audio); `list_sounds`
shows them all. Then set the track. For more than one (music, a voice-over, effects), use
`add_sound_track` (role `music`, `voice` or `sfx`, up to 8; start, trim, gain, fades, pan; music can
`duck` under the voice), `set_sound_track` and `remove_sound_track`. Captions are data: bring a
transcript from the user's own speech server with `import_captions` (`srt`, `vtt`, `words_json`;
OpenDesigner does no speech recognition), or estimate from a script with `captions_from_script`,
then `set_caption_style`. `detect_pauses`, `remove_pauses` and `get_waveform` need ffmpeg. A Freesound sound keeps its
author's license: show the attribution the tool gives you (title, author, license, link) wherever
the sound is used, and prefer CC0.

Template variables: slots you can fill many times (one card per customer, one video per
language). Declare one with its kind and what it drives, then set its value:

```json
{"ops": [{"op": "declare_variable", "variable": {
            "id": "title", "kind": "string", "label": "Headline", "default": "Welcome",
            "portrays": "subject_name", "binds": [{"node": "n_7", "prop": "text"}]}}]}
{"ops": [{"op": "set_variable_value", "id": "title", "value": "Acme"}]}
```

Setting a value rewrites every bound node property through the same validated path as `set`, so it
is attributed, checkpointed and undoable like any other edit. `list_variables` shows the slots and
what they portray; a slot that portrays the subject's identity must come from the user, never from
you. `batch_export` renders the design once per row of values without changing the project.
