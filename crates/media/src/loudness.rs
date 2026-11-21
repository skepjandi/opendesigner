//! Loudness targets for an exported video's sound (`docs/wiki/video-editing.md#loudness`),
//! reached with ffmpeg's `loudnorm` (EBU R128) on the final mix.
//!
//! The export runs two passes: the first measures the mix, the second applies a linear gain
//! computed from that measurement (`linear=true`), which lands on the target far more exactly
//! than the one-pass dynamic mode. When the measurement cannot be read the second pass falls
//! back to the one-pass filter, and a mix that is silent is left alone. The filter strings are
//! built here as plain text so they can be tested without running ffmpeg.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::{Ffmpeg, MediaError};

/// The highest level any sample may reach after normalizing (dBTP), the usual streaming limit.
const TRUE_PEAK: f64 = -1.5;
/// The loudness range loudnorm aims to stay within (LU).
const RANGE: f64 = 11.0;

/// The integrated loudness a mix is brought to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loudness {
    /// -14 LUFS: social platforms and streaming.
    Social,
    /// -16 LUFS: podcasts and voice.
    Podcast,
    /// -23 LUFS: broadcast (EBU R128).
    Broadcast,
}

impl Loudness {
    /// The accepted values, as the tools and the command line list them.
    pub const LUFS: [i32; 3] = [-14, -16, -23];

    /// The target for a number of LUFS; only the three named targets exist.
    pub fn from_lufs(lufs: f64) -> Option<Loudness> {
        [Loudness::Social, Loudness::Podcast, Loudness::Broadcast]
            .into_iter()
            .find(|l| (l.lufs() - lufs).abs() < 1e-9)
    }

    pub fn lufs(self) -> f64 {
        match self {
            Loudness::Social => -14.0,
            Loudness::Podcast => -16.0,
            Loudness::Broadcast => -23.0,
        }
    }

    fn base(self) -> String {
        format!("loudnorm=I={}:TP={TRUE_PEAK}:LRA={RANGE}", self.lufs())
    }

    /// The first pass: measure and print the result as JSON on stderr.
    pub fn measuring_filter(self) -> String {
        format!("{}:print_format=json", self.base())
    }

    /// The one-pass filter, used when there is no usable measurement.
    pub fn single_pass_filter(self) -> String {
        self.base()
    }

    /// The second pass: a linear gain computed from `measured`.
    pub fn applying_filter(self, measured: &Measured) -> String {
        format!(
            "{}:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}:linear=true",
            self.base(),
            measured.input_i,
            measured.input_tp,
            measured.input_lra,
            measured.input_thresh,
            measured.target_offset
        )
    }
}

/// What the first pass found in the mix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    pub input_i: f64,
    pub input_tp: f64,
    pub input_lra: f64,
    pub input_thresh: f64,
    pub target_offset: f64,
}

impl Measured {
    /// Reads loudnorm's JSON report out of ffmpeg's log. `None` when it is missing, or when the
    /// mix is silent (its loudness is minus infinity), which no gain can bring to a target.
    pub fn parse(log: &str) -> Option<Measured> {
        let report = &log[log.rfind('{')?..];
        let field = |name: &str| -> Option<f64> {
            let at = report.find(&format!("\"{name}\""))?;
            let rest = &report[at + name.len() + 2..];
            let value = rest.split_once(':')?.1.trim_start().strip_prefix('"')?;
            value.split('"').next()?.trim().parse().ok()
        };
        let measured = Measured {
            input_i: field("input_i")?,
            input_tp: field("input_tp")?,
            input_lra: field("input_lra")?,
            input_thresh: field("input_thresh")?,
            target_offset: field("target_offset")?,
        };
        let finite = [
            measured.input_i,
            measured.input_tp,
            measured.input_lra,
            measured.input_thresh,
            measured.target_offset,
        ]
        .iter()
        .all(|v| v.is_finite());
        finite.then_some(measured)
    }
}

impl Ffmpeg {
    /// The integrated loudness of a file's sound in LUFS, measured with `loudnorm`.
    pub fn measure_loudness(&self, file: &Path) -> Result<f64, MediaError> {
        let out = Command::new(&self.path)
            .args(["-hide_banner", "-nostats", "-i"])
            .arg(file)
            .args(["-vn", "-af", &Loudness::Social.measuring_filter()])
            .args(["-f", "null", "-"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| MediaError::Io(e.to_string()))?;
        let log = String::from_utf8_lossy(&out.stderr);
        Measured::parse(&log)
            .map(|m| m.input_i)
            .ok_or_else(|| MediaError::Failed(format!("no loudness in the log: {}", log.trim())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "[Parsed_loudnorm_0 @ 0x1] \n{\n\t\"input_i\" : \"-23.54\",\n\t\"input_tp\" : \"-9.80\",\n\t\"input_lra\" : \"0.10\",\n\t\"input_thresh\" : \"-33.54\",\n\t\"output_i\" : \"-14.9\",\n\t\"target_offset\" : \"0.90\"\n}\n";

    #[test]
    fn only_the_three_targets_exist() {
        for (lufs, target) in [
            (-14.0, Loudness::Social),
            (-16.0, Loudness::Podcast),
            (-23.0, Loudness::Broadcast),
        ] {
            assert_eq!(Loudness::from_lufs(lufs), Some(target));
            assert_eq!(target.lufs(), lufs);
        }
        for bad in [0.0, -15.0, -14.5, f64::NAN, f64::NEG_INFINITY] {
            assert_eq!(Loudness::from_lufs(bad), None, "{bad}");
        }
        assert_eq!(Loudness::LUFS.map(f64::from), [-14.0, -16.0, -23.0]);
    }

    #[test]
    fn the_filters_name_the_target_and_the_measurement() {
        assert_eq!(
            Loudness::Social.single_pass_filter(),
            "loudnorm=I=-14:TP=-1.5:LRA=11"
        );
        assert_eq!(
            Loudness::Broadcast.measuring_filter(),
            "loudnorm=I=-23:TP=-1.5:LRA=11:print_format=json"
        );
        let measured = Measured::parse(LOG).unwrap();
        assert_eq!(
            Loudness::Podcast.applying_filter(&measured),
            "loudnorm=I=-16:TP=-1.5:LRA=11:measured_I=-23.54:measured_TP=-9.8:measured_LRA=0.1:measured_thresh=-33.54:offset=0.9:linear=true"
        );
    }

    #[test]
    fn the_report_is_read_from_the_end_of_the_log() {
        let m = Measured::parse(LOG).unwrap();
        assert_eq!((m.input_i, m.input_tp, m.input_lra), (-23.54, -9.8, 0.1));
        assert_eq!((m.input_thresh, m.target_offset), (-33.54, 0.9));
        assert_eq!(Measured::parse("no braces here"), None);
        assert_eq!(Measured::parse("{ \"input_i\" : \"-23.5\" }"), None);
    }

    #[test]
    fn a_silent_mix_has_no_usable_measurement() {
        let silent = LOG.replace("-23.54", "-inf");
        assert_eq!(Measured::parse(&silent), None);
    }
}
