//! `opendesigner transcribe`, `remove-background` and `doctor`: the external-program tools of the
//! MCP server (`od_agent::external_tools`), run headless against a project folder, and the report
//! of which of those programs are installed.

use std::path::PathBuf;

use clap::Args;
use od_agent::external_tools;
use od_agent::tools::{self, CallCtx, Content, Turns};
use od_core::Workspace;
use od_media::external::Capabilities;
use serde_json::{Map, Value, json};

use crate::{CliError, actor};

#[derive(Args)]
pub struct Transcribe {
    /// The project folder.
    dir: PathBuf,
    /// A media/ audio or footage file to transcribe.
    #[arg(long)]
    file: Option<String>,
    /// A video's id: its sound mix is transcribed (required with --apply).
    #[arg(long = "video")]
    video: Option<String>,
    /// `auto` or a language code such as en.
    #[arg(long)]
    language: Option<String>,
    /// whisper-cli: a model file name in the app's models folder or media/; whisper: a model name.
    #[arg(long)]
    model: Option<String>,
    /// Set the words as the video's captions.
    #[arg(long)]
    apply: bool,
    /// With --file and --apply: where the file starts on the video's timeline.
    #[arg(long, allow_negative_numbers = true)]
    offset_ms: Option<i64>,
    #[arg(long)]
    min_pause_ms: Option<u64>,
    #[arg(long)]
    timeout_s: Option<u64>,
    /// Attribute the edit to this agent instead of the user.
    #[arg(long)]
    agent: Option<String>,
    /// Group the edit under an agent turn id.
    #[arg(long)]
    turn: Option<String>,
}

#[derive(Args)]
pub struct RemoveBackground {
    /// The project folder.
    dir: PathBuf,
    /// A media/ png, jpg or webp.
    #[arg(long)]
    file: String,
    /// A rembg model name, for example u2net.
    #[arg(long)]
    model: Option<String>,
    /// Write only the mask.
    #[arg(long)]
    mask_only: bool,
    #[arg(long)]
    timeout_s: Option<u64>,
}

#[derive(Args)]
pub struct Doctor {
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

fn put(args: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(v) = value {
        args.insert(key.into(), v);
    }
}

fn call(
    dir: &std::path::Path,
    agent: Option<String>,
    name: &str,
    args: Map<String, Value>,
) -> Result<(), CliError> {
    let mut ws = Workspace::open(dir)?;
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws: &mut ws,
        actor: actor(agent)?,
        turns: &mut turns,
        extra_servers: &[],
    };
    let out =
        tools::call(&mut ctx, name, Value::Object(args)).map_err(|e| CliError::Invalid(e.0))?;
    for part in out.content {
        if let Content::Text(text) = part {
            match serde_json::from_str::<Value>(&text) {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or(text)),
                Err(_) => println!("{text}"),
            }
        }
    }
    Ok(())
}

pub fn transcribe(a: Transcribe) -> Result<(), CliError> {
    let mut args = Map::new();
    put(&mut args, "file", a.file.map(Value::from));
    put(&mut args, "video", a.video.map(Value::from));
    put(&mut args, "language", a.language.map(Value::from));
    put(&mut args, "model", a.model.map(Value::from));
    put(&mut args, "apply", a.apply.then_some(Value::Bool(true)));
    put(&mut args, "offset_ms", a.offset_ms.map(Value::from));
    put(&mut args, "min_pause_ms", a.min_pause_ms.map(Value::from));
    put(&mut args, "timeout_s", a.timeout_s.map(Value::from));
    put(&mut args, "turn", a.turn.map(Value::from));
    call(&a.dir, a.agent, "transcribe", args)
}

pub fn remove_background(a: RemoveBackground) -> Result<(), CliError> {
    let mut args = Map::new();
    args.insert("file".into(), json!(a.file));
    put(&mut args, "model", a.model.map(Value::from));
    put(
        &mut args,
        "mask_only",
        a.mask_only.then_some(Value::Bool(true)),
    );
    put(&mut args, "timeout_s", a.timeout_s.map(Value::from));
    call(&a.dir, None, "remove_background", args)
}

/// One line per program, then what to do about the missing ones.
pub fn report_text(caps: &Capabilities) -> String {
    let mut out = String::new();
    for p in &caps.programs {
        match (&p.found, &p.problem) {
            (Some(path), _) => out.push_str(&format!(
                "ok       {:<12} {} ({})\n",
                p.name,
                path.display(),
                if p.version.is_empty() {
                    "no version printed"
                } else {
                    &p.version
                }
            )),
            (None, Some(why)) => out.push_str(&format!("PROBLEM  {:<12} {why}\n", p.name)),
            (None, None) => out.push_str(&format!(
                "missing  {:<12} {}\n         used for: {}\n",
                p.name, p.install_hint, p.purpose
            )),
        }
    }
    if let Some(model) = &caps.whisper_model {
        out.push_str(&format!(
            "speech model (OPENDESIGNER_WHISPER_MODEL): {model}\n"
        ));
    }
    out.push_str(
        "These are programs you install; OpenDesigner runs them locally and uploads nothing.\n",
    );
    out
}

pub fn doctor(a: Doctor) {
    let caps = Capabilities::probe();
    if a.json {
        println!("{}", external_tools::capabilities_json(&caps));
    } else {
        print!("{}", report_text(&caps));
    }
}
