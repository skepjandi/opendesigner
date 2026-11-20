//! `opendesigner`: the headless front end. It runs the same operations as the editor against a
//! project folder, so agents and CI work without a window (`docs/wiki/agent-integration.md`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use od_core::{
    Actor, AnnotationTarget, Node, NodeId, OpError, Operation, ProjectError, Session, Workspace,
    project,
};
mod serve;
mod skillcmd;

use od_agent::export::{Plan, export_assets};
use od_render::RenderError;
use od_store::devices::Device;
use od_store::{ImageFacts, Preset, Verification};
use thiserror::Error;

const MAX_OPS_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Parser)]
#[command(name = "opendesigner", version, about = "Headless OpenDesigner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a project folder with one empty page.
    New {
        dir: PathBuf,
        #[arg(long, default_value = "Untitled")]
        name: String,
    },
    /// Print the page tree with node ids.
    Show { dir: PathBuf },
    /// Apply a JSON array of operations as one transaction and print the receipt as JSON.
    Apply {
        dir: PathBuf,
        ops: PathBuf,
        /// Attribute the edit to this agent instead of the user.
        #[arg(long)]
        agent: Option<String>,
        /// Group the edit under an agent turn id.
        #[arg(long)]
        turn: Option<String>,
    },
    /// Name the current state so it can be restored.
    Checkpoint {
        dir: PathBuf,
        name: String,
        #[arg(long)]
        agent: Option<String>,
    },
    /// List checkpoints.
    Checkpoints { dir: PathBuf },
    /// Return to a checkpoint by appending the inverse of everything since it.
    Restore {
        dir: PathBuf,
        checkpoint: u64,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Render a frame to a PNG, validating it against a store preset when one is given.
    Export {
        dir: PathBuf,
        #[arg(long)]
        frame: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        preset: Option<String>,
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Keep the alpha channel (stores reject it for screenshots).
        #[arg(long)]
        alpha: bool,
        /// Show this language's translations.
        #[arg(long)]
        locale: Option<String>,
    },
    /// List the project's languages and the text nodes each still lacks a translation for.
    Locales { dir: PathBuf },
    /// List the project's videos: clips, when each starts, transitions and zoom.
    Videos { dir: PathBuf },
    /// Draw a video and encode it to exports/<file> as an H.264 MP4 (needs ffmpeg).
    ExportVideo {
        dir: PathBuf,
        /// A video id from `videos`.
        #[arg(long)]
        video: String,
        /// A plain file name ending in .mp4.
        #[arg(long)]
        file: String,
        /// A store video preset (see `video-presets`); nothing is written if the video breaks it.
        #[arg(long)]
        preset: Option<String>,
        /// Show this language's translations.
        #[arg(long)]
        locale: Option<String>,
    },
    /// List the store presets for videos (app previews).
    VideoPresets,
    /// Render several frames into the project's exports/ as <prefix>-01.png, -02, ... and
    /// validate them as a set; nothing is written unless all of them pass.
    ExportSet {
        dir: PathBuf,
        /// Frame ids in order (repeat the flag); every frame in the project when omitted.
        #[arg(long = "frame")]
        frames: Vec<String>,
        #[arg(long)]
        prefix: String,
        #[arg(long)]
        preset: Option<String>,
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Export this language's translations (files are named <prefix>-<locale>-NN.png).
        #[arg(long)]
        locale: Option<String>,
    },
    /// Render one wide frame and cut it into equal slides (a panorama) in the project's exports/.
    ExportPanorama {
        dir: PathBuf,
        #[arg(long)]
        frame: String,
        #[arg(long)]
        slices: u32,
        #[arg(long)]
        prefix: String,
        #[arg(long)]
        preset: Option<String>,
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Export this language's translations (files are named <prefix>-<locale>-NN.png).
        #[arg(long)]
        locale: Option<String>,
    },
    /// Print a device frame as a `create` node for `apply`: phone, phone-punch-hole, tablet or
    /// tablet-wide.
    Device {
        kind: String,
        #[arg(long)]
        screen_width: f64,
        /// A media/ path to show on the screen.
        #[arg(long)]
        image: Option<String>,
    },
    /// Print a device in 3D as a `create` node for `apply`: phone, phone-punch-hole, tablet,
    /// laptop or monitor, drawn in perspective inside its box.
    Device3d {
        model: String,
        /// Width of the viewport in document units.
        #[arg(long, default_value_t = 400.0)]
        screen_width: f64,
        /// A media/ path to show on the screen.
        #[arg(long)]
        image: Option<String>,
        /// Turn in degrees: positive turns the right edge away.
        #[arg(long, default_value_t = -18.0)]
        rotate_y: f64,
        /// Tilt in degrees: positive leans the top away.
        #[arg(long, default_value_t = 8.0)]
        rotate_x: f64,
    },
    /// Animate a node with a named move (spin, turntable, hero_reveal, float, ...) as keyframes.
    Animate {
        dir: PathBuf,
        /// A video id from `videos`.
        #[arg(long)]
        video: String,
        /// The node to move.
        #[arg(long)]
        node: String,
        /// The move: spin, turntable, swing, flip_in, hero_reveal, rise_in, drop_in, float,
        /// push_in, pull_back, orbit, scroll_screen, fade_in, fade_out, slide_in_left,
        /// slide_in_right.
        #[arg(long)]
        preset: String,
        /// When the move starts, from the start of the video.
        #[arg(long, default_value_t = 0)]
        start_ms: u64,
        /// How long the move lasts.
        #[arg(long, default_value_t = 1200)]
        duration_ms: u64,
        /// How far the move goes; 1 is as designed.
        #[arg(long, default_value_t = 1.0)]
        strength: f64,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Validate PNG files as one set against a store preset.
    Validate {
        #[arg(long)]
        preset: String,
        files: Vec<PathBuf>,
    },
    /// List the built-in store presets.
    Presets,
    /// Pin a note on a node (or a page region) for an agent to read.
    Annotate {
        dir: PathBuf,
        note: String,
        /// The node the note is pinned to.
        #[arg(long, conflicts_with = "page")]
        node: Option<String>,
        /// A page, with --region x,y,w,h, instead of a node.
        #[arg(long, requires = "region")]
        page: Option<String>,
        #[arg(long, requires = "page")]
        region: Option<String>,
        #[arg(long)]
        agent: Option<String>,
    },
    /// List annotations.
    Annotations {
        dir: PathBuf,
        /// Include resolved ones.
        #[arg(long)]
        all: bool,
    },
    /// Mark an annotation resolved.
    Resolve {
        dir: PathBuf,
        id: u64,
        /// What was done, in the user's terms.
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Print the guidance an agent receives: pages, selection, open notes, recent user edits.
    Guidance {
        dir: PathBuf,
        /// Pretend these nodes are selected (a front end holds the real selection).
        #[arg(long, value_delimiter = ',')]
        select: Vec<String>,
    },
    /// Serve the project to coding agents over MCP on 127.0.0.1 (stop with Ctrl-C).
    ServeMcp {
        dir: PathBuf,
        /// First port to try; the next ones are tried when it is taken.
        #[arg(long, default_value_t = od_agent::DEFAULT_PORT)]
        port: u16,
        /// Print the token itself (by default only the file that holds it).
        #[arg(long)]
        show_token: bool,
        /// Replace the token first; agents must be reconnected.
        #[arg(long)]
        rotate_token: bool,
        /// Which agent to print connection instructions for: omp, claude-code, codex, gemini,
        /// opencode, cursor or other.
        #[arg(long, default_value = "claude-code")]
        agent: String,
    },
    /// Install, inspect or remove the Agent Skill that teaches coding agents to use OpenDesigner.
    Skill {
        #[command(subcommand)]
        action: skillcmd::SkillAction,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Agent(#[from] od_agent::AgentError),
    #[error(transparent)]
    Op(#[from] OpError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Invalid(String),
    /// The export or set does not meet the store rules; exit code 2.
    #[error("{0}")]
    Violations(String),
}

fn io(path: &Path, source: std::io::Error) -> CliError {
    CliError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Violations(msg)) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn actor(agent: Option<String>) -> Result<Actor, CliError> {
    match agent {
        Some(name) => Ok(Actor::agent(&name)?),
        None => Ok(Actor::User),
    }
}

/// The 3D model named on the command line.
fn parse_model(name: &str) -> Result<od_core::DeviceModel, CliError> {
    od_core::DeviceModel::ALL
        .into_iter()
        .find(|m| m.as_str() == name)
        .ok_or_else(|| {
            let ids: Vec<&str> = od_core::DeviceModel::ALL
                .iter()
                .map(|m| m.as_str())
                .collect();
            CliError::Invalid(format!(
                "unknown 3D model {name:?}; use one of {}",
                ids.join(", ")
            ))
        })
}

/// The named move on the command line.
fn parse_preset(name: &str) -> Result<od_core::Preset, CliError> {
    od_core::Preset::ALL
        .into_iter()
        .find(|p| p.as_str() == name)
        .ok_or_else(|| {
            let ids: Vec<&str> = od_core::Preset::ALL.iter().map(|p| p.as_str()).collect();
            CliError::Invalid(format!(
                "unknown move {name:?}; use one of {}",
                ids.join(", ")
            ))
        })
}

fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>, CliError> {
    let size = fs::metadata(path).map_err(|e| io(path, e))?.len();
    if size > limit {
        return Err(CliError::Invalid(format!(
            "{} is {size} bytes, over the {limit} byte limit",
            path.display()
        )));
    }
    fs::read(path).map_err(|e| io(path, e))
}

fn preset(id: &str) -> Result<&'static Preset, CliError> {
    od_store::find(id).ok_or_else(|| {
        let ids: Vec<&str> = od_store::PRESETS.iter().map(|p| p.id).collect();
        CliError::Invalid(format!(
            "unknown preset {id:?}; available: {}",
            ids.join(", ")
        ))
    })
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::New { dir, name } => {
            project::create(&dir, &name)?;
            println!("created {}", dir.display());
        }
        Command::Show { dir } => {
            let session = project::load(&dir)?;
            for page in &session.document().pages {
                println!("page {} {:?}", page.id, page.name);
                for child in &page.root.children {
                    print_node(child, 1);
                }
            }
        }
        Command::Apply {
            dir,
            ops,
            agent,
            turn,
        } => {
            let mut session = project::load(&dir)?;
            let ops: Vec<Operation> =
                serde_json::from_slice(&read_capped(&ops, MAX_OPS_FILE_BYTES)?)
                    .map_err(|e| CliError::Invalid(format!("operations file: {e}")))?;
            let receipt = session.apply(actor(agent)?, turn, &ops)?;
            project::save(&dir, &session)?;
            let out = serde_json::json!({
                "transaction": receipt.id,
                "created": receipt.created,
                "touched": receipt.touched,
            });
            println!("{out}");
        }
        Command::Checkpoint { dir, name, agent } => {
            let mut session = project::load(&dir)?;
            let id = session.checkpoint(actor(agent)?, &name)?;
            project::save(&dir, &session)?;
            println!("checkpoint {id}");
        }
        Command::Checkpoints { dir } => {
            let session = project::load(&dir)?;
            for cp in session.checkpoints() {
                println!("{}\t{}\t{}\t@{}", cp.id, cp.actor, cp.name, cp.position);
            }
        }
        Command::Restore {
            dir,
            checkpoint,
            agent,
        } => {
            let mut session = project::load(&dir)?;
            let receipt = session.restore(actor(agent)?, checkpoint)?;
            project::save(&dir, &session)?;
            println!(
                "restored checkpoint {checkpoint} (transaction {})",
                receipt.id
            );
        }
        Command::Export {
            dir,
            frame,
            out,
            preset: preset_id,
            scale,
            alpha,
            locale,
        } => {
            export(
                &dir,
                &frame,
                &out,
                preset_id.as_deref(),
                scale,
                alpha,
                locale.as_deref(),
            )?;
        }
        Command::Locales { dir } => {
            let ws = Workspace::open(&dir)?;
            let rows = od_core::view::locales(ws.session.document());
            if rows.is_empty() {
                println!("no languages yet; add translations with set_localized_text operations");
            }
            for r in rows {
                println!(
                    "{}\t{} translated\tmissing: {}",
                    r.locale,
                    r.translated,
                    if r.missing.is_empty() {
                        "none".to_string()
                    } else {
                        r.missing.join(", ")
                    }
                );
            }
        }
        Command::Videos { dir } => {
            let ws = Workspace::open(&dir)?;
            let rows = od_core::view::videos(ws.session.document());
            if rows.is_empty() {
                println!("no videos yet; add one with a create_video operation (see `apply`)");
            }
            for v in rows {
                println!(
                    "{}\t{}\t{}x{} at {} fps\t{:.1} s\t{} frames",
                    v.id,
                    v.name,
                    v.width,
                    v.height,
                    v.fps,
                    v.duration_ms as f64 / 1000.0,
                    v.frame_count
                );
                for c in &v.clips {
                    let speed = if c.speed != 1.0 {
                        format!(" at {}x", c.speed)
                    } else {
                        String::new()
                    };
                    println!(
                        "  {}\t{} ({}){}\tstarts {} ms\t{} ms\t{:?} {} ms\tzoom {} to {}",
                        c.id,
                        if c.footage.is_empty() {
                            c.frame_name.clone()
                        } else {
                            format!("footage {} from {} ms", c.footage, c.footage_in_ms)
                        },
                        if c.footage.is_empty() {
                            &c.frame
                        } else {
                            "footage"
                        },
                        speed,
                        c.start_ms,
                        c.duration_ms,
                        c.transition.kind,
                        c.transition_ms,
                        c.motion.scale_from,
                        c.motion.scale_to
                    );
                }
                for a in &v.animations {
                    let keys: Vec<String> = a
                        .keys
                        .iter()
                        .map(|k| format!("{} at {} ms", k.value, k.at_ms))
                        .collect();
                    println!(
                        "  animation\t{} {}\t{}",
                        a.node,
                        a.prop.as_str(),
                        keys.join(", then ")
                    );
                }
                if v.motion_blur > 0 {
                    println!("  motion blur\t{} pictures per frame", v.motion_blur);
                }
            }
        }
        Command::VideoPresets => {
            for p in od_store::video::VIDEO_PRESETS {
                println!(
                    "{}\t{}\t{}-{} s, up to {} fps and {} MB\trules read on {} from {}",
                    p.id,
                    p.describe_sizes(),
                    p.min_ms / 1000,
                    p.max_ms / 1000,
                    p.max_fps,
                    p.max_bytes / 1_000_000,
                    p.checked,
                    p.source
                );
                println!("  not checked: {}", p.not_checked);
            }
        }
        Command::ExportVideo {
            dir,
            video,
            file,
            preset,
            locale,
        } => {
            let ws = Workspace::open(&dir)?;
            let id = NodeId::parse(&video)?;
            let src = od_agent::video_export::Source::of(&ws);
            let ffmpeg =
                od_media::Ffmpeg::locate().map_err(|e| CliError::Invalid(e.to_string()))?;
            let out = od_agent::video_export::export_video(
                &src,
                &id,
                &file,
                locale.as_deref(),
                preset.as_deref(),
                &ffmpeg,
                &mut |done, total| {
                    if done % 30 == 0 || done == total {
                        eprint!("\rframe {done} of {total}");
                    }
                    true
                },
            )
            .map_err(|e| CliError::Invalid(e.0))?;
            eprintln!();
            for w in &out.warnings {
                eprintln!("warning: {w}");
            }
            if out.ffmpeg_gpl {
                eprintln!(
                    "note: {} is a GPL build; the app does not ship it",
                    out.ffmpeg
                );
            }
            println!(
                "{}\t{}x{} at {} fps\t{} frames\t{:.1} s\t{} bytes\t{}",
                out.path,
                out.width,
                out.height,
                out.fps,
                out.frames,
                out.duration_ms as f64 / 1000.0,
                out.bytes,
                out.encoder
            );
        }
        Command::ExportSet {
            dir,
            frames,
            prefix,
            preset: preset_id,
            scale,
            locale,
        } => {
            let ws = Workspace::open(&dir)?;
            let frames = if frames.is_empty() {
                od_core::view::frames(ws.session.document())
                    .iter()
                    .map(|f| NodeId::parse(&f.id))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                frames
                    .iter()
                    .map(|f| NodeId::parse(f))
                    .collect::<Result<Vec<_>, _>>()?
            };
            let plan = Plan::Set {
                frames: &frames,
                prefix: &prefix,
            };
            run_export(&ws, &plan, preset_id.as_deref(), scale, locale.as_deref())?;
        }
        Command::ExportPanorama {
            dir,
            frame,
            slices,
            prefix,
            preset: preset_id,
            scale,
            locale,
        } => {
            let ws = Workspace::open(&dir)?;
            let frame = NodeId::parse(&frame)?;
            let plan = Plan::Panorama {
                frame: &frame,
                slices,
                prefix: &prefix,
            };
            run_export(&ws, &plan, preset_id.as_deref(), scale, locale.as_deref())?;
        }
        Command::Device {
            kind,
            screen_width,
            image,
        } => {
            let device = Device::parse(&kind).ok_or_else(|| {
                let ids: Vec<&str> = Device::ALL.iter().map(|d| d.id()).collect();
                CliError::Invalid(format!(
                    "unknown device {kind:?}; use one of {}",
                    ids.join(", ")
                ))
            })?;
            if !(screen_width.is_finite() && (50.0..=8192.0).contains(&screen_width)) {
                return Err(CliError::Invalid(
                    "--screen-width must be 50 to 8192".into(),
                ));
            }
            let node = od_store::devices::template(device, screen_width, image.as_deref());
            println!(
                "{}",
                serde_json::to_string_pretty(&node)
                    .map_err(|e| CliError::Invalid(e.to_string()))?
            );
        }
        Command::Device3d {
            model,
            screen_width,
            image,
            rotate_y,
            rotate_x,
        } => {
            let model = parse_model(&model)?;
            if !(screen_width.is_finite() && (50.0..=8192.0).contains(&screen_width)) {
                return Err(CliError::Invalid(
                    "--screen-width must be 50 to 8192".into(),
                ));
            }
            let aspect = model.screen_aspect();
            let (w, h) = if aspect < 1.0 {
                (screen_width, screen_width / aspect)
            } else {
                (screen_width * aspect, screen_width)
            };
            let device = od_core::Device3d {
                model,
                rotate_x,
                rotate_y,
                ..od_core::Device3d::default()
            };
            let mut node = od_core::NewNode::new(od_core::NodeKind::Device3d);
            node.name = Some(format!("{} 3D", model.as_str()));
            node.width = w;
            node.height = h;
            node.image = image;
            node.device3d = Some(device);
            println!(
                "{}",
                serde_json::to_string_pretty(&node)
                    .map_err(|e| CliError::Invalid(e.to_string()))?
            );
        }
        Command::Animate {
            dir,
            video,
            node,
            preset,
            start_ms,
            duration_ms,
            strength,
            agent,
        } => {
            let preset = parse_preset(&preset)?;
            let mut session = project::load(&dir)?;
            let video_id = od_core::NodeId::parse(&video)?;
            let node_id = od_core::NodeId::parse(&node)?;
            let ops = od_core::animation::preset_ops(
                session.document(),
                &video_id,
                &node_id,
                od_core::PresetRequest {
                    preset,
                    start_ms,
                    duration_ms,
                    strength,
                    ease: None,
                },
            )?;
            if ops.is_empty() {
                return Err(CliError::Invalid(format!(
                    "{} would not move {node} from where it is",
                    preset.as_str()
                )));
            }
            let receipt = session.apply(actor(agent)?, None, &ops)?;
            project::save(&dir, &session)?;
            println!(
                "{}",
                serde_json::json!({
                    "transaction": receipt.id,
                    "touched": receipt.touched,
                })
            );
        }
        Command::Validate {
            preset: preset_id,
            files,
        } => validate(&preset_id, &files)?,
        Command::Presets => {
            for p in od_store::PRESETS {
                println!("{}\t{}", p.id, p.description);
            }
        }
        Command::Annotate {
            dir,
            note,
            node,
            page,
            region,
            agent,
        } => {
            let mut ws = Workspace::open(&dir)?;
            let target = annotation_target(node, page, region)?;
            let id = ws
                .annotations
                .add(ws.session.document(), target, &note, actor(agent)?)?;
            ws.save()?;
            println!("annotation {id}");
        }
        Command::Annotations { dir, all } => {
            let ws = Workspace::open(&dir)?;
            for a in ws.annotations.all() {
                if all || a.status == od_core::AnnotationStatus::Open {
                    println!("{}", serde_json::to_string(a).map_err(json_err)?);
                }
            }
        }
        Command::Resolve {
            dir,
            id,
            note,
            agent,
        } => {
            let mut ws = Workspace::open(&dir)?;
            ws.annotations.resolve(id, actor(agent)?, note.as_deref())?;
            ws.save()?;
            println!("resolved {id}");
        }
        Command::Guidance { dir, select } => {
            let mut ws = Workspace::open(&dir)?;
            let ids = select
                .iter()
                .map(|s| NodeId::parse(s))
                .collect::<Result<Vec<_>, _>>()?;
            ws.select(ids)?;
            let json = serde_json::to_string_pretty(&ws.guidance()).map_err(json_err)?;
            println!("{json}");
        }
        Command::ServeMcp {
            dir,
            port,
            show_token,
            rotate_token,
            agent,
        } => serve::run(&dir, port, show_token, rotate_token, &agent)?,
        Command::Skill { action } => skillcmd::run(action)?,
    }
    Ok(())
}

fn json_err(e: serde_json::Error) -> CliError {
    CliError::Invalid(e.to_string())
}

fn annotation_target(
    node: Option<String>,
    page: Option<String>,
    region: Option<String>,
) -> Result<AnnotationTarget, CliError> {
    if let (Some(page), Some(region)) = (page, region) {
        let parts: Vec<f64> = region
            .split(',')
            .map(|p| p.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| CliError::Invalid("--region takes x,y,width,height numbers".into()))?;
        let [x, y, width, height] = parts[..] else {
            return Err(CliError::Invalid("--region takes x,y,width,height".into()));
        };
        return Ok(AnnotationTarget::Region {
            page: NodeId::parse(&page)?,
            x,
            y,
            width,
            height,
        });
    }
    match node {
        Some(id) => Ok(AnnotationTarget::Node {
            id: NodeId::parse(&id)?,
        }),
        None => Err(CliError::Invalid(
            "give --node, or --page with --region".into(),
        )),
    }
}

fn print_node(node: &Node, depth: usize) {
    println!(
        "{}{} {} {:?} at {},{} {}x{}",
        "  ".repeat(depth),
        node.id,
        node.kind.as_str(),
        node.name,
        node.x,
        node.y,
        node.width,
        node.height
    );
    for child in &node.children {
        print_node(child, depth + 1);
    }
}

fn note_verification(p: &Preset) {
    if p.verification == Verification::CopiedFromReference {
        eprintln!(
            "note: the {} preset was copied from a reference project; check it against {}",
            p.id, p.source
        );
    }
}

fn run_export(
    ws: &Workspace,
    plan: &Plan,
    preset_id: Option<&str>,
    scale: f32,
    locale: Option<&str>,
) -> Result<(), CliError> {
    let out = export_assets(ws, plan, preset_id, Some(scale), false, locale).map_err(|e| {
        if e.0.starts_with("not written") {
            CliError::Violations(e.0)
        } else {
            CliError::Invalid(e.0)
        }
    })?;
    for note in &out.warnings {
        eprintln!("warning: {note}");
    }
    if let Some(p) = preset_id.and_then(od_store::find) {
        note_verification(p);
    }
    for f in &out.files {
        println!(
            "wrote {} ({} bytes, {}x{})",
            f.path, f.bytes, f.width, f.height
        );
    }
    Ok(())
}

fn export(
    dir: &Path,
    frame: &str,
    out: &Path,
    preset_id: Option<&str>,
    scale: f32,
    alpha: bool,
    locale: Option<&str>,
) -> Result<(), CliError> {
    let target = preset_id.map(preset).transpose()?;
    let session: Session = project::load(dir)?;
    let frame_id = NodeId::parse(frame)?;
    let localized;
    let doc = match locale {
        Some(l) => {
            localized = session.document().localized(l).ok_or_else(|| {
                CliError::Invalid(format!("the project has no translations for {l:?}"))
            })?;
            &localized
        }
        None => session.document(),
    };
    let rendered = od_agent::video_export::render_still(doc, &frame_id, scale, Some(dir))?;
    for warning in &rendered.warnings {
        eprintln!("warning: {warning}");
    }
    let keep_alpha = alpha && target.is_none_or(|p| p.allow_alpha);
    let bytes = od_render::encode_png(&rendered.pixmap, keep_alpha)?;
    if let Some(p) = target {
        let facts = od_store::inspect_png(&bytes).map_err(|e| CliError::Invalid(e.to_string()))?;
        let violations = p.validate_image(&facts);
        if !violations.is_empty() {
            let list: Vec<String> = violations.iter().map(|v| format!("  - {v}")).collect();
            return Err(CliError::Violations(format!(
                "not written: the export does not meet {} ({})\n{}",
                p.id,
                p.source,
                list.join("\n")
            )));
        }
        note_verification(p);
    }
    fs::write(out, &bytes).map_err(|e| io(out, e))?;
    println!(
        "wrote {} ({} bytes, {}x{})",
        out.display(),
        bytes.len(),
        rendered.pixmap.width(),
        rendered.pixmap.height()
    );
    Ok(())
}

fn validate(preset_id: &str, files: &[PathBuf]) -> Result<(), CliError> {
    let p = preset(preset_id)?;
    let mut facts: Vec<ImageFacts> = Vec::with_capacity(files.len());
    for file in files {
        let bytes = read_capped(file, MAX_IMAGE_BYTES)?;
        facts.push(
            od_store::inspect_png(&bytes)
                .map_err(|e| CliError::Invalid(format!("{}: {e}", file.display())))?,
        );
    }
    let violations = p.validate_set(&facts);
    if violations.is_empty() {
        println!("{} files meet {}", files.len(), p.id);
        note_verification(p);
        return Ok(());
    }
    let list: Vec<String> = violations
        .iter()
        .map(|(i, v)| match i {
            Some(i) => format!("  - {}: {v}", files[*i].display()),
            None => format!("  - set: {v}"),
        })
        .collect();
    Err(CliError::Violations(format!(
        "the set does not meet {} ({})\n{}",
        p.id,
        p.source,
        list.join("\n")
    )))
}
