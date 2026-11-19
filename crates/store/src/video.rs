//! Store rules for videos (app previews), checked before and after an export the way the image
//! presets are (`docs/wiki/store-assets.md`). Each rule records the page it was read from and
//! the date.

use thiserror::Error;

use crate::Store;

/// What a video file is, as far as a preset can tell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoFacts {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration_ms: u64,
    /// The size of the file; zero before it is written.
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct VideoPreset {
    pub id: &'static str,
    pub store: Store,
    pub description: &'static str,
    /// Accepted sizes in pixels; the first is the portrait one.
    pub sizes: &'static [(u32, u32)],
    pub min_ms: u64,
    pub max_ms: u64,
    pub max_fps: u32,
    pub max_bytes: u64,
    /// The store documentation the rule comes from.
    pub source: &'static str,
    /// The day the rule was read from `source`.
    pub checked: &'static str,
    /// What this preset does not check, so nobody assumes it does.
    pub not_checked: &'static str,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum VideoViolation {
    #[error("size {width}x{height} is not accepted; use one of {expected}")]
    Size {
        width: u32,
        height: u32,
        expected: String,
    },
    #[error("{ms} ms is shorter than the {min} ms minimum")]
    TooShort { ms: u64, min: u64 },
    #[error("{ms} ms is longer than the {max} ms maximum")]
    TooLong { ms: u64, max: u64 },
    #[error("{fps} fps is above the {max} fps maximum")]
    FrameRate { fps: u32, max: u32 },
    #[error("{bytes} bytes is over the {max} byte limit")]
    TooBig { bytes: u64, max: u64 },
}

const APPLE_PREVIEWS: &str = "https://developer.apple.com/help/app-store-connect/reference/app-information/app-preview-specifications";
const APPLE_NOT_CHECKED: &str = "H.264 profile and level (High, up to 4.0), the 10-12 Mbps target bit rate and the audio track are not checked";

pub const VIDEO_PRESETS: &[VideoPreset] = &[
    VideoPreset {
        id: "apple-app-preview-iphone",
        store: Store::Apple,
        description: "App Store app preview for iPhones with a large display (portrait or landscape)",
        sizes: &[(886, 1920), (1920, 886)],
        min_ms: 15_000,
        max_ms: 30_000,
        max_fps: 30,
        max_bytes: 500 * 1_000_000,
        source: APPLE_PREVIEWS,
        checked: "2026-11-19",
        not_checked: APPLE_NOT_CHECKED,
    },
    VideoPreset {
        id: "apple-app-preview-ipad",
        store: Store::Apple,
        description: "App Store app preview for iPad (most models; portrait or landscape)",
        sizes: &[(1200, 1600), (1600, 1200)],
        min_ms: 15_000,
        max_ms: 30_000,
        max_fps: 30,
        max_bytes: 500 * 1_000_000,
        source: APPLE_PREVIEWS,
        checked: "2026-11-19",
        not_checked: APPLE_NOT_CHECKED,
    },
];

pub fn find_video(id: &str) -> Option<&'static VideoPreset> {
    VIDEO_PRESETS.iter().find(|p| p.id == id)
}

impl VideoPreset {
    pub fn describe_sizes(&self) -> String {
        let list: Vec<String> = self.sizes.iter().map(|(w, h)| format!("{w}x{h}")).collect();
        list.join(" or ")
    }

    /// Every rule `facts` breaks. Before the file exists pass `bytes: 0`: the size limit then
    /// cannot fail.
    pub fn validate(&self, facts: &VideoFacts) -> Vec<VideoViolation> {
        let mut found = Vec::new();
        if !self.sizes.contains(&(facts.width, facts.height)) {
            found.push(VideoViolation::Size {
                width: facts.width,
                height: facts.height,
                expected: self.describe_sizes(),
            });
        }
        if facts.duration_ms < self.min_ms {
            found.push(VideoViolation::TooShort {
                ms: facts.duration_ms,
                min: self.min_ms,
            });
        }
        if facts.duration_ms > self.max_ms {
            found.push(VideoViolation::TooLong {
                ms: facts.duration_ms,
                max: self.max_ms,
            });
        }
        if facts.fps > self.max_fps {
            found.push(VideoViolation::FrameRate {
                fps: facts.fps,
                max: self.max_fps,
            });
        }
        if facts.bytes > self.max_bytes {
            found.push(VideoViolation::TooBig {
                bytes: facts.bytes,
                max: self.max_bytes,
            });
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(width: u32, height: u32, fps: u32, duration_ms: u64, bytes: u64) -> VideoFacts {
        VideoFacts {
            width,
            height,
            fps,
            duration_ms,
            bytes,
        }
    }

    #[test]
    fn an_app_preview_needs_an_accepted_size_15_to_30_seconds_and_at_most_30_fps() {
        let p = find_video("apple-app-preview-iphone").unwrap();
        assert!(p.validate(&facts(886, 1920, 30, 15_000, 0)).is_empty());
        assert!(
            p.validate(&facts(1920, 886, 24, 30_000, 499_000_000))
                .is_empty()
        );
        let bad = p.validate(&facts(1080, 1920, 60, 14_999, 0));
        assert!(matches!(bad[0], VideoViolation::Size { .. }));
        assert!(bad.contains(&VideoViolation::TooShort {
            ms: 14_999,
            min: 15_000
        }));
        assert!(bad.contains(&VideoViolation::FrameRate { fps: 60, max: 30 }));
        assert!(
            p.validate(&facts(886, 1920, 30, 30_001, 0))
                .contains(&VideoViolation::TooLong {
                    ms: 30_001,
                    max: 30_000
                })
        );
        assert!(
            !p.validate(&facts(886, 1920, 30, 20_000, 500_000_001))
                .is_empty()
        );
    }

    #[test]
    fn every_video_preset_cites_its_page_and_the_day_it_was_read() {
        for p in VIDEO_PRESETS {
            assert!(p.source.starts_with("https://"));
            assert_eq!(p.checked.len(), 10, "{}", p.id);
            assert!(!p.not_checked.is_empty());
            assert!(p.sizes.iter().all(|(w, h)| w % 2 == 0 && h % 2 == 0));
        }
    }
}
