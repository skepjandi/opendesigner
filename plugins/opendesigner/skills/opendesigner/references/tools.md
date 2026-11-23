# OpenDesigner MCP tools

Generated from the server's tool definitions. Arguments not marked required are optional. A rejected call returns `isError: true` with the reason and changes nothing.

## `get_guidance`

Read this first. Returns the pages, the nodes the user has selected (with their data), the open notes the user pinned, and the edits the user made after your last turn. 'Make this bigger' means the selection.

Read-only.

No arguments.

## `get_playbook`

Craft guidance for making work that is not mediocre. Topics: design (images), style (named looks with rules), video (start with the inventory of media, footage, sounds and assets), motion (timings, easing, recipes), 3d (devices in 3D, captures from a simulator, moves), formats (sizes and safe zones), logo (identity design), blocks (the catalog of ready-made titles, overlays, backgrounds, charts and motion, and when to use one), scenery (layered flat-vector scenes: scenery blocks, parallax, idle loops, scene moves and the camera), characters (flat-vector characters, bubbles and animated props), sketch (hand-made looks: pen sketches and hatching, line boil, watercolor washes, film grain, grass and wind strokes, hand-drawn emphasis), explainer (anything animated: animation, scenes, explainers, motion graphics, cartoons, 2D illustration, Kurzgesagt or In a Nutshell style; read it FIRST, a jpg backdrop with moving text is never an animated scene), review (look at your own video harshly: strips, crops, grids, diffs, the checklist), packs (style packs: commit to one look before you draw: choosing it from the brief, applying its palette, the 80/20 rule, the motion personality). Call it with no topic for the list; read the ones that apply before you build, not after.

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

Change the design with a list of operations, applied together or not at all. Operations: {op:'create', parent, index?, node:{kind, name?, x, y, width, height, fill?, stroke?, corner_radius?, opacity?, visible?, text?, font_size?, font_family?, font_weight?, text_align?, line_height?, image?, image_fit?, device3d?, path?, view_box?, fill_rule?, rotation?, scale_x?, scale_y?, pivot_x?, pivot_y?, blur?, shadow?, clip?, depth?, children?}}, {op:'set', id, prop:{name, value}} with name one of name, x, y, width, height, visible, opacity, fill, stroke, corner_radius, text, font_size, font_family, font_weight, text_align, line_height, image (a media/ path from list_media), image_fit (cover, contain or fill), path, view_box, fill_rule, stroke_width, trim_start, trim_end, rotation, scale_x, scale_y, scale (both), scale_xy (value {x, y}), pivot_x, pivot_y, blur, drop_shadow, shadow_dx, shadow_dy, shadow_blur, clip, depth (see Transform and effects below), {op:'move', id, x, y}, {op:'reparent', id, parent, index?}, {op:'delete', id}, {op:'set_token', name, value} (a named color '#rrggbb'; value null removes an unused token), {op:'set_localized_text', locale, id, text} (a translation of a text node; text null removes it). Vector shapes: kind 'path' draws SVG path data in its box: path:'M0 0 C10 20 30 20 40 0 Z' (commands M L H V C S Q T A Z, absolute or relative, at most 4000 segments and 64 KB). The path's own bounds are stretched over the node's width and height, so resizing scales it; give no width and height and the box takes the path's size. view_box:{x, y, width, height} maps that rectangle of path coordinates onto the box instead (exact placement for imported art); fill_rule is nonzero (default) or evenodd. Any shape (frame, rect, ellipse, path) takes a gradient as fill or as stroke paint: {type:'linear', angle?: degrees, 0 left to right, 90 top to bottom (default), stops:[{offset: 0 to 1, color}]} or {type:'radial', cx?, cy?: fractions of the box (0.5), r?: fraction of the longer side (0.5), stops}, 2 to 8 stops whose colors are '#rrggbb[aa]' or '{token}'. A stroke is {paint, width, cap?: butt|round|square, join?: miter|round|bevel, dash?: [dash, gap, ...], trim_start?, trim_end?: 0 to 1 along the outline, only the stretch between them is drawn}; animate trim_end from 0 to 1 to draw a line on. A stroke width of 0 draws nothing. import_svg_vector turns a whole SVG illustration into path nodes. 3D devices: kind 'device3d' is a phone, tablet, laptop or monitor drawn in 3D inside its box (the box is the viewport; 1 zoom fits the device with a margin at any turn); its fill is the body finish, its image (with image_fit) is the capture on the screen; create it with device3d:{model: phone|phone_punch_hole|tablet|laptop|monitor, rotate_x?, rotate_y?, rotate_z?, zoom?, fov?, light_angle?, shadow?, glare?, screen_scroll?, screen_footage?:{file:'media/x.mov', in_ms}} and change it with set props model, rotate_x (tilt, + leans the top away), rotate_y (turn, + turns the right edge away), rotate_z (roll, + clockwise), zoom (0.05 to 10), fov (1 to 100 degrees, 30 natural, low is flat), light_angle (degrees, 0 from above, 90 from the right), shadow (0 to 1), glare (0 to 1, screen reflection), screen_scroll (0 top to 1 bottom of a long capture; null centres), screen_footage (value {file, in_ms}: a recording on the screen that plays while the frame is shown; null shows the image). Transform and effects (any node, drawn together with everything inside it; defaults mean no effect): rotation (degrees, clockwise, up to 36000), scale_x and scale_y (default 1, negative flips; scale sets both), pivot_x and pivot_y (fractions of the node's own box that rotation and scale turn about, default 0.5 the center; a group without a size uses the box around its children), blur (Gaussian radius in px, 0 to 500), shadow (create) or drop_shadow (set) = {dx, dy, blur, color} with the color's alpha as its opacity (null removes it; shadow_dx, shadow_dy and shadow_blur change one number of an existing shadow and can be keyframed), clip (true on a group or frame: children are cut at its box; a group needs a width and height for that, a frame always clips) and depth (0 to 10, default 1: how strongly the video's camera moves the node, 0 pins it to the screen, 0.5 moves half as far for parallax; depths multiply down the tree). The exported frame ignores its own transform. A node's bounds (and what a click hits) are the box around it after its transform. Videos (see list_videos): {op:'create_video', name, fps, width, height} (even sizes), {op:'delete_video', id}, {op:'set_video', id, prop:{name, value}} with name one of name, fps, size (value {width, height}), audio (value {file:'media/x.mp3', start_ms?, gain? (0 to 2), fade_in_ms?, fade_out_ms?} for the sound under the picture; null takes it away), motion_blur (0 off, or 2 to 16 pictures blended while anything moves), hold_frames (1 to 6, default 1: every drawing is held for this many frames, 2 is animation on twos, 3 on threes; audio is unaffected), {op:'add_clip', video, frame | footage:{file:'media/x.mp4', in_ms}, duration_ms, index?, transition?, motion?} (a clip shows a design frame or plays footage from list_footage), {op:'remove_clip', video, clip}, {op:'move_clip', video, clip, index}, {op:'set_clip', video, clip, prop:{name, value}} with name one of duration_ms, frame, source (value {frame} or {footage:{file,in_ms}}), transition (value {kind: cut|fade|slide_left|slide_right|slide_up|slide_down|dip|zoom_in|wipe_left|wipe_right|wipe_up|wipe_down|iris|flash|blur_dissolve|glitch_slice|flip|clock_wipe, duration_ms, ease?}), motion (value {scale_from, scale_to, ease?}, 1 to 3), speed (footage only, 0.25 to 4; duration_ms is the length on the timeline, the clip plays duration_ms*speed of the file), speed_ramp (footage only, value [{at_ms, rate 0.25 to 4, ease?}] in clip time, needs speed 1; [] removes it), grade (value {exposure?(-3..3 stops), contrast?, saturation?, temperature?, tint? (all -1..1), vignette? (0..1), blur? (0..50), look?: warm|cool|mono|faded|punchy}, or null; a look fills the numbers and the numbers you give override it; also set on an image node with {op:'set', id, prop:{name:'grade', value}}). Times (durations, starts, in points, transitions, keys) are rounded to the video's frame grid; the receipt shows the value kept. {op:'add_overlay', video, frame, start_ms, duration_ms, fade_ms?, enter?, exit?, ease?} (a design frame drawn over the whole video for a while: titles, lower thirds, call-outs; a frame with no fill is transparent; enter and exit are none, fade, slide_up, slide_down, slide_left, slide_right, pop or zoom, taking fade_ms each), {op:'remove_overlay', video, overlay}, {op:'set_overlay', video, overlay, prop:{name, value}} with name one of frame, start_ms, duration_ms, fade_ms, enter, exit, ease, {op:'animate', video, node, prop, keys:[{at_ms, value, ease?}], repeat?} (keyframes one number of a node over the video's time, at_ms from the start of the video, strictly increasing; the value holds before the first key and after the last unless repeat is loop (start again from the first key, for as long as the video plays; end on the first key's value for a seamless cycle) or pingpong (play back and forth), once being the default and what an animate without repeat sets, so send repeat again when you change the keys of a looping track; prop one of x, y, width, height, opacity, corner_radius, rotation, scale_x, scale_y, scale (both), blur, shadow_dx, shadow_dy, shadow_blur (the node needs a shadow), rotate_x, rotate_y, rotate_z, zoom, fov, light_angle, shadow, glare, screen_scroll, trim_start, trim_end, stroke_width (the node needs a stroke), fill_color (key value '#rrggbb[aa]', mixed in sRGB and listed back as the number 0xRRGGBBAA; the node needs a fill), or on an image node grade_exposure, grade_contrast, grade_saturation, grade_temperature, grade_tint, grade_vignette, grade_blur; the camera props camera_x, camera_y (pan in px: the scene moves the other way), camera_zoom (0.1 to 20, about the frame's center) and camera_rotation (degrees the scene turns clockwise about the center) take the VIDEO's id as node and move every node of a clip's frame by its depth (nodes at depth 0 and overlays stay); a single key holds a camera still; keys:[] removes it; animate_preset writes some of these for you). Eases: linear, ease_in, ease_out, ease_in_out (also ease_in_out_cubic), back (overshoots; also ease_out_back), spring (overshoots and settles), elastic (rings past the end) or bounce (bounces off the end); power2_in|out|in_out and power4_in|out|in_out (stronger cubic-like curves), expo_in|out|in_out, circ_in|out|in_out, sine_in|out|in_out (the gentlest); steps_2, steps_3, steps_4, steps_6, steps_8 and steps_12 (no in-between: the value holds each of that many poses, jumping between them, for the stepped feel of drawn animation); anticipate (pulls back a little, then goes). Tuned eases are text with numbers: spring(visual_ms,bounce) is a real spring that looks done after visual_ms and rings by the bounce (0 none, 0.3 playful, 0.5 bouncy, negative slow and heavy; it is read over the stretch between its two keys, so give that stretch about 2 to 3 times visual_ms for the swings to show, and the presets spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy are spring(250,0), (400,0), (450,0.3), (500,0.5), (800,0.1)); cubic(x1,y1,x2,y2) is the CSS cubic-bezier; back_in|back_out|back_in_out(overshoot) (1.7 is the classic, 0.5 subtle); elastic_out|elastic_in(amplitude,period); steps(n) for any n from 2 to 64. Colors are '#rrggbb', '#rrggbbaa' or '{token.name}'. Template variables (see list_variables): {op:'declare_variable', variable:{id, kind, label, description?, portrays?, default, enum_values?, binds?:[{node, prop}]}} with kind one of string, number, color, bool, enum, image, font (enum_values only for enum) and portrays one of subject_name, subject_domain, subject_tagline, subject_logo, host_identity, {op:'set_variable', id, prop:{name, value}} with name one of label, description, portrays, default, enum_values, binds, {op:'remove_variable', id}, {op:'set_variable_value', id, value} (writes every bound node property through the same validated path as set, so it is attributed and undoable). Ids are minted for you; the result lists the created ids. A checkpoint is taken before your first change in a turn so the user can restore it. Set dry_run true to check the operations without applying them: you get ok, what each operation would do, the node ids that would change, and for a rejection a stable code (E_NODE_NOT_FOUND, E_OUT_OF_BOUNDS, E_LIMIT, ...) with a hint; nothing is changed, checkpointed or saved. An applied result has stage 'applied', the checkpoint taken before the turn, the transaction, the changed ids and whether the project was saved.

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

## `list_sounds`

All the sounds in this project's media/ folder (.mp3 .wav .m4a .aac .ogg), whether made with make_sound, downloaded with import_sound or imported with import_file: the file, size, length when ffmpeg is installed, where it came from (synthesized, freesound or imported) and, for a Freesound sound, the attribution line the user must show. Any of them can be the video's sound track (set_video audio).

Read-only.

No arguments.

## `make_sound`

Make a sound effect or a simple loop yourself, with no download and no licence to honour (it is synthesized and written to media/<name>.wav, normalized, with short fades so it never clicks). Use it for the basics: click, pop, tick, whoosh, swoosh_up, swoosh_down, riser (builds into a cut), hit and impact (a thump: put it on a cut or a reveal; impact is bigger, with a crack and a tail), chime (a notification or a finish), stinger (a 2 to 3 note sting), drone (a soft pad that loops), pad (a slow chord bed that loops), key (a typing click, a little different each press: repeat it with different seeds), blip (a UI blip), scratch (a record scratch) and beat (4-on-the-floor kick and hat at `bpm`, `bars` bars). It cannot make real music or speech: for a music bed use search_sounds (Openverse, Incompetech, ccMixter or Freesound) or ask the user; for a voice-over use a speech MCP server if the user added one (get_guidance lists them). The same kind, duration and seed give the same sound. Then set it as the track: set_video prop {name: "audio", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}.

Arguments:

- `bars` (integer) Beat only: bars of four beats.
- `bpm` (number) Beat only: tempo, default 120.
- `duration_ms` (integer) How long. Optional: each kind has a fitting default. A beat takes its length from bpm and bars.
- `kind` (string, required) Which sound.
- `name` (string) A plain file name without the .wav ending; default is the kind. An existing file is never overwritten.
- `pitch_hz` (number) The base pitch of chime, drone, pad, tick, key, blip, pop, riser, hit, impact and stinger.
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

Draw every frame of a video (or a part of it) and write it to exports/<file>: an H.264 MP4 (default), or by the file's ending an animated GIF (.gif), a VP9 WebM (.webm) or an Apple ProRes MOV (.mov). `transparent: true` keeps what no clip covers clear (a design frame with no fill, a fade) in a .gif (one bit), .webm or .mov (full alpha, for other editors); an .mp4 cannot. `from_ms` and `to_ms` write only that part (and its sound, in an .mp4), `every_nth_frame` writes every Nth frame (a 30 fps video as a 15 fps GIF is 2; the sound is left out). A GIF is scaled to `width` (default 480) and has no sound; .webm and .mov carry no sound here. Needs ffmpeg on the user's computer (the error says what to install). This can take a while. For an animated or illustrated video, run review_video first and fix its findings.

Arguments:

- `every_nth_frame` (integer) Write every Nth frame (default 1).
- `file` (string, required) File name ending in .mp4, .gif, .webm or .mov, for example preview.mp4.
- `from_ms` (integer) Start of the part written (default 0).
- `gif_loops` (integer) A GIF plays again this many times after the first; 0 plays once. Default: for ever.
- `locale` (string) A language from list_locales: text nodes show that language.
- `loudness` (integer) Bring the final mix to this integrated loudness in LUFS: -14 social, -16 podcast, -23 broadcast. Default: leave the level as mixed. Measured first, then applied as a gain (ffmpeg loudnorm); a video with no sound is unaffected.
- `preset` (string) A preset id from list_video_presets: the video is checked against the store's rules and nothing is written if it breaks one.
- `to_ms` (integer) End of the part written (default: the end of the video).
- `transparent` (boolean) Keep uncovered ground transparent (.gif, .webm, .mov).
- `video` (string, required) Id of a video from list_videos.
- `width` (integer) Scale to this width (.gif, .webm, .mov); a GIF defaults to 480.

## `validate_export`

Check exported PNGs in exports/ as one set against a store preset.

Read-only.

Arguments:

- `files` (array, required) 
- `preset` (string, required) A preset id.

## `animate_preset`

Animate a node with a named move, measured from its designed pose, as keyframes you can then inspect in list_videos and change with apply_ops animate. 3D device moves: spin (a full turn; strength 2 is two), turntable (slow side to side), swing (away one way, then the other, back), flip_in (turns from its back to face the viewer, springing), hero_reveal (starts flat and square, settles into the designed tilt and turn), push_in and pull_back (camera closer or settling back), orbit (the view swings around the device), scroll_screen (scrolls a long capture top to bottom). Any node: rise_in, drop_in, float (gentle drift; a 3D device also sways), fade_in, fade_out, slide_in_left, slide_in_right. Keys already inside start_ms..start_ms+duration_ms for the same properties are replaced and the rest kept, so moves chain. Times are from the start of the video; the node should be in a frame the video shows at that time. Scene moves work on any node, not only 3D devices, and write ordinary keyframes. Ambient (loop by default, end where they start, `seed` shifts the phase so nodes do not move in lockstep; duration_ms is one cycle): float (lazy drift), bob (up and down with a little tilt), breathe (squash and stretch), pulse (a beat of scale), spin (intensity = turns), wobble, swing (a pendulum tilt), orbit (the node's middle circles `center_x`/`center_y`, or a point to its left; `radius`). Entrances and impacts (play once): pop_in (scale 0 to 1 with a small overshoot), bounce_in (drops in and bounces), elastic_in (rings in), slide_in_left|right|up|down (with overshoot), fade_up, zoom_in, shake (decays), draw_on (a stroked node's trim_end 0 to 1). Camera moves write the video's camera (the node is the video; layers with a `depth` follow it by that much): camera_push_in, camera_pull_back, camera_pan (`direction` left|right|up|down, `amount` in document units), camera_drift (slow loop), camera_shake (decays), camera_whip (fast pan, `direction`), camera_burst (a kick that is fastest on its first frame, decays over about nine frames with power4_out and then holds; `direction`, `amount`, `start_ms`). animate_along_path: the `path` node's curve is sampled into x/y keys (and, with `orient`, rotation) so `node` follows it over the time range with `ease`. stagger: apply the move named in `of` to every id in `nodes`, `step_ms` apart (default 120), starting `from` first|last| center|edges|random (seeded); with `columns` the nodes are a grid read row by row and each is delayed by its distance (`metric` manhattan, euclid or chebyshev; `step_ms` per unit) from the start cell: `start_index` (an index into `nodes`) or the cell `from` names. A `stagger` object adds more: each_ms, range [near, far] (a value stagger: each node's intensity is multiplied by its value), from as an index or {point}, grid [columns, rows], axis x|y, an eased distribution, reversed, start_ms, seeded jitter_ms, and spatial (distances between the nodes' real centres). `intensity` scales how far a move goes (1 as designed), `repeat` is once|loop|pingpong, `ease` overrides the move's own; a tuned spring `ease` on an entrance stretches duration_ms to the time the spring needs to come to rest (the result says so).

Arguments:

- `amount` (number) camera_pan, camera_whip and camera_burst: how far the camera goes, in document units.
- `arc` (object) Bend the straight run into an arc: a quadratic Bezier whose control point is the start plus the run times `peak` plus the perpendicular times `strength` times the distance. Sampled into x/y keys every ~33 ms, so the node curves instead of sliding straight.
- `center_x` (number) orbit: the middle of the circle, in the node's parent coordinates.
- `center_y` (number) orbit: the middle of the circle, in the node's parent coordinates.
- `columns` (integer) stagger: lay the nodes out as a grid this many columns wide, row by row, and delay by distance from the start cell.
- `direction` (string) camera_pan, camera_whip and camera_burst.
- `duration_ms` (integer) Needed by the older moves; a scene move has its own default (one cycle, for a loop).
- `ease` (string) Overrides the preset's own ease: one of linear, ease_in, ease_out, ease_in_out, back, spring, ease_in_out_cubic, ease_out_back, elastic, bounce, steps_2, steps_3, steps_4, steps_6, steps_8, steps_12, power2_in, power2_out, power2_in_out, power4_in, power4_out, power4_in_out, expo_in, expo_out, expo_in_out, circ_in, circ_out, circ_in_out, sine_in, sine_out, sine_in_out, anticipate, bounce_in, bounce_in_out, spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy or a tuned one: cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to 0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; anticipate (pulls back first, then goes).
- `from` (string) stagger: which node goes first (edges: both ends together, then inward; in a grid, the outer ring first).
- `intensity` (number) How far the move goes; 1 is as designed (the same as strength).
- `layer` (string) replace (default) writes the node's own tracks; additive writes an offset track added on top of them, measured from where the move starts: a bob, float or shake laid over a travel or a rise.
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
- `stagger` (object) How a group of nodes is spread over time (and, with range, over a value). Every field is optional.
- `start_index` (integer) stagger: index in nodes of the node the wave starts from (overrides from).
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

## `layer_image`

Cut a picture into depth layers for a camera move (a 2.5D parallax): the far layer first, the near layer last, each an image node of one group with a `depth` (0.15 far to 1 near), so camera_pan, camera_push_in or camera_drift move the near ones more. Give the picture (`file`, media/<name>) and exactly one grey plane from the user's own programs: `mask` (media/<name>, a subject mask or a cut-out with transparency, from remove_background with mask_only: two layers, the background and the subject) or `depth` (media/<name>, a depth map where brighter is nearer, or `invert_depth` when darker is nearer: `bands` layers of equal area). Each layer also carries `grow` pixels of the colours behind the nearer ones, grown out of its own edge, so a move does not uncover a hole, and the lowest layer fills the whole picture. The layers are overscanned by `overscan` (default 6 percent) so the camera has room. Layers are written to media/<name>-layer-<n>.png and never overwrite a file. Pictures over 24 million pixels are refused. Model licences are the user's to check: when the project is commercial, ask for rembg's isnet-general-use or u2net, and a depth model such as Depth Anything V2 Small (Apache-2.0), not the larger non-commercial ones.

Arguments:

- `bands` (integer) How many layers a depth map is cut into (default 4).
- `depth` (string) media/<name> of a depth map (brighter is nearer): `bands` layers.
- `feather` (integer) Pixels an outer edge is softened over (default 1).
- `file` (string, required) media/<name> of the picture (png, jpg or webp).
- `grow` (integer) Pixels of hidden colour grown behind each nearer layer (default 40).
- `invert_depth` (boolean) The depth map is darker where nearer.
- `mask` (string) media/<name> of a subject mask or a cut-out: two layers.
- `mask_threshold` (integer) How bright a mask pixel must be to count as the subject (default 128).
- `name` (string) Name of the group (default: the picture's name).
- `overscan` (number) Extra size around the picture, as a share of it (default 0.06).
- `parent` (string, required) Id of the frame or group to put the layers in.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `width` (number) Width of the picture in document units (default: the parent's width); the height follows.
- `x` (number) Left of the group in the parent (default 0).
- `y` (number) Top of the group in the parent (default 0).

## `animate_to`

Send one property of a node to a value over a stretch of time, starting from the value it has at at_ms: what its track gives there, or the node's own value when it has none. There is no jump when it is interrupted. The keys the node's track has from at_ms on are replaced (keep_later keeps those after the new move ends), a segment of the old track that is cut in the middle keeps its shape (a non-linear one is baked into 8 linear pieces), and the result is one ordinary animate, so it is attributed, checkpointed and undoable in one step. Returns the value it started from and how many old keys it replaced. A track that loops (repeat) is refused: use apply_ops animate. The additive layer is untouched; add one with apply_ops animate additive:true or animate_preset layer:additive. A colour prop (fill_color) takes a '#rrggbb[aa]' string. A camera prop needs no node.

Arguments:

- `at_ms` (integer, required) When the move starts, from the start of the video.
- `duration_ms` (integer, required) How long the move takes.
- `ease` (string) One of linear, ease_in, ease_out, ease_in_out, back, spring, ease_in_out_cubic, ease_out_back, elastic, bounce, steps_2, steps_3, steps_4, steps_6, steps_8, steps_12, power2_in, power2_out, power2_in_out, power4_in, power4_out, power4_in_out, expo_in, expo_out, expo_in_out, circ_in, circ_out, circ_in_out, sine_in, sine_out, sine_in_out, anticipate, bounce_in, bounce_in_out, spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy or a tuned one: cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to 0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; anticipate (pulls back first, then goes). Default ease_in_out.
- `keep_later` (boolean) Keep the old keys after the new move ends (default: they are replaced).
- `node` (string) Id of the node (omit for the camera_* props).
- `prop` (string, required) 
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `to` (any, required) The value to reach: a number, or a '#rrggbb[aa]' colour for fill_color.
- `turn` (string) Groups related calls into one turn.
- `video` (string, required) Id of a video from list_videos.

## `snapshot_layout`

Record where every node inside a frame (or group) is right now: its box in the frame, its size, opacity and solid fill, the parent it sits in. Stored as a small JSON file in the project's layout-snapshots folder (at most 500 nodes, 24 snapshots; the oldest is dropped). Then edit the document (move, resize, add, restyle, reparent) and call transition_layout to animate from the recorded state to the new one. Needs a project folder. Returns the snapshot name.

Arguments:

- `frame` (string, required) Id of the frame or group to record.
- `name` (string) A plain name: a-z, 0-9, '.', '_' and '-', up to 48 characters. Default snap-1, snap-2, ...

## `transition_layout`

Animate from a snapshot_layout record to the document as it is now, matching nodes by id. A node that moved or resized gets x, y, width and height keys from its old values to its live ones (a change under 0.01 for a position or 1 for a size writes nothing); a changed opacity or solid fill_color is carried the same way; a node that is new (or newly visible) fades in, opacity 0 to its own; a node that moved to another parent is started where it was on the page. A node that was deleted cannot be animated: it is reported as leaving (keep a node you want to fade out and set visible false after the move instead of deleting it). The keys from at_ms on that the node's tracks already had on these properties are replaced. One attributed, undoable turn; returns a table of what each node does. The node's value before at_ms is its old one, after the move its live one.

Arguments:

- `at_ms` (integer, required) When the transition starts, from the start of the video.
- `duration_ms` (integer) How long each node takes (default 350).
- `ease` (string) One of linear, ease_in, ease_out, ease_in_out, back, spring, ease_in_out_cubic, ease_out_back, elastic, bounce, steps_2, steps_3, steps_4, steps_6, steps_8, steps_12, power2_in, power2_out, power2_in_out, power4_in, power4_out, power4_in_out, expo_in, expo_out, expo_in_out, circ_in, circ_out, circ_in_out, sine_in, sine_out, sine_in_out, anticipate, bounce_in, bounce_in_out, spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy or a tuned one: cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to 0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; anticipate (pulls back first, then goes). Default ease_in_out.
- `snapshot` (string, required) Name returned by snapshot_layout.
- `stagger` (object) Spreads the starts over the nodes that move, and over those that enter, in document order: each_ms (default 40), from, reversed, ease, grid, columns, jitter_ms, seed, start_ms.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn.
- `video` (string, required) Id of a video from list_videos.

## `sequence`

Several moves placed against each other in time, written as one turn and one undo step. Give each step a `preset`, the `node` or `nodes` it moves, and where it starts: a number of milliseconds, or a phrase against what is already there (`<` the end of the step before, `<<` its start, `<+=50` to overlap it by 50 ms, `intro+=200` after a step named intro). A step may name itself with `label` and spread its own nodes with a `stagger` object. `fit_to_ms` scales the whole sequence to a length. The result echoes the table of resolved starts and ends, so the agent can see the timing it got.

Arguments:

- `fit_to_ms` (integer) Scale the whole sequence to this many milliseconds.
- `items` (array, required) 
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `spring_to`

Send one property of a node to a value on a spring: it accelerates, overshoots and settles. The spring is `visual_ms` long (how long it looks like it takes) and `bounce` (0 settles straight, 0.3 playful, 0.5 bouncy). The keys run from where the property is now to the value, over the time the spring needs to come to rest. The result says when it looks done and when it has really stopped, so the next beat can be placed after it.

Arguments:

- `at_ms` (integer) When the spring starts (default: after the last key of this property).
- `bounce` (number) How much it overshoots: 0 settles straight, 0.3 playful, 0.5 bouncy (default 0).
- `node` (string) Id of the node to move (the video's camera props need none).
- `prop` (string, required) The property to move, such as x, y, opacity, scale_x, rotation, trim_end, camera_zoom.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `to` (number, required) Where the property ends up.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.
- `visual_ms` (integer) How long the spring looks like it takes (default 400).

## `animate_arc`

Move a node from one point to another along an arc instead of a straight line: a quadratic Bezier whose control point is the start plus the run times `peak` plus the perpendicular times `strength` times the distance, sampled into ordinary x and y keys every ~33 ms on the video's frame grid. The bulge stays on one side whichever way the node goes (up by default). `from` and `to` are the node's x and y (its top left). `orient` also turns the node to face along the curve (rotation by the heading, unwrapped, so it never jumps by a turn). `ease` shapes how far along the curve the node is at each moment. Keys outside the move are kept, so moves chain. The same `arc` object on animate_preset bends any scene move that travels in x and y.

Arguments:

- `arc` (object) Bend the straight run into an arc: a quadratic Bezier whose control point is the start plus the run times `peak` plus the perpendicular times `strength` times the distance. Sampled into x/y keys every ~33 ms, so the node curves instead of sliding straight.
- `at_ms` (integer, required) When the move starts.
- `duration_ms` (integer, required) How long it runs.
- `ease` (string) How far along the curve the node is over time (default ease_in_out). One of linear, ease_in, ease_out, ease_in_out, back, spring, ease_in_out_cubic, ease_out_back, elastic, bounce, steps_2, steps_3, steps_4, steps_6, steps_8, steps_12, power2_in, power2_out, power2_in_out, power4_in, power4_out, power4_in_out, expo_in, expo_out, expo_in_out, circ_in, circ_out, circ_in_out, sine_in, sine_out, sine_in_out, anticipate, bounce_in, bounce_in_out, spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy or a tuned one: cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to 0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; anticipate (pulls back first, then goes).
- `from` (any) Where the move starts, [x, y] or {x, y} (default: where the node is at at_ms).
- `node` (string, required) Id of the node to move.
- `orient` (boolean) Turn the node to face along the curve.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `to` (any, required) Where the move ends, [x, y] or {x, y}.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `animate_keyframes`

CSS-style percentage keyframes over several properties of one node, written as ordinary keys. `keyframes` maps '0%', '40%', '100%' (also 'from' and 'to') to the values the properties have there: {"0%": {"x": 0, "y": 0}, "40%": {"x": 200, "y": -80, "ease": "ease_out"}, "100%": {"x": 400, "y": 0}}. Each property has its own keys: a keyframe that leaves a property out does not touch it. A keyframe's `ease` shapes the move that LEAVES it (as in CSS); the last keyframe's ease is unused (the result says so). A property whose first keyframe is after 0% starts from its current value and goes there over the time before it. Percentages are of duration_ms; past 100% needs `allow_over`. Camera properties (camera_x...) need no node. fill_color takes '#rrggbb'.

Arguments:

- `allow_over` (boolean) Allow percentages past 100% (the move runs longer than duration_ms).
- `at_ms` (integer) When 0% is (default 0).
- `duration_ms` (integer, required) How long 100% takes.
- `ease` (string) The ease of a segment whose leaving keyframe names none (default ease_in_out). One of linear, ease_in, ease_out, ease_in_out, back, spring, ease_in_out_cubic, ease_out_back, elastic, bounce, steps_2, steps_3, steps_4, steps_6, steps_8, steps_12, power2_in, power2_out, power2_in_out, power4_in, power4_out, power4_in_out, expo_in, expo_out, expo_in_out, circ_in, circ_out, circ_in_out, sine_in, sine_out, sine_in_out, anticipate, bounce_in, bounce_in_out, spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy or a tuned one: cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to 0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; anticipate (pulls back first, then goes).
- `keyframes` (object, required) Percentage -> {property: value, ease?}, e.g. {"0%": {"x": 0}, "100%": {"x": 400}}.
- `node` (string) Id of the node (not needed when only camera properties are keyed).
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `text_on_path`

Lay a line of text along a path node's curve: each glyph (the renderer's own shaping and advances, as split_text uses) becomes a text node placed by arc length and turned to the curve's tangent, standing on it. The pieces are a group named like the text, in reading order, spaces keeping their width but not being nodes. `align` is start (default), center or end along the path; `start_offset` shifts the start in document units. A closed path wraps; on an open one a glyph that would fall off the end is left out (the result counts them). `reveal` brings the letters in one by one with a scene move (default pop_in, 40 ms apart, from `video`); animate the group afterwards. The original is removed (`mode: replace`) or hidden (`keep`). Place and style the path node first: the glyphs follow its flattened curve in the text's parent's coordinates (an ancestor's rotation or scale is not applied).

Arguments:

- `align` (string) Where the text sits along the path (default start).
- `mode` (string) keep hides the original text; replace (default) removes it and the group takes its name.
- `path_node` (string, required) Id of the path node to follow.
- `reveal` (object) Bring the letters in one by one.
- `start_offset` (number) Document units from where align puts the start (default 0).
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `text_node` (string, required) Id of the text node.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.

## `morph_to`

Turn one path node into another over time. Both outlines are cut by arc length into the same number of points (about one per 3 document units, at least 16, at most 256); two closed shapes get matching windings and the start point that moves the points least, so the shape does not twist; contours are paired largest first and one with no partner shrinks to its middle (or grows from it). The node keeps its own path data and gains the shapes it morphs through (`path_morph`) and a `morph_progress` animation: 0 is its own shape, 1 the target's, drawn as a polyline of those points, so it looks the same everywhere frames are made. The target stays where it is: the node's shape lands on the target's place. Calling morph_to again on the same node (starting after the first ends) continues the morph to a third shape, with the same contours. The morph does not follow later edits of the node's path: run it again (with `reset`) after changing it. `ease` shapes the progress.

Arguments:

- `at_ms` (integer, required) When the morph starts.
- `duration_ms` (integer, required) How long it runs.
- `ease` (string) How the morph progresses over time (default ease_in_out). One of linear, ease_in, ease_out, ease_in_out, back, spring, ease_in_out_cubic, ease_out_back, elastic, bounce, steps_2, steps_3, steps_4, steps_6, steps_8, steps_12, power2_in, power2_out, power2_in_out, power4_in, power4_out, power4_in_out, expo_in, expo_out, expo_in_out, circ_in, circ_out, circ_in_out, sine_in, sine_out, sine_in_out, anticipate, bounce_in, bounce_in_out, spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy or a tuned one: cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to 0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; anticipate (pulls back first, then goes).
- `node` (string, required) Id of the path node that morphs.
- `reset` (boolean) Start over from the node's own path, dropping an earlier morph and its progress keys.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `to_path` (string, required) Id of the path node whose shape it becomes.
- `turn` (string) Groups related calls into one turn; omit and each call is its own turn.
- `video` (string, required) Id of a video from list_videos.

## `split_text`

Split a text node into a group of one text node per character, word or line, each drawn where the original glyphs were (the renderer's own shaping and wrapping, so the lines are the ones the box width really breaks to) with the original font, size, weight, line height, fill and stroke. Pieces are named '<name>.c0..' (chars), '<name>.w0..' (words) or '<name>.l0..' (lines), in reading order; spaces are not nodes. The group takes the original's box, transform, opacity, blur and shadow. Returns the piece ids in order: pass them to animate_preset stagger as nodes, or use text_reveal to split and animate in one call. One attributed, undoable turn. Limits: a ligature is one glyph and so one character piece, and the kerning between two pieces is lost; a piece is set flush left at the place its glyphs were drawn (a line keeps the text's own alignment), so a centred or right-aligned text may sit a pixel off; position is rounded to whole pixels, so a left-aligned text is drawn exactly as before and other alignments match to within a pixel; right-to-left text is ordered by logical position and only pure runs are exact.

Arguments:

- `by` (string, required) 
- `mode` (string) keep hides the original text (visible false) so it can be shown again; replace (the default) removes it and the group takes its name. One undo restores it either way.
- `node` (string, required) Id of the text node.
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn.
- `wrap` (boolean) Put each piece in its own clipping group (a mask box the size of the piece's line) so it can rise or slide in from outside the box. The animated node is then the text inside the mask.

## `text_reveal`

Bring text in piece by piece: splits the text node first (as split_text does, unless it is already a split group) and writes the keyframes, as one undoable turn. Styles: rise (pieces rise 450 ms with power4_out, from below their mask box; masked by default), fade (400 ms ease_out), pop (scale 0.5 to 1 with a small overshoot, 350 ms), slide (from the left, 500 ms power4_out), typewriter (each piece appears at once, 33 ms apart per character). Stagger defaults: 40 ms per character, 90 ms per word, 140 ms per line (typewriter 33 / 120 / 400). `by` defaults to characters for a headline of 12 characters or fewer on one line, else words (lines past 4 lines or 40 words). Never reveal body copy per character. Keys replace any track a piece already has. Returns the resolved timing (each piece's start and end), when the reveal ends and how long to hold it for it to be readable (1 s plus 0.4 s per short word, review check RV015).

Arguments:

- `at_ms` (integer) When the first piece starts (default 0).
- `by` (string) 
- `duration_ms` (integer) How long each piece takes (style default; ignored by typewriter).
- `each_ms` (integer) Milliseconds from one piece to the next.
- `ease` (string) Overrides the style's ease: one of linear, ease_in, ease_out, ease_in_out, back, spring, ease_in_out_cubic, ease_out_back, elastic, bounce, steps_2, steps_3, steps_4, steps_6, steps_8, steps_12, power2_in, power2_out, power2_in_out, power4_in, power4_out, power4_in_out, expo_in, expo_out, expo_in_out, circ_in, circ_out, circ_in_out, sine_in, sine_out, sine_in_out, anticipate, bounce_in, bounce_in_out, spring_snappy, spring_smooth, spring_playful, spring_bouncy, spring_heavy or a tuned one: cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to 0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; anticipate (pulls back first, then goes).
- `mode` (string) keep hides the original text (visible false) so it can be shown again; replace (the default) removes it and the group takes its name. One undo restores it either way.
- `node` (string, required) Id of the text node, or of a group split_text made.
- `style` (string, required) 
- `summary` (string) One line, in the user's terms, naming what this turn does.
- `turn` (string) Groups related calls into one turn.
- `video` (string, required) Id of a video from list_videos.
- `wrap` (boolean) Put each piece in its own clipping group (a mask box the size of the piece's line) so it can rise or slide in from outside the box. The animated node is then the text inside the mask.

## `list_seeds`

The six scene seeds: starter scenes built from blocks (explainer opener with scenery and a character, product reveal with a 3D phone, kinetic type quote, stats callout, before and after, logo sting). Each says what it builds, how long it is, the beats it is planned around and its layout notes. apply_seed builds one. Read get_playbook seeds.

Read-only.

No arguments.

## `apply_seed`

Build a seed as ONE attributed, undoable turn: a frame, a video with one clip, the seed's blocks (the add_block code path, with the applied style pack's tokens when one is set), ambient motion and a slow camera drift, and a SceneSpec in scene_specs/ with the layer plan, beat map and palette. Returns the layouts notes, the beat times (place sounds by beat name), the open P0 gates and the next call. Applying twice makes a second, separate scene. Then swap the copy, check with validate_document and review_video, and record the review.

Arguments:

- `brand` (object) Copy and colors to swap in; a seed's own words fill what you leave out. Colors are #rrggbb.
- `id` (string, required) A seed id from list_seeds.
- `video` (string) Id of an existing EMPTY video to put the scene in (its size is used). Omit to create one (the workspace's chosen video format, else 1280x720 at 30 fps).

## `list_blocks`

The block catalog: ready-made titles, lower thirds, overlays, backgrounds, charts, store headlines, motion wrappers, rigged flat-vector characters (eighteen expressions, expression changes, reactions), emote marks (exclamation, sweat drops, hearts, lightbulb, zzz), speech and thought bubbles and animated props that expand into ordinary nodes (and keyframes on a video). Search by what you need (`query`: words matched against names, tags and descriptions) and/or `category`. Each result has the block id, its category and what it is for. Then get_block for its parameters and add_block to use it. Scenery blocks (sky_gradient, parallax_hills, starfield, clouds, sun_glow, ocean_waves, city_skyline, cells_backdrop, particles, grid_floor, connector, highlight_ring) build layered, always-moving flat-vector scenes: read get_playbook `scenery` first. Hand-made looks (sketch, boil, watercolor_wash, paper_grain, flow_strokes, marker_underline, scribble_circle) draw wobbly pen outlines and hatching, painted washes, film grain, grass and wind strokes and hand-drawn emphasis: read get_playbook `sketch` first.

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

Set a video's captions from a transcript the user (or their speech MCP server) produced. format `srt` or `vtt` (WebVTT inline <00:00:01.500> word times become word timings), or `words_json`: a list of {text|word, start_ms|start (seconds), end_ms|end}, which is what a speech-to-text server gives, grouped into cues at sentence ends and pauses. This tool does not recognize speech: OpenDesigner has no speech-to-text built in, so a transcript must come from the user's own speech server (or `transcribe`, which runs the user's whisper). Replaces every cue; keeps the style. `offset_ms` shifts all times; cues outside the video are dropped and counted. Captions are drawn in the preview and burned into export_video.

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

## `search_sounds`

Find music and sound effects for a video on Openverse, Freesound, Incompetech or ccMixter. Returns each sound's provider, id, title, creator, length, licence (SPDX style, with its url and whether it needs a credit), the ready-made attribution and a preview to listen to. While the project is commercial (sound_settings, the default) sounds with a non-commercial, no-derivatives or unknown licence are left out and counted in hidden_by_license. Sounds are licensed by their authors, not by us: prefer CC0, and for anything that needs a credit call sound_credits before the video is finished. Import one with import_sound, giving it this provider and id.

Read-only.

Arguments:

- `duration_max_ms` (integer) Keep only sounds no longer than this.
- `page` (integer) Which page of results (1 is the first).
- `provider` (string) Where to look: openverse (keyless; sound effects and music from Freesound, Jamendo and Wikimedia), freesound (needs the user's token; the biggest library of sound effects), incompetech (keyless; Kevin MacLeod's music, all CC BY 4.0) or ccmixter (keyless; music, CC BY). Default: freesound when the user added a token, otherwise openverse.
- `query` (string, required) Words to search for, for example "soft piano loop".
- `source` (string) Openverse only: one hosting site, freesound, jamendo or wikimedia_audio.

## `import_sound`

Download one sound found with search_sounds into this project's media/ folder and write its licence record, media/<name>.license.json, next to it. Pass the provider and id from the search result. Returns the media/ reference and the credit that must be shown (empty for CC0). A sound whose licence the project does not clear (see sound_settings) is refused with what to do. The file is only downloaded from the provider's own hosting sites and is never overwritten. Then put it under a video with set_video prop {name: "audio", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}} or add_sound_track.

Arguments:

- `id` (any, required) The id of a sound from search_sounds.
- `name` (string) A plain file name for the sound, without the ending.
- `provider` (string) Where to look: openverse (keyless; sound effects and music from Freesound, Jamendo and Wikimedia), freesound (needs the user's token; the biggest library of sound effects), incompetech (keyless; Kevin MacLeod's music, all CC BY 4.0) or ccmixter (keyless; music, CC BY). Default: freesound when the user added a token, otherwise openverse.

## `sound_settings`

Read or set how this project may use sounds. `commercial` (default true: a video that may earn money or promote a business) hides and refuses sounds under non-commercial, no-derivatives or unknown licences. Set it to false only when the user says the video is not commercial, and quote them in `reason`; never change it to get a sound through. With no arguments it only reads.

Arguments:

- `commercial` (boolean) Whether the project's videos are commercial.
- `reason` (string) Required with commercial false: the user's own words for why.

## `sound_credits`

Write the credits for the sounds a video uses: the credit line of every sound whose licence asks for one (CC BY and the like), with a note when the video trims, fades, lowers or re-levels it, as the licences require. Saved to exports/<video>-credits.txt and returned. Call it before finishing any video that uses a sound that is not CC0 or synthesized, and put the text in the description or the end card. Also lists sounds with no licence record and ShareAlike sounds.

Arguments:

- `video` (string, required) Id of a video from list_videos.

## `scene_spec_new`

Start the SceneSpec for a video: a starter with the minimum number of details for its complexity tier and placeholders that FAIL strict validation until you replace them with the real layers, details, features, palette and motion. Write the plan before you build anything; see get_playbook build.

Arguments:

- `audience` (string) 
- `intent` (string) explainer, promo, tutorial, story, ...
- `max_per_pass` (integer) Refine actions allowed per pass (default 3).
- `max_total` (integer) Refine actions allowed in total (default 6); not below max_per_pass.
- `name` (string) Plain spec name: lowercase letters, digits, . _ - (default "main"). Stored as scene_specs/<name>.json in the project.
- `overwrite` (boolean) Replace an existing spec of this name, review history included.
- `silent` (boolean) No audio planned.
- `subject` (string, required) What the video is about, one sentence.
- `tier` (string) Complexity tier (default moderate): needs at least 3, 6, 10, 16 inventoried details.
- `video` (string) Video id; takes its size, frame rate and length.

## `scene_spec_write`

Replace the SceneSpec with a full, edited one (a refine-spec, or filling the starter in). The review history is kept as recorded: you cannot rewrite it. Returns the strict validation.

Arguments:

- `name` (string) Plain spec name: lowercase letters, digits, . _ - (default "main"). Stored as scene_specs/<name>.json in the project.
- `spec` (object, required) The whole SceneSpec (schema opendesigner.scene_spec/1).

## `scene_spec_validate`

Check the SceneSpec. Errors always fail; quality findings (routing under 0.82, too few details for the tier, details that map to nothing, more than 5 critical or 3 important features per pass, ambient motion without loop, gait without stance, no audio) fail only with strict, which a build needs.

Read-only.

Arguments:

- `include_spec` (boolean) Also return the spec.
- `name` (string) Plain spec name: lowercase letters, digits, . _ - (default "main"). Stored as scene_specs/<name>.json in the project.
- `strict` (boolean) 

## `scene_status`

Where the build is, from the review history only: current pass, passes completed, refine counters, active or stopped with the reason, missing evidence and the exact next tool call. Call it before every action; never continue from memory.

Read-only.

Arguments:

- `name` (string) Plain spec name: lowercase letters, digits, . _ - (default "main"). Stored as scene_specs/<name>.json in the project.

## `record_review`

Record your review of a pass and choose ONE action: continue, refine-spec (a planned thing is missing or wrong in the plan), refine-code (the plan is right, the build is not), request-input, or stop. A continue for a visual pass needs the contact sheet and frames in exports/, ai_score >= 0.7, every layer score and every critical feature at its minimum; a pass cannot be continued before the one before it. Refused when the loop limit was reached.

Arguments:

- `action` (string, required) 
- `ai_score` (number) Your score of the contact sheet as a whole.
- `defect_tags` (array) Short names of the defects you still see; the same tag twice running means refine-spec.
- `evidence` (object) 
- `feature_reviews` (array) 
- `fidelity` (number, required) 0.2 placeholder, 0.4 silhouette recognizable, 0.6 macro and meso right, 0.75 reads correctly, 0.85 strong, 0.95 near-reference.
- `hard_gate_failures` (array) Numeric gate findings with severity fail.
- `layer_scores` (object) 0 to 1 each: silhouette_proportion, component_structure, form_detail, palette_fidelity, motion_quality.
- `name` (string) Plain spec name: lowercase letters, digits, . _ - (default "main"). Stored as scene_specs/<name>.json in the project.
- `pass_id` (string, required) 
- `reverted` (boolean) This iteration undid the previous one.
- `summary` (string, required) What changed, what still does not match, and what the numeric gate does not measure.

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

An animation director's check of a whole video, in numbers, without changing anything. Call it before export_video on any animated or illustrated video and fix what it finds. It reads each scene's nodes and tracks and renders a few small pictures across the video to measure motion. Findings have stable codes: RV001 a scene is one image with text on it, RV002 only text and rectangles move, RV003 a still stretch over 2.5 s, RV004 no looping ambient motion, RV005 no camera move in a video over 15 s, RV006 too few vector shapes, RV007 repeated scene compositions, RV008 captions cover too much, RV009 no sound, RV010 the sound is the same in every scene, RV011 colors are not from tokens, RV012 too few layers moving, RV013 flash safety (over 3 large brightness changes in a second: an error that makes the video not ready), RV014 cuts off the beat (needs `bpm`), RV015 text leaves before it can be read, RV016 text in a vertical video's unsafe top or bottom zone, RV017 a 2 s stretch with fewer than 3 moving layers, RV018 the same scene twice, RV019 no held last picture. `overlap` lists regions where two layers of a scene overlap, back to front: check each with video_crop and confirm which should be on top. It returns per-scene and whole-video numbers, a score from 0 to 100 and the three fixes that would help most. A score under 70 is not ready.

Read-only.

Arguments:

- `allow_unevaluated` (boolean) Accept a gate that could not be evaluated (the beat without a tempo, the flash scan of a video with footage). The report says which gates were unevaluated; the video is not ready unless this is true.
- `bpm` (number) The tempo of the music, to check that cuts land on a beat (RV014). Videos have no tempo of their own.
- `samples` (integer) How many moments to render (default 12).
- `video` (string) Id of a video from list_videos; defaults to the first.

## `verify_motion`

Proves the tracks of a video play, in numbers, without changing anything. For every animation track (and the camera's) it samples at least 5 moments (default 7, always the start, the middle and the end) and compares the value the track says with the value the renderer's pipeline draws (G1), checks the node exists, the property fits its kind, the node is visible and its scene on screen while the track runs (finding RV020 track-drives-nothing), that every value is finite and legal (G2), and that looping tracks end where they start (G3: rotation 0.5 degrees, position 0.01 of the frame height, scale and opacity 0.005). Each track reports id, node, prop, samples, max_delta, status (pass, fail or unevaluated, never a silent pass) and reasons. With classify it also measures each moving node in fractions of the frame height (travel, rise, speed, rotation, scale, pose return) and names the motion (idle, in-place, walk, run, dash, jump, leap); looping ambient motion must read as idle. `gait` checks foot slide against the 0.01 limit. Call it after the motion pass and fix every fail before review_video.

Read-only.

Arguments:

- `ambient` (array) Node ids that are ambient life and must read as idle (nodes whose every track loops are ambient already).
- `classify` (boolean) Also measure and classify the motion of every animated node.
- `declared_loops` (array) Loop declarations from the plan: {node, prop, loop: true|false|null}. A null loop is unknown: the track is treated as one-shot and the gate is unevaluated.
- `expect` (array) Intended motion classes: {node, class}; the measured labels must include it. A class that names intent (strike, guard) is reported as inferred.
- `gait` (array) Foot-slide checks: {node (the foot), stance: [{from_ms, to_ms}], character_height (default the frame height), ground (a scrolling node whose motion is subtracted)}. A gait with no stance is unevaluated.
- `samples` (integer) Moments sampled per track (default 7, at least 5).
- `video` (string) Id of a video from list_videos; defaults to the first.

## `scene_check`

Geometry gates over a video's document, without rendering the whole video and without changing anything. Run at the union of the keyframe times of each chain plus 12 even moments. Each gate answers pass, fail or unevaluated with reasons: bleed (a layer flagged bleed:true must cover the frame under every camera pan, zoom and turn, scaled by its depth), attach (a child's box within gap_tolerance of its parent's), chirality (left and right parts are a reflection, never a rotation, and the left one is on the viewer's right when facing front), part_coverage (every layer_plan entry is a node; none is fused; unowned nodes and unmapped details warn), motion_roster (every layer that expects motion has a track), and freeze (nodes frozen after the shapes pass have not changed). `freeze_snapshot` returns the hashes to store; `footprint` counts the pixels a node really changes in the picture.

Read-only.

Arguments:

- `added_by` (object) Nodes a later pass added, by pass name: {node_id: pass}.
- `camera` (object) Declared camera extremes, checked as extra poses: {x_max_px, y_max_px, zoom_min, zoom_max, rotation_max_deg}.
- `details` (array) The detail inventory: {id, maps_to: {type: node|track, ref}}; one that maps to nothing real warns.
- `footprint` (object) Pixels node really changes at t_ms (the picture with and without it).
- `freeze` (object) A snapshot from freeze_snapshot: {hashes, attributes}. Later passes may change nothing in it but nodes listed in added_by.
- `freeze_snapshot` (boolean) Return the freeze hashes of the video's scenes instead of comparing.
- `layer_plan` (array) The plan's layers: {id, node_ref (a node id or name), role, bleed, parent, attach: {gap_tolerance_px, contact}, side: l|r, side_source: reference|convention, facing: front|side, mirror_of, expects_motion, ignore}.
- `video` (string) Id of a video; defaults to the first.

## `hit_test`

The node ids under a point at a moment of a video, topmost first (the discriminating test for 'what is covering this?'). The point is in the frame's own units (0,0 is its top-left corner), as the design shows at that moment; a moving camera is not applied.

Read-only.

Arguments:

- `t` (number) Moment in ms from the start of the video (default 0).
- `video` (string) Id of a video; defaults to the first.
- `x` (number, required) 
- `y` (number, required) 

## `get_capabilities`

Which optional programs the user has installed, with their versions: ffmpeg (video export, pauses, waveforms), a speech-to-text program (whisper-cli or whisper, for transcribe) and rembg (for remove_background), each with the exact install hint when it is missing. Read-only: it runs only `--version`. Call it before transcribe, remove_background or export_video. If something you need is missing, ask the user to install it instead of trying. OpenDesigner never installs or downloads these.

Read-only.

No arguments.

## `transcribe`

Speech to text, on the user's computer: runs the user's whisper program over a media/ audio or video file (`file`) or over a video's own sound mix (`video`) and returns the words with their times in ms (the same shape import_captions reads as words_json), the plain text, and gaps between words (like detect_pauses). With `apply: true` the words also become the video's captions with word highlighting, as one undoable change (`video` is then required; with `file` add `offset_ms` where the file starts on the timeline). Nothing is uploaded by OpenDesigner. Slow: minutes for long speech. Call get_capabilities first. When a program is missing, do not try the tool: tell the user what to install (the report has the exact command) and carry on without it. whisper-cli (whisper.cpp) needs a model file: the user sets OPENDESIGNER_WHISPER_MODEL, or you pass `model` as a plain file name the user placed in the app's models folder or in media/; the Python whisper takes a model name such as base or small.en.

Arguments:

- `apply` (boolean) Also set the words as the video's captions (replaces its cues).
- `file` (string) A media/ audio (.mp3 .wav .m4a .aac .ogg) or footage (.mp4 .mov .m4v) file. Give this or `video`.
- `language` (string) `auto` (default) or a two- or three-letter code such as en, de, es.
- `min_pause_ms` (integer) The shortest gap between words reported as a pause (default 500).
- `model` (string) whisper-cli: a plain model file name in the app's models folder or media/. whisper: a model name (base, small.en, large-v3). Default: OPENDESIGNER_WHISPER_MODEL, or base for whisper.
- `offset_ms` (integer) With file and apply: where the file starts on the video's timeline (default 0).
- `summary` (string) 
- `timeout_s` (integer) Stop the program after this long (default 300).
- `turn` (string) 
- `video` (string) Id of a video from list_videos: its sound mix is transcribed, so the times are timeline times. Required with apply.

## `remove_background`

Cut the subject out of an image with the user's rembg, on their computer: reads a media/ image (png, jpg, webp) and writes media/<name>-cutout.png with a transparent background (or, with `mask_only`, media/<name>-mask.png, the black-and-white mask), plus a .rembg.json sidecar naming the source, tool and model. Nothing is overwritten and the source stays as it is. The result is an ordinary image asset: place it with an image node. Nothing is uploaded by OpenDesigner. Call get_capabilities first. When a program is missing, do not try the tool: tell the user what to install (the report has the exact command) and carry on without it.

Arguments:

- `file` (string, required) media/<name> of a png, jpg or webp.
- `mask_only` (boolean) Write only the mask.
- `model` (string) A rembg model name such as u2net, isnet-general-use or birefnet-general (the user's rembg may download it the first time). Default: rembg's own. Model weights have their own licences and some (for example bria-rmbg) are non-commercial: when the project is commercial, name isnet-general-use or u2net.
- `timeout_s` (integer) Stop the program after this long (default 180).

## `video_strip`

Every frame (or every Nth) of a short stretch of a video, in one labeled grid read left to right, top to bottom: the way to judge anticipation, overshoot, settling and a transition frame by frame. At most 36 pictures; a longer stretch is refused with the `every` that fits, so narrow `from_ms` and `to_ms` to the move you are judging. Each picture carries its time in milliseconds.

Read-only.

Arguments:

- `cell_side` (integer) Longest side of each picture (default 192).
- `every` (integer) Take every Nth frame (default 1: all).
- `from_ms` (integer, required) First moment.
- `locale` (string) A language from list_locales.
- `to_ms` (integer, required) Last moment, included.
- `video` (string, required) Id of a video from list_videos.

## `video_crop`

A piece of the video's picture at full resolution (1:1), to inspect a face, a hand, a contact point or small text. Give `x`, `y`, `width`, `height` in video pixels, or a `node` id: the crop then follows that node's box through the camera, the clip's zoom and the node's own transforms at that moment, plus `padding`. `zoom` 2 to 4 enlarges each pixel (no smoothing) for a tiny region. Use frame_grid first to read positions.

Read-only.

Arguments:

- `height` (integer) 
- `locale` (string) A language from list_locales.
- `node` (string) Instead of a region: a node id from get_tree whose box to follow.
- `padding` (integer) Video pixels added around a node's box (default 24).
- `time_ms` (integer, required) The moment.
- `video` (string, required) Id of a video from list_videos.
- `width` (integer) 
- `x` (integer) 
- `y` (integer) 
- `zoom` (integer) Enlarge each pixel this many times (default 1).

## `frame_grid`

The picture at a moment with a labeled coordinate grid over it: lines every `spacing` video pixels, numbered along the top (x) and left (y) edges in video pixels, so you can read where things are and give exact x, y, width, height to video_crop or to apply_ops.

Read-only.

Arguments:

- `locale` (string) A language from list_locales.
- `max_side` (integer) Longest side of the picture (default 768).
- `spacing` (integer) Video pixels between lines (default: about 10 lines along the long side).
- `time_ms` (integer, required) The moment.
- `video` (string, required) Id of a video from list_videos.

## `video_diff`

Two moments side by side with the pixels that changed painted magenta over a dimmed copy: the first picture, the second, then the changes (`mode` overlay gives only the changes). Compare a time with another time of the video (`other_time_ms`), or the same or another time of another video of the project (`other_video`, same size). The text gives the share of pixels changed and their box. Use it on a nearly identical pair, such as the frames before and after a hit, or two takes of a scene.

Read-only.

Arguments:

- `locale` (string) A language from list_locales.
- `max_side` (integer) Longest side of each picture (default 512).
- `mode` (string) Default side_by_side.
- `other_time_ms` (integer) The second moment (default: the same as time_ms).
- `other_video` (string) A second video to take the second moment from (default: the same video).
- `threshold` (integer) A channel must differ by more than this to count as changed (default 12).
- `time_ms` (integer, required) The first moment.
- `video` (string, required) Id of a video from list_videos.

## `watch_footage`

Look at a recording without importing it into a video: up to 10 evenly spaced pictures from a footage file (or from `from_ms` to `to_ms` of it), in one labeled grid you can see, each picture carrying its time in ms. Pass the `question` you want answered and it is repeated next to the picture so you look for the right thing. The pictures are one per equal slice of the range, so motion between them is not visible: narrow the range to look closer. Read-only; needs ffmpeg. Use it before writing a note with index_footage, and again on a narrower range to find an exact moment to cut at.

Read-only.

Arguments:

- `cell_width` (integer) Width of each picture in pixels (default 320).
- `file` (string, required) A footage file from list_footage: media/<name>.mp4, .mov or .m4v.
- `frames` (integer) How many pictures (default 8).
- `from_ms` (integer) Start of the range (default 0).
- `question` (string) What you are trying to learn from the pictures.
- `to_ms` (integer) End of the range (default: the end of the file).

## `index_footage`

Remember what is in a footage file. Cuts it into shots where the picture changes (ffmpeg's scene score; `threshold` 0.3 is a start, lower finds more cuts), optionally adds a word-timed transcript with `transcribe: true` (the user's own whisper program: call get_capabilities first), and stores the result in footage-index/ in the project. Then `watch_footage` each shot you have not seen and write what is in it with `notes`: [{shot, note, tags?}] (shot numbers start at 0; an empty note clears one). Calling it again for an unchanged file with the same threshold only updates notes and the transcript; a changed file or threshold scans again and carries notes over to shots that start within a quarter of a second of where they did. The answer lists every shot with its note and the words spoken in it. Nothing is uploaded.

Arguments:

- `file` (string, required) A footage file from list_footage: media/<name>.mp4, .mov or .m4v.
- `language` (string) With transcribe: `auto` (default) or a code such as en, de.
- `min_shot_ms` (integer) A cut closer than this to the previous one is ignored (default 500).
- `model` (string) With transcribe: the speech model, as for the transcribe tool.
- `notes` (array) What you saw: one entry per shot.
- `rescan` (boolean) Scan again even though the index is current.
- `threshold` (number) How different a picture must be from the one before to count as a cut (default 0.3).
- `timeout_s` (integer) With transcribe: stop the speech program after this long (default 300).
- `transcribe` (boolean) Run the user's speech program and keep the words (slow for long speech).

## `search_footage`

Find a moment in the footage you indexed by describing it: words are matched against your shot notes and tags (strongest) and the transcript. Each result is a shot with its file, start and end in ms, the note, the words spoken and a ready `clip` ({footage: {file, in_ms}, duration_ms}) to put in add_clip. Without `query` it lists what is indexed and how much of it has notes. A file that changed since it was indexed is marked `stale`: run index_footage on it again. Read-only; works from the index alone, so it needs neither ffmpeg nor a speech program.

Read-only.

Arguments:

- `file` (string) A footage file from list_footage: media/<name>.mp4, .mov or .m4v.
- `limit` (integer) Most results (default 10).
- `query` (string) Words to look for, for example `founder laughing at the desk`.

## `audio_reactive`

Make one property of one node follow a sound: the sound of a media/ audio or footage file is measured (`band` level, or only low = bass and kick, mid = voice, high = hats and sibilants), smoothed and written as keyframes between `min` (silence) and `max` (the loudest moment). Examples: a bar's `scale_y` from 0.2 to 1 with the bass, a glow's `opacity` from 0.3 to 1 with the music, a mouth's `scale_y` from 0.1 to 1 with a voice track (lip flap), a logo's `scale` from 0 to 0.12 on the kick (`additive: true` keeps its travel). `start_ms` is where the sound starts on the video's timeline, `from_ms` and `to_ms` the part of the file used. At most 500 keys a track, so a long sound needs fewer `keys_per_second`. One undoable step; replaces any track on that node and property unless `additive`. Needs ffmpeg and a sound; check the result with video_strip.

Arguments:

- `additive` (boolean) Add the values to the node's own instead of replacing it: min 0 and max 20 on x is a shake of up to 20 px (default false).
- `attack` (number) How quickly the value rises with the sound, 1 is at once (default 0.6).
- `band` (string) Which part of the sound to follow (default level).
- `file` (string, required) media/<name> of an audio (.mp3 .wav .m4a .aac .ogg) or footage (.mp4 .mov .m4v) file with sound.
- `floor` (number) Sound quieter than this share of the loudest counts as silence, so room noise does not move anything (default 0.1).
- `from_ms` (integer) Where in the file to start measuring (default 0).
- `keys_per_second` (integer) How often a key is written (default 15).
- `max` (number) The value at the loudest moment (default 1).
- `min` (number) The value in silence (default 0).
- `node` (string, required) Id of the node to move, from get_tree.
- `prop` (string, required) The property to drive, as animate takes it: x, y, width, height, opacity, rotation, scale, scale_x, scale_y, blur, stroke_width, trim_end, and so on.
- `release` (number) How quickly it falls back, lower is a longer tail (default 0.25).
- `start_ms` (integer) Where the sound starts on the video's timeline (default 0).
- `summary` (string) 
- `to_ms` (integer) Where in the file to stop (default: as long as the video lasts after start_ms, or the file's end).
- `turn` (string) 
- `video` (string, required) Id of a video from list_videos.

## `animate_noise`

Organic, never-repeating motion on one node: smooth seeded noise written as keyframes, `amplitude` big around `center`. Examples: a handheld shake is props [x, y] with amplitude 6 and frequency 6; a drifting cloud is x with amplitude 40 and frequency 0.2; a breathing glow is opacity with amplitude 0.15, center 0.85 and `additive: false`; a camera sway is camera_x and camera_y (node = the video's own id). The same seed writes the same curve on every machine; `props` takes up to 4 properties, each with its own stream. `additive` (default true) adds the wobble to wherever the node otherwise is, so it rides on a travel; with false the values are absolute, around `center`. `loop: true` makes the track end where it began and repeat, so a clip can loop without a jump. Replaces that node's track for the property (of the same kind). At most 500 keys a track. Check with video_strip.

Arguments:

- `additive` (boolean) Add to the node's own value (default true).
- `amplitude` (number, required) How far from the center the value goes (in the property's own unit).
- `center` (number) The value the noise moves around (default 0, right for additive).
- `frequency` (number) About how many times a second the value changes direction (default 1.5).
- `from_ms` (integer) Start on the video's timeline (default 0).
- `keys_per_second` (integer) How often a key is written (default 15).
- `loop` (boolean) Seamless loop over from_ms to to_ms, repeating for the rest of the video.
- `node` (string, required) Id of the node (the video's own id for camera_* properties).
- `octaves` (integer) Layers of finer detail (default 2).
- `prop` (string) One property, as animate takes it: x, y, rotation, scale, opacity, blur, camera_x ...
- `props` (array) Several properties at once, each with its own noise.
- `seed` (integer) The same seed gives the same motion (default 1).
- `summary` (string) 
- `to_ms` (integer) End (default: the end of the video).
- `turn` (string) 
- `video` (string, required) Id of a video from list_videos.

## `fit_clip_to_media`

Set a clip's length from a file instead of guessing it. A footage clip with no `file` plays the rest of its own file from its in point (at its speed). With `file` (a media/ audio or footage file) the clip lasts as long as that file: a title card held for the whole voice-over, a design frame under a music bed. `pad_ms` adds a hold after the end (default 0). A clip is at most 60 s: a longer file is capped and the answer says so. Changing a clip's length moves everything after it; overlays, captions, sound tracks and keyframes keep their times, so check them. A footage clip with a speed ramp is refused (its length is not one division). Needs ffmpeg.

Arguments:

- `clip` (string, required) Id of the clip, from list_videos.
- `file` (string) media/<name> of an audio (.mp3 .wav .m4a .aac .ogg) or footage (.mp4 .mov .m4v) file.
- `pad_ms` (integer) Extra milliseconds after the end of the file (default 0).
- `summary` (string) 
- `turn` (string) 
- `video` (string, required) Id of a video from list_videos.

## `export_timeline`

Write a video's timeline for another editor, to exports/<file>: `edl` is a CMX 3600 edit decision list (Final Cut Pro, Premiere Pro, DaVinci Resolve and others import it): one event per clip with its source file, source in and out and record in and out, footage speed, dissolves for transitions, and sound tracks. `srt` and `vtt` are the video's captions as subtitles. What an EDL cannot hold (design-frame clips, overlays, captions, keyframes, grades, speed ramps, the kind of a transition) is listed in `warnings`: tell the user. Design-frame clips are placeholders; export_video renders the finished video for them. Writes only the exports folder.

Arguments:

- `file` (string) A plain file name ending in the format, for example cut.edl (default: timeline.<format>).
- `format` (string, required) edl, srt or vtt.
- `video` (string, required) Id of a video from list_videos.

## `list_style_packs`

The style packs: a ready-made look for a piece of work. Each one carries a palette (bg, surface, fg, muted, border, accent), a type scale, a shape posture and a motion personality, with the rules that keep it coherent. Pick ONE and say why it fits the brief; never mix two. Read the `packs` playbook before you choose.

Read-only.

No arguments.

## `get_style_pack`

One style pack in full: its colours, its type, its shapes, its motion and its rules written as guidance you can follow, plus what it must never do. Read it before you build anything with the pack.

Read-only.

Arguments:

- `id` (string, required) A pack id from list_style_packs.

## `apply_style_pack`

Commit the design to a style pack: writes its palette into the document as the tokens bg, surface, fg, muted, border, accent, accent_2 and shadow, locks the palette (the linter then flags any colour outside it, rule OD092) and records the pack in the workspace brief so later tools know it. It is ONE attributed, checkpointed, undoable step: undo it and the palette goes with it. Paint with the tokens afterwards ({bg}, {accent}, ...) instead of literal colours. `overrides` changes single colours (for example {"accent": "#ff8800"}) when the user's brand demands it.

Arguments:

- `id` (string, required) A pack id from list_style_packs.
- `overrides` (object) Colours to change: token name (bg, surface, fg, muted, border, accent, accent_2, shadow) to a #rrggbb value.

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
`wipe_left`, `wipe_right`, `wipe_up`, `wipe_down`, `iris`, `flash`, `blur_dissolve`, `glitch_slice`, `flip`, `clock_wipe`)
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
OpenDesigner has no speech recognition built in), run the user's own whisper with `transcribe` (word
timings, `apply: true` sets the captions), or estimate from a script with `captions_from_script`,
then `set_caption_style`. Programs the user installs (ffmpeg, whisper, rembg for `remove_background`)
may be missing: call `get_capabilities` before using them and, when one is missing, ask the user to
install it (the report has the command) instead of failing mid-task. `detect_pauses`, `remove_pauses` and `get_waveform` need ffmpeg. A Freesound sound keeps its
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
