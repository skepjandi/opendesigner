//! What a folder already is, from the files in it: the facts an agent reads before it asks the
//! user about a stack, or before it writes a run plan. Read-only; nothing is executed or
//! installed, and files are only looked at when they are plain files of a sane size.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

const MAX_PACKAGE_JSON_BYTES: u64 = 256 * 1024;
const MAX_SCRIPTS: usize = 30;
const MAX_SCRIPT_CHARS: usize = 120;
const MAX_SUBFOLDERS: usize = 40;
const SKIPPED_FOLDERS: &[&str] = &["node_modules", "target", "dist", "build", "vendor"];

const MARKERS: &[(&str, &str)] = &[
    ("package.json", "Node.js project"),
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "Yarn"),
    ("bun.lockb", "Bun"),
    ("package-lock.json", "npm"),
    ("Cargo.toml", "Rust (Cargo)"),
    ("pyproject.toml", "Python project"),
    ("requirements.txt", "Python requirements"),
    ("go.mod", "Go module"),
    ("Gemfile", "Ruby (Bundler)"),
    ("composer.json", "PHP (Composer)"),
    ("pubspec.yaml", "Flutter or Dart"),
    ("Package.swift", "Swift package"),
    ("build.gradle", "Gradle (Android or JVM)"),
    ("build.gradle.kts", "Gradle (Android or JVM)"),
    ("docker-compose.yml", "Docker Compose services"),
    ("Dockerfile", "Dockerfile"),
    ("index.html", "static web page"),
];

const FRAMEWORKS: &[&str] = &[
    "next",
    "react",
    "react-native",
    "expo",
    "vue",
    "nuxt",
    "svelte",
    "@sveltejs/kit",
    "astro",
    "vite",
    "remix",
    "@remix-run/react",
    "angular",
    "@angular/core",
    "express",
    "fastify",
    "electron",
    "tauri",
    "@tauri-apps/api",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Marker {
    pub path: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Detected {
    pub markers: Vec<Marker>,
    /// `package.json` scripts at the root: name and command.
    pub node_scripts: BTreeMap<String, String>,
    pub node_frameworks: Vec<String>,
    /// True when no known marker was found: a new project, so the stack is still open.
    pub empty: bool,
}

fn is_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

fn scan_folder(dir: &Path, prefix: &str, out: &mut Vec<Marker>) {
    for (file, meaning) in MARKERS {
        if is_plain_file(&dir.join(file)) {
            out.push(Marker {
                path: format!("{prefix}{file}"),
                meaning: (*meaning).to_string(),
            });
        }
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type().ok();
        if name.ends_with(".xcodeproj") && kind.is_some_and(|k| k.is_dir()) {
            out.push(Marker {
                path: format!("{prefix}{name}"),
                meaning: "Xcode project (iOS or macOS)".into(),
            });
        }
    }
}

fn read_package_json(root: &Path, detected: &mut Detected) {
    let path = root.join("package.json");
    let usable =
        fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_PACKAGE_JSON_BYTES);
    let Some(json) = usable
        .then(|| fs::read_to_string(&path).ok())
        .flatten()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return;
    };
    if let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) {
        for (name, command) in scripts.iter().take(MAX_SCRIPTS) {
            let command: String = command
                .as_str()
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .take(MAX_SCRIPT_CHARS)
                .collect();
            detected.node_scripts.insert(name.clone(), command);
        }
    }
    for section in ["dependencies", "devDependencies"] {
        let Some(deps) = json.get(section).and_then(|d| d.as_object()) else {
            continue;
        };
        for name in FRAMEWORKS {
            if deps.contains_key(*name) && !detected.node_frameworks.iter().any(|f| f == name) {
                detected.node_frameworks.push((*name).to_string());
            }
        }
    }
}

/// The markers in `root` and in its first-level folders (a monorepo's `apps/web` is not seen;
/// `web/` is), skipping dependency and build folders.
pub fn detect(root: &Path) -> Detected {
    let mut detected = Detected::default();
    scan_folder(root, "", &mut detected.markers);
    read_package_json(root, &mut detected);
    let mut folders: Vec<String> = fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.') && !SKIPPED_FOLDERS.contains(&n.as_str()))
                .filter(|n| n != "opendesigner")
                .collect()
        })
        .unwrap_or_default();
    folders.sort();
    for name in folders.into_iter().take(MAX_SUBFOLDERS) {
        scan_folder(
            &root.join(&name),
            &format!("{name}/"),
            &mut detected.markers,
        );
    }
    detected.empty = detected.markers.is_empty();
    detected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_folder_leaves_the_stack_open() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect(dir.path()).empty);
    }

    #[test]
    fn a_node_project_reports_scripts_and_frameworks() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"dev":"vite","build":"vite build"},"dependencies":{"react":"18"},"devDependencies":{"vite":"5"}}"#,
        )
        .unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        let found = detect(dir.path());
        assert!(!found.empty);
        assert_eq!(
            found.node_scripts.get("dev").map(String::as_str),
            Some("vite")
        );
        assert_eq!(found.node_frameworks, ["react", "vite"]);
        assert!(found.markers.iter().any(|m| m.path == "pnpm-lock.yaml"));
    }

    #[test]
    fn first_level_folders_count_but_dependency_folders_do_not() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("ios/App.xcodeproj")).unwrap();
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/package.json"), "{}").unwrap();
        let found = detect(dir.path());
        assert_eq!(found.markers.len(), 1);
        assert_eq!(found.markers[0].path, "ios/App.xcodeproj");
    }

    #[test]
    fn a_broken_package_json_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{ not json").unwrap();
        let found = detect(dir.path());
        assert!(found.node_scripts.is_empty());
        assert_eq!(found.markers.len(), 1);
    }
}
