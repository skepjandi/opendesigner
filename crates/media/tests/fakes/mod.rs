//! Fake speech and background programs: tiny shell scripts that record their arguments and print
//! canned results in each real program's format. Shared by the integration tests of this crate.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const CPP_JSON: &str = r#"{"transcription": [
  {"offsets": {"from": 0, "to": 100}, "text": "[_BEG_]"},
  {"offsets": {"from": 200, "to": 600}, "text": " Hello"},
  {"offsets": {"from": 600, "to": 900}, "text": " world"},
  {"offsets": {"from": 900, "to": 950}, "text": "."},
  {"offsets": {"from": 2000, "to": 2400}, "text": " Again"}
]}"#;

pub const OPENAI_JSON: &str = r#"{"text": " Hello world.", "segments": [
  {"words": [{"word": " Hello", "start": 0.2, "end": 0.6}, {"word": " world.", "start": 0.6, "end": 0.95}]},
  {"words": [{"word": " Again", "start": 2.0, "end": 2.4}]}
]}"#;

pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A fake `whisper-cli`: records its arguments (one per line) in `args.txt` beside it and writes
/// `CPP_JSON` where `-of` says (plus `.json`).
pub fn fake_cpp(dir: &Path, name: &str) -> PathBuf {
    let rec = dir.join("args.txt");
    script(
        dir,
        name,
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nwhile [ $# -gt 0 ]; do [ \"$1\" = -of ] && of=\"$2\"; shift; done\ncat > \"$of.json\" <<'EOF'\n{CPP_JSON}\nEOF\n",
            rec.display()
        ),
    )
}

/// A fake OpenAI `whisper`: records its arguments and writes `OPENAI_JSON` into `--output_dir`,
/// named after the input.
pub fn fake_openai(dir: &Path, name: &str) -> PathBuf {
    let rec = dir.join("args.txt");
    script(
        dir,
        name,
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nin=\"$1\"\nwhile [ $# -gt 0 ]; do [ \"$1\" = --output_dir ] && out=\"$2\"; shift; done\ncat > \"$out/$(basename \"$in\" .wav).json\" <<'EOF'\n{OPENAI_JSON}\nEOF\n",
            rec.display()
        ),
    )
}

/// A fake `rembg` that copies `png` to its last argument.
pub fn fake_rembg(dir: &Path, png: &Path) -> PathBuf {
    let rec = dir.join("rembg-args.txt");
    script(
        dir,
        "rembg",
        &format!(
            "printf '%s\\n' \"$@\" > '{}'\nfor last; do :; done\ncp '{}' \"$last\"",
            rec.display(),
            png.display()
        ),
    )
}

pub fn recorded(dir: &Path, file: &str) -> Vec<String> {
    std::fs::read_to_string(dir.join(file))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The 8 bytes every PNG starts with, then filler: what the checks look at.
pub fn png_bytes() -> Vec<u8> {
    let mut b = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    b.extend_from_slice(b"cutout");
    b
}
