//! Store export presets and validation (`docs/wiki/store-assets.md`).
//!
//! A preset encodes the size, format, alpha and count rules a store enforces. An export is
//! validated against the preset it claims to meet and fails loudly instead of producing a file a
//! store will reject (AGENTS.md invariant 9).

pub mod devices;
pub mod video;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Apple,
    Google,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeRule {
    /// The image must have exactly one of these pixel sizes.
    OneOf(&'static [(u32, u32)]),
    /// Both sides within `min_side..=max_side`, long side at most `max_ratio` times the short.
    Sides {
        min_side: u32,
        max_side: u32,
        max_ratio: f64,
    },
    /// Width within `min..=max`; height is not constrained by the store beyond being positive.
    WidthRange { min: u32, max: u32 },
}

/// How far a preset has been checked against the store's own documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    /// Copied from a reference project's notes; the primary source was not re-read.
    CopiedFromReference,
    /// Read against the primary source on this date (`YYYY-MM-DD`).
    PrimaryChecked(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub id: &'static str,
    pub store: Store,
    pub description: &'static str,
    pub size: SizeRule,
    pub formats: &'static [Format],
    pub allow_alpha: bool,
    pub min_count: usize,
    pub max_count: usize,
    /// The store documentation the rule comes from.
    pub source: &'static str,
    pub verification: Verification,
}

const APPLE_SCREENSHOTS: &str = "https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/";
const APPLE_CREATIVE: &str = "https://developer.apple.com/help/app-store-connect/reference/app-information/creative-assets-specifications";
const GOOGLE_ASSETS: &str =
    "https://support.google.com/googleplay/android-developer/answer/9866151";

const IMAGE_FORMATS: &[Format] = &[Format::Png, Format::Jpeg];

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "apple-iphone-6.9",
        store: Store::Apple,
        description: "App Store iPhone 6.9\" display screenshots (portrait)",
        size: SizeRule::OneOf(&[(1260, 2736), (1290, 2796), (1320, 2868)]),
        formats: IMAGE_FORMATS,
        allow_alpha: false,
        min_count: 1,
        max_count: 10,
        source: APPLE_SCREENSHOTS,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "apple-iphone-6.3",
        store: Store::Apple,
        description: "App Store iPhone 6.3\" display screenshots (portrait)",
        size: SizeRule::OneOf(&[(1206, 2622), (1179, 2556)]),
        formats: IMAGE_FORMATS,
        allow_alpha: false,
        min_count: 1,
        max_count: 10,
        source: APPLE_SCREENSHOTS,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "apple-ipad-13",
        store: Store::Apple,
        description: "App Store iPad 13\" display screenshots (portrait)",
        size: SizeRule::OneOf(&[(2064, 2752), (2048, 2732)]),
        formats: IMAGE_FORMATS,
        allow_alpha: false,
        min_count: 1,
        max_count: 10,
        source: APPLE_SCREENSHOTS,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "apple-creative-universal",
        store: Store::Apple,
        description: "App Store creative asset: product page header and search results (16:9)",
        size: SizeRule::OneOf(&[(5244, 2950)]),
        formats: &[Format::Png],
        allow_alpha: false,
        min_count: 1,
        max_count: 1,
        source: APPLE_CREATIVE,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "apple-creative-header",
        store: Store::Apple,
        description: "App Store creative asset: product page header only (21:9)",
        size: SizeRule::OneOf(&[(3840, 1646)]),
        formats: &[Format::Png],
        allow_alpha: false,
        min_count: 1,
        max_count: 1,
        source: APPLE_CREATIVE,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "apple-creative-search",
        store: Store::Apple,
        description: "App Store creative asset: search results only (3:2)",
        size: SizeRule::OneOf(&[(3840, 2560)]),
        formats: &[Format::Png],
        allow_alpha: false,
        min_count: 1,
        max_count: 1,
        source: APPLE_CREATIVE,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "google-phone",
        store: Store::Google,
        description: "Google Play phone screenshots",
        size: SizeRule::Sides {
            min_side: 320,
            max_side: 3840,
            max_ratio: 2.0,
        },
        formats: IMAGE_FORMATS,
        allow_alpha: false,
        min_count: 2,
        max_count: 8,
        source: GOOGLE_ASSETS,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "google-tablet-10",
        store: Store::Google,
        description: "Google Play 7\" and 10\" tablet screenshots",
        size: SizeRule::WidthRange {
            min: 1080,
            max: 7680,
        },
        formats: IMAGE_FORMATS,
        allow_alpha: false,
        min_count: 1,
        max_count: 8,
        source: GOOGLE_ASSETS,
        verification: Verification::CopiedFromReference,
    },
    Preset {
        id: "google-feature-graphic",
        store: Store::Google,
        description: "Google Play feature graphic",
        size: SizeRule::OneOf(&[(1024, 500)]),
        formats: IMAGE_FORMATS,
        allow_alpha: false,
        min_count: 1,
        max_count: 1,
        source: GOOGLE_ASSETS,
        verification: Verification::CopiedFromReference,
    },
];

pub fn find(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// What validation needs to know about an image, without its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageFacts {
    pub width: u32,
    pub height: u32,
    pub format: Format,
    pub has_alpha: bool,
}

#[derive(Debug, Error, PartialEq)]
pub enum Violation {
    #[error("size {width}x{height} is not allowed; {expected}")]
    Size {
        width: u32,
        height: u32,
        expected: String,
    },
    #[error("{0} is not an accepted format")]
    Format(&'static str),
    #[error("the image has an alpha channel, which this store rejects")]
    Alpha,
    #[error("{count} images is outside the allowed {min}..={max}")]
    Count {
        count: usize,
        min: usize,
        max: usize,
    },
}

impl Preset {
    pub fn describe_size(&self) -> String {
        match self.size {
            SizeRule::OneOf(sizes) => {
                let list: Vec<String> = sizes.iter().map(|(w, h)| format!("{w}x{h}")).collect();
                format!("one of {}", list.join(", "))
            }
            SizeRule::Sides {
                min_side,
                max_side,
                max_ratio,
            } => format!(
                "each side {min_side}-{max_side} px, long side at most {max_ratio}x the short side"
            ),
            SizeRule::WidthRange { min, max } => format!("width {min}-{max} px"),
        }
    }

    pub fn validate_image(&self, image: &ImageFacts) -> Vec<Violation> {
        let mut found = Vec::new();
        let (w, h) = (image.width, image.height);
        let size_ok = match self.size {
            SizeRule::OneOf(sizes) => sizes.contains(&(w, h)),
            SizeRule::Sides {
                min_side,
                max_side,
                max_ratio,
            } => {
                let (short, long) = (w.min(h), w.max(h));
                short >= min_side
                    && long <= max_side
                    && short > 0
                    && f64::from(long) <= max_ratio * f64::from(short)
            }
            SizeRule::WidthRange { min, max } => (min..=max).contains(&w) && h > 0,
        };
        if !size_ok {
            found.push(Violation::Size {
                width: w,
                height: h,
                expected: self.describe_size(),
            });
        }
        if !self.formats.contains(&image.format) {
            found.push(Violation::Format(image.format.name()));
        }
        if image.has_alpha && !self.allow_alpha {
            found.push(Violation::Alpha);
        }
        found
    }

    /// Validates a whole set: every image, plus how many there are.
    pub fn validate_set(&self, images: &[ImageFacts]) -> Vec<(Option<usize>, Violation)> {
        let mut found = Vec::new();
        if images.len() < self.min_count || images.len() > self.max_count {
            found.push((
                None,
                Violation::Count {
                    count: images.len(),
                    min: self.min_count,
                    max: self.max_count,
                },
            ));
        }
        for (i, image) in images.iter().enumerate() {
            found.extend(self.validate_image(image).into_iter().map(|v| (Some(i), v)));
        }
        found
    }
}

#[derive(Debug, Error)]
pub enum InspectError {
    #[error("not a PNG or the header is damaged: {0}")]
    Png(String),
    #[error("only PNG files can be inspected so far")]
    Unsupported,
}

/// Reads dimensions and color type from a PNG header, without decoding the pixels.
pub fn inspect_png(bytes: &[u8]) -> Result<ImageFacts, InspectError> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(SIGNATURE) {
        return Err(InspectError::Unsupported);
    }
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| InspectError::Png(e.to_string()))?;
    let info = reader.info();
    let has_alpha = matches!(
        info.color_type,
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
    ) || info.trns.is_some();
    Ok(ImageFacts {
        width: info.width,
        height: info.height,
        format: Format::Png,
        has_alpha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_facts(width: u32, height: u32, has_alpha: bool) -> ImageFacts {
        ImageFacts {
            width,
            height,
            format: Format::Png,
            has_alpha,
        }
    }

    fn preset(id: &str) -> &'static Preset {
        find(id).unwrap()
    }

    #[test]
    fn preset_ids_are_unique_and_every_preset_cites_a_source() {
        let mut ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len());
        assert!(PRESETS.iter().all(|p| p.source.starts_with("https://")));
        assert!(PRESETS.iter().all(|p| p.min_count <= p.max_count));
    }

    #[test]
    fn apple_screenshots_need_an_accepted_size_and_no_alpha() {
        let p = preset("apple-iphone-6.9");
        assert!(p.validate_image(&png_facts(1320, 2868, false)).is_empty());
        assert!(p.validate_image(&png_facts(1290, 2796, false)).is_empty());
        let bad = p.validate_image(&png_facts(1320, 2869, true));
        assert!(matches!(bad[0], Violation::Size { .. }));
        assert_eq!(bad[1], Violation::Alpha);
    }

    #[test]
    fn google_phone_enforces_the_two_to_one_limit() {
        let p = preset("google-phone");
        assert!(p.validate_image(&png_facts(1080, 1920, false)).is_empty());
        // an iPhone 17 Pro Max capture is about 2.17:1 and cannot be reused as-is
        assert!(!p.validate_image(&png_facts(1320, 2868, false)).is_empty());
        assert!(!p.validate_image(&png_facts(300, 600, false)).is_empty());
        assert!(!p.validate_image(&png_facts(1080, 3841, false)).is_empty());
    }

    #[test]
    fn google_tablet_constrains_width_only() {
        let p = preset("google-tablet-10");
        assert!(p.validate_image(&png_facts(1440, 2560, false)).is_empty());
        assert!(!p.validate_image(&png_facts(1000, 2560, false)).is_empty());
        assert!(!p.validate_image(&png_facts(7681, 2560, false)).is_empty());
    }

    #[test]
    fn feature_graphic_is_exactly_1024_by_500() {
        let p = preset("google-feature-graphic");
        assert!(p.validate_image(&png_facts(1024, 500, false)).is_empty());
        assert!(!p.validate_image(&png_facts(1024, 501, false)).is_empty());
    }

    #[test]
    fn set_validation_counts_images() {
        let p = preset("google-phone");
        let ok = png_facts(1080, 1920, false);
        assert!(matches!(
            p.validate_set(&[ok])[0],
            (
                None,
                Violation::Count {
                    count: 1,
                    min: 2,
                    max: 8
                }
            )
        ));
        assert!(p.validate_set(&[ok, ok]).is_empty());
        assert!(p.validate_set(&[ok; 9]).iter().any(|(i, _)| i.is_none()));
    }

    #[test]
    fn inspects_png_headers() {
        let mut bytes = Vec::new();
        let mut enc = png::Encoder::new(&mut bytes, 3, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[0u8; 3 * 2 * 3]).unwrap();
        drop(w);
        assert_eq!(inspect_png(&bytes).unwrap(), png_facts(3, 2, false));
        assert!(matches!(
            inspect_png(b"nope"),
            Err(InspectError::Unsupported)
        ));
    }
}
