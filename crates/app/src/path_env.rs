//! The program search path of an app started from the Finder or the Dock.
//!
//! A program launched that way gets the system's minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`),
//! not the one the user's terminal has, so the agent (`omp`, `claude`, `codex`), `ffmpeg` and the
//! other programs the editor starts are "not installed" although they are. At startup the editor
//! adds the folders a terminal would have: the ones the user's login shell reports, and the usual
//! install folders of Homebrew, Cargo, Bun, npm and `~/.local`. Folders already on `PATH` keep their
//! place; nothing is removed.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The folders programs are commonly installed in, relative to the home folder or absolute.
const HOME_DIRS: &[&str] = &[
    ".local/bin",
    ".bun/bin",
    ".cargo/bin",
    ".npm-global/bin",
    ".volta/bin",
    "bin",
];
const SYSTEM_DIRS: &[&str] = &["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"];

/// `current` followed by every folder of `extra` that is not already in it, in order.
pub fn merge_path(current: &OsString, extra: &[PathBuf]) -> OsString {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(current).collect();
    for dir in extra {
        if !dir.as_os_str().is_empty() && !dirs.contains(dir) {
            dirs.push(dir.clone());
        }
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| current.clone())
}

/// The folders from the usual install places that exist.
pub fn usual_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = SYSTEM_DIRS.iter().map(PathBuf::from).collect();
    if let Some(home) = home {
        dirs.extend(HOME_DIRS.iter().map(|d| home.join(d)));
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

/// Whether `path` is the bare system path of a program that was not started from a terminal.
pub fn is_minimal(path: &OsString) -> bool {
    std::env::split_paths(path).all(|d| {
        matches!(
            d.to_str(),
            Some("/usr/bin" | "/bin" | "/usr/sbin" | "/sbin" | "/usr/local/bin")
        )
    })
}

/// The `PATH` between two markers in the output of a shell, ignoring whatever else the shell's
/// start-up files printed.
pub fn path_from_shell_output(output: &str) -> Option<String> {
    const MARK: &str = "__OPENDESIGNER_PATH__";
    let rest = output.split_once(MARK)?.1;
    let path = rest.split_once(MARK)?.0;
    (!path.is_empty()).then(|| path.to_string())
}

/// Adds the folders a terminal would have to this process's `PATH`. Call it before any thread
/// starts: changing the environment is not safe while another thread reads it.
#[cfg(unix)]
pub fn inherit_login_path() {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut extra = Vec::new();
    if is_minimal(&current)
        && let Some(path) = login_shell_path()
    {
        extra.extend(std::env::split_paths(&path));
    }
    extra.extend(usual_dirs(home.as_deref()));
    let merged = merge_path(&current, &extra);
    if merged != current {
        // SAFETY: called first in `run`, before Qt or any thread of ours exists, so no other
        // thread can be reading the environment.
        unsafe { std::env::set_var("PATH", merged) };
    }
}

#[cfg(not(unix))]
pub fn inherit_login_path() {}

/// Asks the user's login shell for its `PATH` (an interactive one, because that is where people
/// put `bun`, `nvm` and the like). Gives up after a few seconds.
#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let shell = std::env::var_os("SHELL").filter(|s| !s.is_empty())?;
    let mut child = Command::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            r#"printf '__OPENDESIGNER_PATH__%s__OPENDESIGNER_PATH__' "$PATH""#,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    path_from_shell_output(&reader.join().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_folders_are_added_after_the_old_ones_without_repeats() {
        let merged = merge_path(
            &OsString::from("/usr/bin:/bin"),
            &[
                PathBuf::from("/opt/x/bin"),
                PathBuf::from("/bin"),
                PathBuf::from("/opt/x/bin"),
            ],
        );
        assert_eq!(merged, OsString::from("/usr/bin:/bin:/opt/x/bin"));
    }

    #[test]
    fn only_the_bare_system_path_counts_as_minimal() {
        assert!(is_minimal(&OsString::from("/usr/bin:/bin:/usr/sbin:/sbin")));
        assert!(!is_minimal(&OsString::from("/usr/bin:/Users/a/.bun/bin")));
    }

    #[test]
    fn the_shell_path_is_read_between_the_markers() {
        let out = "welcome!\n__OPENDESIGNER_PATH__/a:/b__OPENDESIGNER_PATH__\nbye";
        assert_eq!(path_from_shell_output(out).as_deref(), Some("/a:/b"));
        assert_eq!(path_from_shell_output("no markers"), None);
        assert_eq!(
            path_from_shell_output("__OPENDESIGNER_PATH____OPENDESIGNER_PATH__"),
            None
        );
    }

    #[test]
    fn only_existing_install_folders_are_offered() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".bun/bin")).unwrap();
        let dirs = usual_dirs(Some(home.path()));
        assert!(dirs.contains(&home.path().join(".bun/bin")));
        assert!(!dirs.contains(&home.path().join(".cargo/bin")));
    }

    #[cfg(unix)]
    #[test]
    fn the_login_shell_reports_a_path_when_there_is_a_shell() {
        if let Some(path) = login_shell_path() {
            assert!(path.contains("/bin"), "{path}");
        }
    }
}
