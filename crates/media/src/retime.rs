//! The sound of a retimed clip (`docs/wiki/video-editing.md#speed`).
//!
//! A clip played at another speed plays its sound at that speed too. The file's span is cut into
//! slices, each stretched or squeezed to its slice of the timeline with `atempo` (which keeps
//! the pitch; a slice's rate beyond what one `atempo` does, 0.5 to 2, is a chain of them), and
//! the slices are joined. A constant speed is one slice; a ramp is a slice every 250 ms of
//! timeline, each at the average rate of its stretch.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::{Ffmpeg, MediaError};

/// One stretch of the file and how long it lasts once retimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    /// Where it starts in the file, in milliseconds from the start of the file.
    pub source_start_ms: u64,
    pub source_ms: u64,
    /// How long it lasts on the timeline.
    pub timeline_ms: u64,
}

impl Slice {
    /// How fast the slice plays: seconds of file per second of timeline.
    pub fn rate(&self) -> f64 {
        self.source_ms as f64 / self.timeline_ms.max(1) as f64
    }
}

/// The `atempo` factors that multiply to `rate`, each inside the 0.5 to 2 one filter allows.
pub fn tempo_chain(rate: f64) -> Vec<f64> {
    let mut left = rate.clamp(0.0625, 16.0);
    let mut chain = Vec::new();
    while left > 2.0 {
        chain.push(2.0);
        left /= 2.0;
    }
    while left < 0.5 {
        chain.push(0.5);
        left /= 0.5;
    }
    chain.push(left);
    chain
}

fn seconds(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// The `-filter_complex` graph that turns input 0's sound into one retimed stream `[a]`.
pub fn filter_graph(slices: &[Slice]) -> String {
    let n = slices.len();
    let mut graph = String::new();
    if n > 1 {
        let labels: String = (0..n).map(|i| format!("[s{i}]")).collect();
        graph.push_str(&format!("[0:a]asplit={n}{labels};"));
    }
    for (i, s) in slices.iter().enumerate() {
        let input = if n > 1 {
            format!("[s{i}]")
        } else {
            "[0:a]".to_string()
        };
        graph.push_str(&format!(
            "{input}atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS",
            seconds(s.source_start_ms),
            seconds(s.source_start_ms + s.source_ms)
        ));
        for factor in tempo_chain(s.rate()) {
            if (factor - 1.0).abs() > 1e-6 {
                graph.push_str(&format!(",atempo={factor:.6}"));
            }
        }
        graph.push_str(&format!("[r{i}];"));
    }
    let joined: String = (0..n).map(|i| format!("[r{i}]")).collect();
    graph.push_str(&format!("{joined}concat=n={n}:v=0:a=1[a]"));
    graph
}

/// Writes the retimed sound of `input` to `output` (WAV, 48 kHz stereo).
pub fn retime_audio(
    ffmpeg: &Ffmpeg,
    input: &Path,
    slices: &[Slice],
    output: &Path,
) -> Result<(), MediaError> {
    if slices.is_empty() {
        return Err(MediaError::Io("nothing to retime".into()));
    }
    let out = Command::new(&ffmpeg.path)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input)
        .args(["-filter_complex", &filter_graph(slices), "-map", "[a]"])
        .args(["-vn", "-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le"])
        .args(["-f", "wav"])
        .arg(output)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| MediaError::Io(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        let _ = std::fs::remove_file(output);
        Err(MediaError::Failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_chains_multiply_to_the_rate_with_every_step_in_range() {
        for rate in [0.25, 0.3, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0] {
            let chain = tempo_chain(rate);
            assert!(
                chain.iter().all(|f| (0.5..=2.0).contains(f)),
                "{rate}: {chain:?}"
            );
            let product: f64 = chain.iter().product();
            assert!((product - rate).abs() < 1e-9, "{rate}: {product}");
        }
        assert_eq!(tempo_chain(4.0), [2.0, 2.0]);
        assert_eq!(tempo_chain(0.25), [0.5, 0.5]);
    }

    #[test]
    fn a_constant_speed_is_one_trimmed_and_stretched_slice() {
        let graph = filter_graph(&[Slice {
            source_start_ms: 1000,
            source_ms: 4000,
            timeline_ms: 2000,
        }]);
        assert_eq!(
            graph,
            "[0:a]atrim=start=1.000:end=5.000,asetpts=PTS-STARTPTS,atempo=2.000000[r0];[r0]concat=n=1:v=0:a=1[a]"
        );
    }

    #[test]
    fn a_ramp_splits_the_input_and_joins_the_slices_in_order() {
        let slices = [
            Slice {
                source_start_ms: 0,
                source_ms: 250,
                timeline_ms: 250,
            },
            Slice {
                source_start_ms: 250,
                source_ms: 750,
                timeline_ms: 250,
            },
        ];
        let graph = filter_graph(&slices);
        assert!(graph.starts_with("[0:a]asplit=2[s0][s1];"), "{graph}");
        assert!(graph.contains("[s0]atrim=start=0.000:end=0.250,asetpts=PTS-STARTPTS[r0];"));
        assert!(graph.contains("[s1]atrim=start=0.250:end=1.000,asetpts=PTS-STARTPTS,atempo=2.000000,atempo=1.500000[r1];"));
        assert!(graph.ends_with("[r0][r1]concat=n=2:v=0:a=1[a]"));
    }
}
