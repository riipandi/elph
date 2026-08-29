//! Workspace skill discovery for slash commands and harness resources.

use std::collections::HashMap;

use crate::utils::path::AppPaths;
use elph_agent::harness::Skill;
use elph_agent::runtime::LocalExecutionEnv;
use elph_agent::skills::load_skills;
use elph_tui::utils::truncate_with_ellipsis;

use crate::platform::{Paths, Settings};

use super::resource_paths::{dedupe_resource_dirs, resource_path_identity};

/// Max display width for slash palette / `/help` descriptions (skills, templates, builtins).
pub const MAX_PALETTE_DESCRIPTION_CHARS: usize = 72;

/// A skill name defined in multiple directories; the later directory wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillConflict {
    pub name: String,
    pub overridden_label: String,
    pub winner_label: String,
}

/// Result of loading skills from all configured workspace directories.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceSkills {
    pub skills: Vec<Skill>,
    pub conflicts: Vec<SkillConflict>,
}

/// Skill directory search order (lowest priority first, last-wins).
fn skill_dir_entries(
    paths: &Paths,
    cwd: &std::path::Path,
    include_project: bool,
    extra: &[String],
) -> Vec<(String, String)> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| paths.config_dir().clone());
    let project = paths.project_dir();
    let project_display = project.display();
    let mut entries = vec![
        (
            paths.bundled_dir().join("skills").to_string_lossy().to_string(),
            "~/.config/elph/bundled/skills".to_string(),
        ),
        (
            home.join(".agents/skills").to_string_lossy().to_string(),
            "~/.agents/skills".to_string(),
        ),
        (
            paths.skills_dir().to_string_lossy().to_string(),
            "~/.config/elph/skills".to_string(),
        ),
    ];
    if include_project {
        entries.push((
            project.join(".agents/skills").to_string_lossy().to_string(),
            format!("{project_display}/.agents/skills"),
        ));
        entries.push((
            paths.project_elph_dir().join("skills").to_string_lossy().to_string(),
            format!("{project_display}/.elph/skills"),
        ));
    }
    for path in extra {
        entries.push((path.clone(), path.clone()));
    }
    dedupe_resource_dirs(entries, &[cwd, project])
}

/// Load skills from user and project skill folders with last-wins conflict resolution.
pub async fn load_workspace_skills(env: &LocalExecutionEnv, paths: &Paths, settings: &Settings) -> WorkspaceSkills {
    let mut source_by_name: HashMap<String, (String, String)> = HashMap::new();
    let mut skills_by_name: HashMap<String, Skill> = HashMap::new();
    let mut conflicts = Vec::new();

    let extra = settings.extra_skill_paths();
    let bases = [paths.project_dir().as_path()];
    for (path, label) in skill_dir_entries(paths, paths.project_dir(), settings.include_project_resources(), &extra) {
        let result = load_skills(env, &[path.as_str()]).await;
        let skills = settings.filter_skills_for_project(result.skills, paths.project_dir());
        for skill in skills {
            let identity = resource_path_identity(&skill.file_path, &bases)
                .to_string_lossy()
                .into_owned();
            if let Some((previous_id, previous_label)) = source_by_name.get(&skill.name)
                && previous_id != &identity
            {
                conflicts.push(SkillConflict {
                    name: skill.name.clone(),
                    overridden_label: previous_label.clone(),
                    winner_label: label.clone(),
                });
            }
            source_by_name.insert(skill.name.clone(), (identity.clone(), label.clone()));
            skills_by_name.insert(skill.name.clone(), skill);
        }
    }

    let mut skills: Vec<Skill> = skills_by_name.into_values().collect();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    conflicts.sort_by(|left, right| left.name.cmp(&right.name));

    WorkspaceSkills { skills, conflicts }
}

/// Transcript notice when duplicate skill names were resolved by directory priority.
///
/// Delegates to the unified conflict formatter (skills-only section).
pub fn format_skill_conflict_notice(conflicts: &[SkillConflict]) -> Option<String> {
    super::conflict_notice::format_name_conflicts(conflicts, &[], &[], &[])
}

/// Legacy slash prefix: `/skill:review fix this` (backward-compat; skills now dispatch by raw name).
pub fn parse_skill_slash(body: &str) -> Option<(String, String)> {
    let body = body.trim();
    let rest = body.strip_prefix("skill:")?;
    let (name, args) = rest
        .split_once(' ')
        .map_or((rest.trim(), ""), |(n, a)| (n.trim(), a.trim()));
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), args.to_string()))
}

/// Palette / dispatch command name for a skill (raw name, no prefix).
pub fn skill_slash_name(skill_name: &str) -> String {
    skill_name.to_string()
}

/// Shorten a description for palette rows.
///
/// `max_width` caps the line to `max_width` display columns (ellipsis suffix). Pass
/// `None` when a later stage performs width-aware truncation using the actual
/// terminal/box width (e.g. the TUI palette card via [`wrap_palette_description`]), so
/// the description can use the full available space instead of a fixed column count.
///
/// Always collapses `description` to its first non-empty line.
pub fn truncate_palette_description(description: &str, max_width: Option<usize>) -> String {
    let first_line = description.lines().next().unwrap_or(description).trim();
    match max_width {
        Some(width) if width > 0 => truncate_with_ellipsis(first_line, width),
        _ => first_line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_slash_extracts_name_and_args() {
        assert_eq!(
            parse_skill_slash("skill:code-review src/main.rs"),
            Some(("code-review".into(), "src/main.rs".into()))
        );
        assert_eq!(parse_skill_slash("skill:debug"), Some(("debug".into(), "".into())));
        assert_eq!(parse_skill_slash("compact"), None);
    }

    #[test]
    fn skill_slash_name_uses_prefix() {
        assert_eq!(skill_slash_name("tui-design"), "tui-design");
    }

    #[test]
    fn format_skill_conflict_notice_lists_overrides() {
        let notice = format_skill_conflict_notice(&[SkillConflict {
            name: "debug".into(),
            overridden_label: "~/.agents/skills".into(),
            winner_label: "~/.config/elph/skills".into(),
        }]);
        let text = notice.expect("notice");
        // Unified formatter uses the shared "Resource name conflicts" header.
        assert!(text.contains("Skills:"));
        assert!(text.contains("debug"));
        assert!(text.contains("~/.agents/skills"));
        assert!(text.contains("~/.config/elph/skills"));
    }

    #[test]
    fn truncate_none_keeps_full_first_line() {
        let long = "a".repeat(200);
        assert_eq!(
            truncate_palette_description(&long, None),
            long,
            "uncapped truncation keeps the whole first line for box-aware render"
        );
    }

    #[test]
    fn truncate_some_caps_to_width() {
        let desc = "Reload hooks and prompt templates from disk after editing skill definitions";
        assert!(elph_tui::utils::display_width(desc) > MAX_PALETTE_DESCRIPTION_CHARS);
        let out = truncate_palette_description(desc, Some(MAX_PALETTE_DESCRIPTION_CHARS));
        assert!(elph_tui::utils::display_width(&out) <= MAX_PALETTE_DESCRIPTION_CHARS);
        assert!(out.ends_with('…'));
    }

    #[tokio::test]
    async fn extra_relative_project_dir_is_not_a_conflict() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = crate::platform::Paths::from_dirs(
            tmp.path().join("config"),
            tmp.path().join("data"),
            tmp.path().join("project"),
        );
        let skill_dir = paths.project_dir().join(".agents").join("skills").join("demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: demo\ndescription: once\n---\nBody\n").unwrap();

        let env = LocalExecutionEnv::new(paths.project_dir());
        let mut settings = Settings::defaults();
        settings.resources.skills = vec![".agents/skills".into()];
        let loaded = load_workspace_skills(&env, &paths, &settings).await;
        assert!(loaded.skills.iter().any(|s| s.name == "demo"));
        assert!(
            loaded.conflicts.is_empty(),
            "relative extra must not conflict with the project dir: {:?}",
            loaded.conflicts
        );
    }

    #[tokio::test]
    async fn extra_nested_skill_dir_does_not_conflict_with_parent_scan() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = crate::platform::Paths::from_dirs(
            tmp.path().join("config"),
            tmp.path().join("data"),
            tmp.path().join("project"),
        );
        let skill_dir = paths.project_dir().join(".agents/skills/demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: demo\ndescription: once\n---\nBody\n").unwrap();

        let env = LocalExecutionEnv::new(paths.project_dir());
        let mut settings = Settings::defaults();
        settings.resources.skills = vec![".agents/skills/demo".into()];
        let loaded = load_workspace_skills(&env, &paths, &settings).await;

        assert_eq!(loaded.skills.iter().filter(|skill| skill.name == "demo").count(), 1);
        assert!(
            loaded.conflicts.is_empty(),
            "the same skill file loaded through nested directories must not conflict: {:?}",
            loaded.conflicts
        );
    }

    #[tokio::test]
    async fn filtered_duplicate_skill_does_not_report_conflict() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = crate::platform::Paths::from_dirs(
            tmp.path().join("config"),
            tmp.path().join("data"),
            tmp.path().join("project"),
        );
        for root in [".agents/skills", ".elph/skills"] {
            let skill_dir = paths.project_dir().join(root).join("demo");
            std::fs::create_dir_all(&skill_dir).unwrap();
            std::fs::write(skill_dir.join("SKILL.md"), "---\nname: demo\ndescription: once\n---\nBody\n").unwrap();
        }

        let env = LocalExecutionEnv::new(paths.project_dir());
        let mut settings = Settings::defaults();
        settings.resources.skills = vec!["!.elph/skills".into()];
        let loaded = load_workspace_skills(&env, &paths, &settings).await;

        assert_eq!(loaded.skills.iter().filter(|skill| skill.name == "demo").count(), 1);
        assert!(
            loaded.conflicts.is_empty(),
            "filtered resources must not conflict: {:?}",
            loaded.conflicts
        );
    }

    #[test]
    fn truncate_collapses_to_first_line() {
        let desc = "First line of the doc\nSecond line that should be dropped";
        assert_eq!(truncate_palette_description(desc, None), "First line of the doc");
        assert_eq!(truncate_palette_description(desc, Some(10)), "First lin…");
    }
}
