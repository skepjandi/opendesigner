//! Several designs side by side in one repository: an app store set, a reel, a landing page.
//! Each is its own project folder under `<repo>/opendesigner/workspaces/<name>/` with its own
//! pages, videos, history, notes and media. A project created before workspaces existed lives
//! directly in `<repo>/opendesigner/` and is listed as `main`.

use std::path::{Path, PathBuf};

use od_core::Workspace;

pub const WORKSPACES_DIR: &str = "workspaces";
/// The name of a project that sits directly in `opendesigner/`.
pub const LEGACY_NAME: &str = "main";
pub const MAX_NAME_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
}

/// A workspace name: letters, digits, space, `-` and `_`; not empty, not starting with a dot.
pub fn valid_name(name: &str) -> Result<(), String> {
    let ok = !name.trim().is_empty()
        && name == name.trim()
        && name.chars().count() <= MAX_NAME_LEN
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "a workspace name is up to {MAX_NAME_LEN} letters, digits, spaces, - or _ (for example \"app store\" or \"reel\")"
        ))
    }
}

/// The folder that holds the workspaces, given the folder of one of them (or of the legacy
/// project): `<repo>/opendesigner/workspaces`.
pub fn root_of(project_dir: &Path) -> PathBuf {
    if project_dir
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == WORKSPACES_DIR)
    {
        project_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    } else {
        project_dir.join(WORKSPACES_DIR)
    }
}

/// Every workspace next to `project_dir`, the legacy project first, then by name.
pub fn list(project_dir: &Path) -> Vec<Entry> {
    let root = root_of(project_dir);
    let mut out = Vec::new();
    if let Some(legacy) = root.parent().filter(|p| p.join("od.toml").is_file()) {
        out.push(Entry {
            name: LEGACY_NAME.to_string(),
            path: legacy.to_path_buf(),
        });
    }
    let mut named: Vec<Entry> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("od.toml").is_file())
        .map(|e| Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            path: e.path(),
        })
        .collect();
    named.sort_by(|a, b| a.name.cmp(&b.name));
    out.extend(named);
    out
}

/// The path of workspace `name` next to `project_dir`, if it exists.
pub fn find(project_dir: &Path, name: &str) -> Option<PathBuf> {
    list(project_dir)
        .into_iter()
        .find(|e| e.name == name)
        .map(|e| e.path)
}

/// Makes an empty workspace called `name` next to `project_dir`.
pub fn create(project_dir: &Path, name: &str) -> Result<Workspace, String> {
    valid_name(name)?;
    if find(project_dir, name).is_some() {
        return Err(format!("there is already a workspace called {name:?}"));
    }
    let root = root_of(project_dir);
    // `main` is the project that sits directly in `opendesigner/`
    let path = if name == LEGACY_NAME {
        root.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.clone())
    } else {
        root.join(name)
    };
    Workspace::create(&path, name).map_err(|e| e.to_string())
}

/// The name of the workspace whose folder is `project_dir`.
pub fn name_of(project_dir: &Path) -> String {
    if root_of(project_dir).join(project_dir.file_name().unwrap_or_default()) == project_dir {
        project_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        LEGACY_NAME.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspaces_sit_side_by_side_and_a_legacy_project_is_main() {
        let repo = tempfile::tempdir().unwrap();
        let legacy = repo.path().join("opendesigner");
        Workspace::create(&legacy, "old").unwrap();
        assert_eq!(
            list(&legacy)
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            ["main"]
        );
        assert_eq!(name_of(&legacy), "main");

        let reel = create(&legacy, "reel").unwrap();
        let reel_dir = reel.dir().unwrap().to_path_buf();
        assert_eq!(reel_dir, legacy.join("workspaces/reel"));
        let store = create(&reel_dir, "app store").unwrap();
        assert_eq!(store.dir().unwrap(), legacy.join("workspaces/app store"));
        let names: Vec<String> = list(&reel_dir).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["main", "app store", "reel"]);
        assert_eq!(name_of(&reel_dir), "reel");
        assert_eq!(find(store.dir().unwrap(), "main"), Some(legacy.clone()));
        assert!(find(&legacy, "nope").is_none());
    }

    #[test]
    fn names_are_plain_and_unique() {
        let repo = tempfile::tempdir().unwrap();
        let dir = repo.path().join("opendesigner");
        Workspace::create(&dir, "x").unwrap();
        for bad in ["", " lead", "a/b", "..", ".hidden", "a\\b", &"x".repeat(41)] {
            assert!(valid_name(bad).is_err(), "{bad:?}");
        }
        create(&dir, "reel").unwrap();
        assert!(create(&dir, "reel").is_err(), "duplicate");
        assert!(create(&dir, "main").is_err(), "taken by the legacy project");
        let fresh = repo.path().join("other/opendesigner");
        let main = create(&fresh, "main").unwrap();
        assert_eq!(
            main.dir().unwrap(),
            fresh,
            "a first workspace called main is the plain project"
        );
    }
}
