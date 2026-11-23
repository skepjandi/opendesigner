#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The pixel-art game-asset tools through the MCP handler: `export_atlas`, `quantize_image` and
//! the `pixel_art` option of the exports. Everything is checked on the pixels and the files.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_core::Workspace;
use serde_json::{Value, json};

struct Fixture {
    handler: Handler,
    session: String,
    root: String,
    dir: PathBuf,
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("game.od");
    let ws = Workspace::create(&dir, "Game").unwrap();
    let root = ws.session.document().pages[0].root.id.to_string();
    let handler = Handler::new(Arc::new(Mutex::new(ws)));
    let out = handler.handle(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"claude-code","version":"1"}}}),
    );
    Fixture {
        handler,
        session: out.session_id.unwrap(),
        root,
        dir,
        _tmp: tmp,
    }
}

impl Fixture {
    fn tool(&self, name: &str, args: Value) -> (bool, String) {
        let out = self.handler.handle(
            Some(&self.session),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
                   "params":{"name": name, "arguments": args}}),
        );
        let result = &out.body.unwrap()["result"];
        (
            result["isError"].as_bool().unwrap(),
            result["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        )
    }

    fn ok(&self, name: &str, args: Value) -> Value {
        let (err, text) = self.tool(name, args);
        assert!(!err, "{name} failed: {text}");
        serde_json::from_str(&text).unwrap()
    }

    fn err(&self, name: &str, args: Value) -> String {
        let (err, text) = self.tool(name, args);
        assert!(err, "{name} should fail but said: {text}");
        text
    }

    /// Creates a frame (no fill: transparent) with `children` and returns its id.
    fn frame(&self, name: &str, w: u32, h: u32, children: Vec<Value>) -> String {
        let made = self.ok(
            "apply_ops",
            json!({"ops": [{"op": "create", "parent": self.root, "node": {
                "kind": "frame", "name": name, "width": w, "height": h,
                "children": children}}]}),
        );
        made["created"][0].as_str().unwrap().to_string()
    }

    fn png(&self, rel: &str) -> (u32, u32, Vec<u8>) {
        od_render::decode_rgba(&self.dir.join(rel), 64 * 1024 * 1024).unwrap()
    }

    fn pixel(&self, rel: &str, x: u32, y: u32) -> [u8; 4] {
        let (w, _, data) = self.png(rel);
        let i = (y * w + x) as usize * 4;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }
}

fn rect(x: u32, y: u32, w: u32, h: u32, fill: &str) -> Value {
    json!({"kind": "rect", "x": x, "y": y, "width": w, "height": h, "fill": fill})
}

/// Twelve 48 px cells (3 columns by 4 rows), each filled with its own colour.
fn sheet_children() -> Vec<Value> {
    (0..12u32)
        .map(|i| {
            let (col, row) = (i % 3, i / 3);
            rect(
                col * 48,
                row * 48,
                48,
                48,
                &format!("#{:02x}{:02x}40", i * 20, 255 - i * 20),
            )
        })
        .collect()
}

fn cell_colour(i: u32) -> [u8; 4] {
    [(i * 20) as u8, (255 - i * 20) as u8, 0x40, 255]
}

#[test]
fn the_tools_are_listed_with_schemas() {
    let f = fixture();
    let out = f.handler.handle(
        Some(&f.session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let tools = out.body.unwrap()["result"]["tools"].clone();
    let find = |name: &str| {
        tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} is not listed"))
    };
    let atlas = find("export_atlas");
    assert_eq!(atlas["inputSchema"]["required"], json!(["name"]));
    for key in [
        "frame",
        "cols",
        "rows",
        "cell_w",
        "cell_h",
        "frames",
        "names",
        "pivot",
        "nine_slice",
        "pixel_art",
    ] {
        assert!(atlas["inputSchema"]["properties"][key].is_object(), "{key}");
    }
    let quantize = find("quantize_image");
    assert_eq!(
        quantize["inputSchema"]["required"],
        json!(["file", "palette"])
    );
    for name in ["render_preview", "export", "export_set", "export_panorama"] {
        assert!(
            find(name)["inputSchema"]["properties"]["pixel_art"].is_object(),
            "{name} takes pixel_art"
        );
    }
}

#[test]
fn a_character_sheet_is_cut_into_a_grid_and_described_by_its_manifest() {
    let f = fixture();
    let frame = f.frame("Hero", 144, 192, sheet_children());
    let names: Vec<String> = ["down", "left", "right", "up"]
        .iter()
        .flat_map(|d| (0..3).map(move |n| format!("walk_{d}_{n}")))
        .collect();
    let out = f.ok(
        "export_atlas",
        json!({"name": "hero", "frame": frame, "cols": 3, "rows": 4, "cell_w": 48, "cell_h": 48,
               "names": names, "pivot": {"x": 24, "y": 44}, "pixel_art": true}),
    );
    assert_eq!(out["image"], "exports/hero.png");
    assert_eq!(out["manifest_path"], "exports/hero.json");

    let (w, h, _) = f.png("exports/hero.png");
    assert_eq!((w, h), (144, 192));
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(f.dir.join("exports/hero.json")).unwrap())
            .unwrap();
    assert_eq!(manifest, out["manifest"]);
    assert_eq!(manifest["image"], "hero.png");
    assert_eq!(manifest["size"], json!({"w": 144, "h": 192}));
    assert!(
        manifest["generator"]
            .as_str()
            .unwrap()
            .starts_with("opendesigner ")
    );
    let created = manifest["created"].as_str().unwrap();
    assert_eq!(created.len(), 20, "{created}");
    assert!(created.ends_with('Z') && created.as_bytes()[10] == b'T');

    let frames = manifest["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 12);
    for (i, frame) in frames.iter().enumerate() {
        let i = i as u32;
        let (col, row) = (i % 3, i / 3);
        assert_eq!(frame["name"], names[i as usize]);
        assert_eq!(
            [
                frame["x"].as_u64(),
                frame["y"].as_u64(),
                frame["w"].as_u64(),
                frame["h"].as_u64()
            ],
            [
                Some(u64::from(col * 48)),
                Some(u64::from(row * 48)),
                Some(48),
                Some(48)
            ],
            "frame {i}"
        );
        assert_eq!(frame["pivot"], json!({"x": 24.0, "y": 44.0}));
        assert!(frame.get("nine_slice").is_none());
        // the pixels agree with the manifest: the centre of the cell is the colour it was drawn in
        assert_eq!(
            f.pixel("exports/hero.png", col * 48 + 24, row * 48 + 24),
            cell_colour(i)
        );
    }
    // every rect lies inside the PNG and no two overlap
    for (i, a) in frames.iter().enumerate() {
        let (ax, ay) = (a["x"].as_u64().unwrap(), a["y"].as_u64().unwrap());
        assert!(ax + a["w"].as_u64().unwrap() <= 144 && ay + a["h"].as_u64().unwrap() <= 192);
        for b in &frames[i + 1..] {
            let (bx, by) = (b["x"].as_u64().unwrap(), b["y"].as_u64().unwrap());
            assert!(ax != bx || ay != by);
        }
    }
    let palette: BTreeSet<&str> = manifest["palette"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(palette.len(), 12, "{palette:?}");
    assert!(!palette.contains("#003240") && palette.contains("#00ff40"));
}

#[test]
fn a_grid_is_scaled_before_it_is_cut() {
    let f = fixture();
    let frame = f.frame(
        "Icons",
        64,
        32,
        vec![
            rect(0, 0, 32, 32, "#ff0000"),
            rect(32, 0, 32, 32, "#0000ff"),
        ],
    );
    let out = f.ok(
        "export_atlas",
        json!({"name": "icons2x", "frame": frame, "cols": 2, "rows": 1,
               "cell_w": 64, "cell_h": 64, "scale": 2, "pixel_art": true}),
    );
    assert_eq!(out["width"], 128);
    assert_eq!(out["height"], 64);
    let frames = out["manifest"]["frames"].as_array().unwrap();
    assert_eq!(frames[1]["x"], 64);
    assert_eq!(frames[1]["name"], "icons2x-01");
    assert_eq!(f.pixel("exports/icons2x.png", 96, 32), [0, 0, 255, 255]);
}

#[test]
fn frames_are_packed_into_one_sheet() {
    let f = fixture();
    let a = f.frame("sword", 32, 32, vec![rect(0, 0, 32, 32, "#ff0000")]);
    let b = f.frame("potion", 32, 32, vec![rect(0, 0, 32, 32, "#00ff00")]);
    let c = f.frame("key", 16, 24, vec![rect(0, 0, 16, 24, "#0000ff")]);
    let out = f.ok(
        "export_atlas",
        json!({"name": "items", "frames": [a, b, c], "columns": 2, "padding": 2,
               "nine_slice": {"l": 4, "t": 4, "r": 4, "b": 4},
               "frame_meta": {"key": {"pivot": {"x": 8, "y": 24}}}, "pixel_art": true}),
    );
    // cells are 32x32 plus 2 px around each: 2 columns, 2 rows
    assert_eq!(
        (out["width"].as_u64(), out["height"].as_u64()),
        (Some(2 + 2 * 34), Some(2 + 2 * 34))
    );
    let frames = out["manifest"]["frames"].as_array().unwrap();
    let rects: Vec<(u64, u64, u64, u64)> = frames
        .iter()
        .map(|fr| {
            (
                fr["x"].as_u64().unwrap(),
                fr["y"].as_u64().unwrap(),
                fr["w"].as_u64().unwrap(),
                fr["h"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(rects, [(2, 2, 32, 32), (36, 2, 32, 32), (2, 36, 16, 24)]);
    let names: Vec<&str> = frames
        .iter()
        .map(|fr| fr["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["sword", "potion", "key"]);
    assert_eq!(
        frames[0]["nine_slice"],
        json!({"l": 4, "t": 4, "r": 4, "b": 4})
    );
    assert_eq!(frames[2]["pivot"], json!({"x": 8.0, "y": 24.0}));
    assert!(frames[0].get("pivot").is_none());
    assert_eq!(f.pixel("exports/items.png", 10, 10), [255, 0, 0, 255]);
    assert_eq!(f.pixel("exports/items.png", 40, 10), [0, 255, 0, 255]);
    assert_eq!(f.pixel("exports/items.png", 5, 40), [0, 0, 255, 255]);
    assert_eq!(
        f.pixel("exports/items.png", 0, 0)[3],
        0,
        "padding is transparent"
    );
    assert_eq!(
        f.pixel("exports/items.png", 30, 50)[3],
        0,
        "the rest of the cell is transparent"
    );
}

#[test]
fn a_bad_atlas_writes_nothing() {
    let f = fixture();
    let frame = f.frame("Hero", 144, 192, sheet_children());
    let exports = f.dir.join("exports");
    let nothing = || !exports.join("bad.png").exists() && !exports.join("bad.json").exists();
    let base =
        json!({"name": "bad", "frame": frame, "cols": 3, "rows": 4, "cell_w": 48, "cell_h": 48});
    let with = |key: &str, value: Value| {
        let mut v = base.clone();
        v[key] = value;
        v
    };

    let msg = f.err("export_atlas", with("cell_w", json!(40)));
    assert!(msg.contains("144x192") && msg.contains("120x192"), "{msg}");
    let msg = f.err("export_atlas", with("names", json!(["a", "b"])));
    assert!(
        msg.contains("2 entries") && msg.contains("12 cells"),
        "{msg}"
    );
    let dup: Vec<String> = (0..12).map(|i| format!("n{}", i % 11)).collect();
    let msg = f.err("export_atlas", with("names", json!(dup)));
    assert!(msg.contains("twice"), "{msg}");
    let msg = f.err(
        "export_atlas",
        with(
            "names",
            json!((0..12).map(|i| format!("a b{i}")).collect::<Vec<_>>()),
        ),
    );
    assert!(msg.contains("not a frame name"), "{msg}");
    let msg = f.err("export_atlas", with("pivot", json!({"x": 60, "y": 10})));
    assert!(msg.contains("pivot"), "{msg}");
    let msg = f.err(
        "export_atlas",
        with("nine_slice", json!({"l": 30, "t": 0, "r": 30, "b": 0})),
    );
    assert!(msg.contains("nine_slice"), "{msg}");
    let msg = f.err(
        "export_atlas",
        with("frame_meta", json!({"nobody": {"pivot": {"x": 1, "y": 1}}})),
    );
    assert!(msg.contains("nobody"), "{msg}");
    let msg = f.err("export_atlas", with("name", json!("../bad")));
    assert!(msg.contains("plain name"), "{msg}");
    assert!(nothing());

    let msg = f.err("export_atlas", json!({"name": "bad"}));
    assert!(msg.contains("frame"), "{msg}");
    let msg = f.err(
        "export_atlas",
        json!({"name": "bad", "frame": frame, "frames": [frame]}),
    );
    assert!(msg.contains("not both"), "{msg}");
    let msg = f.err(
        "export_atlas",
        json!({"name": "bad", "frame": frame, "cols": 3}),
    );
    assert!(msg.contains("cell_h"), "{msg}");
    assert!(nothing());
}

#[test]
fn a_half_pixel_shape_is_crisp_with_pixel_art_and_soft_without() {
    let f = fixture();
    let children = vec![
        json!({"kind": "rect", "x": 0.5, "y": 0.5, "width": 6, "height": 6, "fill": "#ff0000"}),
    ];
    let frame = f.frame("Edge", 8, 8, children);
    let colours = |file: &str| -> BTreeSet<[u8; 4]> {
        let (_, _, data) = f.png(file);
        data.chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect()
    };
    f.ok(
        "export",
        json!({"frame": frame, "file": "soft.png", "alpha": true}),
    );
    f.ok(
        "export",
        json!({"frame": frame, "file": "crisp.png", "alpha": true, "pixel_art": true}),
    );
    assert!(colours("exports/soft.png").len() > 2);
    let crisp = colours("exports/crisp.png");
    assert_eq!(crisp, BTreeSet::from([[0, 0, 0, 0], [255, 0, 0, 255]]));

    let set = f.ok(
        "export_set",
        json!({"frames": [frame], "prefix": "set", "alpha": true, "pixel_art": true, "scale": 3}),
    );
    assert_eq!(set["files"][0]["width"], 24);
    assert_eq!(colours("exports/set-01.png"), crisp);
}

#[test]
fn render_preview_accepts_pixel_art() {
    let f = fixture();
    let frame = f.frame("Edge", 8, 8, vec![rect(1, 1, 6, 6, "#ff0000")]);
    let (err, text) = f.tool(
        "render_preview",
        json!({"frame": frame, "scale": 2, "pixel_art": true}),
    );
    assert!(!err, "{text}");
}

/// A 4x2 picture: three solid colours, one translucent pixel, one near-transparent pixel and two
/// shades between palette colours.
fn write_sprite(dir: &std::path::Path, name: &str) -> Vec<[u8; 4]> {
    let px: Vec<[u8; 4]> = vec![
        [250, 10, 10, 255],
        [10, 240, 20, 255],
        [20, 20, 230, 255],
        [200, 60, 60, 255],
        [200, 200, 200, 255],
        [40, 40, 40, 255],
        [255, 255, 255, 40],
        [0, 0, 0, 0],
    ];
    let pixmap = od_render::pixmap_from_rgba(4, 2, &px.concat()).unwrap();
    std::fs::create_dir_all(dir.join("media")).unwrap();
    std::fs::write(
        dir.join("media").join(name),
        od_render::encode_png(&pixmap, true).unwrap(),
    )
    .unwrap();
    px
}

#[test]
fn quantize_snaps_every_pixel_to_the_palette() {
    let f = fixture();
    let original = write_sprite(&f.dir, "sprite.png");
    let before = std::fs::read(f.dir.join("media/sprite.png")).unwrap();
    f.ok(
        "apply_ops",
        json!({"ops": [{"op": "set_token", "name": "ink", "value": "#000000"}]}),
    );
    let out = f.ok(
        "quantize_image",
        json!({"file": "media/sprite.png",
               "palette": ["#ff0000", "{ink}", "#00ff00", "#0000ff", "#ffffff"],
               "dither": "none"}),
    );
    assert_eq!(out["file"], "media/sprite-q.png");
    assert_eq!(out["source"], "media/sprite.png");
    assert_eq!(
        (out["width"].as_u64(), out["height"].as_u64()),
        (Some(4), Some(2))
    );
    assert_eq!(out["palette_size"], 5);
    assert_eq!(out["transparent_pixels"], 2);
    assert_eq!(
        std::fs::read(f.dir.join("media/sprite.png")).unwrap(),
        before,
        "the original stays"
    );

    let palette: BTreeSet<[u8; 4]> = [
        [255, 0, 0, 255],
        [0, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ]
    .into();
    let (_, _, data) = f.png("media/sprite-q.png");
    let got: Vec<[u8; 4]> = data
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect();
    let opaque: BTreeSet<[u8; 4]> = got.iter().copied().filter(|p| p[3] == 255).collect();
    assert!(opaque.is_subset(&palette), "{opaque:?}");
    assert!(
        got.iter().all(|p| p[3] == 255 || *p == [0, 0, 0, 0]),
        "alpha is 0 or 255: {got:?}"
    );
    assert_eq!(
        got[..6],
        [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 0, 0, 255],
            [255, 255, 255, 255],
            [0, 0, 0, 255],
        ],
        "nearest in OKLab, from {original:?}"
    );
    assert_eq!(
        got[6],
        [0, 0, 0, 0],
        "a mostly transparent pixel is dropped"
    );
    assert_eq!(got[7], [0, 0, 0, 0]);
    let counted: u64 = out["colors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["pixels"].as_u64().unwrap())
        .sum();
    assert_eq!(counted, 6);
    assert_eq!(out["colors_after"], out["colors"].as_array().unwrap().len());
    assert_eq!(out["colors_before"], 6);

    // the same call again does not overwrite
    let again = f.ok(
        "quantize_image",
        json!({"file": "media/sprite.png", "palette": ["#ffffff", "#000000"]}),
    );
    assert_eq!(again["file"], "media/sprite-q-2.png");
    assert!(f.dir.join("media/sprite-q.png").exists());
}

#[test]
fn quantize_takes_tokens_a_style_pack_and_an_alpha_threshold() {
    let f = fixture();
    write_sprite(&f.dir, "sprite.png");
    f.ok(
        "apply_ops",
        json!({"ops": [{"op": "set_token", "name": "brand.gold", "value": "#ffcd75"}]}),
    );
    let out = f.ok(
        "quantize_image",
        json!({"file": "media/sprite.png", "palette": ["brand.gold", "#000000"], "alpha_threshold": 30}),
    );
    // alpha 40 is now kept
    assert_eq!(out["transparent_pixels"], 1);
    assert_eq!(f.pixel("media/sprite-q.png", 2, 1)[3], 255);

    let pack = f.ok("get_style_pack", json!({"id": "pixel-16"}));
    let out = f.ok(
        "quantize_image",
        json!({"file": "media/sprite.png", "palette": "pixel-16"}),
    );
    let swatches: BTreeSet<String> = pack["tokens"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_str().unwrap().to_lowercase())
        .collect();
    for c in out["colors"].as_array().unwrap() {
        let hex = c["color"].as_str().unwrap().to_lowercase();
        assert!(swatches.contains(&hex), "{hex} is not a colour of the pack");
    }
}

#[test]
fn quantize_refuses_what_it_cannot_do() {
    let f = fixture();
    write_sprite(&f.dir, "sprite.png");
    let media = f.dir.join("media");
    let files = || std::fs::read_dir(&media).unwrap().count();
    let n = files();
    assert!(
        f.err(
            "quantize_image",
            json!({"file": "media/sprite.png", "palette": "no-such-pack"})
        )
        .contains("style pack")
    );
    assert!(
        f.err(
            "quantize_image",
            json!({"file": "media/sprite.png", "palette": ["nope"]})
        )
        .contains("neither")
    );
    assert!(
        f.err(
            "quantize_image",
            json!({"file": "media/sprite.png", "palette": []})
        )
        .contains("1 to")
    );
    assert!(
        f.err(
            "quantize_image",
            json!({"file": "media/sprite.png", "palette": ["#fff0zz"]})
        )
        .contains("solid colour")
    );
    assert!(
        f.err(
            "quantize_image",
            json!({"file": "media/missing.png", "palette": ["#ffffff"]})
        )
        .len()
            > 5
    );
    assert!(
        f.err(
            "quantize_image",
            json!({"file": "../etc/passwd", "palette": ["#ffffff"]})
        )
        .len()
            > 5
    );
    assert!(
        f.err(
            "quantize_image",
            json!({"file": "media/sprite.png", "palette": ["#ffffff"], "dither": "floyd"})
        )
        .contains("dither")
    );
    assert_eq!(files(), n, "nothing was written");
}
