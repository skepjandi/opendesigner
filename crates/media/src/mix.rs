//! The ffmpeg filter graph that mixes a video's sound: every piece is cut, normalized to stereo,
//! given its volume, fades, pan and delay, music that ducks listens to the voice through a
//! side-chain compressor, and everything is summed in one `amix`
//! (`docs/wiki/video-editing.md#mixing`).
//!
//! Ducking is `sidechaincompress` rather than an envelope computed in Rust: it follows the voice
//! sample by sample with no analysis pass, and the graph is a pure function of the pieces, so what
//! is built here can be tested without running ffmpeg.

use std::path::PathBuf;

/// What a piece of sound is, which decides what it listens to and what listens to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PieceRole {
    /// The sound of a clip of footage: always mixed, never ducked.
    #[default]
    Footage,
    Music,
    /// Drives the ducking of music.
    Voice,
    Sfx,
}

/// How far, how fast and how slowly music gives way to the voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuckSettings {
    /// 0 to 1: the compressor's ratio runs from 1:1 to 20:1.
    pub depth: f32,
    pub attack_ms: u32,
    pub release_ms: u32,
}

/// The compressor starts to act when the voice passes this level (linear, about -30 dBFS).
const DUCK_THRESHOLD: f32 = 0.03;
const MAX_RATIO: f32 = 20.0;

/// One piece of sound on the finished timeline.
#[derive(Debug, Clone)]
pub struct AudioPiece {
    pub file: PathBuf,
    pub role: PieceRole,
    pub in_ms: u64,
    pub start_ms: u64,
    pub duration_ms: u64,
    /// The volume it is played at: 1 is as recorded, 0 is silent.
    pub gain: f32,
    /// How long it takes to reach its volume, and how long it takes to leave.
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
    /// -1 is all left, 1 all right.
    pub pan: f32,
    /// Music only: lower under the voice pieces.
    pub duck: Option<DuckSettings>,
}

fn secs(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// The side-chain compressor's ratio for a ducking depth.
pub fn duck_ratio(depth: f32) -> f32 {
    1.0 + (MAX_RATIO - 1.0) * depth.clamp(0.0, 1.0)
}

fn chain(input: usize, label: usize, p: &AudioPiece) -> String {
    let mut f = format!(
        "[{input}:a]aresample=48000,aformat=channel_layouts=stereo,atrim=start={:.3}:duration={:.3},asetpts=PTS-STARTPTS",
        secs(p.in_ms),
        secs(p.duration_ms)
    );
    if (p.gain - 1.0).abs() > f32::EPSILON {
        f.push_str(&format!(",volume={:.3}", p.gain));
    }
    let fade_in = p.fade_in_ms.min(p.duration_ms);
    if fade_in > 0 {
        f.push_str(&format!(",afade=t=in:st=0:d={:.3}", secs(fade_in)));
    }
    let fade_out = p.fade_out_ms.min(p.duration_ms);
    if fade_out > 0 {
        f.push_str(&format!(
            ",afade=t=out:st={:.3}:d={:.3}",
            secs(p.duration_ms - fade_out),
            secs(fade_out)
        ));
    }
    if p.pan.abs() > f32::EPSILON {
        f.push_str(&format!(
            ",pan=stereo|c0={:.3}*c0|c1={:.3}*c1",
            (1.0 - p.pan).min(1.0),
            (1.0 + p.pan).min(1.0)
        ));
    }
    f.push_str(&format!(",adelay={}:all=1[p{label}];", p.start_ms));
    f
}

/// The `-filter_complex` graph for `pieces` (input `n + 1` is piece `n`, input 0 is the picture),
/// ending in the labelled output `[a]`: the mix padded with silence, so the output ends where the
/// picture does (`-shortest`) even when every sound is shorter than the video. `pieces` must not
/// be empty.
pub fn filter_graph(pieces: &[AudioPiece]) -> String {
    filter_graph_with(pieces, None)
}

/// [`filter_graph`] with `master` (a loudness filter such as `loudnorm=...`) applied to the
/// summed mix, before it is padded, so the filter sees exactly the sound that exports.
pub fn filter_graph_with(pieces: &[AudioPiece], master: Option<&str>) -> String {
    let mut graph: String = pieces
        .iter()
        .enumerate()
        .map(|(n, p)| chain(n + 1, n, p))
        .collect();
    let voices: Vec<usize> = indices(pieces, |p| p.role == PieceRole::Voice);
    let ducked: Vec<usize> = indices(pieces, |p| {
        p.role == PieceRole::Music && p.duck.is_some_and(|d| d.depth > 0.0)
    });
    let ducking = !voices.is_empty() && !ducked.is_empty();
    let mut mixed: Vec<String> = Vec::new();
    for n in 0..pieces.len() {
        let is_voice = voices.contains(&n);
        if !(ducking && is_voice) {
            mixed.push(format!("[p{n}]"));
        }
    }
    if ducking {
        graph.push_str(&voice_bus(&voices, ducked.len()));
        mixed.push("[vo]".into());
        for (k, &n) in ducked.iter().enumerate() {
            let d = pieces[n].duck.unwrap_or(DuckSettings {
                depth: 0.0,
                attack_ms: 1,
                release_ms: 10,
            });
            graph.push_str(&format!(
                "[sc{k}]apad[sp{k}];[p{n}][sp{k}]sidechaincompress=threshold={DUCK_THRESHOLD}:ratio={:.2}:attack={}:release={}:makeup=1[d{n}];",
                duck_ratio(d.depth),
                d.attack_ms,
                d.release_ms
            ));
            if let Some(slot) = mixed.iter_mut().find(|m| **m == format!("[p{n}]")) {
                *slot = format!("[d{n}]");
            }
        }
    }
    graph.push_str(&format!(
        "{}amix=inputs={}:normalize=0:duration=longest[m];",
        mixed.concat(),
        mixed.len()
    ));
    match master {
        Some(filter) => graph.push_str(&format!("[m]{filter}[n];[n]apad[a]")),
        None => graph.push_str("[m]apad[a]"),
    }
    graph
}

fn indices(pieces: &[AudioPiece], keep: impl Fn(&AudioPiece) -> bool) -> Vec<usize> {
    pieces
        .iter()
        .enumerate()
        .filter(|(_, p)| keep(p))
        .map(|(n, _)| n)
        .collect()
}

/// The voices summed into one bus, then split: `[vo]` for the final mix and one side-chain
/// `[sc<k>]` per ducked music piece (each padded with silence afterwards, because the
/// compressor would otherwise end the music where the voice ends).
fn voice_bus(voices: &[usize], ducked: usize) -> String {
    let bus = if let [one] = voices {
        format!("[p{one}]")
    } else {
        let inputs: String = voices.iter().map(|n| format!("[p{n}]")).collect();
        format!(
            "{inputs}amix=inputs={}:normalize=0:duration=longest[vbus];[vbus]",
            voices.len()
        )
    };
    let outs: String = std::iter::once("[vo]".to_string())
        .chain((0..ducked).map(|k| format!("[sc{k}]")))
        .collect();
    format!("{bus}asplit={}{outs};", ducked + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(role: PieceRole) -> AudioPiece {
        AudioPiece {
            file: PathBuf::from("x.wav"),
            role,
            in_ms: 0,
            start_ms: 0,
            duration_ms: 4000,
            gain: 1.0,
            fade_in_ms: 0,
            fade_out_ms: 0,
            pan: 0.0,
            duck: None,
        }
    }

    fn ducking() -> Option<DuckSettings> {
        Some(DuckSettings {
            depth: 1.0,
            attack_ms: 30,
            release_ms: 400,
        })
    }

    #[test]
    fn a_plain_piece_is_cut_leveled_faded_and_delayed() {
        let mut p = piece(PieceRole::Footage);
        (p.in_ms, p.start_ms, p.gain) = (1500, 2000, 0.5);
        (p.fade_in_ms, p.fade_out_ms) = (500, 9999);
        let g = filter_graph(&[p]);
        assert!(g.starts_with("[1:a]aresample=48000,aformat=channel_layouts=stereo,atrim=start=1.500:duration=4.000,asetpts=PTS-STARTPTS,volume=0.500,afade=t=in:st=0:d=0.500,afade=t=out:st=0.000:d=4.000,adelay=2000:all=1[p0];"), "{g}");
        assert!(
            g.ends_with("[p0]amix=inputs=1:normalize=0:duration=longest[m];[m]apad[a]"),
            "{g}"
        );
        assert!(!g.contains("pan=") && !g.contains("sidechain"));
    }

    #[test]
    fn pan_turns_down_the_far_side() {
        let mut p = piece(PieceRole::Sfx);
        p.pan = 0.5;
        assert!(filter_graph(&[p.clone()]).contains("pan=stereo|c0=0.500*c0|c1=1.000*c1"));
        p.pan = -1.0;
        assert!(filter_graph(&[p]).contains("pan=stereo|c0=1.000*c0|c1=0.000*c1"));
    }

    #[test]
    fn music_that_ducks_listens_to_the_voice_through_a_side_chain() {
        let mut music = piece(PieceRole::Music);
        music.duck = ducking();
        let g = filter_graph(&[music, piece(PieceRole::Voice)]);
        assert!(g.contains("[p1]asplit=2[vo][sc0];"), "{g}");
        assert!(
            g.contains("[sc0]apad[sp0];[p0][sp0]sidechaincompress=threshold=0.03:ratio=20.00:attack=30:release=400:makeup=1[d0];"),
            "{g}"
        );
        assert!(
            g.ends_with("[d0][vo]amix=inputs=2:normalize=0:duration=longest[m];[m]apad[a]"),
            "{g}"
        );
    }

    #[test]
    fn several_voices_are_summed_first_and_each_ducked_music_gets_its_own_side_chain() {
        let mut m1 = piece(PieceRole::Music);
        m1.duck = ducking();
        let mut m2 = piece(PieceRole::Music);
        m2.duck = Some(DuckSettings {
            depth: 0.0,
            ..ducking().unwrap()
        });
        let mut m3 = piece(PieceRole::Music);
        m3.duck = ducking();
        let pieces = [
            m1,
            piece(PieceRole::Voice),
            m2,
            piece(PieceRole::Voice),
            m3,
            piece(PieceRole::Footage),
        ];
        let g = filter_graph(&pieces);
        assert!(g.contains("[p1][p3]amix=inputs=2:normalize=0:duration=longest[vbus];[vbus]asplit=3[vo][sc0][sc1];"), "{g}");
        assert!(
            g.contains("[sc0]apad[sp0];[p0][sp0]sidechaincompress"),
            "{g}"
        );
        assert!(
            g.contains("[sc1]apad[sp1];[p4][sp1]sidechaincompress"),
            "{g}"
        );
        assert!(!g.contains("[p2][sp"), "a depth of 0 does not duck");
        assert!(
            g.ends_with(
                "[d0][p2][d4][p5][vo]amix=inputs=5:normalize=0:duration=longest[m];[m]apad[a]"
            ),
            "{g}"
        );
    }

    #[test]
    fn without_voice_nothing_ducks_and_voice_alone_is_just_mixed() {
        let mut music = piece(PieceRole::Music);
        music.duck = ducking();
        assert!(!filter_graph(&[music]).contains("sidechain"));
        let g = filter_graph(&[piece(PieceRole::Voice)]);
        assert!(g.ends_with("[p0]amix=inputs=1:normalize=0:duration=longest[m];[m]apad[a]"));
    }

    #[test]
    fn the_ratio_runs_from_one_to_twenty() {
        assert_eq!(duck_ratio(0.0), 1.0);
        assert_eq!(duck_ratio(1.0), 20.0);
        assert!((duck_ratio(0.5) - 10.5).abs() < 1e-6);
        assert_eq!(duck_ratio(7.0), 20.0);
    }
}

#[cfg(test)]
mod master_tests {
    use super::*;

    fn piece() -> AudioPiece {
        AudioPiece {
            file: "a.wav".into(),
            role: PieceRole::Music,
            in_ms: 0,
            start_ms: 0,
            duration_ms: 1000,
            gain: 1.0,
            fade_in_ms: 0,
            fade_out_ms: 0,
            pan: 0.0,
            duck: None,
        }
    }

    #[test]
    fn the_master_filter_sits_between_the_mix_and_its_padding() {
        let plain = filter_graph(&[piece()]);
        assert!(plain.ends_with("[m]apad[a]"), "{plain}");
        let loud = filter_graph_with(&[piece()], Some("loudnorm=I=-14"));
        assert!(loud.ends_with("[m]loudnorm=I=-14[n];[n]apad[a]"), "{loud}");
        assert_eq!(filter_graph_with(&[piece()], None), plain);
    }
}
