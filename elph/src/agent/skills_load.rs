//! Workspace skill discovery for slash commands and harness resources.

use std::collections::HashMap;

use elph_agent::{LocalExecutionEnv, Skill, load_skills};
use elph_core::utils::path::AppPaths;
use elph_tui::utils::truncate_with_ellipsis;

use crate::platform::Paths;

/// Max display width for skill descriptions in slash palette and `/help`.
pub const MAX_SKILL_PALETTE_DESCRIPTION_CHARS: usize = 72;

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
fn skill_dir_entries(paths: &Paths) -> Vec<(String, String)> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| paths.config_dir().clone());
    let project = paths.project_dir();
    let project_display = project.display();
    vec![
        (
            home.join(".agents/skills").to_string_lossy().to_string(),
            "~/.agents/skills".to_string(),
        ),
        (paths.skills_dir().to_string_lossy().to_string(), "~/.elph/skills".to_string()),
        (
            project.join(".agents/skills").to_string_lossy().to_string(),
            format!("{project_display}/.agents/skills"),
        ),
        (
            paths.project_elph_dir().join("skills").to_string_lossy().to_string(),
            format!("{project_display}/.elph/skills"),
        ),
    ]
}

/// Load skills from user and project skill folders with last-wins conflict resolution.
pub async fn load_workspace_skills(env: &LocalExecutionEnv, paths: &Paths) -> WorkspaceSkills {
    let mut source_by_name: HashMap<String, String> = HashMap::new();
    let mut skills_by_name: HashMap<String, Skill> = HashMap::new();
    let mut conflicts = Vec::new();

    for (path, label) in skill_dir_entries(paths) {
        let result = load_skills(env, &[path.as_str()]).await;
        for skill in result.skills {
            if let Some(previous_label) = source_by_name.get(&skill.name) {
                conflicts.push(SkillConflict {
                    name: skill.name.clone(),
                    overridden_label: previous_label.clone(),
                    winner_label: label.clone(),
                });
            }
            source_by_name.insert(skill.name.clone(), label.clone());
            skills_by_name.insert(skill.name.clone(), skill);
        }
    }

    let mut skills: Vec<Skill> = skills_by_name.into_values().collect();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    conflicts.sort_by(|left, right| left.name.cmp(&right.name));

    WorkspaceSkills { skills, conflicts }
}

/// Transcript notice when duplicate skill names were resolved by directory priority.
pub fn format_skill_conflict_notice(conflicts: &[SkillConflict]) -> Option<String> {
    if conflicts.is_empty() {
        return None;
    }
    let mut lines = vec!["Skill name conflicts resolved (last directory wins):".to_string()];
    for conflict in conflicts {
        lines.push(format!(
            "  • {}: {} → {}",
            conflict.name, conflict.overridden_label, conflict.winner_label
        ));
    }
    Some(lines.join("\n"))
}

/// Pi-style slash prefix: `/skill:review fix this`.
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

/// Palette / dispatch command name for a skill.
pub fn skill_slash_name(skill_name: &str) -> String {
    format!("skill:{skill_name}")
}

/// Shorten a skill description for palette rows (first line, ellipsis).
pub fn truncate_skill_palette_description(description: &str) -> String {
    let first_line = description.lines().next().unwrap_or(description).trim();
    truncate_with_ellipsis(first_line, MAX_SKILL_PALETTE_DESCRIPTION_CHARS)
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
        assert_eq!(skill_slash_name("tui-design"), "skill:tui-design");
    }

    #[test]
    fn truncate_skill_palette_description_caps_length() {
        let long = "a".repeat(120);
        let out = truncate_skill_palette_description(&long);
        assert!(out.chars().count() <= MAX_SKILL_PALETTE_DESCRIPTION_CHARS);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_skill_palette_description_uses_first_line_only() {
        let out = truncate_skill_palette_description("First line\nSecond line");
        assert_eq!(out, "First line");
    }

    #[test]
    fn format_skill_conflict_notice_lists_overrides() {
        let notice = format_skill_conflict_notice(&[SkillConflict {
            name: "debug".into(),
            overridden_label: "~/.agents/skills".into(),
            winner_label: "~/.elph/skills".into(),
        }]);
        let text = notice.expect("notice");
        assert!(text.contains("debug"));
        assert!(text.contains("~/.agents/skills"));
        assert!(text.contains("~/.elph/skills"));
    }
}
