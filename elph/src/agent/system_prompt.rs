//! System prompt assembly for coding sessions.

use elph_agent::{AgentHarnessResources, Skill, format_skills_for_system_prompt, now_iso_timestamp};
use std::path::Path;

pub fn build_system_prompt(
    cwd: &Path,
    resources: &AgentHarnessResources,
    tool_names: &[String],
    agents_md: Option<&str>,
) -> String {
    let date = now_iso_timestamp().chars().take(10).collect::<String>();
    let mut parts = vec![
        "You are Elph, a helpful AI coding agent.".to_string(),
        format!("Working directory: {}", cwd.display()),
        format!("Current date: {date}"),
    ];

    if !tool_names.is_empty() {
        parts.push(format!("Active tools: {}", tool_names.join(", ")));
    }

    if let Some(agents_md) = agents_md.filter(|s| !s.trim().is_empty()) {
        parts.push(format!("# Project context\n{agents_md}"));
    }

    if !resources.skills.is_empty() {
        parts.push(format_skills_for_system_prompt(&resources.skills));
    }

    parts.join("\n\n")
}

pub fn agents_md_for_cwd(cwd: &Path) -> Option<String> {
    find_agents_md(cwd)
}

fn find_agents_md(mut dir: &Path) -> Option<String> {
    for _ in 0..8 {
        let candidate = dir.join("AGENTS.md");
        if candidate.is_file() {
            return std::fs::read_to_string(candidate).ok();
        }
        dir = dir.parent()?;
    }
    None
}

pub fn load_skills_metadata(skills_dir: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();
    if !skills_dir.is_dir() {
        return skills;
    }
    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        return skills;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("skill").to_string();
        let description = std::fs::read_to_string(&skill_file)
            .ok()
            .and_then(|content| content.lines().find(|l| !l.trim().is_empty()).map(str::to_string))
            .unwrap_or_else(|| "Skill".to_string());
        let content = std::fs::read_to_string(&skill_file).unwrap_or_default();
        skills.push(Skill {
            name,
            description,
            content,
            file_path: skill_file.display().to_string(),
            ..Default::default()
        });
    }
    skills
}
