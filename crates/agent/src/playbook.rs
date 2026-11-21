//! The craft guidance every agent gets, whether or not the skill is installed: the quality bar
//! goes out in the MCP server's `instructions` and in the in-app chat's first message, and the
//! full playbooks are served by the `get_playbook` tool and written into the installed skill
//! (`docs/wiki/agent-integration.md#making-agents-good`).

/// Short enough to send with every session, strong enough to change what an agent does.
pub const QUALITY_BAR: &str = "You are the art director as well as the maker. The bar is work a \
studio would ship: a first draft that is merely correct is not done, and a design that looks \
generated is not done.\n\
1. Look before you make: call get_guidance (selection, pinned notes, the workspace's brief), read \
the product's own repository, screens, colors and copy, and use the user's media (list_media, \
find_files, import_file). Never invent facts, numbers, reviews or features.\n\
2. Read the craft: call get_playbook with `design` for images, `video` and `motion` for video, \
`formats` to choose sizes, `style` to pick a named look, `logo` for marks, `blocks` for the catalog of ready-made \
titles, overlays, backgrounds and charts. Say which look you chose and its rules.\n\
3. Build a system first (tokens, one or two type families and a scale, a grid, named layers), then \
one hero frame, look at it, fix it, and only then the rest.\n\
4. Check, do not hope: render_preview (also at 160 to 320 px), review_frame on every frame and fix \
what it finds, validate_document for the whole design (stable codes, fix every error and warning), \
render_video_frame at each beat and mid-transition, read the story in list_videos. \
Iterate at least once after you first look. A video also gets sound unless the user asked for \
silence: make_sound for effects, search_sounds or the user's own file for a music bed (gain 0.15 to \
0.3 under a voice, fades 300 to 800 ms), then set_video audio.\n\
5. Assets: before drawing an icon, blob, background or texture by hand, search_assets, \
import_asset (recolored to your tokens) and make_asset; one icon set per design; show the credit line \
an import returns whenever it says attribution_required. A video starts with an inventory: \
list_media, list_footage, list_sounds, search_assets and list_asset_sources; every asset reaches the \
timeline as an image node (its `image` is the media/ file) in a frame that is a clip or an overlay, \
so decorate videos with icons, generated backgrounds and pack art instead of bare colors.\n\
6. Use every tool that helps (media, device frames, tokens, languages, presets, blocks (a catalog of \
ready-made titles, overlays, backgrounds and charts), workspaces, exports). Sizes the user chose are \
theirs: change them only through set_format, which asks. If they \
chose nothing, pick from list_formats and record it with set_format.\n\
7. Memory: get_guidance lists the notes earlier runs left (list_memory, read_memory): read the \
relevant ones before you build. Before you finish, write_memory what the next run should know \
(brand facts such as colors, fonts and copy and where they came from, what the user corrected or \
liked, what worked, mistakes to avoid, decisions) in brand.md, lessons.md, decisions.md or style.md. \
Short and factual; never secrets or tokens.\n\
8. Show the result: the user judges in the Preview panel, not on the canvas. Call show_preview with \
the deliverable once there is one and again when you improve it. When asked for options or \
directions, build each as its own named frame or video, call offer_variants, tell the user to pick \
in the Variants panel, then STOP until they have; call get_variants for the pick before you build \
the full set, and clear_variants after.\n\
9. End by saying what you made, what you decided for them and what you would try next.";

pub const TOPICS: &[(&str, &str, &str)] = &[
    (
        "design",
        "images: process, hierarchy, type, color, layout recipes, banned defaults",
        include_str!("../skill/design-direction.md"),
    ),
    (
        "style",
        "named looks with rules (Swiss kinetic, flat explainer, dark keynote, screencast, data story, editorial, soft, brutalist)",
        include_str!("../skill/style-directions.md"),
    ),
    (
        "video",
        "video process: brief, script, beats, arc, captions, editing footage, quality gate",
        include_str!("../skill/video-direction.md"),
    ),
    (
        "motion",
        "timings, easing, transitions, stagger, recipes for overlays, pre-export checks",
        include_str!("../skill/motion-playbook.md"),
    ),
    (
        "3d",
        "3D devices: the models, the pose, captures from a simulator, emulator or browser, and the moves",
        include_str!("../skill/three-d-devices.md"),
    ),
    (
        "logo",
        "logos and marks: brief, concepts, SVG construction, tests, icon sets",
        include_str!("../skill/logo-design.md"),
    ),
    (
        "formats",
        "sizes, aspect ratios and safe zones per platform",
        include_str!("../skill/formats.md"),
    ),
    (
        "blocks",
        "the block catalog: ready-made titles, overlays, backgrounds, charts and motion, and when to use one",
        include_str!("../skill/blocks-playbook.md"),
    ),
    (
        "scenery",
        "flat-vector scenes: layered scenery blocks, parallax, idle loops, scene moves and camera presets",
        include_str!("../skill/scenery-playbook.md"),
    ),
];

pub fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_is_substantial_and_the_bar_names_real_tools() {
        for (name, about, text) in TOPICS {
            assert!(text.len() > 1500, "{name} is too thin");
            assert!(!about.is_empty());
            assert!(text.starts_with("# "), "{name}");
        }
        let defs = crate::tools::definitions();
        let tools: Vec<&str> = defs.iter().filter_map(|d| d["name"].as_str()).collect();
        for t in [
            "get_guidance",
            "get_playbook",
            "list_media",
            "find_files",
            "import_file",
            "make_sound",
            "search_sounds",
            "render_preview",
            "review_frame",
            "render_video_frame",
            "list_videos",
            "set_format",
            "list_formats",
            "list_memory",
            "read_memory",
            "write_memory",
            "search_assets",
            "import_asset",
            "make_asset",
        ] {
            assert!(QUALITY_BAR.contains(t), "the bar should mention {t}");
            assert!(tools.contains(&t), "{t} is not a tool");
        }
        // every tool the playbooks name exists
        for (name, _, text) in TOPICS {
            for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if word.contains('_')
                    && word
                        .split('_')
                        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase()))
                    && matches!(
                        word.split('_').next(),
                        Some(
                            "get"
                                | "list"
                                | "set"
                                | "render"
                                | "review"
                                | "export"
                                | "find"
                                | "make"
                                | "search"
                                | "import"
                                | "create"
                                | "open"
                                | "resolve"
                                | "device"
                                | "validate"
                                | "read"
                                | "write"
                        )
                    )
                {
                    let ops = [
                        "set_token",
                        "set_clip",
                        "set_video",
                        "set_overlay",
                        "create_video",
                        "set_localized_text",
                        "create_workspace",
                        "open_workspace",
                        "export_set",
                        "export_panorama",
                        "export_video",
                    ];
                    assert!(
                        tools.contains(&word) || ops.contains(&word) || word == "device_template",
                        "{name} names {word}, which is not a tool or an operation"
                    );
                }
            }
        }
    }

    #[test]
    fn topics_resolve_by_name() {
        assert!(topic("motion").unwrap().contains("Timing"));
        assert!(topic("nope").is_none());
    }

    /// Tool and operation names written in guidance text (snake_case words that start like one).
    fn named_calls(text: &str) -> Vec<String> {
        const VERBS: &[&str] = &[
            "get", "list", "set", "render", "review", "export", "find", "make", "search", "import",
            "install", "unload", "create", "add", "remove", "resolve", "write", "read", "detect",
        ];
        text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|w| {
                w.contains('_')
                    && w.split('_')
                        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase()))
                    && w.split('_').next().is_some_and(|v| VERBS.contains(&v))
            })
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn a_video_task_is_routed_to_the_asset_tools_before_anything_is_built() {
        let defs = crate::tools::definitions();
        let tools: Vec<&str> = defs.iter().filter_map(|d| d["name"].as_str()).collect();
        let inventory = [
            "list_media",
            "list_footage",
            "list_sounds",
            "list_asset_sources",
            "search_assets",
        ];
        for t in inventory {
            assert!(tools.contains(&t), "{t} is not a tool");
        }

        // the video playbook: the inventory comes before the process, which is where it builds
        let video = topic("video").unwrap();
        let process = video.find("## The process").unwrap();
        assert!(video.find("## First: take inventory").unwrap() < process);
        for t in inventory {
            let at = video
                .find(t)
                .unwrap_or_else(|| panic!("video playbook lacks {t}"));
            assert!(at < process, "{t} comes after the build process");
        }
        for t in ["make_asset", "import_asset", "add_overlay", "add_clip"] {
            assert!(video.contains(t), "video playbook lacks {t}");
        }
        assert!(video.contains("`image` node"));

        // the quality bar (MCP instructions and the first chat message) says it too
        for t in inventory.into_iter().chain(["make_asset", "import_asset"]) {
            assert!(QUALITY_BAR.contains(t), "the bar should mention {t}");
        }

        // the installed skill's body says it, and the get_playbook description points at it
        let skill = crate::skill::generate();
        let body = &skill
            .iter()
            .find(|f| f.path == "SKILL.md")
            .unwrap()
            .contents;
        for t in inventory {
            assert!(body.contains(t), "SKILL.md should mention {t}");
        }
        let desc = defs
            .iter()
            .find(|d| d["name"] == "get_playbook")
            .and_then(|d| d["description"].as_str())
            .unwrap();
        assert!(desc.contains("inventory"));
    }

    #[test]
    fn every_tool_named_in_the_guidance_and_in_tool_descriptions_exists() {
        let defs = crate::tools::definitions();
        let tools: Vec<String> = defs
            .iter()
            .filter_map(|d| d["name"].as_str().map(str::to_string))
            .collect();
        let ops = [
            "create_video",
            "add_clip",
            "remove_clip",
            "add_overlay",
            "remove_overlay",
            "set_clip",
            "set_video",
            "set_overlay",
            "set_token",
            "create_workspace",
            "open_workspace",
            "set_localized_text",
            "set_variable",
            "set_variable_value",
            "remove_variable",
        ];
        let known =
            |w: &str| tools.iter().any(|t| t == w) || ops.contains(&w) || w == "device_template";
        let skill = crate::skill::generate();
        let body = skill.iter().find(|f| f.path == "SKILL.md").unwrap();
        for (what, text) in [
            ("quality bar", QUALITY_BAR),
            ("skill body", body.contents.as_str()),
            ("video playbook", topic("video").unwrap()),
            ("motion playbook", topic("motion").unwrap()),
        ] {
            for w in named_calls(text) {
                assert!(
                    known(&w),
                    "{what} names {w}, which is not a tool or an operation"
                );
            }
        }
        for d in &defs {
            let (name, desc) = (
                d["name"].as_str().unwrap(),
                d["description"].as_str().unwrap(),
            );
            if name == "apply_ops" {
                continue; // lists every operation, which are not tools
            }
            for w in named_calls(desc) {
                assert!(known(&w), "{name}'s description names {w}");
            }
        }
    }
}
