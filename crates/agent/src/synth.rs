//! A small sound maker, so a video never has to wait for a sound from the internet: clicks,
//! whooshes, risers, hits, chimes, a drone, a metronome tick, a beat and a short stinger, written
//! as 16-bit PCM WAV files at 44.1 kHz into the project's `media/` folder.
//!
//! Everything is computed here from sine waves, noise and envelopes (no samples, no network, no
//! dependency), so the result has no licence to honour: the sidecar next to the file says it was
//! synthesized. The output is deterministic: the same kind, length, pitch and seed give the same
//! bytes. Every sound is normalized to a peak of [`PEAK`] (about -1 dBFS, so it never clips) and
//! starts and ends at exact zero after a short fade, so it never clicks.
//!
//! These are building blocks (accents, transitions, a simple loop), not music: a music bed is
//! better from Freesound (`search_sounds`) or from the user.

use std::f32::consts::{PI, TAU};
use std::path::Path;

use serde::Serialize;

/// Samples per second of every sound made here.
pub const SAMPLE_RATE: u32 = 44_100;
/// The loudest sample after normalizing, as a fraction of full scale (about -1 dBFS).
pub const PEAK: f32 = 0.89;
/// The longest sound, in milliseconds.
pub const MAX_DURATION_MS: u32 = 60_000;
/// The shortest sound, in milliseconds.
pub const MIN_DURATION_MS: u32 = 20;

/// The sounds that can be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Click,
    Pop,
    Whoosh,
    SwooshUp,
    SwooshDown,
    Riser,
    Hit,
    Chime,
    Drone,
    Tick,
    Beat,
    Stinger,
    Key,
    Blip,
    Pad,
    Impact,
    Scratch,
}

impl Kind {
    /// The names the tool accepts, one per kind.
    pub const NAMES: [&'static str; 17] = [
        "click",
        "pop",
        "whoosh",
        "swoosh_up",
        "swoosh_down",
        "riser",
        "hit",
        "chime",
        "drone",
        "tick",
        "beat",
        "stinger",
        "key",
        "blip",
        "pad",
        "impact",
        "scratch",
    ];

    /// A kind by name.
    pub fn parse(name: &str) -> Option<Kind> {
        Some(match name {
            "click" => Kind::Click,
            "pop" => Kind::Pop,
            "whoosh" => Kind::Whoosh,
            "swoosh_up" => Kind::SwooshUp,
            "swoosh_down" => Kind::SwooshDown,
            "riser" => Kind::Riser,
            "hit" => Kind::Hit,
            "chime" => Kind::Chime,
            "drone" => Kind::Drone,
            "tick" => Kind::Tick,
            "beat" => Kind::Beat,
            "stinger" => Kind::Stinger,
            "key" => Kind::Key,
            "blip" => Kind::Blip,
            "pad" => Kind::Pad,
            "impact" => Kind::Impact,
            "scratch" => Kind::Scratch,
            _ => return None,
        })
    }

    /// A fitting length when the caller gives none.
    pub fn default_ms(self) -> u32 {
        match self {
            Kind::Click => 80,
            Kind::Tick => 60,
            Kind::Pop => 150,
            Kind::Hit => 1_200,
            Kind::Chime => 1_500,
            Kind::Whoosh | Kind::SwooshUp | Kind::SwooshDown => 800,
            Kind::Riser => 2_000,
            Kind::Drone => 8_000,
            Kind::Beat => 4_000,
            Kind::Stinger => 1_200,
            Kind::Key => 120,
            Kind::Blip => 140,
            Kind::Pad => 6_000,
            Kind::Impact => 1_600,
            Kind::Scratch => 700,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Click => "click",
            Kind::Pop => "pop",
            Kind::Whoosh => "whoosh",
            Kind::SwooshUp => "swoosh_up",
            Kind::SwooshDown => "swoosh_down",
            Kind::Riser => "riser",
            Kind::Hit => "hit",
            Kind::Chime => "chime",
            Kind::Drone => "drone",
            Kind::Tick => "tick",
            Kind::Beat => "beat",
            Kind::Stinger => "stinger",
            Kind::Key => "key",
            Kind::Blip => "blip",
            Kind::Pad => "pad",
            Kind::Impact => "impact",
            Kind::Scratch => "scratch",
        }
    }

    /// Whether the kind goes through the mastering pass (a common level and a limiter) instead of
    /// the plain peak normalisation.
    pub fn mastered(self) -> bool {
        matches!(
            self,
            Kind::Key | Kind::Blip | Kind::Pad | Kind::Impact | Kind::Scratch
        )
    }
}

/// What to make.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub kind: Kind,
    /// How long the sound is. A `beat` takes its length from `bpm` and `bars` instead.
    pub duration_ms: u32,
    /// Changes the noise, the choice of notes and, when no pitch or tempo is given, the pitch
    /// and the tempo; the same seed gives the same sound.
    pub seed: u64,
    /// The base pitch of the tonal kinds (chime, drone, tick, riser, hit, stinger).
    pub pitch_hz: Option<f32>,
    /// Beats per minute of a `beat` (default 120).
    pub bpm: Option<f32>,
    /// Bars of 4 beats in a `beat` (default: as many as fit `duration_ms`, at least 1).
    pub bars: Option<u32>,
    /// Two channels instead of one (the right one a few milliseconds later, for width).
    pub stereo: bool,
}

impl Spec {
    pub fn new(kind: Kind, duration_ms: u32) -> Spec {
        Spec {
            kind,
            duration_ms,
            seed: 1,
            pitch_hz: None,
            bpm: None,
            bars: None,
            stereo: false,
        }
    }
}

/// A finished sound.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// One vector per channel, samples in -1..1.
    pub channels: Vec<Vec<f32>>,
    pub duration_ms: u32,
}

/// A small deterministic random number generator (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in -1..1.
    fn noise(&mut self) -> f32 {
        ((self.next() >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// A state-variable filter (Chamberlin): low, band and high pass of one signal.
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    fn new() -> Svf {
        Svf {
            low: 0.0,
            band: 0.0,
        }
    }

    /// One sample through the filter at `fc` Hz; returns (low, band, high).
    fn step(&mut self, x: f32, fc: f32) -> (f32, f32, f32) {
        let f = 2.0 * (PI * fc.min(8_000.0) / SAMPLE_RATE as f32).sin();
        self.low += f * self.band;
        let high = x - self.low - 0.35 * self.band;
        self.band += f * high;
        (self.low, self.band, high)
    }
}

fn secs(n: usize) -> f32 {
    n as f32 / SAMPLE_RATE as f32
}

fn samples_of(ms: u32) -> usize {
    (ms as u64 * SAMPLE_RATE as u64 / 1000) as usize
}

fn hz_of_semitones(base: f32, semitones: f32) -> f32 {
    base * (semitones / 12.0).exp2()
}

/// Semitone steps of a major pentatonic, so every pitch a seed picks sounds good with the others.
const SCALE_STEPS: [f32; 5] = [0.0, 2.0, 4.0, 7.0, 9.0];
/// Tempos a seed picks for a `beat`, in beats per minute; the first is the default of seed 1.
const TEMPOS: [f32; 6] = [120.0, 104.0, 112.0, 96.0, 124.0, 108.0];

/// What a seed changes when the caller named no pitch or tempo: the pitch moves along the scale
/// (and an octave, now and then) and the beat takes another tempo. Seed 1 keeps the defaults.
fn vary_by_seed(spec: &mut Spec) {
    let n = spec.seed.wrapping_sub(1);
    if spec.pitch_hz.is_none() {
        let step = SCALE_STEPS[(n % 5) as usize] + 12.0 * ((n / 5) % 2) as f32;
        let base = match spec.kind {
            Kind::Pop => 700.0,
            Kind::Riser => 220.0,
            Kind::Hit => 48.0,
            Kind::Chime => 880.0,
            Kind::Drone => 110.0,
            Kind::Tick => 1_500.0,
            Kind::Stinger => 523.25,
            Kind::Key => 1_400.0,
            Kind::Blip => 880.0,
            Kind::Pad => 220.0,
            Kind::Impact => 62.0,
            Kind::Scratch => 900.0,
            _ => 0.0,
        };
        if base > 0.0 {
            // a low thump stays low: its steps go down an octave instead of up
            let step = if matches!(spec.kind, Kind::Hit | Kind::Impact) {
                step % 12.0
            } else {
                step
            };
            spec.pitch_hz = Some(hz_of_semitones(base, step));
        }
    }
    if spec.kind == Kind::Beat && spec.bpm.is_none() {
        spec.bpm = Some(TEMPOS[(n % TEMPOS.len() as u64) as usize]);
    }
}

/// Makes the sound. Errors say what to change.
pub fn render(spec: &Spec) -> Result<Rendered, String> {
    let mut spec = spec.clone();
    vary_by_seed(&mut spec);
    if spec.kind == Kind::Beat {
        let bpm = spec.bpm.unwrap_or(120.0);
        if !(40.0..=240.0).contains(&bpm) {
            return Err("bpm is between 40 and 240".into());
        }
        let bar_ms = 4.0 * 60_000.0 / bpm;
        let bars = match spec.bars {
            Some(0) => return Err("bars is at least 1".into()),
            Some(b) => b,
            None => ((spec.duration_ms as f32 / bar_ms).round() as u32).max(1),
        };
        spec.bpm = Some(bpm);
        spec.bars = Some(bars);
        let ms = (bar_ms * bars as f32).round();
        if ms > MAX_DURATION_MS as f32 {
            return Err(format!(
                "{bars} bars at {bpm} bpm last {:.0} s; the longest sound is {} s",
                ms / 1000.0,
                MAX_DURATION_MS / 1000
            ));
        }
        spec.duration_ms = ms as u32;
    }
    if !(MIN_DURATION_MS..=MAX_DURATION_MS).contains(&spec.duration_ms) {
        return Err(format!(
            "duration_ms is between {MIN_DURATION_MS} and {MAX_DURATION_MS}"
        ));
    }
    if let Some(p) = spec.pitch_hz {
        if !(20.0..=8_000.0).contains(&p) {
            return Err("pitch_hz is between 20 and 8000".into());
        }
    }
    let n = samples_of(spec.duration_ms).max(2);
    let mut rng = Rng(spec.seed ^ 0xA5A5_5A5A_1234_5678);
    let mut mono = match spec.kind {
        Kind::Click => click(n, &mut rng),
        Kind::Pop => pop(n, spec.pitch_hz),
        Kind::Whoosh => sweep(n, &mut rng, Sweep::Whoosh),
        Kind::SwooshUp => sweep(n, &mut rng, Sweep::Up),
        Kind::SwooshDown => sweep(n, &mut rng, Sweep::Down),
        Kind::Riser => riser(n, &mut rng, spec.pitch_hz),
        Kind::Hit => hit(n, &mut rng, spec.pitch_hz),
        Kind::Chime => chime(n, spec.pitch_hz),
        Kind::Drone => drone(n, spec.pitch_hz),
        Kind::Tick => tick(n, spec.pitch_hz),
        Kind::Beat => beat(
            n,
            &mut rng,
            spec.bpm.unwrap_or(120.0),
            spec.bars.unwrap_or(1),
        ),
        Kind::Stinger => stinger(n, &mut rng, spec.pitch_hz),
        Kind::Key => key(n, &mut rng, spec.pitch_hz),
        Kind::Blip => blip(n, &mut rng, spec.pitch_hz),
        Kind::Pad => pad(n, &mut rng, spec.pitch_hz),
        Kind::Impact => impact(n, &mut rng, spec.pitch_hz),
        Kind::Scratch => scratch(n, &mut rng, spec.pitch_hz),
    };
    // A loopable drone starts and ends at zero by construction: a 2 ms fade is all it needs.
    let fade_ms = if spec.kind == Kind::Drone { 2.0 } else { 8.0 };
    fade(&mut mono, fade_ms);
    if spec.kind.mastered() {
        master(&mut mono)?;
    } else {
        normalize(&mut mono)?;
    }
    let channels = if spec.stereo {
        let delay = (SAMPLE_RATE as usize * 11 / 1000).min(n / 2);
        let mut right = vec![0.0; delay];
        right.extend_from_slice(&mono[..n - delay]);
        vec![mono, right]
    } else {
        vec![mono]
    };
    Ok(Rendered {
        channels,
        duration_ms: spec.duration_ms,
    })
}

/// Fades in and out over `ms` (at most a quarter of the sound each); the first and the last
/// sample are exactly zero.
fn fade(x: &mut [f32], ms: f32) {
    let n = x.len();
    let len = ((ms / 1000.0 * SAMPLE_RATE as f32) as usize).clamp(2, (n / 4).max(2));
    for i in 0..len.min(n) {
        let g = i as f32 / len as f32;
        x[i] *= g;
        x[n - 1 - i] *= g;
    }
}

/// The loudness of a sound: how loud its loudest sample is and how loud it sounds on average,
/// both in dBFS (0 dBFS is full scale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    pub peak_dbfs: f64,
    pub rms_dbfs: f64,
}

/// Measures a sound. A silent sound reads as -120 dBFS rather than as an error.
pub fn measure_loudness(samples: &[f32]) -> Loudness {
    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let rms = if samples.is_empty() {
        0.0
    } else {
        (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt()
    };
    Loudness {
        peak_dbfs: to_dbfs(peak as f64),
        rms_dbfs: to_dbfs(rms as f64),
    }
}

/// A level in dBFS; silence is -120 dBFS.
fn to_dbfs(value: f64) -> f64 {
    if value <= 1e-6 {
        -120.0
    } else {
        20.0 * value.log10()
    }
}

/// Softens the loudest part of a sound before it is scaled: the samples above the knee are bent
/// back with a curve that never passes full scale, so the result has the punch of the original
/// without the harsh edge.
fn soften(x: &mut [f32], knee: f32) {
    for v in x.iter_mut() {
        let a = v.abs();
        if a > knee {
            let over = (a - knee) / (1.0 - knee);
            let bent = knee + (1.0 - knee) * over / (1.0 + over);
            *v = v.signum() * bent;
        }
    }
}

/// The master stage: the sound is softened and then brought to a peak of [`PEAK`], so it never
/// clips and sits at the same level as the other sounds.
fn master(x: &mut [f32]) -> Result<(), String> {
    soften(x, 0.55);
    normalize(x)
}

/// Scales to a peak of [`PEAK`]; a silent result is an error, never a silent file.
fn normalize(x: &mut [f32]) -> Result<(), String> {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if !peak.is_finite() || peak < 1e-6 {
        return Err("the sound came out silent: make it longer".into());
    }
    let g = PEAK / peak;
    for v in x.iter_mut() {
        *v *= g;
    }
    Ok(())
}

fn exp_decay(t: f32, tau: f32) -> f32 {
    (-t / tau.max(1e-4)).exp()
}

fn click(n: usize, rng: &mut Rng) -> Vec<f32> {
    let tau = (secs(n) / 6.0).min(0.012);
    let mut low = 0.0f32;
    (0..n)
        .map(|i| {
            let t = secs(i);
            // a 1 ms noise burst for the snap, a 2.2 kHz body for the tap
            let burst = if t < 0.001 { rng.noise() } else { 0.0 };
            low += 0.5 * (rng.noise() - low);
            let body = (TAU * 2_200.0 * t).sin() * exp_decay(t, tau);
            0.6 * burst + 0.3 * low * exp_decay(t, tau * 0.3) + body
        })
        .collect()
}

fn pop(n: usize, pitch: Option<f32>) -> Vec<f32> {
    let top = pitch.unwrap_or(700.0);
    let tau = (secs(n) / 6.0).min(0.06);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = secs(i);
            // a bubble: pitch falls fast from `top` to a third of it
            let f = top * (1.0 / 3.0 + (2.0 / 3.0) * exp_decay(t, 0.012));
            phase += TAU * f / SAMPLE_RATE as f32;
            phase.sin() * exp_decay(t, tau)
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Sweep {
    Whoosh,
    Up,
    Down,
}

/// Noise through a band-pass whose centre moves: up and back down (whoosh), up or down.
fn sweep(n: usize, rng: &mut Rng, how: Sweep) -> Vec<f32> {
    let mut filter = Svf::new();
    let (lo, hi) = (250.0f32, 6_500.0f32);
    (0..n)
        .map(|i| {
            let p = i as f32 / (n - 1) as f32;
            let (pos, env) = match how {
                Sweep::Whoosh => {
                    let bell = (PI * p).sin();
                    (bell, bell * bell)
                }
                Sweep::Up => (p, p.powf(1.6) * (1.0 - p.powi(12)).max(0.0)),
                Sweep::Down => (1.0 - p, (1.0 - p).powf(1.6) * (1.0 - (1.0 - p).powi(12))),
            };
            let fc = lo * (hi / lo).powf(pos);
            let (_, band, _) = filter.step(rng.noise(), fc);
            band * env
        })
        .collect()
}

/// A tone that climbs two octaves under noise that opens up, getting louder until it ends.
fn riser(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let start = pitch.unwrap_or(220.0);
    let mut phase = 0.0f32;
    let mut filter = Svf::new();
    (0..n)
        .map(|i| {
            let p = i as f32 / (n - 1) as f32;
            let f = start * 4.0f32.powf(p);
            phase += TAU * f / SAMPLE_RATE as f32;
            let (_, band, _) = filter.step(rng.noise(), 400.0 * 12.0f32.powf(p));
            let env = p * p;
            env * (0.6 * phase.sin() + 0.3 * (2.0 * phase).sin() + 0.8 * band)
        })
        .collect()
}

/// A low thump whose pitch drops, with a short burst of noise on top.
fn hit(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let low = pitch.unwrap_or(48.0);
    let total = secs(n);
    let tau = (total / 7.0).min(0.9);
    let mut phase = 0.0f32;
    let mut filter = Svf::new();
    (0..n)
        .map(|i| {
            let t = secs(i);
            let f = low * (1.0 + 2.5 * exp_decay(t, 0.04));
            phase += TAU * f / SAMPLE_RATE as f32;
            let thump = (1.6 * phase.sin()).tanh() * exp_decay(t, tau);
            let (_, _, high) = filter.step(rng.noise(), 1_800.0);
            let burst = (0.5 * high + 0.5 * rng.noise()) * exp_decay(t, 0.05f32.min(total / 8.0));
            thump + 0.45 * burst
        })
        .collect()
}

/// A bell: partials that are not whole multiples, the higher ones dying sooner.
fn chime(n: usize, pitch: Option<f32>) -> Vec<f32> {
    let base = pitch.unwrap_or(880.0);
    let total = secs(n);
    const PARTIALS: [(f32, f32); 4] = [(1.0, 1.0), (2.76, 0.5), (5.4, 0.25), (8.93, 0.12)];
    (0..n)
        .map(|i| {
            let t = secs(i);
            let attack = (t / 0.003).min(1.0);
            PARTIALS
                .iter()
                .enumerate()
                .map(|(k, (ratio, amp))| {
                    let f = base * ratio;
                    if f > 16_000.0 {
                        return 0.0;
                    }
                    amp * (TAU * f * t).sin() * exp_decay(t, total / (5.0 * (1.0 + 0.7 * k as f32)))
                })
                .sum::<f32>()
                * attack
        })
        .collect()
}

/// A soft pad of sines (root, fifth, octave, each with a detuned twin) that loops without a seam:
/// every frequency and the slow swell fit a whole number of times in the sound.
fn drone(n: usize, pitch: Option<f32>) -> Vec<f32> {
    let root = pitch.unwrap_or(110.0);
    let total = secs(n);
    let cycles = |f: f32| (f * total).round().max(1.0);
    let voices: Vec<(f32, f32)> = [
        (1.0, 1.0),
        (1.0035, 0.8),
        (1.5, 0.55),
        (1.4965, 0.45),
        (2.0, 0.35),
        (2.0070, 0.3),
    ]
    .iter()
    .map(|(r, a)| (cycles(root * r), *a))
    .collect();
    let swell = (total / 4.0).round().max(1.0);
    (0..n)
        .map(|i| {
            let p = i as f32 / n as f32;
            let lfo = 1.0 + 0.25 * (TAU * swell * p).sin();
            lfo * voices
                .iter()
                .map(|(c, a)| a * (TAU * c * p).sin())
                .sum::<f32>()
        })
        .collect()
}

/// A metronome tick: a short wooden knock.
fn tick(n: usize, pitch: Option<f32>) -> Vec<f32> {
    let f = pitch.unwrap_or(1_500.0);
    let tau = (secs(n) / 6.0).min(0.02);
    (0..n)
        .map(|i| {
            let t = secs(i);
            let env = exp_decay(t, tau);
            env * ((TAU * f * t).sin() + 0.5 * (TAU * f * 2.4 * t).sin() * exp_decay(t, tau / 2.0))
        })
        .collect()
}

/// A kick on every beat and a hi-hat between the beats, `bars` bars of four.
fn beat(n: usize, rng: &mut Rng, bpm: f32, bars: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; n];
    let beat_len = SAMPLE_RATE as f32 * 60.0 / bpm;
    let beats = bars * 4;
    for b in 0..beats {
        let at = (b as f32 * beat_len) as usize;
        let mut phase = 0.0f32;
        for k in 0..(SAMPLE_RATE as usize * 3 / 10) {
            let Some(slot) = out.get_mut(at + k) else {
                break;
            };
            let t = secs(k);
            let f = 48.0 + 110.0 * exp_decay(t, 0.025);
            phase += TAU * f / SAMPLE_RATE as f32;
            *slot += (1.3 * phase.sin()).tanh() * exp_decay(t, 0.09);
        }
        // the hat sits half a beat later
        let hat_at = ((b as f32 + 0.5) * beat_len) as usize;
        let mut filter = Svf::new();
        for k in 0..(SAMPLE_RATE as usize / 10) {
            let Some(slot) = out.get_mut(hat_at + k) else {
                break;
            };
            let t = secs(k);
            let (_, _, high) = filter.step(rng.noise(), 7_000.0);
            *slot += 0.28 * high * exp_decay(t, 0.025);
        }
    }
    out
}

/// A two or three note motif that ends on a long note, in a major pentatonic around `pitch`.
fn stinger(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let root = pitch.unwrap_or(523.25);
    const MOTIFS: [[f32; 3]; 4] = [
        [0.0, 4.0, 7.0],
        [4.0, 7.0, 12.0],
        [0.0, 7.0, 9.0],
        [7.0, 4.0, 0.0],
    ];
    let motif = MOTIFS[rng.below(MOTIFS.len())];
    let count = 2 + rng.below(2);
    let steps: Vec<f32> = if count == 3 {
        motif.to_vec()
    } else {
        vec![motif[0], motif[2]]
    };
    let total = secs(n);
    let gap = (total / 5.0).min(0.16);
    let mut out = vec![0.0f32; n];
    for (k, semis) in steps.iter().enumerate() {
        let f = hz_of_semitones(root, *semis);
        let start = k as f32 * gap;
        let last = k + 1 == steps.len();
        let tau = if last {
            (total - start) / 5.0
        } else {
            gap * 1.5
        };
        let from = (start * SAMPLE_RATE as f32) as usize;
        for (i, slot) in out.iter_mut().enumerate().skip(from) {
            let t = secs(i) - start;
            let attack = (t / 0.004).min(1.0);
            *slot += attack
                * exp_decay(t, tau)
                * ((TAU * f * t).sin() + 0.3 * (TAU * 2.0 * f * t).sin());
        }
    }
    out
}

/// A typing click: a short noise burst over a woody body, humanized by the seed (the pitch of
/// the click and the moment it lands move a little).
fn key(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let base = pitch.unwrap_or(1_400.0);
    let total = secs(n);
    let tau = (total / 5.0).min(0.03);
    let mut phase = 0.0f32;
    let mut filter = Svf::new();
    (0..n)
        .map(|i| {
            let t = secs(i);
            let f = base * (1.0 + 0.05 * (t / total.max(1e-4)));
            phase += TAU * f / SAMPLE_RATE as f32;
            let (_, band, _) = filter.step(rng.noise(), 2_600.0);
            let click = band * exp_decay(t, tau * 0.35);
            let body = (phase.sin() + 0.4 * (2.0 * phase).sin()) * exp_decay(t, tau);
            0.75 * click + 0.5 * body
        })
        .collect()
}

/// A UI blip: a short tone that steps up and falls away. The seed moves the step and the colour.
fn blip(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let base = pitch.unwrap_or(880.0);
    let total = secs(n);
    let tau = (total / 4.0).min(0.05);
    let step = 0.4 + 0.5 * rng.next() as f32 / u64::MAX as f32;
    let colour = 2.0 + rng.below(3) as f32;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = secs(i);
            let p = (t / total.max(1e-4)).min(1.0);
            let f = base * (1.0 + step * p);
            phase += TAU * f / SAMPLE_RATE as f32;
            let attack = (t / 0.002).min(1.0);
            let tone = (phase.sin() + 0.25 * (colour * phase).sin()) * exp_decay(t, tau);
            attack * tone
        })
        .collect()
}

/// A slow chord bed: a root, its fifth and its octave, each with a detuned twin, swelling slowly.
/// The seed moves the detuning and the swell.
fn pad(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let root = pitch.unwrap_or(220.0);
    let total = secs(n);
    let detune = 0.002 + 0.004 * rng.next() as f32 / u64::MAX as f32;
    let cycles = |f: f32| (f * total).round().max(1.0);
    let voices: Vec<(f32, f32)> = [
        (1.0, 1.0),
        (1.0 + detune, 0.85),
        (1.5, 0.7),
        (1.5 - detune, 0.6),
        (2.0, 0.45),
        (2.0 + detune, 0.4),
    ]
    .iter()
    .map(|(r, a)| (cycles(root * r), *a))
    .collect();
    let swell = (total / 3.0).round().max(1.0);
    (0..n)
        .map(|i| {
            let p = i as f32 / n as f32;
            let lfo = 0.8 + 0.2 * (TAU * swell * p).sin();
            lfo * voices
                .iter()
                .map(|(c, a)| a * (TAU * c * p).sin())
                .sum::<f32>()
        })
        .collect()
}

/// A heavy impact: a deep thump that drops, with a crack of noise on top and a long tail.
fn impact(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let low = pitch.unwrap_or(62.0);
    let total = secs(n);
    let tau = (total / 4.0).min(1.2);
    let mut phase = 0.0f32;
    let mut filter = Svf::new();
    (0..n)
        .map(|i| {
            let t = secs(i);
            let f = low * (1.0 + 3.0 * exp_decay(t, 0.05));
            phase += TAU * f / SAMPLE_RATE as f32;
            let thump = (2.2 * phase.sin()).tanh() * exp_decay(t, tau);
            let (_, _, high) = filter.step(rng.noise(), 1_600.0);
            let crack = (high + 0.4 * rng.noise()) * exp_decay(t, 0.04);
            thump + 0.4 * crack
        })
        .collect()
}

/// A record scratch: noise dragged through a band that moves, over a rising tone.
fn scratch(n: usize, rng: &mut Rng, pitch: Option<f32>) -> Vec<f32> {
    let base = pitch.unwrap_or(900.0);
    let total = secs(n);
    let tau = (total / 3.0).min(0.25);
    let mut phase = 0.0f32;
    let mut filter = Svf::new();
    (0..n)
        .map(|i| {
            let t = secs(i);
            let p = (t / total.max(1e-4)).min(1.0);
            let f = base * (0.7 + 1.4 * p);
            phase += TAU * f / SAMPLE_RATE as f32;
            let fc = 700.0 * 6.0f32.powf(p);
            let (_, band, _) = filter.step(rng.noise(), fc);
            let tone = (phase.sin() + 0.3 * (2.0 * phase).sin()) * exp_decay(t, tau);
            0.8 * band * (1.0 - 0.3 * p) + 0.35 * tone
        })
        .collect()
}

/// The sound as a WAV file: 16-bit PCM, 44.1 kHz, the channels interleaved.
pub fn wav_bytes(channels: &[Vec<f32>]) -> Vec<u8> {
    let frames = channels.first().map_or(0, Vec::len);
    let count = channels.len() as u16;
    let data = (frames * channels.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2 * u32::from(count)).to_le_bytes());
    out.extend_from_slice(&(2 * count).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        for ch in channels {
            let v = (ch[i].clamp(-1.0, 1.0) * 32_767.0).round() as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

/// What is written next to a made sound: `media/<name>.synth.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sidecar {
    pub synthesized: bool,
    pub kind: String,
    pub duration_ms: u32,
    pub seed: u64,
    pub pitch_hz: Option<f32>,
    pub bpm: Option<f32>,
    pub bars: Option<u32>,
    pub stereo: bool,
    pub licence: &'static str,
}

/// What [`make`] wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Made {
    /// `media/<name>.wav`
    pub file: String,
    pub sidecar: String,
    pub duration_ms: u32,
    pub channels: u8,
    pub bytes: u64,
}

/// The sidecar's file name ending.
pub const SIDECAR_SUFFIX: &str = ".synth.json";

/// Makes the sound and writes it into `<project>/media/` with its sidecar. Nothing is
/// overwritten: a free name is found instead.
pub fn make(project: &Path, spec: &Spec, name: Option<&str>) -> Result<Made, String> {
    let rendered = render(spec)?;
    let stem = clean_stem(name.unwrap_or(spec.kind.name()))?;
    let media = project.join("media");
    std::fs::create_dir_all(&media).map_err(|e| format!("{}: {e}", media.display()))?;
    let stem = free_stem(&media, &stem)?;
    let bytes = wav_bytes(&rendered.channels);
    let wav_path = media.join(format!("{stem}.wav"));
    write_new(&wav_path, &bytes)?;
    let sidecar = Sidecar {
        synthesized: true,
        kind: spec.kind.name().to_string(),
        duration_ms: rendered.duration_ms,
        seed: spec.seed,
        pitch_hz: spec.pitch_hz,
        bpm: spec.bpm.filter(|_| spec.kind == Kind::Beat),
        bars: spec.bars.filter(|_| spec.kind == Kind::Beat),
        stereo: spec.stereo,
        licence: "synthesized by OpenDesigner from nothing but math; no third-party licence, nothing to attribute",
    };
    let sidecar_name = format!("{stem}{SIDECAR_SUFFIX}");
    let text = serde_json::to_string_pretty(&sidecar).map_err(|e| e.to_string())?;
    let synth_path = media.join(&sidecar_name);
    if let Err(e) = write_new(&synth_path, text.as_bytes()) {
        let _ = std::fs::remove_file(&wav_path);
        return Err(e);
    }
    let license = crate::sound_license::Sidecar::synthesized(spec.kind.name(), &bytes);
    if let Err(e) = crate::sound_license::write_sidecar(&media, &stem, &license) {
        let _ = std::fs::remove_file(&wav_path);
        let _ = std::fs::remove_file(&synth_path);
        return Err(e.to_string());
    }
    Ok(Made {
        file: format!("media/{stem}.wav"),
        sidecar: format!("media/{sidecar_name}"),
        duration_ms: rendered.duration_ms,
        channels: rendered.channels.len() as u8,
        bytes: bytes.len() as u64,
    })
}

/// A plain file name without an ending: letters, digits, space, `_` and `-`.
fn clean_stem(name: &str) -> Result<String, String> {
    let name = name
        .trim()
        .trim_end_matches(".wav")
        .trim_end_matches(".WAV");
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(['-', ' ']).to_string();
    if trimmed.is_empty() {
        return Err(format!("{name:?} is not a plain file name"));
    }
    Ok(trimmed.chars().take(60).collect())
}

fn free_stem(media: &Path, stem: &str) -> Result<String, String> {
    for n in 1..10_000 {
        let candidate = if n == 1 {
            stem.to_string()
        } else {
            format!("{stem}-{n}")
        };
        let license = format!("{candidate}{}", crate::sound_license::SIDECAR_SUFFIX);
        if !media.join(format!("{candidate}.wav")).exists()
            && !media.join(format!("{candidate}{SIDECAR_SUFFIX}")).exists()
            && !media.join(license).exists()
        {
            return Ok(candidate);
        }
    }
    Err(format!("no free file name for {stem:?} in media/"))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(bytes).map_err(|e| {
        let _ = std::fs::remove_file(path);
        format!("{}: {e}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: Kind, ms: u32) -> Spec {
        Spec::new(kind, ms)
    }

    fn mono(kind: Kind, ms: u32) -> Vec<f32> {
        render(&spec(kind, ms)).unwrap().channels.remove(0)
    }

    #[test]
    fn every_kind_has_the_right_length_never_clips_is_not_silent_and_fades_at_both_ends() {
        for name in Kind::NAMES {
            let kind = Kind::parse(name).unwrap();
            assert_eq!(kind.name(), name);
            let ms = if kind == Kind::Beat { 2000 } else { 700 };
            let x = mono(kind, ms);
            assert_eq!(x.len(), samples_of(ms), "{name}: length");
            let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak <= PEAK + 1e-4, "{name} clips at {peak}");
            assert!(peak >= PEAK - 1e-3, "{name} is not normalized: {peak}");
            let rms = (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
            assert!(rms > 0.01, "{name} is nearly silent: rms {rms}");
            assert!(x.iter().all(|v| v.is_finite()), "{name}: not finite");
            assert_eq!(x[0], 0.0, "{name} does not start at zero");
            assert_eq!(*x.last().unwrap(), 0.0, "{name} does not end at zero");
            // the tonal kinds have no jump between neighbours anywhere (a click would be one);
            // noise changes fast by nature, so it is left out
            let tonal = matches!(
                kind,
                Kind::Chime | Kind::Drone | Kind::Pop | Kind::Stinger | Kind::Tick | Kind::Blip
            );
            let step = x
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0, f32::max);
            assert!(!tonal || step < 0.5, "{name} has a discontinuity of {step}");
            // the first and last milliseconds are quiet (the fade), the 16-bit file too
            let edge = SAMPLE_RATE as usize / 1000;
            let quiet = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(quiet(&x[..edge]) < 0.4 * peak, "{name} start is loud");
            assert!(
                quiet(&x[x.len() - edge..]) < 0.4 * peak,
                "{name} end is loud"
            );
        }
    }

    #[test]
    fn every_new_kind_is_deterministic_and_another_seed_another_sound() {
        for kind in [
            Kind::Key,
            Kind::Blip,
            Kind::Pad,
            Kind::Impact,
            Kind::Scratch,
        ] {
            let mut first = spec(kind, 700);
            first.seed = 7;
            let mut same = first.clone();
            same.seed = 7;
            assert_eq!(
                wav_bytes(&render(&first).unwrap().channels),
                wav_bytes(&render(&same).unwrap().channels),
                "{kind:?} is not deterministic"
            );
            let mut other = first.clone();
            other.seed = 8;
            assert_ne!(
                wav_bytes(&render(&first).unwrap().channels),
                wav_bytes(&render(&other).unwrap().channels),
                "{kind:?} gives the same sound for every seed"
            );
            let x = mono(kind, 700);
            let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak < 1.0, "{kind:?} clips at {peak}");
            let loud = measure_loudness(&x);
            assert!(
                loud.peak_dbfs < 0.0 && loud.peak_dbfs > -6.0,
                "{kind:?}: {loud:?}"
            );
            assert!(loud.rms_dbfs < loud.peak_dbfs, "{kind:?}: {loud:?}");
        }
    }

    #[test]
    fn a_mastered_sound_sits_at_the_same_level_as_the_others() {
        let hit = mono(Kind::Hit, 700);
        let pad = mono(Kind::Pad, 700);
        let hit = measure_loudness(&hit);
        let pad = measure_loudness(&pad);
        assert!(
            (hit.peak_dbfs - pad.peak_dbfs).abs() < 0.5,
            "{hit:?} {pad:?}"
        );
    }

    #[test]
    fn another_seed_gives_every_tonal_kind_and_the_beat_another_sound() {
        for kind in [
            Kind::Pop,
            Kind::Chime,
            Kind::Tick,
            Kind::Drone,
            Kind::Riser,
            Kind::Hit,
            Kind::Beat,
        ] {
            let sounds: std::collections::HashSet<Vec<u8>> = (1..=6)
                .map(|seed| {
                    let mut sp = spec(kind, 900);
                    sp.seed = seed;
                    wav_bytes(&render(&sp).unwrap().channels)
                })
                .collect();
            assert!(sounds.len() >= 5, "{kind:?} gave {} sounds", sounds.len());
        }
    }

    #[test]
    fn a_pitch_or_tempo_asked_for_wins_over_the_seed() {
        let mut a = spec(Kind::Chime, 900);
        a.pitch_hz = Some(440.0);
        let mut b = a.clone();
        b.seed = 4;
        assert_eq!(render(&a).unwrap(), render(&b).unwrap());
    }

    #[test]
    fn the_same_seed_gives_the_same_bytes_and_another_seed_another_sound() {
        for kind in [
            Kind::Whoosh,
            Kind::Hit,
            Kind::Stinger,
            Kind::Beat,
            Kind::Click,
        ] {
            let a = render(&spec(kind, 800)).unwrap();
            let b = render(&spec(kind, 800)).unwrap();
            assert_eq!(wav_bytes(&a.channels), wav_bytes(&b.channels), "{kind:?}");
        }
        let mut other = spec(Kind::Whoosh, 800);
        other.seed = 99;
        assert_ne!(
            render(&spec(Kind::Whoosh, 800)).unwrap(),
            render(&other).unwrap()
        );
        // the stinger's notes follow the seed
        let motifs: std::collections::HashSet<Vec<u8>> = (0..16)
            .map(|s| {
                let mut sp = spec(Kind::Stinger, 900);
                sp.seed = s;
                wav_bytes(&render(&sp).unwrap().channels)
            })
            .collect();
        assert!(motifs.len() > 1, "every seed gave the same stinger");
    }

    #[test]
    fn a_drone_loops_without_a_seam() {
        let x = mono(Kind::Drone, 4000);
        // the end runs into the start as smoothly as any two neighbours do
        let seam = (x[0] - x[x.len() - 1]).abs();
        let max_step = x
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(
            seam <= max_step,
            "seam {seam} against steps up to {max_step}"
        );
    }

    #[test]
    fn a_beat_has_a_kick_on_every_beat_and_its_length_follows_the_tempo() {
        let mut s = spec(Kind::Beat, 1000);
        s.bpm = Some(120.0);
        s.bars = Some(2);
        let r = render(&s).unwrap();
        assert_eq!(r.duration_ms, 4000);
        let x = &r.channels[0];
        assert_eq!(x.len(), samples_of(4000));
        let beat_len = SAMPLE_RATE as usize / 2;
        // the 20 ms after each beat holds the kick's thump: far louder than just before it
        let energy = |a: usize, b: usize| x[a..b].iter().map(|v| v * v).sum::<f32>();
        for b in 1..8 {
            let at = b * beat_len;
            assert!(
                energy(at + 100, at + 900) > 4.0 * energy(at - 900, at - 100),
                "no kick on beat {b}"
            );
        }
        // without bars they follow the duration
        let mut s = spec(Kind::Beat, 4000);
        s.bpm = Some(120.0);
        assert_eq!(render(&s).unwrap().duration_ms, 4000);
    }

    #[test]
    fn pitch_moves_a_chime_and_stereo_has_two_channels() {
        // the dominant partial of a chime sits at the pitch asked for
        let mut s = spec(Kind::Chime, 600);
        s.pitch_hz = Some(440.0);
        let x = render(&s).unwrap().channels.remove(0);
        let power = |f: f32| {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, v) in x.iter().enumerate() {
                let a = TAU * f * secs(i);
                re += v * a.cos();
                im += v * a.sin();
            }
            re * re + im * im
        };
        assert!(power(440.0) > 10.0 * power(660.0));
        s.stereo = true;
        let two = render(&s).unwrap();
        assert_eq!(two.channels.len(), 2);
        assert_eq!(two.channels[0].len(), two.channels[1].len());
        assert_ne!(two.channels[0], two.channels[1]);
    }

    #[test]
    fn bad_asks_say_what_to_change() {
        assert!(
            render(&spec(Kind::Click, 5))
                .unwrap_err()
                .contains("duration_ms")
        );
        assert!(render(&spec(Kind::Click, 600_000)).is_err());
        let mut s = spec(Kind::Chime, 500);
        s.pitch_hz = Some(5.0);
        assert!(render(&s).unwrap_err().contains("pitch_hz"));
        let mut s = spec(Kind::Beat, 1000);
        s.bpm = Some(500.0);
        assert!(render(&s).unwrap_err().contains("bpm"));
        s.bpm = Some(40.0);
        s.bars = Some(16);
        assert!(render(&s).unwrap_err().contains("longest"));
        assert_eq!(Kind::parse("impact"), Some(Kind::Impact));
        assert_eq!(Kind::parse("pad"), Some(Kind::Pad));
        assert_eq!(Kind::parse("laser"), None);
    }

    #[test]
    fn the_wav_is_a_valid_16_bit_pcm_file() {
        let r = render(&{
            let mut s = spec(Kind::Pop, 300);
            s.stereo = true;
            s
        })
        .unwrap();
        let w = wav_bytes(&r.channels);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([w[22], w[23]]), 2, "channels");
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), 44_100);
        assert_eq!(u16::from_le_bytes([w[34], w[35]]), 16);
        let data = u32::from_le_bytes([w[40], w[41], w[42], w[43]]) as usize;
        assert_eq!(data, w.len() - 44);
        assert_eq!(data, samples_of(300) * 4);
        assert_eq!(
            u32::from_le_bytes([w[4], w[5], w[6], w[7]]) as usize,
            w.len() - 8
        );
    }

    #[test]
    fn make_writes_the_file_and_a_sidecar_and_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let a = make(d.path(), &spec(Kind::Hit, 800), None).unwrap();
        assert_eq!(a.file, "media/hit.wav");
        assert_eq!(a.sidecar, "media/hit.synth.json");
        let again = make(d.path(), &spec(Kind::Hit, 800), Some("hit.wav")).unwrap();
        assert_eq!(again.file, "media/hit-2.wav");
        let weird = make(d.path(), &spec(Kind::Click, 100), Some("../x/y z")).unwrap();
        assert!(weird.file.starts_with("media/") && !weird.file[6..].contains('/'));
        assert!(make(d.path(), &spec(Kind::Click, 100), Some("///")).is_err());
        let wav = std::fs::read(d.path().join("media/hit.wav")).unwrap();
        assert_eq!(wav.len() as u64, a.bytes);
        let side: serde_json::Value =
            serde_json::from_slice(&std::fs::read(d.path().join("media/hit.synth.json")).unwrap())
                .unwrap();
        assert_eq!(side["synthesized"], true);
        assert_eq!(side["kind"], "hit");
        assert!(side["licence"].as_str().unwrap().contains("no third-party"));
    }
}
