//! Where the editor's panels are: side by side, stacked, grouped as tabs, in their own windows, or
//! hidden. Pure data and operations so every rule is tested without a window; QML only draws it
//! and sends the user's moves back (`docs/wiki/qt-desktop.md#customizable-layout`).
//!
//! Invariants, checked after every operation (a failing operation changes nothing):
//! every known panel is in exactly one place (the main window's tree, a floating window's tree,
//! or the hidden list); a group holds at least one panel and names one of them as active; a split
//! has at least two children and a positive weight for each; the main window keeps a panel.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Every panel, with the title its tab shows.
pub const PANELS: &[(&str, &str)] = &[
    ("layers", "Layers"),
    ("canvas", "Canvas"),
    ("inspector", "Inspector"),
    ("notes", "Notes"),
    ("history", "History"),
    ("memory", "Memory"),
    ("video", "Video"),
    ("connect", "Connect"),
    ("chat", "Chat"),
];

pub const FORMAT_VERSION: u32 = 1;
const MIN_WINDOW: f64 = 240.0;
const MAX_WINDOW: f64 = 8000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    /// Children sit side by side.
    Row,
    /// Children are stacked.
    Column,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Split {
        id: u32,
        dir: Dir,
        weights: Vec<f64>,
        children: Vec<Node>,
    },
    Group {
        id: u32,
        panels: Vec<String>,
        active: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Floating {
    pub id: u32,
    pub root: Node,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where a dropped panel goes relative to a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    /// Join the group's tabs.
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl Zone {
    pub fn parse(s: &str) -> Result<Zone, String> {
        match s {
            "center" => Ok(Zone::Center),
            "left" => Ok(Zone::Left),
            "right" => Ok(Zone::Right),
            "top" => Ok(Zone::Top),
            "bottom" => Ok(Zone::Bottom),
            other => Err(format!("unknown place {other:?}")),
        }
    }

    fn dir(self) -> Option<Dir> {
        match self {
            Zone::Center => None,
            Zone::Left | Zone::Right => Some(Dir::Row),
            Zone::Top | Zone::Bottom => Some(Dir::Column),
        }
    }

    /// The new group goes before the existing content.
    fn before(self) -> bool {
        matches!(self, Zone::Left | Zone::Top)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub version: u32,
    pub main: Node,
    pub floating: Vec<Floating>,
    pub hidden: Vec<String>,
    next_id: u32,
}

fn group(id: u32, panels: &[&str], active: &str) -> Node {
    Node::Group {
        id,
        panels: panels.iter().map(ToString::to_string).collect(),
        active: active.to_string(),
    }
}

impl Default for Layout {
    /// Layers on the left, the canvas in the middle, the inspector above the working tabs on the
    /// right: what the window looked like before it could be rearranged.
    fn default() -> Layout {
        Layout {
            version: FORMAT_VERSION,
            main: Node::Split {
                id: 1,
                dir: Dir::Row,
                weights: vec![0.17, 0.6, 0.23],
                children: vec![
                    group(2, &["layers"], "layers"),
                    group(3, &["canvas"], "canvas"),
                    Node::Split {
                        id: 4,
                        dir: Dir::Column,
                        weights: vec![0.5, 0.5],
                        children: vec![
                            group(5, &["inspector"], "inspector"),
                            group(
                                6,
                                &["notes", "history", "memory", "video", "connect", "chat"],
                                "notes",
                            ),
                        ],
                    },
                ],
            },
            floating: Vec::new(),
            hidden: Vec::new(),
            next_id: 7,
        }
    }
}

// ---- tree helpers ----

fn panels_of(n: &Node, out: &mut Vec<String>) {
    match n {
        Node::Group { panels, .. } => out.extend(panels.iter().cloned()),
        Node::Split { children, .. } => children.iter().for_each(|c| panels_of(c, out)),
    }
}

fn ids_of(n: &Node, out: &mut Vec<u32>) {
    match n {
        Node::Group { id, .. } => out.push(*id),
        Node::Split { id, children, .. } => {
            out.push(*id);
            children.iter().for_each(|c| ids_of(c, out));
        }
    }
}

fn contains(n: &Node, panel: &str) -> bool {
    let mut v = Vec::new();
    panels_of(n, &mut v);
    v.iter().any(|p| p == panel)
}

fn remove_panel_from(n: &mut Node, panel: &str) {
    match n {
        Node::Group { panels, active, .. } => {
            panels.retain(|p| p != panel);
            if !panels.iter().any(|p| p == active) {
                *active = panels.first().cloned().unwrap_or_default();
            }
        }
        Node::Split { children, .. } => children
            .iter_mut()
            .for_each(|c| remove_panel_from(c, panel)),
    }
}

/// Drops empty groups, replaces a split with one child by that child, merges a split into a
/// parent split that runs the same way (keeping each child's share), and normalizes weights.
fn prune(n: Node) -> Option<Node> {
    match n {
        Node::Group { ref panels, .. } if panels.is_empty() => None,
        Node::Group { .. } => Some(n),
        Node::Split {
            id,
            dir,
            weights,
            children,
        } => {
            let mut kept: Vec<(f64, Node)> = Vec::new();
            for (w, c) in weights.into_iter().zip(children) {
                if let Some(c) = prune(c) {
                    kept.push((w, c));
                }
            }
            if kept.len() <= 1 {
                return kept.pop().map(|(_, c)| c);
            }
            let total: f64 = kept.iter().map(|(w, _)| w).sum();
            let mut weights = Vec::new();
            let mut children = Vec::new();
            for (w, c) in kept {
                let share = w / total;
                match c {
                    Node::Split {
                        dir: d,
                        weights: cw,
                        children: cc,
                        ..
                    } if d == dir => {
                        let ct: f64 = cw.iter().sum();
                        for (w2, c2) in cw.into_iter().zip(cc) {
                            weights.push(share * w2 / ct);
                            children.push(c2);
                        }
                    }
                    other => {
                        weights.push(share);
                        children.push(other);
                    }
                }
            }
            Some(Node::Split {
                id,
                dir,
                weights,
                children,
            })
        }
    }
}

fn group_mut(n: &mut Node, gid: u32) -> Option<&mut Node> {
    match n {
        Node::Group { id, .. } if *id == gid => Some(n),
        Node::Group { .. } => None,
        Node::Split { children, .. } => children.iter_mut().find_map(|c| group_mut(c, gid)),
    }
}

fn split_mut(n: &mut Node, sid: u32) -> Option<&mut Node> {
    match n {
        Node::Split { id, .. } if *id == sid => Some(n),
        Node::Split { children, .. } => children.iter_mut().find_map(|c| split_mut(c, sid)),
        Node::Group { .. } => None,
    }
}

/// The last group in reading order: the right-most, bottom-most one.
fn last_group_mut(n: &mut Node) -> &mut Node {
    match n {
        Node::Group { .. } => n,
        Node::Split { children, .. } => match children.last_mut() {
            Some(c) => last_group_mut(c),
            None => unreachable!("validated splits have children"),
        },
    }
}

fn first_group(n: &Node) -> Option<&Node> {
    match n {
        Node::Group { .. } => Some(n),
        Node::Split { children, .. } => children.iter().find_map(first_group),
    }
}

impl Layout {
    fn fresh_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id - 1
    }

    /// Every panel in the main window, in reading order.
    pub fn main_panels(&self) -> Vec<String> {
        let mut v = Vec::new();
        panels_of(&self.main, &mut v);
        v
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut seen: Vec<String> = Vec::new();
        let mut ids: Vec<u32> = Vec::new();
        let mut roots: Vec<&Node> = vec![&self.main];
        roots.extend(self.floating.iter().map(|f| &f.root));
        for root in &roots {
            panels_of(root, &mut seen);
            ids_of(root, &mut ids);
            check_node(root)?;
        }
        seen.extend(self.hidden.iter().cloned());
        for (name, _) in PANELS {
            let n = seen.iter().filter(|p| p == name).count();
            if n != 1 {
                return Err(format!("panel {name:?} appears {n} times"));
            }
        }
        if let Some(p) = seen.iter().find(|p| !PANELS.iter().any(|(n, _)| n == p)) {
            return Err(format!("unknown panel {p:?}"));
        }
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != ids.len() {
            return Err("two groups or splits share an id".into());
        }
        let mut fids: Vec<u32> = self.floating.iter().map(|f| f.id).collect();
        fids.sort_unstable();
        fids.dedup();
        if fids.len() != self.floating.len() {
            return Err("two windows share an id".into());
        }
        if ids.iter().chain(fids.iter()).any(|i| *i >= self.next_id) {
            return Err("an id is not below the next id".into());
        }
        if self.main_panels().is_empty() {
            return Err("the main window must keep a panel".into());
        }
        for f in &self.floating {
            let ok = [f.x, f.y]
                .iter()
                .all(|v| v.is_finite() && v.abs() < 100_000.0)
                && (MIN_WINDOW..=MAX_WINDOW).contains(&f.width)
                && (MIN_WINDOW..=MAX_WINDOW).contains(&f.height);
            if !ok {
                return Err("a window has an impossible place or size".into());
            }
        }
        Ok(())
    }

    /// Takes `panel` out of wherever it is (a tree, a floating window or the hidden list),
    /// tidying what it leaves behind.
    fn take(&mut self, panel: &str) {
        self.hidden.retain(|p| p != panel);
        remove_panel_from(&mut self.main, panel);
        if let Some(m) = prune(std::mem::replace(&mut self.main, group(0, &[], ""))) {
            self.main = m;
        } else {
            // emptied: validation reports it and the caller discards this copy
            self.main = group(0, &[], "");
        }
        for f in &mut self.floating {
            remove_panel_from(&mut f.root, panel);
        }
        let floating = std::mem::take(&mut self.floating);
        self.floating = floating
            .into_iter()
            .filter_map(|mut f| {
                f.root = prune(f.root)?;
                Some(f)
            })
            .collect();
    }

    /// Merges splits that run the same way as their parent and drops what is empty, everywhere.
    fn tidy(&mut self) {
        if let Some(m) = prune(std::mem::replace(&mut self.main, group(0, &[], ""))) {
            self.main = m;
        }
        for f in &mut self.floating {
            if let Some(r) = prune(std::mem::replace(&mut f.root, group(0, &[], ""))) {
                f.root = r;
            }
        }
    }

    /// Runs `f` on a copy and keeps it only if the result is valid.
    fn edit(&mut self, f: impl FnOnce(&mut Layout) -> Result<(), String>) -> Result<(), String> {
        let mut next = self.clone();
        f(&mut next)?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    fn require_panel(name: &str) -> Result<(), String> {
        if PANELS.iter().any(|(n, _)| *n == name) {
            Ok(())
        } else {
            Err(format!("unknown panel {name:?}"))
        }
    }

    /// Moves `panel` next to or into `target` (a group id, in the main window or a floating one).
    pub fn move_panel(&mut self, panel: &str, target: u32, zone: Zone) -> Result<(), String> {
        Self::require_panel(panel)?;
        self.edit(|l| {
            let alone_there = {
                let mut found = false;
                let mut roots: Vec<&mut Node> = vec![&mut l.main];
                roots.extend(l.floating.iter_mut().map(|f| &mut f.root));
                for r in roots {
                    if let Some(Node::Group { panels, .. }) = group_mut(r, target) {
                        found = true;
                        if panels.len() == 1 && panels[0] == panel {
                            return if zone == Zone::Center {
                                Ok(())
                            } else {
                                Err("a panel cannot be split from itself".to_string())
                            };
                        }
                        if panels.iter().any(|p| p == panel) && zone == Zone::Center {
                            return Ok(());
                        }
                    }
                }
                found
            };
            if !alone_there {
                return Err(format!("no group {target}"));
            }
            l.take(panel);
            let new_id = l.fresh_id();
            let split_id = l.fresh_id();
            let mut placed = false;
            let mut roots: Vec<&mut Node> = vec![&mut l.main];
            roots.extend(l.floating.iter_mut().map(|f| &mut f.root));
            for r in roots {
                if insert_at(r, target, panel, zone, new_id, split_id) {
                    placed = true;
                    break;
                }
            }
            if !placed {
                return Err(format!("no group {target}"));
            }
            l.tidy();
            Ok(())
        })
    }

    /// Puts `panel` in its own group along one edge of the main window (or of the window that
    /// holds `window`, when given).
    pub fn dock_edge(
        &mut self,
        panel: &str,
        zone: Zone,
        window: Option<u32>,
    ) -> Result<(), String> {
        Self::require_panel(panel)?;
        let Some(dir) = zone.dir() else {
            return Err("choose left, right, top or bottom".into());
        };
        self.edit(|l| {
            l.take(panel);
            let (gid, sid) = (l.fresh_id(), l.fresh_id());
            let g = group(gid, &[panel], panel);
            let root = match window {
                None => &mut l.main,
                Some(w) => {
                    &mut l
                        .floating
                        .iter_mut()
                        .find(|f| f.id == w)
                        .ok_or_else(|| format!("no window {w}"))?
                        .root
                }
            };
            let old = std::mem::replace(root, group(0, &[], ""));
            let children = if zone.before() {
                vec![g, old]
            } else {
                vec![old, g]
            };
            *root = Node::Split {
                id: sid,
                dir,
                weights: if zone.before() {
                    vec![0.28, 0.72]
                } else {
                    vec![0.72, 0.28]
                },
                children,
            };
            if let Some(p) = prune(std::mem::replace(root, group(0, &[], ""))) {
                *root = p;
            }
            Ok(())
        })
    }

    /// Gives `panel` a window of its own at the given place.
    pub fn float(
        &mut self,
        panel: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        Self::require_panel(panel)?;
        self.edit(|l| {
            if l.floating.iter().any(|f| matches!(&f.root, Node::Group { panels, .. } if panels.len() == 1 && panels[0] == panel)) {
                return Ok(());
            }
            l.take(panel);
            let (id, gid) = (l.fresh_id(), l.fresh_id());
            l.floating.push(Floating {
                id,
                root: group(gid, &[panel], panel),
                x,
                y,
                width: width.clamp(MIN_WINDOW, MAX_WINDOW),
                height: height.clamp(MIN_WINDOW, MAX_WINDOW),
            });
            Ok(())
        })
    }

    pub fn hide(&mut self, panel: &str) -> Result<(), String> {
        Self::require_panel(panel)?;
        self.edit(|l| {
            if l.hidden.iter().any(|p| p == panel) {
                return Ok(());
            }
            l.take(panel);
            l.hidden.push(panel.to_string());
            Ok(())
        })
    }

    /// Brings a hidden panel back as a tab of the main window's last group.
    pub fn show(&mut self, panel: &str) -> Result<(), String> {
        Self::require_panel(panel)?;
        self.edit(|l| {
            if !l.hidden.iter().any(|p| p == panel) {
                return Ok(());
            }
            l.hidden.retain(|p| p != panel);
            tab_into_main(l, &[panel.to_string()]);
            Ok(())
        })
    }

    /// Closes a floating window: its panels become tabs of the main window's last group.
    pub fn dock_window(&mut self, window: u32) -> Result<(), String> {
        self.edit(|l| {
            let at = l
                .floating
                .iter()
                .position(|f| f.id == window)
                .ok_or_else(|| format!("no window {window}"))?;
            let f = l.floating.remove(at);
            let mut panels = Vec::new();
            panels_of(&f.root, &mut panels);
            tab_into_main(l, &panels);
            Ok(())
        })
    }

    /// Makes `panel` visible and its tab the active one. Returns the window it is in, when it is
    /// in a floating one, so the front end can raise it.
    pub fn reveal(&mut self, panel: &str) -> Result<Option<u32>, String> {
        Self::require_panel(panel)?;
        self.show(panel)?;
        let mut window = None;
        self.edit(|l| {
            fn activate(n: &mut Node, panel: &str) {
                match n {
                    Node::Group { panels, active, .. } if panels.iter().any(|p| p == panel) => {
                        *active = panel.to_string();
                    }
                    Node::Group { .. } => {}
                    Node::Split { children, .. } => {
                        children.iter_mut().for_each(|c| activate(c, panel))
                    }
                }
            }
            activate(&mut l.main, panel);
            for f in &mut l.floating {
                if contains(&f.root, panel) {
                    window = Some(f.id);
                    activate(&mut f.root, panel);
                }
            }
            Ok(())
        })?;
        Ok(window)
    }

    pub fn set_active(&mut self, group_id: u32, panel: &str) -> Result<(), String> {
        self.edit(|l| {
            let mut roots: Vec<&mut Node> = vec![&mut l.main];
            roots.extend(l.floating.iter_mut().map(|f| &mut f.root));
            for r in roots {
                if let Some(Node::Group { panels, active, .. }) = group_mut(r, group_id) {
                    if !panels.iter().any(|p| p == panel) {
                        return Err(format!("{panel} is not in group {group_id}"));
                    }
                    *active = panel.to_string();
                    return Ok(());
                }
            }
            Err(format!("no group {group_id}"))
        })
    }

    pub fn set_weights(&mut self, split_id: u32, new_weights: &[f64]) -> Result<(), String> {
        self.edit(|l| {
            let mut roots: Vec<&mut Node> = vec![&mut l.main];
            roots.extend(l.floating.iter_mut().map(|f| &mut f.root));
            for r in roots {
                if let Some(Node::Split { weights, .. }) = split_mut(r, split_id) {
                    if weights.len() != new_weights.len() {
                        return Err("wrong number of sizes".into());
                    }
                    if new_weights.iter().any(|w| !w.is_finite() || *w <= 0.0) {
                        return Err("sizes must be positive".into());
                    }
                    let total: f64 = new_weights.iter().sum();
                    *weights = new_weights.iter().map(|w| w / total).collect();
                    return Ok(());
                }
            }
            Err(format!("no split {split_id}"))
        })
    }

    pub fn set_geometry(
        &mut self,
        window: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        self.edit(|l| {
            let f = l
                .floating
                .iter_mut()
                .find(|f| f.id == window)
                .ok_or_else(|| format!("no window {window}"))?;
            (f.x, f.y) = (x, y);
            f.width = width.clamp(MIN_WINDOW, MAX_WINDOW);
            f.height = height.clamp(MIN_WINDOW, MAX_WINDOW);
            Ok(())
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The saved layout, or the default when there is none or it is not valid (a layout from a
    /// newer version, a hand edit, a panel that no longer exists).
    pub fn load(path: &Path) -> Layout {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Layout>(&t).ok())
            .filter(|l| l.version == FORMAT_VERSION && l.validate().is_ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.part");
        std::fs::write(&tmp, self.to_json()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

fn check_node(n: &Node) -> Result<(), String> {
    match n {
        Node::Group { panels, active, .. } => {
            if panels.is_empty() {
                return Err("an empty group".into());
            }
            if !panels.contains(active) {
                return Err("a group's active tab is not in it".into());
            }
            Ok(())
        }
        Node::Split {
            weights, children, ..
        } => {
            if children.len() < 2 || weights.len() != children.len() {
                return Err("a split needs two or more children and a size for each".into());
            }
            if weights.iter().any(|w| !w.is_finite() || *w <= 0.0) {
                return Err("sizes must be positive".into());
            }
            children.iter().try_for_each(check_node)
        }
    }
}

/// Adds `panels` as tabs to the main window's last group (the right-most, bottom-most) and makes
/// the first of them active.
fn tab_into_main(l: &mut Layout, panels: &[String]) {
    if let Node::Group {
        panels: tabs,
        active,
        ..
    } = last_group_mut(&mut l.main)
    {
        for p in panels {
            if !tabs.contains(p) {
                tabs.push(p.clone());
            }
        }
        if let Some(first) = panels.first() {
            *active = first.clone();
        }
    }
}

/// Inserts `panel` relative to group `target` somewhere under `n`. Returns whether it was found.
fn insert_at(
    n: &mut Node,
    target: u32,
    panel: &str,
    zone: Zone,
    new_id: u32,
    split_id: u32,
) -> bool {
    match n {
        Node::Group { id, panels, active } if *id == target => {
            match zone.dir() {
                None => {
                    panels.push(panel.to_string());
                    *active = panel.to_string();
                }
                Some(dir) => {
                    let old = std::mem::replace(n, group(0, &[], ""));
                    let g = group(new_id, &[panel], panel);
                    let (children, weights) = if zone.before() {
                        (vec![g, old], vec![0.5, 0.5])
                    } else {
                        (vec![old, g], vec![0.5, 0.5])
                    };
                    *n = Node::Split {
                        id: split_id,
                        dir,
                        weights,
                        children,
                    };
                }
            }
            true
        }
        Node::Group { .. } => false,
        Node::Split { children, .. } => children
            .iter_mut()
            .any(|c| insert_at(c, target, panel, zone, new_id, split_id)),
    }
}

/// The first group of the main window, for a front end that needs somewhere to start.
pub fn first_group_id(l: &Layout) -> Option<u32> {
    match first_group(&l.main) {
        Some(Node::Group { id, .. }) => Some(*id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_group(l: &Layout, panel: &str) -> Option<u32> {
        fn go(n: &Node, panel: &str) -> Option<u32> {
            match n {
                Node::Group { id, panels, .. } => panels.iter().any(|p| p == panel).then_some(*id),
                Node::Split { children, .. } => children.iter().find_map(|c| go(c, panel)),
            }
        }
        go(&l.main, panel).or_else(|| l.floating.iter().find_map(|f| go(&f.root, panel)))
    }

    #[test]
    fn the_default_is_valid_and_has_every_panel_once() {
        let l = Layout::default();
        l.validate().unwrap();
        assert_eq!(l.main_panels().len(), PANELS.len());
        assert_eq!(find_group(&l, "layers"), Some(2));
    }

    #[test]
    fn a_panel_can_join_another_groups_tabs_and_the_group_it_left_disappears() {
        let mut l = Layout::default();
        let target = find_group(&l, "notes").unwrap();
        l.move_panel("layers", target, Zone::Center).unwrap();
        assert_eq!(find_group(&l, "layers"), Some(target));
        let Node::Split { children, .. } = &l.main else {
            panic!()
        };
        assert_eq!(
            children.len(),
            2,
            "the empty Layers group is gone: {children:?}"
        );
        l.validate().unwrap();
    }

    #[test]
    fn dropping_on_an_edge_splits_and_merges_with_a_split_that_runs_the_same_way() {
        let mut l = Layout::default();
        let canvas = find_group(&l, "canvas").unwrap();
        // chat to the right of the canvas: the root already runs side by side, so it just joins
        l.move_panel("chat", canvas, Zone::Right).unwrap();
        let Node::Split {
            dir,
            children,
            weights,
            ..
        } = &l.main
        else {
            panic!()
        };
        assert_eq!((*dir, children.len()), (Dir::Row, 4));
        assert!((weights.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        // video above the canvas: a stack inside the row
        l.move_panel("video", canvas, Zone::Top).unwrap();
        let Node::Split { children, .. } = &l.main else {
            panic!()
        };
        assert!(children.iter().any(|c| matches!(c, Node::Split { dir: Dir::Column, children, .. } if children.len() == 2 && matches!(&children[0], Node::Group { panels, .. } if panels == &["video"]))));
        l.validate().unwrap();
    }

    #[test]
    fn a_panel_cannot_be_split_from_itself_and_unknown_things_are_refused() {
        let mut l = Layout::default();
        let layers = find_group(&l, "layers").unwrap();
        let before = l.clone();
        assert!(l.move_panel("layers", layers, Zone::Left).is_err());
        assert!(
            l.move_panel("layers", layers, Zone::Center).is_ok(),
            "already there is a no-op"
        );
        assert!(l.move_panel("layers", 999, Zone::Center).is_err());
        assert!(l.move_panel("nope", layers, Zone::Center).is_err());
        assert_eq!(l, before, "a refused move changes nothing");
    }

    #[test]
    fn docking_to_an_edge_wraps_the_window() {
        let mut l = Layout::default();
        l.dock_edge("chat", Zone::Bottom, None).unwrap();
        let Node::Split { dir, children, .. } = &l.main else {
            panic!()
        };
        assert_eq!(*dir, Dir::Column);
        assert!(matches!(&children[1], Node::Group { panels, .. } if panels == &["chat"]));
        assert!(l.dock_edge("chat", Zone::Center, None).is_err());
        l.validate().unwrap();
    }

    #[test]
    fn a_panel_floats_in_its_own_window_and_comes_back_as_a_tab() {
        let mut l = Layout::default();
        l.float("chat", 100.0, 80.0, 420.0, 560.0).unwrap();
        assert_eq!(l.floating.len(), 1);
        assert!(!l.main_panels().contains(&"chat".to_string()));
        // floating it again is a no-op, not a second window
        l.float("chat", 0.0, 0.0, 300.0, 300.0).unwrap();
        assert_eq!(l.floating.len(), 1);
        // a second panel joins the window as a tab
        let w = find_group(&l, "chat").unwrap();
        l.move_panel("video", w, Zone::Center).unwrap();
        assert_eq!(l.floating[0].root, group(w, &["chat", "video"], "video"));
        let id = l.floating[0].id;
        l.dock_window(id).unwrap();
        assert!(l.floating.is_empty());
        assert!(
            l.main_panels().contains(&"chat".to_string())
                && l.main_panels().contains(&"video".to_string())
        );
        l.validate().unwrap();
    }

    #[test]
    fn the_main_window_always_keeps_a_panel_and_nothing_is_lost() {
        let mut l = Layout::default();
        for p in [
            "layers",
            "canvas",
            "inspector",
            "notes",
            "history",
            "memory",
            "video",
            "connect",
        ] {
            l.float(p, 10.0, 10.0, 300.0, 300.0).unwrap();
        }
        // only chat is left in the main window
        assert_eq!(l.main_panels(), ["chat"]);
        assert!(l.float("chat", 0.0, 0.0, 300.0, 300.0).is_err());
        assert!(l.hide("chat").is_err());
        assert_eq!(
            l.main_panels(),
            ["chat"],
            "the refused move changed nothing"
        );
        let total = l.main_panels().len() + l.floating.len() + l.hidden.len();
        assert_eq!(total, PANELS.len());
        l.validate().unwrap();
    }

    #[test]
    fn hidden_panels_come_back_as_tabs_and_reveal_finds_them_anywhere() {
        let mut l = Layout::default();
        l.hide("history").unwrap();
        assert!(!l.main_panels().contains(&"history".to_string()));
        assert_eq!(l.reveal("history").unwrap(), None);
        assert_eq!(
            find_group(&l, "history"),
            find_group(&l, "chat"),
            "back in the last group, active"
        );
        l.float("notes", 50.0, 50.0, 400.0, 400.0).unwrap();
        let window = l.reveal("notes").unwrap();
        assert_eq!(window, Some(l.floating[0].id));
        l.set_active(find_group(&l, "chat").unwrap(), "video")
            .unwrap();
        assert!(
            l.set_active(find_group(&l, "chat").unwrap(), "layers")
                .is_err()
        );
    }

    #[test]
    fn sizes_are_normalized_and_bad_ones_refused() {
        let mut l = Layout::default();
        l.set_weights(1, &[1.0, 2.0, 1.0]).unwrap();
        let Node::Split { weights, .. } = &l.main else {
            panic!()
        };
        assert!((weights[1] - 0.5).abs() < 1e-9);
        assert!(l.set_weights(1, &[1.0, 2.0]).is_err());
        assert!(l.set_weights(1, &[1.0, -2.0, 1.0]).is_err());
        assert!(l.set_weights(1, &[1.0, f64::NAN, 1.0]).is_err());
        assert!(l.set_weights(77, &[1.0, 1.0]).is_err());
        l.float("chat", 1.0, 2.0, 300.0, 300.0).unwrap();
        let w = l.floating[0].id;
        l.set_geometry(w, 20.0, 30.0, 10.0, 99_999.0).unwrap();
        assert_eq!(
            (l.floating[0].width, l.floating[0].height),
            (MIN_WINDOW, MAX_WINDOW)
        );
        assert!(l.set_geometry(99, 0.0, 0.0, 300.0, 300.0).is_err());
    }

    #[test]
    fn it_survives_a_round_trip_and_a_bad_file_gives_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.json");
        assert_eq!(Layout::load(&path), Layout::default());
        let mut l = Layout::default();
        l.float("chat", 40.0, 40.0, 500.0, 600.0).unwrap();
        l.hide("history").unwrap();
        l.dock_edge("video", Zone::Left, None).unwrap();
        l.save(&path).unwrap();
        // sizes may come back one rounding step apart; everything else is exact
        let near = |a: &Layout, b: &Layout| {
            fn eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
                match (a, b) {
                    (serde_json::Value::Number(x), serde_json::Value::Number(y)) => {
                        (x.as_f64().unwrap_or(0.0) - y.as_f64().unwrap_or(1.0)).abs() < 1e-9
                    }
                    (serde_json::Value::Array(x), serde_json::Value::Array(y)) => {
                        x.len() == y.len() && x.iter().zip(y).all(|(p, q)| eq(p, q))
                    }
                    (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
                        x.len() == y.len()
                            && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| eq(v, w)))
                    }
                    _ => a == b,
                }
            }
            eq(
                &serde_json::to_value(a).unwrap(),
                &serde_json::to_value(b).unwrap(),
            )
        };
        assert!(near(&Layout::load(&path), &l));
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Layout::load(&path), Layout::default());
        // a layout that loses a panel is not trusted
        let mut broken = Layout::default();
        broken.hidden.push("chat".into());
        std::fs::write(&path, serde_json::to_string(&broken).unwrap()).unwrap();
        assert_eq!(Layout::load(&path), Layout::default());
    }

    #[test]
    fn every_sequence_of_moves_keeps_the_layout_valid() {
        // a deterministic pseudo-random walk over every operation
        let mut l = Layout::default();
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..600 {
            let panel = PANELS[(next() % PANELS.len() as u64) as usize].0;
            let mut groups = Vec::new();
            fn collect(n: &Node, out: &mut Vec<u32>) {
                match n {
                    Node::Group { id, .. } => out.push(*id),
                    Node::Split { children, .. } => children.iter().for_each(|c| collect(c, out)),
                }
            }
            collect(&l.main, &mut groups);
            for f in &l.floating {
                collect(&f.root, &mut groups);
            }
            let g = groups[(next() % groups.len() as u64) as usize];
            let zone = [
                Zone::Center,
                Zone::Left,
                Zone::Right,
                Zone::Top,
                Zone::Bottom,
            ][(next() % 5) as usize];
            let _ = match next() % 7 {
                0 => l.move_panel(panel, g, zone),
                1 => l.dock_edge(panel, zone, None),
                2 => l.float(panel, 10.0, 10.0, 300.0, 300.0),
                3 => l.hide(panel),
                4 => l.show(panel),
                5 => l.reveal(panel).map(|_| ()),
                _ => match l.floating.first().map(|f| f.id) {
                    Some(w) => l.dock_window(w),
                    None => Ok(()),
                },
            };
            l.validate().unwrap_or_else(|e| panic!("{e}: {l:?}"));
        }
    }
}
