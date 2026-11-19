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
`formats` to choose sizes, `style` to pick a named look. Say which look you chose and its rules.\n\
3. Build a system first (tokens, one or two type families and a scale, a grid, named layers), then \
one hero frame, look at it, fix it, and only then the rest.\n\
4. Check, do not hope: render_preview (also at 160 to 320 px), review_frame on every frame and fix \
what it finds, render_video_frame at each beat and mid-transition, read the story in list_videos. \
Iterate at least once after you first look.\n\
5. Use every tool that helps (media, device frames, tokens, languages, presets, workspaces, \
exports). Sizes the user chose are theirs: change them only through set_format, which asks. If they \
chose nothing, pick from list_formats and record it with set_format.\n\
6. Memory: get_guidance lists the notes earlier runs left (list_memory, read_memory): read the \
relevant ones before you build. Before you finish, write_memory what the next run should know \
(brand facts such as colors, fonts and copy and where they came from, what the user corrected or \
liked, what worked, mistakes to avoid, decisions) in brand.md, lessons.md, decisions.md or style.md. \
Short and factual; never secrets or tokens.\n\
7. End by saying what you made, what you decided for them and what you would try next.";

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
        "formats",
        "sizes, aspect ratios and safe zones per platform",
        include_str!("../skill/formats.md"),
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
            "render_preview",
            "review_frame",
            "render_video_frame",
            "list_videos",
            "set_format",
            "list_formats",
            "list_memory",
            "read_memory",
            "write_memory",
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
}
