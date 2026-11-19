//! What OpenDesigner knows about: the bundled set, the generators, the downloadable packs, the
//! optional MCP servers, and the sources that are only worth a link. Every licence and size here
//! was read from the source on 2026-11-20 (`docs/wiki/store-assets.md#asset-packs`).

/// How a pack's archive is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Download {
    /// Kenney's own download button: ask `kenney.nl/download-link/<hex of assets/slug>` (it
    /// answers only a request that names the pack's page as its referer) for a signed link.
    Kenney(&'static str),
    /// A fixed address.
    Url(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tgz,
}

/// A pack: what to fetch and which of its files to keep.
#[derive(Debug, Clone, Copy)]
pub struct PackSpec {
    pub id: &'static str,
    pub name: &'static str,
    /// icon | illustration | sound
    pub kind: &'static str,
    pub about: &'static str,
    pub licence: &'static str,
    pub licence_url: &'static str,
    /// Whether the licence asks the user to show a credit line.
    pub attribution_required: bool,
    pub credit: &'static str,
    pub page: &'static str,
    pub download: Download,
    pub format: Format,
    /// The archive's size when it was read, in bytes: what the user is asked to agree to.
    pub download_bytes: u64,
    /// npm tarballs put everything under `package/`.
    pub strip_top: bool,
    /// Only files under one of these folders are kept (empty: any).
    pub prefixes: &'static [&'static str],
    /// Only files with one of these extensions are kept.
    pub exts: &'static [&'static str],
}

const KENNEY_LICENCE: &str = "CC0 1.0";
const CC0_URL: &str = "https://creativecommons.org/publicdomain/zero/1.0/";

#[allow(clippy::too_many_arguments)]
const fn kenney(
    id: &'static str,
    slug: &'static str,
    page: &'static str,
    name: &'static str,
    kind: &'static str,
    about: &'static str,
    download_bytes: u64,
    prefixes: &'static [&'static str],
    exts: &'static [&'static str],
) -> PackSpec {
    PackSpec {
        id,
        name,
        kind,
        about,
        licence: KENNEY_LICENCE,
        licence_url: CC0_URL,
        attribution_required: false,
        credit: "Kenney (kenney.nl)",
        page,
        download: Download::Kenney(slug),
        format: Format::Zip,
        download_bytes,
        strip_top: false,
        prefixes,
        exts,
    }
}

#[allow(clippy::too_many_arguments)]
const fn npm(
    id: &'static str,
    name: &'static str,
    about: &'static str,
    licence: &'static str,
    licence_url: &'static str,
    credit: &'static str,
    page: &'static str,
    url: &'static str,
    download_bytes: u64,
    prefix: &'static [&'static str],
) -> PackSpec {
    PackSpec {
        id,
        name,
        kind: "icon",
        about,
        licence,
        licence_url,
        attribution_required: true,
        credit,
        page,
        download: Download::Url(url),
        format: Format::Tgz,
        download_bytes,
        strip_top: true,
        prefixes: prefix,
        exts: &["svg"],
    }
}

pub const PACKS: &[PackSpec] = &[
    npm(
        "tabler-icons",
        "Tabler Icons (outline)",
        "5,491 outline icons, 24 px grid, stroke currentColor",
        "MIT",
        "https://github.com/tabler/tabler-icons/blob/main/LICENSE",
        "Tabler Icons, Paweł Kuna",
        "https://tabler.io/icons",
        "https://registry.npmjs.org/@tabler/icons/-/icons-3.52.0.tgz",
        1_299_871,
        &["icons/outline/"],
    ),
    npm(
        "phosphor-icons",
        "Phosphor Icons (regular)",
        "1,512 icons, flexible friendly style; this keeps the regular weight",
        "MIT",
        "https://github.com/phosphor-icons/core/blob/main/LICENSE",
        "Phosphor Icons",
        "https://phosphoricons.com",
        "https://registry.npmjs.org/@phosphor-icons/core/-/core-2.1.1.tgz",
        1_443_064,
        &["assets/regular/"],
    ),
    npm(
        "heroicons",
        "Heroicons (outline)",
        "324 outline icons by the Tailwind team, 24 px grid",
        "MIT",
        "https://github.com/tailwindlabs/heroicons/blob/master/LICENSE",
        "Heroicons, Tailwind Labs",
        "https://heroicons.com",
        "https://registry.npmjs.org/heroicons/-/heroicons-2.2.0.tgz",
        166_699,
        &["24/outline/"],
    ),
    npm(
        "iconoir",
        "Iconoir (regular)",
        "1,383 clean line icons",
        "MIT",
        "https://github.com/iconoir-icons/iconoir/blob/main/LICENSE",
        "Iconoir",
        "https://iconoir.com",
        "https://registry.npmjs.org/iconoir/-/iconoir-7.12.2.tgz",
        757_419,
        &["icons/regular/"],
    ),
    npm(
        "bootstrap-icons",
        "Bootstrap Icons",
        "2,000+ general icons, outline and filled, fill currentColor",
        "MIT",
        "https://github.com/twbs/icons/blob/main/LICENSE",
        "Bootstrap Icons, The Bootstrap Authors",
        "https://icons.getbootstrap.com",
        "https://registry.npmjs.org/bootstrap-icons/-/bootstrap-icons-1.13.2.tgz",
        883_452,
        &["icons/"],
    ),
    kenney(
        "kenney-game-icons",
        "game-icons",
        "https://kenney.nl/assets/game-icons",
        "Kenney Game Icons",
        "icon",
        "105 icons in black and white, 2x PNG",
        1_045_980,
        &["PNG/White/2x/", "PNG/Black/2x/"],
        &["png"],
    ),
    kenney(
        "kenney-board-game-icons",
        "board-game-icons",
        "https://kenney.nl/assets/board-game-icons",
        "Kenney Board Game Icons",
        "icon",
        "255 vector icons for cards, dice, tokens, arrows",
        1_039_954,
        &["Vector/Icons/"],
        &["svg"],
    ),
    kenney(
        "kenney-ui-pack",
        "ui-pack",
        "https://kenney.nl/assets/ui-pack",
        "Kenney UI Pack",
        "icon",
        "buttons, sliders, checkboxes and icons as vectors in five colors",
        1_229_750,
        &["Vector/"],
        &["svg"],
    ),
    kenney(
        "kenney-shape-characters",
        "shape-characters",
        "https://kenney.nl/assets/shape-characters",
        "Kenney Shape Characters",
        "illustration",
        "friendly geometric characters, bodies, faces and limbs as PNG",
        546_442,
        &["PNG/Default/"],
        &["png"],
    ),
    kenney(
        "kenney-interface-sounds",
        "interface-sounds",
        "https://kenney.nl/assets/interface-sounds",
        "Kenney Interface Sounds",
        "sound",
        "100 clicks, ticks, switches and confirmations (OGG)",
        834_536,
        &["Audio/"],
        &["ogg"],
    ),
    kenney(
        "kenney-impact-sounds",
        "impact-sounds",
        "https://kenney.nl/assets/impact-sounds",
        "Kenney Impact Sounds",
        "sound",
        "130 thuds, hits, footsteps and clacks (OGG)",
        800_850,
        &["Audio/"],
        &["ogg"],
    ),
    kenney(
        "kenney-ui-audio",
        "ui-audio",
        "https://kenney.nl/assets/ui-audio",
        "Kenney UI Audio",
        "sound",
        "50 interface clicks and rollovers (OGG)",
        411_949,
        &["Audio/"],
        &["ogg"],
    ),
];

/// A file's path with the folder the pack keeps it under taken off: `outline/home.svg` in a
/// pack that keeps `icons/` is `outline/home.svg`; Kenney's `Blue/check.svg` stays as it is.
pub fn relative<'a>(spec: &PackSpec, path: &'a str) -> &'a str {
    spec.prefixes
        .iter()
        .find_map(|p| path.strip_prefix(p))
        .unwrap_or(path)
}

pub fn pack(id: &str) -> Option<&'static PackSpec> {
    PACKS.iter().find(|p| p.id == id)
}

/// The bundled icon set.
pub struct Bundled {
    pub id: &'static str,
    pub name: &'static str,
    pub licence: &'static str,
    pub licence_url: &'static str,
    pub credit: &'static str,
    pub version: &'static str,
}

pub const LUCIDE: Bundled = Bundled {
    id: "lucide",
    name: "Lucide",
    licence: "ISC (a few icons MIT, from Feather)",
    licence_url: "https://github.com/lucide-icons/lucide/blob/main/LICENSE",
    credit: "Lucide Icons and Contributors",
    version: "1.76.0",
};

/// What `make_asset` can make.
pub const GENERATORS: &[(&str, &str, &str)] = &[
    (
        "blob",
        "blob",
        "smooth organic closed shape; variants: points count (\"blob-8\"); 1 color, or 2 for a gradient",
    ),
    (
        "gradient",
        "background",
        "linear or radial color ramp across 2 to 8 colors; variants: linear, radial",
    ),
    (
        "aurora",
        "background",
        "soft blurred color clouds on a base color (first color is the base); the mesh-gradient look",
    ),
    (
        "grain",
        "background",
        "film grain / paper noise, over a base color if one is given",
    ),
    (
        "pattern",
        "pattern",
        "variants: dots, lines, waves, grid, checker, zigzag, topo; colors: background, ink; scale = cell size in px",
    ),
    (
        "shape",
        "shape",
        "variants: circle, rounded-rect, triangle, star (\"star-6\"), polygon (\"polygon-5\"), ring, burst; 1 color, or 2 for a gradient",
    ),
];

/// An MCP server that fits OpenDesigner, as the Connect tab offers it.
pub struct Recommended {
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub env: &'static [&'static str],
    pub about: &'static str,
    pub licence: &'static str,
}

/// Only servers whose package, command and licence were read on 2026-11-20. None of them was
/// run by us.
pub const RECOMMENDED_SERVERS: &[Recommended] = &[
    Recommended {
        name: "3dassets",
        command: "npx",
        args: &["-y", "3dassets-mcp"],
        env: &[],
        about: "search a hosted catalogue of CC0 3D models (GLB); needs no key to search",
        licence: "CC0-1.0 (package and assets)",
    },
    Recommended {
        name: "sozai",
        command: "uvx",
        args: &[
            "--from",
            "git+https://github.com/kanaworksai-hue/sozai-search-mcp",
            "sozai",
        ],
        env: &[],
        about: "search ~39,000 3D assets across Poly Haven, ambientCG, Kenney and more (a search index; needs uv and Python 3.10+); ask it for redistributable_only results",
        licence: "MIT (code); each library's own licence for assets",
    },
    Recommended {
        name: "asset-gate",
        command: "npx",
        args: &["-y", "github:codepawl/asset-gate"],
        env: &[],
        about: "search proxy over Hugging Face hosted 3D asset packs; filter to CC0 only",
        licence: "MIT (code); mixed licences for assets",
    },
];

/// Sources that were read and are not built in, with the reason. Shown by `list_asset_sources`.
pub const LINK_ONLY: &[(&str, &str, &str)] = &[
    (
        "unDraw",
        "https://undraw.co/license",
        "custom licence forbids redistributing the illustrations in an app or pack",
    ),
    (
        "Haikei / Blobmaker",
        "https://haikei.app/terms/",
        "its terms grant nothing over generated output; make_asset draws blobs and gradients instead",
    ),
    (
        "SVG Backgrounds",
        "https://www.svgbackgrounds.com/license/",
        "forbids redistribution and use as-is",
    ),
    (
        "Hero Patterns",
        "https://heropatterns.com",
        "CC BY 4.0, needs a credit; make_asset pattern covers the common ones with no credit",
    ),
    (
        "Kitbitz",
        "https://kitbitz.art",
        "site says CC0 (read from its page metadata); no download interface or terms page could be read, so it is not fetched",
    ),
    (
        "Open Doodles / Open Peeps",
        "https://www.opendoodles.com",
        "CC0, but distributed as single files and Gumroad downloads, not an archive a pack can fetch",
    ),
    (
        "Simple Icons",
        "https://simpleicons.org",
        "CC0 files, but the brands' trademarks are not covered; use only with permission",
    ),
];
