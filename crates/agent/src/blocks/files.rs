//! The pictures a block needs: generated backgrounds and bundled icons. A block names them by
//! recipe, so the file name is the recipe's hash: the same block asks for the same file, the
//! plan stays pure, and `write` only touches the project's `media/` folder.

use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::BlockError;
use crate::assets::generate::{self, Params};
use crate::assets::index::{self, Payload};
use crate::assets::project::{self, Sidecar};
use crate::assets::raster;

const MAX_ICON_PX: u32 = 512;
const MAX_BACKDROP_SIDE: f64 = 1600.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Asset {
    /// A picture from `crate::assets::generate`.
    Generated {
        generator: &'static str,
        variant: Option<String>,
        colors: Vec<String>,
        seed: u64,
        width: u32,
        height: u32,
        scale: Option<u32>,
    },
    /// A bundled Lucide icon, recolored and drawn `px` pixels square.
    Icon {
        name: String,
        color: String,
        px: u32,
    },
}

impl Asset {
    /// A backdrop drawn at the box's aspect, capped so the file stays small.
    pub fn backdrop(
        generator: &'static str,
        variant: Option<&str>,
        colors: Vec<String>,
        seed: u64,
        box_w: f64,
        box_h: f64,
        scale: Option<u32>,
    ) -> Asset {
        let k = (MAX_BACKDROP_SIDE / box_w.max(box_h)).min(1.0);
        Asset::Generated {
            generator,
            variant: variant.map(str::to_string),
            colors,
            seed,
            width: (box_w * k).round().max(2.0) as u32,
            height: (box_h * k).round().max(2.0) as u32,
            scale: scale.map(|s| (f64::from(s) * k).round().max(2.0) as u32),
        }
    }

    /// A generated shape (`star`, `triangle`, ...) in one color, sharp at up to 3x its size.
    pub fn shape(variant: &str, color: String, size: f64) -> Asset {
        let px = ((size * 3.0).ceil() as u32).clamp(32, MAX_ICON_PX);
        Asset::Generated {
            generator: "shape",
            variant: Some(variant.to_string()),
            colors: vec![color],
            seed: 0,
            width: px,
            height: px,
            scale: None,
        }
    }

    /// An icon sharp at up to 3x its size in the design.
    pub fn icon(name: &str, color: String, size: f64) -> Asset {
        Asset::Icon {
            name: name.to_string(),
            color,
            px: ((size * 3.0).ceil() as u32).clamp(32, MAX_ICON_PX),
        }
    }

    fn recipe(&self) -> Value {
        match self {
            Asset::Generated {
                generator,
                variant,
                colors,
                seed,
                width,
                height,
                scale,
            } => json!({
                "generator": generator, "variant": variant, "colors": colors, "seed": seed,
                "width": width, "height": height, "scale": scale,
            }),
            Asset::Icon { name, color, px } => json!({"icon": name, "color": color, "px": px}),
        }
    }

    fn stem(&self) -> String {
        let digest = Sha256::digest(self.recipe().to_string().as_bytes());
        let hash: String = digest.iter().take(5).map(|b| format!("{b:02x}")).collect();
        let kind = match self {
            Asset::Generated { generator, .. } => generator,
            Asset::Icon { .. } => "icon",
        };
        format!("block-{kind}-{hash}")
    }

    /// What an image node names.
    pub fn file(&self) -> String {
        format!("media/{}.png", self.stem())
    }

    /// Whether the icon or generator exists, so a plan fails before anything is written.
    pub fn check(&self) -> Result<(), BlockError> {
        let fail = |e: crate::assets::Error| BlockError::Asset(e.to_string());
        match self {
            Asset::Generated {
                generator,
                variant,
                colors,
                seed,
                width,
                height,
                scale,
            } => generate::svg(
                generator,
                &Params {
                    width: *width,
                    height: *height,
                    colors: colors.clone(),
                    seed: *seed,
                    variant: variant.clone(),
                    scale: *scale,
                },
            )
            .map(|_| ())
            .map_err(fail),
            Asset::Icon { name, .. } => index::resolve(Path::new(""), &format!("lucide:{name}"))
                .map(|_| ())
                .map_err(fail),
        }
    }

    fn render(&self) -> Result<(Vec<u8>, Sidecar), BlockError> {
        let fail = |e: crate::assets::Error| BlockError::Asset(e.to_string());
        match self {
            Asset::Generated {
                generator,
                variant,
                colors,
                seed,
                width,
                height,
                scale,
            } => {
                let params = Params {
                    width: *width,
                    height: *height,
                    colors: colors.clone(),
                    seed: *seed,
                    variant: variant.clone(),
                    scale: *scale,
                };
                let svg = generate::svg(generator, &params).map_err(fail)?;
                let (png, ..) =
                    raster::rasterize(&svg, Some(*width), Some(*height)).map_err(fail)?;
                let sidecar = Sidecar {
                    id: format!("generated:{generator}"),
                    source: "OpenDesigner generator".into(),
                    licence: "none (generated from a seed)".into(),
                    licence_url: String::new(),
                    attribution_required: false,
                    attribution: String::new(),
                    recipe: Some(self.recipe()),
                };
                Ok((png, sidecar))
            }
            Asset::Icon { name, color, px } => {
                let r = index::resolve(Path::new(""), &format!("lucide:{name}")).map_err(fail)?;
                let Payload::Svg(svg) = &r.payload else {
                    return Err(BlockError::Asset(format!("{name} is not an icon")));
                };
                let svg = raster::recolor(svg, color);
                let (png, ..) = raster::rasterize(&svg, Some(*px), Some(*px)).map_err(fail)?;
                let sidecar = Sidecar {
                    id: r.id,
                    source: r.source,
                    licence: r.licence,
                    licence_url: r.licence_url,
                    attribution_required: r.attribution_required,
                    attribution: r.attribution,
                    recipe: Some(self.recipe()),
                };
                Ok((png, sidecar))
            }
        }
    }

    /// The credit line this asset needs shown with the work, if any.
    pub fn credit(&self) -> Option<String> {
        match self {
            Asset::Icon { .. } => index::resolve(Path::new(""), "lucide:star")
                .ok()
                .filter(|r| r.attribution_required)
                .map(|r| r.attribution),
            Asset::Generated { .. } => None,
        }
    }

    /// Puts the picture in the project's `media/` unless a file of this recipe is there already.
    pub fn write(&self, project_dir: &Path) -> Result<(), BlockError> {
        let stem = self.stem();
        if project_dir
            .join("media")
            .join(format!("{stem}.png"))
            .exists()
        {
            return Ok(());
        }
        let (png, sidecar) = self.render()?;
        let placed = project::place(project_dir, &stem, "png", &png, &sidecar, None)
            .map_err(|e| BlockError::Asset(e.to_string()))?;
        if placed.file != self.file() {
            return Err(BlockError::Asset(format!(
                "{} is taken by another file",
                self.file()
            )));
        }
        Ok(())
    }
}
