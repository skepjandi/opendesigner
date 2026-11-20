# OpenDesigner MCP tools

Generated from the server's tool definitions. Arguments not marked required are optional. A rejected call returns `isError: true` with the reason and changes nothing.

## `get_guidance`

Read this first. Returns the pages, the nodes the user has selected (with their data), the open notes the user pinned, and the edits the user made after your last turn. 'Make this bigger' means the selection.

Read-only.

No arguments.

## `get_playbook`

Craft guidance for making work that is not mediocre. Topics: design (images), style (named looks with rules), video (start with the inventory of media, footage, sounds and assets), motion (timings, easing, recipes), 3d (devices in 3D, captures from a simulator, moves), formats (sizes and safe zones), logo (identity design). Call it with no topic for the list; read the ones that apply before you build, not after.

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

Change the design with a list of operations, applied together or not at all. Operations: {op:'create', parent, index?, node:{kind, name?, x, y, width, height, fill?, stroke?, corner_radius?, opacity?, visible?, text?, font_size?, font_family?, font_weight?, text_align?, line_height?, image?, image_fit?, device3d?, children?}}, {op:'set', id, prop:{name, value}} with name one of name, x, y, width, height, visible, opacity, fill, stroke, corner_radius, text, font_size, font_family, font_weight, text_align, line_height, image (a media/ path from list_media), image_fit (cover, contain or fill), {op:'move', id, x, y}, {op:'reparent', id, parent, index?}, {op:'delete', id}, {op:'set_token', name, value} (a named color '#rrggbb'; value null removes an unused token), {op:'set_localized_text', locale, id, text} (a translation of a text node; text null removes it). 3D devices: kind 'device3d' is a phone, tablet, laptop or monitor drawn in 3D inside its box (the box is the viewport; 1 zoom fits the device with a margin at any turn); its fill is the body finish, its image (with image_fit) is the capture on the screen; create it with device3d:{model: phone|phone_punch_hole|tablet|laptop|monitor, rotate_x?, rotate_y?, rotate_z?, zoom?, fov?, light_angle?, shadow?, glare?, screen_scroll?, screen_footage?:{file:'media/x.mov', in_ms}} and change it with set props model, rotate_x (tilt, + leans the top away), rotate_y (turn, + turns the right edge away), rotate_z (roll, + clockwise), zoom (0.05 to 10), fov (1 to 100 degrees, 30 natural, low is flat), light_angle (degrees, 0 from above, 90 from the right), shadow (0 to 1), glare (0 to 1, screen reflection), screen_scroll (0 top to 1 bottom of a long capture; null centres), screen_footage (value {file, in_ms}: a recording on the screen that plays while the frame is shown; null shows the image). Videos (see list_videos): {op:'create_video', name, fps, width, height} (even sizes), {op:'delete_video', id}, {op:'set_video', id, prop:{name, value}} with name one of name, fps, size (value {width, height}), audio (value {file:'media/x.mp3', start_ms?, gain? (0 to 2), fade_in_ms?, fade_out_ms?} for the sound under the picture; null takes it away), motion_blur (0 off, or 2 to 16 pictures blended while anything moves), {op:'add_clip', video, frame | footage:{file:'media/x.mp4', in_ms}, duration_ms, index?, transition?, motion?} (a clip shows a design frame or plays footage from list_footage), {op:'remove_clip', video, clip}, {op:'move_clip', video, clip, index}, {op:'set_clip', video, clip, prop:{name, value}} with name one of duration_ms, frame, source (value {frame} or {footage:{file,in_ms}}), transition (value {kind: cut|fade|slide_left|slide_right|slide_up|slide_down|dip|zoom_in, duration_ms, ease?}), motion (value {scale_from, scale_to, ease?}, 1 to 3), {op:'add_overlay', video, frame, start_ms, duration_ms, fade_ms?, enter?, exit?, ease?} (a design frame drawn over the whole video for a while: titles, lower thirds, call-outs; a frame with no fill is transparent; enter and exit are none, fade, slide_up, slide_down, slide_left, slide_right, pop or zoom, taking fade_ms each), {op:'remove_overlay', video, overlay}, {op:'set_overlay', video, overlay, prop:{name, value}} with name one of frame, start_ms, duration_ms, fade_ms, enter, exit, ease, {op:'animate', video, node, prop, keys:[{at_ms, value, ease?}]} (keyframes one number of a node over the video's time, at_ms from the start of the video, strictly increasing; the value holds before the first key and after the last; prop one of x, y, width, height, opacity, corner_radius, rotate_x, rotate_y, rotate_z, zoom, fov, light_angle, shadow, glare, screen_scroll; keys:[] removes it; animate_preset writes these for you). Eases: linear, ease_in, ease_out, ease_in_out, back (overshoots) or spring (overshoots and settles). Colors are '#rrggbb', '#rrggbbaa' or '{token.name}'. Template variables (see list_variables): {op:'declare_variable', variable:{id, kind, label, description?, portrays?, default, enum_values?, binds?:[{node, prop}]}} with kind one of string, number, color, bool, enum, image, font (enum_values only for enum) and portrays one of subject_name, subject_domain, subject_tagline, subject_logo, host_identity, {op:'set_variable', id, prop:{name, value}} with name one of label, description, portrays, default, enum_values, binds, {op:'remove_variable', id}, {op:'set_variable_value', id, value} (writes every bound node property through the same validated path as set, so it is attributed and undoable). Ids are minted for you; the result lists the created ids. A checkpoint is taken before your first change in a turn so the user can restore it. Set dry_run true to check the operations without applying them: you get ok, what each operation would do, the node ids that would change, and for a rejection a stable code (E_NODE_NOT_FOUND, E_OUT_OF_BOUNDS, E_LIMIT, ...) with a hint; nothing is changed, checkpointed or saved. An applied result has stage 'applied', the checkpoint taken before the turn, the transaction, the changed ids and whether the project was saved.

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
- `seed` (integer) Another seed gives another variation (noise, and the notes of a stinger). Default 1.
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

Draw every frame of a video and write it as an H.264 MP4 to exports/<file>. Needs ffmpeg on the user's computer (the error says what to install). This can take a while.

Arguments:

- `file` (string, required) File name ending in .mp4, for example preview.mp4.
- `locale` (string) A language from list_locales: text nodes show that language.
- `preset` (string) A preset id from list_video_presets: the video is checked against the store's rules and nothing is written if it breaks one.
- `video` (string, required) Id of a video from list_videos.

## `validate_export`

Check exported PNGs in exports/ as one set against a store preset.

Read-only.

Arguments:

- `files` (array, required) 
- `preset` (string, required) A preset id.

## `animate_preset`

Animate a node with a named move, measured from its designed pose, as keyframes you can then inspect in list_videos and change with apply_ops animate. 3D device moves: spin (a full turn; strength 2 is two), turntable (slow side to side), swing (away one way, then the other, back), flip_in (turns from its back to face the viewer, springing), hero_reveal (starts flat and square, settles into the designed tilt and turn), push_in and pull_back (camera closer or settling back), orbit (the view swings around the device), scroll_screen (scrolls a long capture top to bottom). Any node: rise_in, drop_in, float (gentle drift; a 3D device also sways), fade_in, fade_out, slide_in_left, slide_in_right. Keys already inside start_ms..start_ms+duration_ms for the same properties are replaced and the rest kept, so moves chain. Times are from the start of the video; the node should be in a frame the video shows at that time.

Arguments:

- `duration_ms` (integer, required) 
- `ease` (string) Overrides the preset's own ease.
- `node` (string, required) Id of the node to move (a device3d for the 3D moves).
- `preset` (string, required) 
- `start_ms` (integer, required) 
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
null), `image`. Not every property applies to every kind of node; a rejection says which.

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
so use frames of the video's shape. A transition (`cut`, `fade`, `slide_left`, `slide_right`)
overlaps the end of the clip before it, so it makes the video shorter by its length; the first
clip's transition is not shown. `motion` zooms slowly from `scale_from` to `scale_to` (1 to 3) over
the clip. Change a clip with `set_clip` (`duration_ms`, `frame`, `transition`, `motion`), reorder
with `move_clip`, remove with `remove_clip`. A frame that a clip shows cannot be deleted until the
clip is removed. Look at a moment with `render_video_frame` (try the middle of a transition) and
write the file with `export_video`, which needs `ffmpeg` on the user's computer and tells you if it
is missing. For the App Store pass `preset` to `export_video` (see `list_video_presets`): sizes, 15 to 30
seconds and 30 fps are checked before anything is encoded, and the preset says what it does not
check (the audio track, the H.264 profile).

Sound: a video can carry one sound track under its picture, a file in the project's `media/`
folder. `{"op": "set_video", "video": "v_20", "prop": {"name": "audio", "value": {"file":
"media/song.mp3", "start_ms": 0, "gain": 1.0, "fade_in_ms": 500, "fade_out_ms": 500}}}` sets it
(`gain` is 0 to 2, the fades are milliseconds; `"value": null` takes the track away). The track
never makes the video longer: it is cut at the end. To find music or sound effects, use
`make_sound` (synthesized in the project: click, pop, tick, whoosh, swoosh_up, swoosh_down, riser,
hit, chime, drone, beat, stinger; no licence, but no music), `search_sounds` (Freesound; it needs
the user's token, and the answer says so if there is none), `import_sound` to bring one into
`media/`, or `import_file` with a sound file of the user's (`find_files` kind audio); `list_sounds`
shows them all. Then set the track. A Freesound sound keeps its
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
