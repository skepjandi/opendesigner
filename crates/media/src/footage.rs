//! Looking into footage and sound without a decoder library: where a recording's shots change,
//! one picture of it, and how loud it is moment by moment. Each is a read of what an `ffmpeg`
//! child process prints or decodes (`docs/wiki/video-editing.md#footage-memory`).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{Ffmpeg, MediaError};

/// The most footage scanned for shot changes: an hour.
const MAX_SCAN_S: u64 = 3_600;
/// Pictures are scanned at this width, which is plenty to tell one shot from the next.
const SCAN_WIDTH: u32 = 320;
/// The sample rate sound is decoded at to measure how loud it is.
const ENVELOPE_RATE: u32 = 16_000;
/// A filter that starts on a sound that is already playing rings for a moment.
const FILTER_WARMUP_MS: u64 = 60;

/// Replaces the start of filtered sound, where the filter is still settling, with the stretch
/// that follows it; a sound too short to have both is left alone.
fn hide_filter_start(samples: &mut [i16], rate: u32) {
    let warm = (u64::from(rate) * FILTER_WARMUP_MS / 1000) as usize;
    if samples.len() >= 2 * warm {
        let (head, tail) = samples.split_at_mut(warm);
        head.copy_from_slice(&tail[..warm]);
    }
}
/// The most sound read for an envelope: ten minutes (19 MB of 16-bit mono).
pub const MAX_ENVELOPE_MS: u64 = 10 * 60 * 1000;

/// A stretch of footage with no cut in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// The moments in ffmpeg's `showinfo` log, in milliseconds: one per picture the `select` filter
/// let through.
pub fn parse_cuts(log: &str) -> Vec<u64> {
    log.lines()
        .filter(|l| l.contains("showinfo"))
        .filter_map(|l| {
            let rest = l.split("pts_time:").nth(1)?;
            let seconds: f64 = rest.split_whitespace().next()?.parse().ok()?;
            seconds
                .is_finite()
                .then(|| (seconds.max(0.0) * 1000.0).round() as u64)
        })
        .collect()
}

/// Cuts a file of `total_ms` into shots at `cuts`. A cut closer than `min_shot_ms` to the
/// previous one, to the start or to the end is dropped, so a flash or a fade does not make a
/// shot of a few frames. A file with no cut is one shot.
pub fn shots_from_cuts(cuts: &[u64], total_ms: u64, min_shot_ms: u64) -> Vec<Shot> {
    let mut sorted = cuts.to_vec();
    sorted.sort_unstable();
    let mut starts = vec![0u64];
    for cut in sorted {
        let last = starts.last().copied().unwrap_or(0);
        if cut >= last + min_shot_ms && cut + min_shot_ms <= total_ms {
            starts.push(cut);
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &start_ms)| Shot {
            start_ms,
            end_ms: starts.get(i + 1).copied().unwrap_or(total_ms),
        })
        .collect()
}

/// Which part of the sound an envelope follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// Everything: the loudness.
    Level,
    /// Below 250 Hz: kick and bass.
    Low,
    /// 250 Hz to 2 kHz: voice and most instruments.
    Mid,
    /// Above 2 kHz: hats, sibilants, air.
    High,
}

impl Band {
    pub fn parse(name: &str) -> Option<Band> {
        match name {
            "level" => Some(Band::Level),
            "low" => Some(Band::Low),
            "mid" => Some(Band::Mid),
            "high" => Some(Band::High),
            _ => None,
        }
    }

    fn filter(self) -> Option<&'static str> {
        match self {
            Band::Level => None,
            Band::Low => Some("lowpass=f=250"),
            Band::Mid => Some("highpass=f=250,lowpass=f=2000"),
            Band::High => Some("highpass=f=2000"),
        }
    }
}

/// The loudness (root mean square, 0 to 1) of each `window_ms` slice of 16-bit mono `samples`
/// recorded at `rate` Hz. The last slice may be shorter; a window shorter than one sample reads
/// as one sample.
pub fn rms_windows(samples: &[i16], rate: u32, window_ms: u32) -> Vec<f32> {
    let window = ((u64::from(rate) * u64::from(window_ms) / 1000) as usize).max(1);
    samples
        .chunks(window)
        .map(|chunk| {
            let sum: f64 = chunk
                .iter()
                .map(|&s| (f64::from(s) / 32768.0).powi(2))
                .sum();
            (sum / chunk.len() as f64).sqrt().min(1.0) as f32
        })
        .collect()
}

/// Anything this quiet (about -50 dBFS) is silence, however loud it is next to nothing: a band the
/// sound does not have must not be stretched up to full scale.
const SILENCE: f32 = 0.003;

/// Loudness windows turned into a 0 to 1 curve: the loudest window is 1, and anything quieter
/// than `floor` (a share of the loudest) is 0, so room noise does not move a picture. `attack`
/// and `release` (0 to 1, higher is quicker) smooth the rise and the fall.
pub fn normalize(values: &[f32], floor: f32, attack: f32, release: f32) -> Vec<f32> {
    let peak = values.iter().copied().fold(0.0f32, f32::max);
    if peak <= SILENCE {
        return vec![0.0; values.len()];
    }
    let mut level = 0.0f32;
    values
        .iter()
        .map(|&v| {
            let target = ((v / peak - floor) / (1.0 - floor)).clamp(0.0, 1.0);
            let k = if target > level { attack } else { release };
            level += (target - level) * k.clamp(0.0, 1.0);
            level
        })
        .collect()
}

fn io(e: impl ToString) -> MediaError {
    MediaError::Io(e.to_string())
}

impl Ffmpeg {
    /// The moments `file` cuts to another shot, found by ffmpeg's scene score. `threshold` is how
    /// different a picture must be from the one before it, from 0.01 to 0.99; 0.3 is a good start
    /// and a lower number finds more. At most an hour is scanned, at a small size.
    pub fn detect_cuts(&self, file: &Path, threshold: f64) -> Result<Vec<u64>, MediaError> {
        if !(0.01..=0.99).contains(&threshold) {
            return Err(MediaError::Failed(format!(
                "a scene threshold is 0.01 to 0.99, not {threshold}"
            )));
        }
        let filter = format!("scale={SCAN_WIDTH}:-2,select='gt(scene,{threshold:.3})',showinfo");
        let out = Command::new(&self.path)
            .args([
                "-hide_banner",
                "-nostats",
                "-t",
                &MAX_SCAN_S.to_string(),
                "-i",
            ])
            .arg(file)
            .args(["-an", "-vf", &filter, "-f", "null", "-"])
            .stdin(Stdio::null())
            .output()
            .map_err(io)?;
        let log = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() {
            return Err(MediaError::Failed(format!(
                "{} has no picture ffmpeg can scan: {}",
                file.display(),
                log.lines().last().unwrap_or("").trim()
            )));
        }
        Ok(parse_cuts(&log))
    }

    /// The RGBA picture of `file` at `at_ms`, scaled to exactly `w` x `h`. An error when the file
    /// has no picture there (a moment past its end).
    pub fn frame_rgba(
        &self,
        file: &Path,
        at_ms: u64,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, MediaError> {
        let seconds = format!("{:.3}", at_ms as f64 / 1000.0);
        let size = w as usize * h as usize * 4;
        let out = Command::new(&self.path)
            .args(["-hide_banner", "-loglevel", "error", "-ss", &seconds, "-i"])
            .arg(file)
            .args(["-an", "-frames:v", "1", "-vf", &format!("scale={w}:{h}")])
            .args(["-pix_fmt", "rgba", "-f", "rawvideo", "-"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(io)?;
        if out.stdout.len() != size {
            return Err(MediaError::Failed(format!(
                "{} has no picture at {at_ms} ms (is that past its end?)",
                file.display()
            )));
        }
        Ok(out.stdout)
    }

    /// How loud the `band` of `file`'s sound is in each `window_ms` slice between `from_ms` and
    /// `to_ms` (0 to 1, not yet scaled to the loudest). At most ten minutes are read. An error
    /// when the file has no sound there.
    pub fn envelope(
        &self,
        file: &Path,
        band: Band,
        from_ms: u64,
        to_ms: u64,
        window_ms: u32,
    ) -> Result<Vec<f32>, MediaError> {
        let length = to_ms.saturating_sub(from_ms).min(MAX_ENVELOPE_MS);
        if length == 0 {
            return Err(MediaError::Failed("the range of sound is empty".into()));
        }
        let mut command = Command::new(&self.path);
        command
            .args(["-hide_banner", "-loglevel", "error"])
            .args(["-ss", &format!("{:.3}", from_ms as f64 / 1000.0)])
            .args(["-t", &format!("{:.3}", length as f64 / 1000.0), "-i"])
            .arg(file)
            .arg("-vn");
        if let Some(filter) = band.filter() {
            command.args(["-af", filter]);
        }
        let mut child = command
            .args([
                "-ac",
                "1",
                "-ar",
                &ENVELOPE_RATE.to_string(),
                "-f",
                "s16le",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(io)?;
        let mut bytes = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout.read_to_end(&mut bytes).map_err(io)?;
        }
        let status = child.wait().map_err(io)?;
        if !status.success() || bytes.len() < 2 {
            return Err(MediaError::Failed(format!(
                "{} has no sound ffmpeg can decode between {from_ms} and {to_ms} ms",
                file.display()
            )));
        }
        let mut samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        if band.filter().is_some() {
            hide_filter_start(&mut samples, ENVELOPE_RATE);
        }
        Ok(rms_windows(&samples, ENVELOPE_RATE, window_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_are_read_from_the_showinfo_lines_only() {
        let log = "[Parsed_showinfo_2 @ 0x1] n:   0 pts:  76800 pts_time:2.5 duration: 512\n\
                   frame=  10 fps=0.0 pts_time:99\n\
                   [Parsed_showinfo_2 @ 0x1] n:   1 pts: 230400 pts_time:7.5 duration: 512\n\
                   [Parsed_showinfo_2 @ 0x1] n:   2 pts_time:oops\n";
        assert_eq!(parse_cuts(log), [2500, 7500]);
    }

    #[test]
    fn a_cut_near_another_or_near_an_edge_makes_no_shot() {
        let shots = shots_from_cuts(&[7000, 2000, 2200, 9900, 100], 10_000, 500);
        let starts: Vec<u64> = shots.iter().map(|s| s.start_ms).collect();
        assert_eq!(starts, [0, 2000, 7000]);
        assert_eq!(shots.last().map(|s| s.end_ms), Some(10_000));
        assert_eq!(
            shots_from_cuts(&[], 4_000, 500),
            [Shot {
                start_ms: 0,
                end_ms: 4_000
            }]
        );
    }

    #[test]
    fn windows_measure_loudness_and_the_last_one_may_be_short() {
        let mut samples = vec![0i16; 160];
        samples.extend(vec![16384i16; 160]);
        samples.extend(vec![-32768i16; 80]);
        let rms = rms_windows(&samples, 16_000, 10);
        assert_eq!(rms.len(), 3);
        assert_eq!(rms[0], 0.0);
        assert!((rms[1] - 0.5).abs() < 1e-3);
        assert!((rms[2] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn normalizing_makes_the_loudest_one_and_the_noise_floor_zero() {
        let curve = normalize(&[0.02, 0.5, 1.0, 0.0], 0.1, 1.0, 1.0);
        assert_eq!(curve[0], 0.0);
        assert!((curve[2] - 1.0).abs() < 1e-6);
        assert!(curve[1] > 0.4 && curve[1] < 0.5);
        assert_eq!(normalize(&[0.0, 0.0], 0.1, 1.0, 1.0), [0.0, 0.0]);
    }

    #[test]
    fn release_makes_a_fall_slower_than_a_rise() {
        let curve = normalize(&[1.0, 0.0, 0.0], 0.0, 1.0, 0.5);
        assert_eq!(curve, [1.0, 0.5, 0.25]);
    }

    #[test]
    fn a_band_name_is_one_of_four() {
        assert_eq!(Band::parse("low"), Some(Band::Low));
        assert_eq!(Band::parse("bass"), None);
    }

    #[test]
    fn a_sound_this_quiet_stays_at_zero_however_loud_it_is_next_to_nothing() {
        assert_eq!(
            normalize(&[0.002, 0.001, 0.0], 0.0, 1.0, 1.0),
            [0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn the_settling_start_of_a_filter_is_replaced_by_what_follows_it() {
        let mut samples: Vec<i16> = (0..2_000)
            .map(|n| if n < 960 { 9_999 } else { 10 })
            .collect();
        hide_filter_start(&mut samples, 16_000);
        assert!(samples.iter().all(|&s| s == 10));
        let mut short = vec![5i16; 100];
        hide_filter_start(&mut short, 16_000);
        assert_eq!(short, vec![5i16; 100]);
    }
}
