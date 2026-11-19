//! Finding a program the way a shell would: the first match on `PATH` wins.

use std::path::PathBuf;

/// The path of `program` if it is on `PATH` (with the extensions Windows uses).
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    std::env::split_paths(&path).find_map(|dir| {
        names.iter().find_map(|ext| {
            let candidate = dir.join(format!("{program}{ext}"));
            candidate.is_file().then_some(candidate)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_on_the_path_is_found_and_one_that_is_not_is_not() {
        assert!(which("this-program-does-not-exist-anywhere").is_none());
        let found = which("sh");
        if cfg!(unix) {
            assert!(found.is_some(), "sh should be on the path");
        }
    }
}
