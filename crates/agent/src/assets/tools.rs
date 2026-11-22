//! The asset tools an agent sees. They are wired into `crate::tools` with one hook each for
//! the definitions and the dispatch; everything else lives here.

use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::catalog::{self, GENERATORS, LUCIDE, RECOMMENDED_SERVERS};
use super::generate::{self, Params};
use super::index::{self, Payload};
use super::project::{self, Sidecar};
use super::{Error, packs, raster};
use crate::tools::{Content, Output, ToolError};

const NAMES: &[&str] = &[
    "list_asset_sources",
    "search_assets",
    "make_asset",
    "import_asset",
    "list_asset_packs",
    "install_asset_pack",
    "unload_asset_pack",
];
const KINDS: &[&str] = &[
    "icon",
    "shape",
    "blob",
    "background",
    "pattern",
    "illustration",
    "font",
    "sound",
    "3d",
];

pub fn handles(name: &str) -> bool {
    NAMES.contains(&name)
}

fn tool(
    name: &str,
    description: &str,
    props: Value,
    required: &[&str],
    read_only: bool,
    open_world: bool,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": props, "required": required, "additionalProperties": false},
        "annotations": {"readOnlyHint": read_only, "openWorldHint": open_world},
    })
}

pub fn definitions() -> Vec<Value> {
    let text = |d: &str| json!({"type": "string", "description": d});
    let side = |d: &str| json!({"type": "integer", "minimum": 1, "maximum": raster::MAX_SIDE, "description": d});
    vec![
        tool(
            "list_asset_sources",
            "What assets exist and under which licence: the bundled Lucide icon set, the generators \
             make_asset runs, the packs install_asset_pack can download (with sizes and whether \
             installed), optional external MCP servers for 3D assets, and sources that are only a \
             link and why. Call it once before you draw an icon, blob, background or pattern by hand, \
             and at the start of every video (with list_media, list_footage and list_sounds).",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "search_assets",
            "Search icons, shapes, blobs, backgrounds, patterns, illustrations and sounds by name \
             and tag across the bundled icon set, the generators and every installed pack. Each \
             result has an id and says whether to use import_asset (a stored file) or make_asset \
             (a generator). Use this before drawing an icon or decoration by hand, and when planning \
             a video (sounds are listed too). Needs no network.",
            json!({
                "query": text("Words, e.g. \"arrow right\", \"camera\", \"click\". Empty lists everything of the kind."),
                "kind": {"type": "string", "enum": KINDS, "description": "Narrow to one kind. font and 3d have nothing built in; the answer says where to look."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100, "description": "How many results (default 20)."}
            }),
            &["query"],
            true,
            false,
        ),
        tool(
            "make_asset",
            "Generate a picture from a seed and your colors and save it as a PNG in media/ (with a \
             sidecar that holds the recipe). generator: blob (organic shape), gradient (linear or \
             radial), aurora (soft mesh-gradient clouds on a base color), grain (film/paper noise), \
             pattern (dots, lines, waves, grid, checker, zigzag, topo) or shape (circle, rounded-rect, \
             triangle, star, polygon, ring, burst). The same seed always gives the same picture. Pass \
             hex colors from list_tokens so it matches the design. Then use the returned file in an \
             image node. No licence conditions. For a video: make the background or texture the \
             video's size, put it in an image node of a frame, and use that frame as a clip or overlay.",
            json!({
                "generator": {"type": "string", "enum": ["blob", "gradient", "aurora", "grain", "pattern", "shape"]},
                "width": side("Pixels."),
                "height": side("Pixels."),
                "colors": {"type": "array", "items": {"type": "string"}, "maxItems": 8, "description": "Hex colors (#rrggbb). Meaning per generator: blob/shape one fill (two = gradient); gradient 2 to 8 stops; aurora the base color then the clouds; grain base then speck color; pattern background then ink."},
                "seed": {"type": "integer", "minimum": 0, "description": "Another seed, another picture. Default 0."},
                "variant": text("Which kind: pattern dots|lines|waves|grid|checker|zigzag|topo; shape circle|rounded-rect|triangle|star[-N]|polygon[-N]|ring|burst; gradient linear|radial; blob[-N points]."),
                "scale": {"type": "integer", "minimum": 1, "maximum": 1024, "description": "A pattern's cell size in pixels, or the grain's speck size (1 to 16)."},
                "name": text("File name without ending; a free one is chosen.")
            }),
            &["generator", "width", "height"],
            false,
            false,
        ),
        tool(
            "import_asset",
            "Put one asset from search_assets into the project: an icon or other SVG is recolored \
             and drawn as a crisp PNG at the size you ask for in media/; a PNG or sound is copied as \
             is. A sidecar next to it names the source and licence. Returns the media/ file and, when \
             the licence asks for it, the credit line you must show with the work (say it in your \
             closing message and keep it in the credits). Keep one icon set per design. For a video: \
             create an image node with this file as `image` in a clip frame or a transparent overlay \
             frame (apply_ops); an imported sound goes to set_video audio.",
            json!({
                "id": text("An id from search_assets, e.g. lucide:camera."),
                "width": side("Draw this wide (the icon is fitted inside, centered)."),
                "height": side("Draw this tall."),
                "color": text("Hex color to recolor an SVG icon (replaces currentColor or its fills); use a token color."),
                "name": text("File name without ending; a free one is chosen.")
            }),
            &["id"],
            false,
            false,
        ),
        tool(
            "list_asset_packs",
            "The downloadable packs (icon sets, Kenney CC0 packs, sounds): state, download size, \
             size on disk when installed, hash and licence, the cap on what packs may take, and any \
             pack an agent asked for that the user has not answered yet.",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "install_asset_pack",
            "Download a pack into the user's cache (not the project) so search_assets and \
             import_asset can use it. The user must agree first: the first call only asks (the \
             Connect tab shows the request with the size); tell the user, stop, and call again after \
             they approved. The download shows as a job. Install only what the design needs.",
            json!({"id": text("A pack id from list_asset_packs.")}),
            &["id"],
            false,
            true,
        ),
        tool(
            "unload_asset_pack",
            "Delete an installed pack from the user's cache to free its space. Files already \
             imported into projects stay. Do this when the work that needed the pack is done.",
            json!({"id": text("A pack id from list_asset_packs.")}),
            &["id"],
            false,
            false,
        ),
    ]
}

fn args<T: DeserializeOwned>(v: Value) -> Result<T, ToolError> {
    serde_json::from_value(v).map_err(|e| ToolError(format!("bad arguments: {e}")))
}

fn fail(e: Error) -> ToolError {
    ToolError(e.to_string())
}

fn out(v: &Value) -> Output {
    Output {
        content: vec![Content::Text(v.to_string())],
    }
}

/// Runs an asset tool against the project folder (if the project has one).
pub fn call(project: Option<&Path>, name: &str, a: Value) -> Result<Output, ToolError> {
    let root = packs::root().map_err(fail)?;
    call_in(&root, packs::base_override().as_deref(), project, name, a)
}

/// Whether `install_asset_pack` with these arguments will download now (the pack is approved and
/// not installed), so the caller can show it as a job.
pub fn will_download(a: &Value) -> Option<String> {
    let id = a.get("id")?.as_str()?;
    let root = packs::root().ok()?;
    let spec = catalog::pack(id)?;
    (packs::is_approved(&root, id) && !packs::installed_ids(&root).iter().any(|i| i == id))
        .then(|| format!("Downloading {}", spec.name))
}

/// [`call`] with the pack folder and the download address named (the tests).
pub fn call_in(
    root: &Path,
    base: Option<&str>,
    project: Option<&Path>,
    name: &str,
    a: Value,
) -> Result<Output, ToolError> {
    match name {
        "list_asset_sources" => Ok(out(&sources(root))),
        "search_assets" => search(root, args(a)?),
        "make_asset" => make(project, args(a)?),
        "import_asset" => import(root, project, args(a)?),
        "list_asset_packs" => Ok(out(&pack_list(root))),
        "install_asset_pack" => install(root, base, args(a)?),
        "unload_asset_pack" => {
            let a: IdArgs = args(a)?;
            let freed = packs::unload(root, &a.id).map_err(fail)?;
            Ok(out(&json!({"unloaded": a.id, "freed_bytes": freed})))
        }
        other => Err(ToolError(format!("no asset tool {other:?}"))),
    }
}

fn pack_list(root: &Path) -> Value {
    json!({
        "packs": packs::states(root),
        "installed_bytes": packs::total_bytes(root),
        "cap_bytes": packs::TOTAL_CAP_BYTES,
        "asked_for_by_agents": packs::requests(root),
    })
}

fn sources(root: &Path) -> Value {
    json!({
        "bundled": [{
            "id": LUCIDE.id, "name": LUCIDE.name, "kind": "icon", "version": LUCIDE.version,
            "licence": LUCIDE.licence, "licence_url": LUCIDE.licence_url,
            "about": "2,404 consistent line icons on a 24 px grid; import_asset recolors and sizes them",
        }],
        "generators": GENERATORS.iter().map(|(n, k, a)| json!({"id": n, "kind": k, "about": a, "use_with": "make_asset"})).collect::<Vec<_>>(),
        "packs": pack_list(root),
        "external_mcp_servers": RECOMMENDED_SERVERS.iter().map(|s| json!({
            "name": s.name, "command": format!("{} {}", s.command, s.args.join(" ")),
            "about": s.about, "licence": s.licence,
            "how": "the user adds it in the Connect tab (Recommended servers); OpenDesigner cannot place 3D models yet",
        })).collect::<Vec<_>>(),
        "link_only": catalog::LINK_ONLY.iter().map(|(n, u, why)| json!({"name": n, "url": u, "why": why})).collect::<Vec<_>>(),
        "rules": "Show the credit line import_asset returns whenever attribution_required is true. Never copy from link_only sources.",
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

fn search(root: &Path, a: SearchArgs) -> Result<Output, ToolError> {
    if let Some(k) = &a.kind
        && !KINDS.contains(&k.as_str())
    {
        return Err(ToolError(format!("kind is one of {}", KINDS.join(", "))));
    }
    let hits = index::search(
        root,
        &a.query,
        a.kind.as_deref(),
        a.limit.unwrap_or(20).clamp(1, 100),
    );
    let mut v = json!({"count": hits.len(), "results": hits});
    match a.kind.as_deref() {
        Some("3d") => {
            v["note"] = json!(
                "no 3D assets are built in and OpenDesigner cannot place 3D models yet; list_asset_sources names optional MCP servers that search them"
            )
        }
        Some("font") => {
            v["note"] = json!(
                "no font files are offered: text nodes name a font family installed on the machine (or the bundled one); pick a family there"
            )
        }
        Some("sound") if hits.is_empty() => {
            v["note"] = json!(
                "no sound pack is installed: list_asset_packs shows kenney-interface-sounds, kenney-impact-sounds, kenney-ui-audio, kenney-sci-fi-sounds, kenney-digital-audio, kenney-rpg-audio, kenney-music-jingles, kenney-casino-audio, kenney-voiceover-pack and kenney-voiceover-pack-fighter; install_asset_pack asks the user"
            )
        }
        _ if hits.is_empty() => {
            v["note"] = json!(
                "nothing found; try one plain word, drop the kind, or check list_asset_packs for packs you could install"
            )
        }
        _ => {}
    }
    Ok(out(&v))
}

fn project_dir(project: Option<&Path>) -> Result<&Path, ToolError> {
    project
        .ok_or_else(|| ToolError("the project has no folder, so nowhere to put the asset".into()))
}

fn check_color(c: &str) -> Result<(), ToolError> {
    if raster::is_hex_color(c) {
        Ok(())
    } else {
        Err(ToolError(format!(
            "{c:?} is not a hex color like #1a2b3c; take it from list_tokens"
        )))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MakeArgs {
    generator: String,
    width: u32,
    height: u32,
    #[serde(default)]
    colors: Vec<String>,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default)]
    scale: Option<u32>,
    #[serde(default)]
    name: Option<String>,
}

fn make(project: Option<&Path>, a: MakeArgs) -> Result<Output, ToolError> {
    let dir = project_dir(project)?;
    let seed = a.seed.unwrap_or(0);
    let recipe = json!({
        "generator": a.generator, "width": a.width, "height": a.height, "colors": a.colors,
        "seed": seed, "variant": a.variant, "scale": a.scale,
    });
    let params = Params {
        width: a.width,
        height: a.height,
        colors: a.colors,
        seed,
        variant: a.variant.clone(),
        scale: a.scale,
    };
    let svg = generate::svg(&a.generator, &params).map_err(fail)?;
    let (png, w, h) = raster::rasterize(&svg, Some(a.width), Some(a.height)).map_err(fail)?;
    let stem = a.name.unwrap_or_else(|| {
        format!(
            "{}{}-{seed}",
            a.generator,
            a.variant
                .as_deref()
                .map_or(String::new(), |v| format!("-{v}"))
        )
    });
    let sidecar = Sidecar {
        id: format!("generated:{}", a.generator),
        source: "OpenDesigner generator".into(),
        licence: "none (generated from a seed)".into(),
        licence_url: String::new(),
        attribution_required: false,
        attribution: String::new(),
        recipe: Some(recipe),
    };
    let placed = project::place(dir, &stem, "png", &png, &sidecar, Some((w, h))).map_err(fail)?;
    Ok(out(&json!({
        "file": placed.file, "sidecar": placed.sidecar, "width": w, "height": h,
        "attribution_required": false,
        "next": "Put it in an image node (set the node's media reference to this file).",
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportArgs {
    id: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

/// A PNG's size from its header.
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    let w = u32::from_be_bytes(data.get(16..20)?.try_into().ok()?);
    let h = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?);
    Some((w, h))
}

fn import(root: &Path, project: Option<&Path>, a: ImportArgs) -> Result<Output, ToolError> {
    let dir = project_dir(project)?;
    if let Some(c) = &a.color {
        check_color(c)?;
    }
    let r = index::resolve(root, &a.id).map_err(fail)?;
    let stem = a.name.clone().unwrap_or_else(|| r.name.clone());
    let sidecar = Sidecar {
        id: r.id.clone(),
        source: r.source.clone(),
        licence: r.licence.clone(),
        licence_url: r.licence_url.clone(),
        attribution_required: r.attribution_required,
        attribution: r.attribution.clone(),
        recipe: Some(json!({"width": a.width, "height": a.height, "color": a.color})),
    };
    let mut note = None;
    let placed = match &r.payload {
        Payload::Svg(svg) => {
            let svg = a.color.as_deref().map_or_else(|| svg.clone(), |c| raster::recolor(svg, c));
            let (png, w, h) = raster::rasterize(&svg, a.width, a.height).map_err(fail)?;
            project::place(dir, &stem, "png", &png, &sidecar, Some((w, h)))
        }
        Payload::File { ext, data } if ext == "png" => {
            if a.width.is_some() || a.height.is_some() || a.color.is_some() {
                note = Some("this asset is a PNG: copied as is, width, height and color were not applied; prefer an SVG icon to get an exact size and color");
            }
            project::place(dir, &stem, "png", data, &sidecar, png_size(data))
        }
        Payload::File { ext, data } if matches!(ext.as_str(), "ogg" | "mp3" | "wav") => {
            project::place(dir, &stem, ext, data, &sidecar, None)
        }
        Payload::File { ext, .. } => {
            return Err(ToolError(format!(".{ext} files cannot be placed in a project")));
        }
    }
    .map_err(fail)?;
    let mut v = json!({
        "file": placed.file, "sidecar": placed.sidecar,
        "licence": r.licence, "source": r.source,
        "attribution_required": r.attribution_required,
    });
    if let (Some(w), Some(h)) = (placed.width, placed.height) {
        v["width"] = json!(w);
        v["height"] = json!(h);
    }
    if r.attribution_required {
        v["attribution"] = json!(r.attribution);
        v["credit_rule"] = json!(
            "show this credit line with the finished work (credits slide, README or the closing message) and keep the sidecar"
        );
    }
    if let Some(n) = note {
        v["note"] = json!(n);
    }
    Ok(out(&v))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    id: String,
}

fn install(root: &Path, base: Option<&str>, a: IdArgs) -> Result<Output, ToolError> {
    let spec = catalog::pack(&a.id).ok_or_else(|| {
        ToolError(format!(
            "there is no pack {:?}; list_asset_packs names them",
            a.id
        ))
    })?;
    if packs::installed_ids(root).contains(&a.id) {
        return Ok(out(
            &json!({"installed": true, "id": a.id, "note": "already installed"}),
        ));
    }
    if !packs::is_approved(root, &a.id) {
        packs::request(root, &a.id).map_err(fail)?;
        return Ok(out(&json!({
            "installed": false,
            "waiting_for_user": true,
            "id": a.id,
            "download_bytes": spec.download_bytes,
            "message": format!(
                "The user has been asked in the Connect tab to download {} ({:.1} MB from {}). Tell them what you want it for and stop; call install_asset_pack again after they approved. Meanwhile make_asset and the bundled icons need no download.",
                spec.name, spec.download_bytes as f64 / 1_048_576.0, spec.page
            ),
        })));
    }
    let meta = packs::install(root, &a.id, base).map_err(fail)?;
    Ok(out(&json!({
        "installed": true, "id": a.id, "files": meta.files, "stored_bytes": meta.stored_bytes,
        "archive_bytes": meta.archive_bytes, "sha256": meta.sha256,
        "next": "search_assets now finds its files; unload_asset_pack frees the space when done",
    })))
}
