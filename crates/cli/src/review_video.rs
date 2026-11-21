//! `opendesigner review-video`: the animation director's check of a video, the same report the
//! `review_video` tool gives an agent. Read-only.

use std::path::PathBuf;

use clap::Args;
use od_agent::review_video;
use od_agent::video_export::Source;
use od_core::Workspace;

use crate::CliError;

#[derive(Args)]
pub struct ReviewVideo {
    /// The project folder.
    dir: PathBuf,
    /// Id of the video (default: the first).
    #[arg(long)]
    video: Option<String>,
    /// How many moments to render, 8 to 16.
    #[arg(long)]
    samples: Option<usize>,
    /// The music's tempo in beats a minute (30 to 300), to check that cuts land on a beat.
    #[arg(long)]
    bpm: Option<f64>,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

pub fn run(args: ReviewVideo) -> Result<(), CliError> {
    let ws = Workspace::open(&args.dir)?;
    let report = review_video::review_at_tempo(
        &Source::of(&ws),
        args.video.as_deref(),
        args.samples,
        args.bpm,
    )
    .map_err(|e| CliError::Invalid(e.0))?;
    if args.json {
        println!("{}", report.to_json());
    } else {
        println!("{}", report.text());
    }
    Ok(())
}
