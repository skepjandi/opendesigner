//! The asset tools against a local server: packs are installed, searched, imported from and
//! unloaded without ever touching the real network or the real config folder.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};

use flate2::Compression;
use flate2::write::GzEncoder;
use od_agent::assets::{packs, tools};
use od_agent::tools::Content;
use serde_json::{Value, json};

const ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\"><path d=\"M3 12l9-9 9 9v9H3z\"/></svg>";

/// A gzipped tar of `(path, bytes)`.
fn tgz(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, data) in files {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[156] = b'0';
        tar.extend_from_slice(&h);
        tar.extend_from_slice(data);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.extend_from_slice(&[0; 1024]);
    let mut g = GzEncoder::new(Vec::new(), Compression::default());
    g.write_all(&tar).unwrap();
    g.finish().unwrap()
}

/// A zip of stored (uncompressed) files.
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let offset = out.len() as u32;
        let size = (data.len() as u32).to_le_bytes();
        let nlen = (name.len() as u16).to_le_bytes();
        out.extend_from_slice(&[0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&size);
        out.extend_from_slice(&size);
        out.extend_from_slice(&nlen);
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&[
            0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        central.extend_from_slice(&size);
        central.extend_from_slice(&size);
        central.extend_from_slice(&nlen);
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let at = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&[0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

struct Mock {
    base: String,
    seen: Arc<Mutex<Vec<String>>>,
}

fn start(files: Vec<(&'static str, Vec<u8>)>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = seen.clone();
    let files = Arc::new(files);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (log, files) = (log.clone(), files.clone());
            std::thread::spawn(move || serve(stream, &log, &files));
        }
    });
    Mock { base, seen }
}

fn serve(mut stream: TcpStream, log: &Mutex<Vec<String>>, files: &[(&'static str, Vec<u8>)]) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).unwrap();
        if h == "\r\n" || h.is_empty() {
            break;
        }
    }
    let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
    log.lock().unwrap().push(path.clone());
    let found = files.iter().find(|(p, _)| *p == path);
    let (status, body) = match found {
        Some((_, b)) => ("200 OK", b.clone()),
        None => ("404 Not Found", b"no".to_vec()),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

fn run(root: &Path, base: &str, project: &Path, name: &str, args: Value) -> Result<Value, String> {
    tools::call_in(root, Some(base), Some(project), name, args)
        .map(|o| match &o.content[0] {
            Content::Text(t) => serde_json::from_str(t).unwrap(),
            Content::Image { .. } => panic!("unexpected image"),
        })
        .map_err(|e| e.0)
}

fn png_width(path: &Path) -> u32 {
    let b = std::fs::read(path).unwrap();
    u32::from_be_bytes(b[16..20].try_into().unwrap())
}

#[test]
fn a_pack_is_requested_approved_installed_searched_imported_and_unloaded() {
    let mock = start(vec![
        (
            "/tabler-icons.tgz",
            tgz(&[
                ("package/icons/outline/home.svg", ICON.as_bytes()),
                ("package/icons/filled/home.svg", ICON.as_bytes()),
                ("package/LICENSE", b"MIT License"),
            ]),
        ),
        (
            "/kenney-interface-sounds.zip",
            zip(&[
                ("Audio/click_001.ogg", b"OggS-fake"),
                ("License.txt", b"CC0"),
            ]),
        ),
    ]);
    let cache = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (root, base, dir) = (cache.path(), mock.base.as_str(), project.path());

    // nothing installed: a search of the packs finds no icon by that name
    assert_eq!(
        run(
            root,
            base,
            dir,
            "search_assets",
            json!({"query": "home", "kind": "icon"})
        )
        .unwrap()["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["id"].as_str().unwrap().starts_with("tabler"))
            .count(),
        0
    );

    // the first call only asks
    let asked = run(
        root,
        base,
        dir,
        "install_asset_pack",
        json!({"id": "tabler-icons"}),
    )
    .unwrap();
    assert_eq!(asked["waiting_for_user"], true);
    assert_eq!(packs::requests(root), ["tabler-icons"]);
    assert!(
        mock.seen.lock().unwrap().is_empty(),
        "nothing is downloaded before the user agrees"
    );
    let listed = run(root, base, dir, "list_asset_packs", json!({})).unwrap();
    let state = |v: &Value, id: &str| {
        v["packs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()["state"]
            .clone()
    };
    assert_eq!(state(&listed, "tabler-icons"), "requested");

    // the user approves; the next call downloads
    packs::approve(root, "tabler-icons").unwrap();
    let done = run(
        root,
        base,
        dir,
        "install_asset_pack",
        json!({"id": "tabler-icons"}),
    )
    .unwrap();
    assert_eq!(done["installed"], true);
    assert_eq!(done["files"], 1, "only the outline icons are kept");
    assert_eq!(mock.seen.lock().unwrap().len(), 1);
    assert_eq!(done["sha256"].as_str().unwrap().len(), 64);
    assert!(packs::requests(root).is_empty() && !packs::is_approved(root, "tabler-icons"));

    // size accounting: the number reported is the file on disk
    let on_disk = std::fs::metadata(root.join("tabler-icons/pack.oda"))
        .unwrap()
        .len();
    assert_eq!(done["stored_bytes"], on_disk);
    let listed = run(root, base, dir, "list_asset_packs", json!({})).unwrap();
    assert_eq!(listed["installed_bytes"], on_disk);
    assert_eq!(state(&listed, "tabler-icons"), "installed");

    // search finds it; import recolors and sizes it and writes the sidecar
    let found = run(
        root,
        base,
        dir,
        "search_assets",
        json!({"query": "home", "kind": "icon"}),
    )
    .unwrap();
    let id = "tabler-icons:icons/outline/home.svg";
    assert!(
        found["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == id)
    );
    let placed = run(
        root,
        base,
        dir,
        "import_asset",
        json!({"id": id, "width": 64, "color": "#ff0000", "name": "home-red"}),
    )
    .unwrap();
    assert_eq!(placed["file"], "media/home-red.png");
    assert_eq!(placed["width"], 64);
    assert_eq!(png_width(&dir.join("media/home-red.png")), 64);
    let side: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("media/home-red.asset.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(side["licence"], "MIT");
    assert_eq!(side["attribution_required"], true);
    assert!(placed["attribution"].as_str().unwrap().contains("Tabler"));
    // a second import never overwrites
    let again = run(
        root,
        base,
        dir,
        "import_asset",
        json!({"id": id, "width": 32, "name": "home-red"}),
    )
    .unwrap();
    assert_eq!(again["file"], "media/home-red-2.png");

    // a sound pack is copied as is, with no credit needed
    packs::approve(root, "kenney-interface-sounds").unwrap();
    run(
        root,
        base,
        dir,
        "install_asset_pack",
        json!({"id": "kenney-interface-sounds"}),
    )
    .unwrap();
    let sound = run(
        root,
        base,
        dir,
        "import_asset",
        json!({"id": "kenney-interface-sounds:Audio/click_001.ogg"}),
    )
    .unwrap();
    assert_eq!(sound["file"], "media/click_001.ogg");
    assert_eq!(sound["attribution_required"], false);
    assert_eq!(
        std::fs::read(dir.join("media/click_001.ogg")).unwrap(),
        b"OggS-fake"
    );

    // unloading frees the cache and the pack's files can no longer be imported
    let freed = run(
        root,
        base,
        dir,
        "unload_asset_pack",
        json!({"id": "tabler-icons"}),
    )
    .unwrap();
    assert_eq!(freed["freed_bytes"], on_disk);
    assert!(!root.join("tabler-icons").exists());
    assert!(
        run(root, base, dir, "import_asset", json!({"id": id}))
            .unwrap_err()
            .contains("not installed")
    );
    assert!(
        dir.join("media/home-red.png").exists(),
        "what the project already holds stays"
    );
}

#[test]
fn a_failed_download_installs_nothing_and_keeps_the_approval() {
    let mock = start(vec![(
        "/heroicons.tgz",
        tgz(&[("package/README.md", b"no icons here")]),
    )]);
    let cache = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (root, base, dir) = (cache.path(), mock.base.as_str(), project.path());
    packs::approve(root, "iconoir").unwrap();
    let e = run(
        root,
        base,
        dir,
        "install_asset_pack",
        json!({"id": "iconoir"}),
    )
    .unwrap_err();
    assert!(e.contains("404"), "{e}");
    packs::approve(root, "heroicons").unwrap();
    let e = run(
        root,
        base,
        dir,
        "install_asset_pack",
        json!({"id": "heroicons"}),
    )
    .unwrap_err();
    assert!(e.contains("none of the files"), "{e}");
    assert!(packs::installed_ids(root).is_empty());
    assert!(packs::is_approved(root, "iconoir"));
    assert!(run(root, base, dir, "install_asset_pack", json!({"id": "nope"})).is_err());
}

#[test]
fn bundled_icons_and_generated_assets_land_in_the_project_with_sidecars() {
    let cache = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (root, dir) = (cache.path(), project.path());
    let icon = run(
        root,
        "http://127.0.0.1:1",
        dir,
        "import_asset",
        json!({"id": "lucide:camera", "height": 96, "color": "#112233"}),
    )
    .unwrap();
    assert_eq!(icon["file"], "media/camera.png");
    assert_eq!(icon["attribution_required"], true);
    assert!(icon["attribution"].as_str().unwrap().contains("ISC"));
    assert_eq!(png_width(&dir.join("media/camera.png")), 96);

    let args = json!({"generator": "blob", "width": 300, "height": 200, "colors": ["#ff8800", "#cc2200"], "seed": 42});
    let mut args_a = args.clone();
    args_a["name"] = json!("a");
    let mut args_b = args.clone();
    args_b["name"] = json!("b");
    let a = run(root, "http://127.0.0.1:1", dir, "make_asset", args_a).unwrap();
    let b = run(root, "http://127.0.0.1:1", dir, "make_asset", args_b).unwrap();
    assert_eq!(
        (a["width"].clone(), a["height"].clone()),
        (json!(300), json!(200))
    );
    assert_eq!(
        std::fs::read(dir.join("media/a.png")).unwrap(),
        std::fs::read(dir.join("media/b.png")).unwrap(),
        "same seed, same picture"
    );
    args_b_seed(root, dir);
    let side: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("media/a.asset.json")).unwrap())
            .unwrap();
    assert_eq!(side["recipe"]["seed"], 42);
    assert_eq!(side["attribution_required"], false);
    assert_eq!(b["file"], "media/b.png");

    // errors say what to do
    let e = run(
        root,
        "http://127.0.0.1:1",
        dir,
        "make_asset",
        json!({"generator": "blob", "width": 10, "height": 10, "colors": ["red"]}),
    )
    .unwrap_err();
    assert!(e.contains("hex"));
    let e = run(
        root,
        "http://127.0.0.1:1",
        dir,
        "search_assets",
        json!({"query": "x", "kind": "weird"}),
    )
    .unwrap_err();
    assert!(e.contains("kind is one of"));
    let sources = run(
        root,
        "http://127.0.0.1:1",
        dir,
        "list_asset_sources",
        json!({}),
    )
    .unwrap();
    assert!(sources["packs"]["packs"].as_array().unwrap().len() >= 10);
    assert!(sources["external_mcp_servers"].as_array().unwrap().len() == 3);
}

fn args_b_seed(root: &Path, dir: &Path) {
    let make = |seed: u64, name: &str| {
        run(
            root,
            "http://127.0.0.1:1",
            dir,
            "make_asset",
            json!({"generator": "aurora", "width": 120, "height": 80, "seed": seed, "name": name}),
        )
        .unwrap();
        std::fs::read(dir.join(format!("media/{name}.png"))).unwrap()
    };
    assert_ne!(
        make(1, "s1"),
        make(2, "s2"),
        "another seed, another picture"
    );
}

/// Reaches the real kenney.nl and registry.npmjs.org. Run by hand:
/// `cargo test -p opendesigner-agent --test assets -- --ignored --nocapture`.
#[test]
#[ignore = "uses the network"]
fn real_downloads_work_by_hand() {
    let cache = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (root, dir) = (cache.path(), project.path());
    for id in [
        "kenney-interface-sounds",
        "kenney-board-game-icons",
        "heroicons",
    ] {
        packs::approve(root, id).unwrap();
        let done = tools::call_in(
            root,
            None,
            Some(dir),
            "install_asset_pack",
            json!({"id": id}),
        )
        .unwrap();
        let Content::Text(t) = &done.content[0] else {
            panic!()
        };
        println!("{id}: {t}");
    }
    let hits = tools::call_in(
        root,
        None,
        Some(dir),
        "search_assets",
        json!({"query": "bell", "limit": 5}),
    )
    .unwrap();
    let Content::Text(t) = &hits.content[0] else {
        panic!()
    };
    println!("{t}");
    let v: Value = serde_json::from_str(t).unwrap();
    let id = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"].as_str().unwrap().starts_with("heroicons"))
        .unwrap()["id"]
        .clone();
    let placed = tools::call_in(
        root,
        None,
        Some(dir),
        "import_asset",
        json!({"id": id, "width": 128, "color": "#336699"}),
    )
    .unwrap();
    let Content::Text(t) = &placed.content[0] else {
        panic!()
    };
    println!("{t}");
}
