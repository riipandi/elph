//! Custom agent discovery (markdown + YAML frontmatter).
//!
//! Search order (lowest priority first; last wins on name conflict):
//! 1. `CONFIG_DIR/bundled/agents/`
//! 2. `CONFIG_DIR/agents/` (user-managed)
//! 3. `<project>/.agents/agents/`
//! 4. `<project>/.elph/agents/`

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::path::AppPaths;
use serde::Deserialize;

use crate::platform::Paths;

/// A discovered agent definition from markdown frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAgent {
    pub name: String,
    pub description: String,
    pub body: String,
    pub path: PathBuf,
    pub source_label: String,
    pub tools: Option<Vec<String>>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConflict {
    pub name: String,
    pub overridden_label: String,
    pub winner_label: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceAgents {
    pub agents: Vec<WorkspaceAgent>,
    pub conflicts: Vec<AgentConflict>,
}

#[derive(Debug, Default, Deserialize)]
struct AgentFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tools: Option<Vec<String>>,
    #[serde(default)]
    model: Option<String>,
}

/// Directory search order (lowest priority first).
pub fn agent_dir_entries(paths: &Paths) -> Vec<(PathBuf, String)> {
    let project = paths.project_dir();
    let project_display = project.display();
    vec![
        (paths.bundled_dir().join("agents"), "~/.config/elph/bundled/agents".into()),
        (paths.agents_dir(), "~/.config/elph/agents".into()),
        (
            project.join(".agents").join("agents"),
            format!("{project_display}/.agents/agents"),
        ),
        (project.join(".elph").join("agents"), format!("{project_display}/.elph/agents")),
    ]
}

/// Load agents from configured directories with last-wins conflict resolution.
pub fn load_workspace_agents(paths: &Paths) -> WorkspaceAgents {
    let mut source_by_name: HashMap<String, String> = HashMap::new();
    let mut by_name: HashMap<String, WorkspaceAgent> = HashMap::new();
    let mut conflicts = Vec::new();

    for (dir, label) in agent_dir_entries(paths) {
        for agent in load_agents_from_dir(&dir, &label) {
            if let Some(previous) = source_by_name.get(&agent.name) {
                conflicts.push(AgentConflict {
                    name: agent.name.clone(),
                    overridden_label: previous.clone(),
                    winner_label: label.clone(),
                });
            }
            source_by_name.insert(agent.name.clone(), label.clone());
            by_name.insert(agent.name.clone(), agent);
        }
    }

    let mut agents: Vec<WorkspaceAgent> = by_name.into_values().collect();
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    conflicts.sort_by(|a, b| a.name.cmp(&b.name));
    WorkspaceAgents { agents, conflicts }
}

fn load_agents_from_dir(dir: &Path, source_label: &str) -> Vec<WorkspaceAgent> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut agents = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Nested: agents/<name>/AGENT.md or agents/<name>.md
            let agent_md = path.join("AGENT.md");
            let skill_like = path.join("agent.md");
            if agent_md.is_file() {
                if let Some(agent) = parse_agent_file(&agent_md, source_label, Some(&path)) {
                    agents.push(agent);
                }
            } else if skill_like.is_file()
                && let Some(agent) = parse_agent_file(&skill_like, source_label, Some(&path))
            {
                agents.push(agent);
            }
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("md") {
            continue;
        }
        // Skip README.md in agents dirs
        if path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("README"))
        {
            continue;
        }
        if let Some(agent) = parse_agent_file(&path, source_label, None) {
            agents.push(agent);
        }
    }
    agents
}

fn parse_agent_file(path: &Path, source_label: &str, dir_hint: Option<&Path>) -> Option<WorkspaceAgent> {
    let raw = fs::read_to_string(path).ok()?;
    let (fm, body) = split_frontmatter(&raw);
    let frontmatter: AgentFrontmatter = fm
        .as_deref()
        .and_then(|yaml| yaml_serde::from_str(yaml).ok())
        .unwrap_or_default();

    let fallback_name = dir_hint
        .and_then(|d| d.file_name())
        .or_else(|| path.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("agent")
        .to_string();

    let name = frontmatter
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(fallback_name);
    let description = frontmatter.description.unwrap_or_default();

    Some(WorkspaceAgent {
        name,
        description,
        body: body.trim().to_string(),
        path: path.to_path_buf(),
        source_label: source_label.to_string(),
        tools: frontmatter.tools,
        model: frontmatter.model,
    })
}

fn split_frontmatter(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return (None, raw.to_string());
    }
    let rest = trimmed.strip_prefix("---").unwrap_or(trimmed);
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    if let Some(end) = rest.find("\n---") {
        let yaml = rest[..end].to_string();
        let body = rest[end + 4..].trim_start_matches('\n').to_string();
        return (Some(yaml), body);
    }
    (None, raw.to_string())
}

pub fn format_agent_conflict_notice(conflicts: &[AgentConflict]) -> Option<String> {
    if conflicts.is_empty() {
        return None;
    }
    let mut lines = vec!["Agent name conflicts resolved (last directory wins):".to_string()];
    for c in conflicts {
        lines.push(format!("  • {}: {} → {}", c.name, c.overridden_label, c.winner_label));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Paths;

    #[test]
    fn loads_agents_last_wins() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = tmp.path().join("config");
        let data = tmp.path().join("data");
        let project = tmp.path().join("repo");
        let paths = Paths::from_dirs(config.clone(), data, project.clone());

        fs::create_dir_all(paths.bundled_dir().join("agents")).unwrap();
        fs::create_dir_all(paths.agents_dir()).unwrap();
        fs::create_dir_all(project.join(".elph/agents")).unwrap();

        fs::write(
            paths.bundled_dir().join("agents/reviewer.md"),
            "---\nname: reviewer\ndescription: bundled\n---\nBundled body\n",
        )
        .unwrap();
        fs::write(
            paths.agents_dir().join("reviewer.md"),
            "---\nname: reviewer\ndescription: user\n---\nUser body\n",
        )
        .unwrap();
        fs::write(
            project.join(".elph/agents/reviewer.md"),
            "---\nname: reviewer\ndescription: project\ntools:\n  - read\n---\nProject body\n",
        )
        .unwrap();

        let loaded = load_workspace_agents(&paths);
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.agents[0].description, "project");
        assert_eq!(loaded.agents[0].body, "Project body");
        assert_eq!(loaded.agents[0].tools.as_deref(), Some(&["read".to_string()][..]));
        assert!(!loaded.conflicts.is_empty());
    }

    #[test]
    fn nested_agent_md_supported() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = tmp.path().join("config");
        let paths = Paths::from_dirs(config.clone(), tmp.path().join("data"), tmp.path().join("repo"));
        let dir = paths.agents_dir().join("planner");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("AGENT.md"), "---\nname: planner\ndescription: plans\n---\nPlan well\n").unwrap();
        let loaded = load_workspace_agents(&paths);
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.agents[0].name, "planner");
    }
}
