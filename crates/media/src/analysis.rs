//! Looking into audio: pauses (`silencedetect`) and a waveform's peaks, both read from what an
//! `ffmpeg` child process prints or decodes (`docs/wiki/video-editing.md#pauses-and-waveforms`).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{Ffmpeg, MediaError, parse_duration};

/// A stretch of a file with no sound above the noise floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pause {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// The sample rate the waveform is decoded at: enough for a picture of the loudness, cheap to read.
const WAVEFORM_RATE: u64 = 8_000;
/// The most sound decoded for one waveform (ten minutes of 16-bit mono at 8 kHz is 9.6 MB).
const MAX_WAVEFORM_MS: u64 = 30 * 60 * 1000;
pub const MIN_BUCKETS: usize = 50;
pub const MAX_BUCKETS: usize = 2_000;

fn ms_of(seconds: &str) -> Option<u64> {
    let s: f64 = seconds.trim().parse().ok()?;
    s.is_finite().then(|| (s.max(0.0) * 1000.0).round() as u64)
}

/// The pauses in ffmpeg's `silencedetect` log. A pause that is still open at the end of the file
/// ends where the file does (`total_ms`, or the `Duration:` line of the same log).
pub fn parse_pauses(log: &str, total_ms: Option<u64>) -> Vec<Pause> {
    let total = total_ms.or_else(|| parse_duration(log));
    let mut open: Option<u64> = None;
    let mut out = Vec::new();
    for line in log.lines() {
        if let Some(rest) = line.split("silence_start:").nth(1) {
            open = ms_of(rest);
        } else if let Some(rest) = line.split("silence_end:").nth(1)
            && let (Some(start), Some(end)) =
                (open.take(), ms_of(rest.split('|').next().unwrap_or("")))
            && end > start
        {
            out.push(Pause {
                start_ms: start,
                end_ms: end,
            });
        }
    }
    if let (Some(start), Some(end)) = (open, total)
        && end > start
    {
        out.push(Pause {
            start_ms: start,
            end_ms: end,
        });
    }
    out
}

/// One peak per bucket (0 to 1): the loudest sample in each of `buckets` equal slices of
/// `samples`. Fewer samples than buckets give fewer buckets.
pub fn peaks_from_samples(samples: &[i16], buckets: usize) -> Vec<f32> {
    let buckets = buckets.min(samples.len());
    (0..buckets)
        .map(|b| {
            let (from, to) = (
                b * samples.len() / buckets,
                (b + 1) * samples.len() / buckets,
            );
            let loudest = samples[from..to]
                .iter()
                .map(|s| s.unsigned_abs())
                .max()
                .unwrap_or(0);
            (f32::from(loudest) / 32768.0).min(1.0)
        })
        .collect()
}

impl Ffmpeg {
    /// The pauses in the sound of `file`: stretches of at least `min_ms` quieter than `noise_db`
    /// (a negative number of decibels, -30 is a good start). The file's own sound is read, picture
    /// ignored. An error when the file has no sound.
    pub fn detect_pauses(
        &self,
        file: &Path,
        noise_db: f64,
        min_ms: u64,
    ) -> Result<Vec<Pause>, MediaError> {
        let filter = format!(
            "silencedetect=noise={noise_db:.1}dB:d={:.3}",
            min_ms as f64 / 1000.0
        );
        let out = Command::new(&self.path)
            .args(["-hide_banner", "-nostats", "-i"])
            .arg(file)
            .args(["-vn", "-af", &filter, "-f", "null", "-"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| MediaError::Io(e.to_string()))?;
        let log = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() {
            return Err(MediaError::Failed(format!(
                "{} has no sound ffmpeg can analyze: {}",
                file.display(),
                log.lines().last().unwrap_or("").trim()
            )));
        }
        Ok(parse_pauses(&log, None))
    }

    /// `buckets` peaks (0 to 1) of the sound of `file`, decoded by ffmpeg to 8 kHz mono PCM: the
    /// data of a waveform picture. At most half an hour of sound is read.
    pub fn waveform(&self, file: &Path, buckets: usize) -> Result<Vec<f32>, MediaError> {
        if !(MIN_BUCKETS..=MAX_BUCKETS).contains(&buckets) {
            return Err(MediaError::Failed(format!(
                "a waveform has {MIN_BUCKETS} to {MAX_BUCKETS} buckets, not {buckets}"
            )));
        }
        let mut child = Command::new(&self.path)
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(file)
            .args(["-vn", "-ac", "1", "-ar", &WAVEFORM_RATE.to_string()])
            .args(["-t", &(MAX_WAVEFORM_MS / 1000).to_string()])
            .args(["-f", "s16le", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| MediaError::Io(e.to_string()))?;
        let mut bytes = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout
                .read_to_end(&mut bytes)
                .map_err(|e| MediaError::Io(e.to_string()))?;
        }
        let status = child.wait().map_err(|e| MediaError::Io(e.to_string()))?;
        if !status.success() || bytes.len() < 2 {
            return Err(MediaError::Failed(format!(
                "{} has no sound ffmpeg can decode",
                file.display()
            )));
        }
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        Ok(peaks_from_samples(&samples, buckets))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauses_are_read_from_the_log_and_an_open_one_ends_with_the_file() {
        let log = "Input #0, wav, from 'a.wav':\n  Duration: 00:00:10.00, bitrate: 705 kb/s\n[silencedetect @ 0x1] silence_start: 1.5\n[silencedetect @ 0x1] silence_end: 3.25 | silence_duration: 1.75\n[silencedetect @ 0x1] silence_start: 8.5\n";
        assert_eq!(
            parse_pauses(log, None),
            [
                Pause {
                    start_ms: 1500,
                    end_ms: 3250
                },
                Pause {
                    start_ms: 8500,
                    end_ms: 10_000
                },
            ]
        );
        assert_eq!(
            parse_pauses(log, Some(9000)).last().map(|p| p.end_ms),
            Some(9000)
        );
    }

    #[test]
    fn a_negative_start_is_clamped_and_noise_lines_are_ignored() {
        let log = "[silencedetect @ 0x1] silence_start: -0.0023\nsize=N/A time=00:00:01\n[silencedetect @ 0x1] silence_end: 0.5 | silence_duration: 0.5\nsilence_end: oops\n";
        assert_eq!(
            parse_pauses(log, None),
            [Pause {
                start_ms: 0,
                end_ms: 500
            }]
        );
        assert!(parse_pauses("nothing here", None).is_empty());
    }

    #[test]
    fn peaks_are_the_loudest_sample_of_each_slice() {
        let samples = [0, 100, -16384, 50, 32767, -32768, 0, 0];
        let peaks = peaks_from_samples(&samples, 4);
        assert_eq!(peaks.len(), 4);
        assert!((peaks[0] - 100.0 / 32768.0).abs() < 1e-6);
        assert!((peaks[1] - 0.5).abs() < 1e-6);
        assert_eq!(peaks[2], 1.0);
        assert_eq!(peaks[3], 0.0);
        assert_eq!(peaks_from_samples(&samples, 100).len(), samples.len());
        assert!(peaks_from_samples(&[], 10).is_empty());
    }
}
