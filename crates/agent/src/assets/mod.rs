//! Assets for the agent to design with: icons, shapes, blobs, backgrounds, patterns, sounds
//! (`docs/wiki/store-assets.md#asset-packs`).
//!
//! Three kinds of source, so the app stays small and the project only ever holds what it uses:
//!
//! 1. **Bundled** (`index`): one icon set (Lucide, ISC), 2,404 icons in a 150 KB compressed
//!    container. Nothing else ships.
//! 2. **Procedural** (`generate`): blobs, gradients, auroras, grain, patterns and shapes written
//!    as SVG from a seed and the agent's colors. No data at all.
//! 3. **Packs** (`packs`): more icon sets and the Kenney CC0 packs, downloaded only after the user
//!    agrees, into the per-user cache (never the project), kept as one compressed container each,
//!    and deleted on request.
//!
//! A file reaches the project (`project`) only when the agent imports or makes it: rasterized to
//! a PNG at the size asked for (`raster`), next to a sidecar that names the source and licence.

pub mod archive;
pub mod catalog;
pub mod container;
pub mod generate;
pub mod index;
pub mod logo;
pub mod packs;
pub mod project;
pub mod raster;
pub mod tools;

use thiserror::Error;

/// A refused or failed asset call. The text goes back to the agent.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error("{0}")]
    Http(#[from] crate::http::HttpError),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn msg<T>(text: impl Into<String>) -> Result<T> {
    Err(Error::Msg(text.into()))
}

pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.display().to_string(),
        source,
    }
}
