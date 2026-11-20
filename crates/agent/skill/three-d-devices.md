# 3D devices and motion: a phone, a tablet, a laptop, a monitor that you can turn

A `device3d` node is a device drawn in 3D inside its own box: a phone, a tablet, a laptop or a
monitor, with a capture (or a recording) on its screen. It is the tool for store screenshots that
should feel like a product shot rather than a pasted rectangle, and for the video moves that make
an app preview feel alive.

Read `video-direction` and `motion-playbook` for the rest of the video craft; this page is about
the device, the picture on its screen and the movement.

## What the node is

- **The box is the viewport, not the device.** The device is drawn inside the node's box; the box
  is as big as you want the shot to be. Zoom 1 fits the device with a margin whatever its turn,
  so a spin never runs out of the box.
- **Models**: `phone` (pill cut-out at the top, camera island on the back), `phone_punch_hole`
  (round camera, camera bar on the back), `tablet` (4:3), `laptop` (lid and keyboard, 16:10),
  `monitor` (panel on a stand, 16:9). They are generic shapes with typical proportions, not any
  manufacturer's artwork.
- **The screen takes a capture**: the node's `image` (a `media/` path from `list_media`) with
  `image_fit` (`cover` by default, `contain` or `fill`). A capture taller than the screen is
  centred; `screen_scroll` (0 = its top, 1 = its bottom) puts a long page where you want it, and
  keyframing that property scrolls a web page inside the screen.
- **A recording on the screen**: `screen_footage` with `{"file": "media/x.mov", "in_ms": 0}` plays
  while the frame is shown. In a still it shows the picture at `in_ms`.
- **The body**: the node's `fill` is the finish (a dark graphite by default). The light comes from
  `light_angle` (degrees around the device: 0 from above, 90 from the right), the shadow under it
  from `shadow` (0 to 1) and the reflection on the glass from `glare` (0 to 1).

Turning: `rotate_x` (tilt; positive leans the top away from the viewer), `rotate_y` (turn;
positive turns the right edge away, so you see the device's left side), `rotate_z` (roll;
positive is clockwise). `fov` is the lens: 30 is a natural product shot, 8 to 15 is nearly flat
and flattering, above 50 starts to distort.

## The pose that reads as a product shot

- **Turn 15 to 25 degrees** (`rotate_y` between -25 and -15 for a phone turned to its right) and
  **tilt 5 to 12 degrees** (`rotate_x` positive). Flat-on is the flat picture you came to avoid.
- **Keep the device off the edge.** The model fits the box with a margin at any turn; put the
  device where the text can breathe, not where it touches the safe zone.
- **One light, one shadow.** The default light (from above and to the left) suits a dark
  background; a bright background wants the light from the other side (`light_angle` 120 to 160).
- **The screen is the product.** A capture with its own status bar and rounded corners looks
  better on a device than cropped to the bezel; keep the whole capture if you can.
- **Do not fake a device with a rectangle.** If it is a phone, use a phone.

## Getting a capture into the screen

You can capture the user's own app; ask before you take over their simulator, and never upload
anything anywhere.

- **iOS simulator** (macOS, Xcode installed):
  - clean status bar: `xcrun simctl status_bar booted override --time 9:41 --batteryState charged
    --batteryLevel 100 --wifiBars 3 --cellularBars 4` (clear it afterwards with
    `xcrun simctl status_bar booted clear`);
  - still: `xcrun simctl io booted screenshot /tmp/shot.png`;
  - recording: `xcrun simctl io booted recordVideo --codec=h264 --force /tmp/rec.mov` and stop it
    with Ctrl-C.
- **Android emulator**: demo mode for a clean bar
  (`adb shell settings put global policy_control immersive.full=*`), still
  `adb exec-out screencap -p > /tmp/shot.png`, recording `adb shell screenrecord /sdcard/rec.mp4`.
- **Web**: the agent's own browser tool if it has one, or `chrome --headless --screenshot` for a
  page; take the whole page (`--window-size` plus a full-page capture) so the screen can scroll.
- Then bring the file in with `import_file` (kind image or video) and set it: `image` on the
  device node, or `screen_footage` with the recording's `media/` path.
- If the user has no capture, ask for one or use what is already in `media/` (`list_media`); never
  invent a screenshot that pretends to be the app.

## Movement

`animate_preset` writes the keyframes for a named move, measured from the device's designed pose:
`spin` (a full turn; `strength` 2 is two), `turntable` (slow side to side), `swing` (away one way,
then the other, back), `flip_in` (turns from its back and springs to face the viewer),
`hero_reveal` (starts flat and square, settles into the designed tilt and turn), `push_in` and
`pull_back` (the camera moves), `orbit` (the view swings around the device), `scroll_screen`
(scrolls a long capture). `rise_in`, `drop_in`, `float`, `fade_in`, `fade_out`, `slide_in_left`
and `slide_in_right` work on any node.

- **Times are from the start of the video.** A move that starts at 0 and lasts 1.2 s lands while
  its clip is on screen.
- **Chain, do not stack.** A reveal that ends at 1.2 s and a swing that starts at 2 s keep their
  own keys; a second move on the same property replaces only the keys inside its own stretch.
- **One move at a time.** A device that spins and slides and zooms at once reads as noise.
- **Keyframes by hand** when a preset is not enough:
  `{"op": "animate", "video": ..., "node": ..., "prop": "rotate_y", "keys": [{"at_ms": 0,
  "value": -20, "ease": "ease_in_out"}, {"at_ms": 1200, "value": 20}]}`. The value holds before
  the first key and after the last; `keys: []` removes the animation.
- **Eases**: `linear` (only for a continuous spin), `ease_in`, `ease_out`, `ease_in_out`,
  `back` (a small overshoot), `spring` (an overshoot that settles: an arrival with weight).
- **Motion blur** (`set_video` with `motion_blur`, 2 to 16 pictures) smears fast movement the way
  a real shutter does. Use it on a spin or a fast swipe, never on a slow reveal.

## Checking the result

- `render_preview` on the frame, and `render_video_frame` on the video at the moments that matter:
  the first frame, the middle of the move, the frame it settles on.
- `video_contact_sheet` shows several moments at once: the way to see whether a move has an arc,
  whether it settles, and whether the device ever leaves the box.
- `list_videos` (`story`) says what moves, through which values, with which ease. Read it: a story
  that reads in sentences is a video that works.
- Look for: the device clipped by the frame, the screen showing the wrong part of the capture, a
  shadow that fights the light, a reflection that hides the screen, and a move that ends before
  the viewer has read it.
