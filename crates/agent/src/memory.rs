//! The agent's memory across tasks: small text notes per workspace, in
//! `<repo>/.opendesigner/workspaces/<name>/memory/` (the legacy project: `<repo>/.opendesigner/memory/`), next to
//! (not inside) the `opendesigner/` folder of generated assets, so the notes can be tracked in git
//! while the assets are ignored. Every path is built from a validated plain name; symlinks are
//! never followed; sizes are capped; writes are atomic (`docs/wiki/agent-integration.md#agent-memory`).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use thiserror::Error;

pub const FOLDER: &str = ".opendesigner";
/// The folder the generated assets live in (`opendesigner/`); the notes live next to it.
pub const ASSETS_DIR: &str = "opendesigner";
pub const MEMORY_DIR: &str = "memory";
pub const MAX_FILE_BYTES: usize = 64 * 1024;
pub const MAX_FILES: usize = 200;
pub const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_NAME_LEN: usize = 64;

const README: &str = "# .opendesigner\n\n\
This folder is the memory of the AI agents that work on this project through OpenDesigner.\n\
It holds small text notes only (Markdown or plain text), never images, video or other binaries.\n\n\
- **Track this folder in git.** The notes are what the next agent (or teammate) should know:\n  \
brand facts, what the user corrected or liked, what worked, mistakes to avoid, decisions.\n\
- The `opendesigner/` folder next to it holds the generated assets (media, exports, pages, \
videos). It can be large, so it is fine to add it to `.gitignore`. OpenDesigner never edits your \
`.gitignore`.\n\
- Notes live in `memory/`. Agents read them before they build (`get_guidance` lists them, \
`read_memory` opens one) and add to them when they finish (`write_memory`). You can read and edit \
them like any file.\n\
- Never put secrets, tokens or passwords here: this folder is meant to be committed and shared.\n";

#[derive(Debug, Error)]
#[error("{0}")]
pub struct MemoryError(String);

fn fail<T>(msg: impl Into<String>) -> Result<T, MemoryError> {
    Err(MemoryError(msg.into()))
}

fn io(what: &str, e: std::io::Error) -> MemoryError {
    MemoryError(format!("{what}: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Append,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub name: String,
    pub bytes: u64,
    /// The first non-empty line, without leading `#` marks.
    pub title: String,
    /// Seconds since the Unix epoch.
    pub modified: u64,
}

/// The folder that holds the notes of the project at `project_dir`.
///
/// Notes belong to one workspace, so what is learned in one never reaches another:
/// - a named workspace (`<repo>/opendesigner/workspaces/<name>/`) keeps them in
///   `<repo>/.opendesigner/workspaces/<name>/`, next to the assets folder so they can be tracked
///   in git while the assets are ignored;
/// - the legacy project (`<repo>/opendesigner/`, the workspace called `main`) keeps them in
///   `<repo>/.opendesigner/`; a named workspace never reads that folder;
/// - a project that is not inside an `opendesigner/` folder keeps them in
///   `<project>/.opendesigner/`, so nothing is ever written outside the folder the user picked.
pub fn folder_of(project_dir: &Path) -> Option<PathBuf> {
    let repo = repo_of(project_dir)?;
    let base = repo.join(FOLDER);
    if is_named_workspace(project_dir) {
        Some(
            base.join(crate::workspaces::WORKSPACES_DIR)
                .join(project_dir.file_name()?),
        )
    } else {
        Some(base)
    }
}

/// Whether `project_dir` is a workspace folder inside `opendesigner/workspaces/`.
fn is_named_workspace(project_dir: &Path) -> bool {
    let root = crate::workspaces::root_of(project_dir);
    project_dir.parent() == Some(root.as_path())
        && root
            .parent()
            .and_then(|a| a.file_name())
            .and_then(|n| n.to_str())
            == Some(ASSETS_DIR)
}

/// The repository folder that holds the assets of the project at `project_dir`.
///
/// The assets folder is the `opendesigner/` folder above the workspaces; its parent is the
/// repository root, where the notes live. A project that is not inside an `opendesigner/`
/// folder keeps its notes inside itself, so nothing is written outside the picked folder.
pub fn repo_of(project_dir: &Path) -> Option<PathBuf> {
    let root = crate::workspaces::root_of(project_dir);
    let assets = root.parent()?;
    if assets.file_name().and_then(|n| n.to_str()) == Some(ASSETS_DIR) {
        assets.parent().map(Path::to_path_buf)
    } else {
        Some(project_dir.to_path_buf())
    }
}

fn memory_dir(project_dir: &Path) -> Result<PathBuf, MemoryError> {
    let folder = folder_of(project_dir)
        .ok_or_else(|| MemoryError("this project has no repository folder for memory".into()))?;
    Ok(folder.join(MEMORY_DIR))
}

/// The memory folder, with the links in the way refused.
fn memory_dir_checked(project_dir: &Path) -> Result<PathBuf, MemoryError> {
    let mem = memory_dir(project_dir)?;
    if let Some(folder) = mem.parent() {
        refuse_links_in(folder)?;
    }
    refuse_symlink(&mem)?;
    Ok(mem)
}

/// Plain names only: `[a-z0-9._-]+`, ending `.md` or `.txt`, no leading dot.
pub fn valid_name(name: &str) -> Result<(), MemoryError> {
    let plain = name.len() <= MAX_NAME_LEN
        && !name.starts_with('.')
        && !name.contains("..")
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
        && (name.ends_with(".md") || name.ends_with(".txt"))
        && name.len() > 3;
    if plain {
        Ok(())
    } else {
        fail(format!(
            "{name:?} is not a valid note name: use lowercase letters, digits, . _ - only, up to \
             {MAX_NAME_LEN} characters, ending .md or .txt (for example brand.md or lessons.md)"
        ))
    }
}

fn is_real_dir(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

fn refuse_symlink(p: &Path) -> Result<(), MemoryError> {
    match fs::symlink_metadata(p) {
        Ok(m) if m.file_type().is_symlink() => fail(format!(
            "{} is a symbolic link; memory never follows links",
            p.display()
        )),
        _ => Ok(()),
    }
}

/// Refuses a link in `folder` or in any folder between it and `.opendesigner` (inclusive).
fn refuse_links_in(folder: &Path) -> Result<(), MemoryError> {
    for part in folder.ancestors() {
        refuse_symlink(part)?;
        if part.file_name().is_some_and(|n| n == FOLDER) {
            break;
        }
    }
    Ok(())
}

/// Creates `.opendesigner/` with its README and `memory/`. Never overwrites an existing README.
pub fn ensure(project_dir: &Path) -> Result<PathBuf, MemoryError> {
    let mem = memory_dir(project_dir)?;
    let folder = mem
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| MemoryError("no memory folder".into()))?;
    refuse_symlink(&folder)?;
    refuse_links_in(&folder)?;
    fs::create_dir_all(&folder).map_err(|e| io("cannot create .opendesigner", e))?;
    refuse_symlink(&mem)?;
    fs::create_dir_all(&mem).map_err(|e| io("cannot create .opendesigner/memory", e))?;
    if !is_real_dir(&folder) || !is_real_dir(&mem) {
        return fail("the memory folder is not a plain folder");
    }
    let readme = folder.join("README.md");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&readme)
    {
        Ok(mut f) => f
            .write_all(README.as_bytes())
            .map_err(|e| io("cannot write the README", e))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(io("cannot write the README", e)),
    }
    Ok(mem)
}

fn title_of(path: &Path) -> String {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return String::new();
    };
    if !meta.is_file() {
        return String::new();
    }
    let Ok(file) = fs::File::open(path) else {
        return String::new();
    };
    let Ok(opened) = file.metadata() else {
        return String::new();
    };
    if !same_file(&meta, &opened) {
        return String::new();
    }
    let mut buf = Vec::new();
    if file.take(4096).read_to_end(&mut buf).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&buf)
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .take(120)
        .collect()
}

fn notes_in(mem: &Path) -> Vec<Note> {
    let mut out: Vec<Note> = fs::read_dir(mem)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            valid_name(&name).ok()?;
            let meta = fs::symlink_metadata(e.path()).ok()?;
            if !meta.is_file() {
                return None;
            }
            Some(Note {
                title: title_of(&e.path()),
                bytes: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The notes, by name. No folder yet means no notes.
pub fn list(project_dir: &Path) -> Result<Vec<Note>, MemoryError> {
    let mem = memory_dir(project_dir)?;
    if let Some(folder) = mem.parent() {
        refuse_symlink(folder)?;
    }
    refuse_symlink(&mem)?;
    Ok(notes_in(&mem))
}

/// Opens a note for reading: the file behind the name must be the file itself, not a link.
fn open_note(path: &Path, name: &str) -> Result<fs::File, MemoryError> {
    let meta =
        fs::symlink_metadata(path).map_err(|_| MemoryError(format!("no note called {name}")))?;
    if !meta.is_file() {
        return fail(format!("{name} is not a regular file"));
    }
    let file = fs::File::open(path).map_err(|e| io("cannot read the note", e))?;
    if !same_file(
        &meta,
        &file.metadata().map_err(|e| io("cannot read the note", e))?,
    ) {
        return fail(format!(
            "{name} is a symbolic link; memory never follows links"
        ));
    }
    Ok(file)
}

/// Whether two metadata describe the same file (a link would describe a different one).
#[cfg(unix)]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}

fn read_text(project_dir: &Path, name: &str) -> Result<String, MemoryError> {
    let mem = memory_dir_checked(project_dir)?;
    let mut file = open_note(&mem.join(name), name)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| io("cannot read the note", e))?;
    String::from_utf8(bytes).map_err(|_| MemoryError(format!("{name} is not UTF-8 text")))
}

pub fn read(project_dir: &Path, name: &str) -> Result<String, MemoryError> {
    valid_name(name)?;
    let mem = memory_dir_checked(project_dir)?;
    let meta = fs::symlink_metadata(mem.join(name))
        .map_err(|_| MemoryError(format!("no note called {name}")))?;
    if meta.len() as usize > MAX_FILE_BYTES {
        return fail(format!(
            "{name} is larger than {} KB; edit it down by hand",
            MAX_FILE_BYTES / 1024
        ));
    }
    read_text(project_dir, name)
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
pub(crate) fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

fn appended(existing: &str, text: &str, date: &str) -> String {
    if existing.trim().is_empty() {
        return format!("{}\n", text.trim_end());
    }
    format!(
        "{}\n\n### {date}\n{}\n",
        existing.trim_end(),
        text.trim_end()
    )
}

/// Writes a note and returns it. `Append` adds a dated entry; `Replace` swaps the whole file.
pub fn write(project_dir: &Path, name: &str, text: &str, mode: Mode) -> Result<Note, MemoryError> {
    valid_name(name)?;
    if text.trim().is_empty() {
        return fail("the note text is empty");
    }
    if text.contains('\0') {
        return fail("the note text contains a NUL byte");
    }
    let mem = ensure(project_dir)?;
    let path = mem.join(name);
    let exists = match fs::symlink_metadata(&path) {
        Ok(m) if !m.is_file() => return fail(format!("{name} is not a regular file")),
        Ok(_) => true,
        Err(_) => false,
    };
    let body = match mode {
        Mode::Append if exists => appended(&read(project_dir, name)?, text, &today()),
        _ => appended("", text, ""),
    };
    if body.len() > MAX_FILE_BYTES {
        return fail(format!(
            "{name} would be {} KB; a note is at most {} KB. Replace it with a shorter, merged version",
            body.len() / 1024 + 1,
            MAX_FILE_BYTES / 1024
        ));
    }
    let others = notes_in(&mem);
    if !exists && others.len() >= MAX_FILES {
        return fail(format!(
            "there are already {MAX_FILES} notes; merge some before adding more"
        ));
    }
    let total: u64 = others
        .iter()
        .filter(|n| n.name != name)
        .map(|n| n.bytes)
        .sum();
    if total as usize + body.len() > MAX_TOTAL_BYTES {
        return fail(format!(
            "the notes would exceed {} MB in total; shorten or merge some",
            MAX_TOTAL_BYTES / (1024 * 1024)
        ));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp = mem.join(format!(".tmp-{}-{stamp}-{name}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| io("cannot write the note", e))?;
    if let Err(e) = file
        .write_all(body.as_bytes())
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&tmp, &path))
    {
        let _ = fs::remove_file(&tmp);
        return Err(io("cannot write the note", e));
    }
    notes_in(&mem)
        .into_iter()
        .find(|n| n.name == name)
        .ok_or_else(|| MemoryError("the note was not written".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("opendesigner/workspaces/app");
        fs::create_dir_all(&dir).unwrap();
        (tmp, dir)
    }

    #[test]
    fn notes_belong_to_one_workspace_and_never_reach_another() {
        let (tmp, dir) = project();
        let other = tmp.path().join("opendesigner/workspaces/other");
        fs::create_dir_all(&other).unwrap();
        write(&dir, "brand.md", "# Brand\nInter", Mode::Append).unwrap();
        let legacy = tmp.path().join("opendesigner");
        write(&legacy, "old.md", "# Old", Mode::Append).unwrap();
        assert!(list(&other).unwrap().is_empty());
        assert!(read(&other, "brand.md").is_err());
        assert!(read(&other, "old.md").is_err());
        let names: Vec<String> = list(&dir).unwrap().into_iter().map(|n| n.name).collect();
        assert_eq!(names, ["brand.md"]);
        assert_ne!(folder_of(&dir), folder_of(&other));
    }

    #[test]
    fn the_folder_depends_on_the_layout() {
        let (tmp, dir) = project();
        assert_eq!(
            folder_of(&dir).unwrap(),
            tmp.path().join(".opendesigner/workspaces/app")
        );
        let legacy = tmp.path().join("opendesigner");
        assert_eq!(
            folder_of(&legacy).unwrap(),
            tmp.path().join(".opendesigner")
        );
    }

    #[test]
    fn ensure_makes_the_folder_and_never_overwrites_the_readme() {
        let (tmp, dir) = project();
        ensure(&dir).unwrap();
        let readme = tmp.path().join(".opendesigner/workspaces/app/README.md");
        assert!(
            fs::read_to_string(&readme)
                .unwrap()
                .contains("Track this folder in git")
        );
        assert!(
            tmp.path()
                .join(".opendesigner/workspaces/app/memory")
                .is_dir()
        );
        assert!(!tmp.path().join(".gitignore").exists());
        fs::write(&readme, "mine").unwrap();
        ensure(&dir).unwrap();
        assert_eq!(fs::read_to_string(&readme).unwrap(), "mine");
    }

    #[test]
    fn names_are_plain() {
        for ok in ["brand.md", "lessons-2.txt", "a_b.md"] {
            assert!(valid_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            ".md",
            ".hidden.md",
            "../x.md",
            "a/b.md",
            "a\\b.md",
            "a..b.md",
            "Brand.md",
            "x.png",
            "x",
            "a b.md",
            "x.md\0",
            "é.md",
            "..",
            "/etc/passwd",
        ] {
            assert!(valid_name(bad).is_err(), "{bad:?}");
        }
        assert!(valid_name(&format!("{}.md", "a".repeat(80))).is_err());
    }

    #[test]
    fn write_list_read_and_append() {
        let (_tmp, dir) = project();
        assert!(list(&dir).unwrap().is_empty());
        let n = write(&dir, "brand.md", "# Brand\nPrimary #112233", Mode::Append).unwrap();
        assert_eq!(n.title, "Brand");
        write(&dir, "brand.md", "Font: Inter", Mode::Append).unwrap();
        let text = read(&dir, "brand.md").unwrap();
        assert!(text.starts_with("# Brand\nPrimary #112233\n\n### "));
        assert!(text.ends_with("\nFont: Inter\n"));
        write(&dir, "brand.md", "# New\nall new", Mode::Replace).unwrap();
        assert_eq!(read(&dir, "brand.md").unwrap(), "# New\nall new\n");
        let rows = list(&dir).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "brand.md");
        assert!(read(&dir, "nope.md").is_err());
    }

    #[test]
    fn replace_is_atomic_and_leaves_no_temp_files() {
        let (tmp, dir) = project();
        write(&dir, "a.md", "one", Mode::Replace).unwrap();
        write(&dir, "a.md", "two", Mode::Replace).unwrap();
        let names: Vec<String> =
            fs::read_dir(tmp.path().join(".opendesigner/workspaces/app/memory"))
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
        assert_eq!(names, ["a.md"]);
    }

    #[test]
    fn traversal_and_empty_text_are_refused() {
        let (tmp, dir) = project();
        for bad in ["../../evil.md", "..", "sub/x.md", ".git"] {
            assert!(write(&dir, bad, "x", Mode::Replace).is_err(), "{bad}");
            assert!(read(&dir, bad).is_err(), "{bad}");
        }
        assert!(write(&dir, "a.md", "  \n", Mode::Replace).is_err());
        assert!(!tmp.path().join("evil.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_followed() {
        let (tmp, dir) = project();
        let secret = tmp.path().join("secret.txt");
        fs::write(&secret, "top secret").unwrap();
        let mem = ensure(&dir).unwrap();
        std::os::unix::fs::symlink(&secret, mem.join("link.md")).unwrap();
        assert!(read(&dir, "link.md").is_err());
        assert!(write(&dir, "link.md", "x", Mode::Replace).is_err());
        assert!(write(&dir, "link.md", "x", Mode::Append).is_err());
        assert_eq!(fs::read_to_string(&secret).unwrap(), "top secret");
        assert!(list(&dir).unwrap().is_empty());

        let (tmp2, dir2) = project();
        let elsewhere = tmp2.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        let folder = tmp2.path().join(".opendesigner/workspaces/app");
        fs::create_dir_all(&folder).unwrap();
        std::os::unix::fs::symlink(&elsewhere, folder.join("memory")).unwrap();
        assert!(write(&dir2, "a.md", "x", Mode::Replace).is_err());
        assert!(list(&dir2).is_err());
        assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
    }

    #[test]
    fn caps_are_enforced() {
        let (_tmp, dir) = project();
        let big = "x".repeat(MAX_FILE_BYTES + 1);
        assert!(write(&dir, "big.md", &big, Mode::Replace).is_err());
        write(
            &dir,
            "ok.md",
            &"y".repeat(MAX_FILE_BYTES - 10),
            Mode::Replace,
        )
        .unwrap();
        assert!(write(&dir, "ok.md", &"y".repeat(100), Mode::Append).is_err());
        for i in 0..31 {
            write(
                &dir,
                &format!("n{i}.md"),
                &"z".repeat(MAX_FILE_BYTES - 10),
                Mode::Replace,
            )
            .unwrap();
        }
        let err = write(
            &dir,
            "n99.md",
            &"z".repeat(MAX_FILE_BYTES - 10),
            Mode::Replace,
        )
        .unwrap_err();
        assert!(err.to_string().contains("in total"), "{err}");
    }

    #[test]
    fn the_file_count_is_capped() {
        let (_tmp, dir) = project();
        for i in 0..MAX_FILES {
            write(&dir, &format!("n{i}.md"), "x", Mode::Replace).unwrap();
        }
        assert!(write(&dir, "one-more.md", "x", Mode::Replace).is_err());
        write(&dir, "n0.md", "again", Mode::Replace).unwrap();
    }

    #[test]
    fn dates_are_civil() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_453), (2025, 12, 31));
        assert_eq!(civil(19_782), (2024, 2, 29));
    }
}
