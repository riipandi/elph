//! Load skills, prompts, agents, and project context into harness resources.
//!
//! Name conflicts across directories use last-wins priority (later path wins).
//! Callers surface conflicts to the user transcript via [`format_resource_conflict_notice`].

use std::collections::HashMap;
use std::path::Path;

use crate::utils::path::AppPaths;
use elph_agent::LocalExecutionEnv;
use elph_agent::harness::{AgentHarnessResources, PromptTemplate};
use elph_agent::load_prompt_templates;

use super::agents_load::{AgentConflict, WorkspaceAgents, load_workspace_agents};
use super::conflict_notice::{self, CrossKindConflict, TemplateConflict};
use super::resource_paths::{dedupe_resource_dirs, resource_dir_identity};
use super::skills_load::{SkillConflict, WorkspaceSkills, load_workspace_skills};
use crate::platform::{Paths, Settings};

#[derive(Debug, Clone, Default)]
pub struct LoadResourcesResult {
    pub resources: AgentHarnessResources,
    pub skill_conflicts: Vec<SkillConflict>,
    pub agent_conflicts: Vec<AgentConflict>,
    pub template_conflicts: Vec<TemplateConflict>,
    pub cross_kind_conflicts: Vec<CrossKindConflict>,
    /// Non-fatal load diagnostics (parse/list failures, etc.).
    pub warnings: Vec<String>,
}

impl LoadResourcesResult {
    pub fn has_conflicts(&self) -> bool {
        !self.skill_conflicts.is_empty()
            || !self.agent_conflicts.is_empty()
            || !self.template_conflicts.is_empty()
            || !self.cross_kind_conflicts.is_empty()
    }

    pub fn skill_count(&self) -> usize {
        self.resources.skills.len()
    }

    pub fn template_count(&self) -> usize {
        self.resources.prompt_templates.len()
    }
}

/// `(absolute path, display label)` for prompt template search (lowest priority first).
pub fn prompt_template_dir_entries(
    paths: &Paths,
    cwd: &Path,
    include_project: bool,
    extra: &[String],
) -> Vec<(String, String)> {
    let mut entries = vec![(
        paths.prompts_dir().to_string_lossy().to_string(),
        "~/.config/elph/prompts".to_string(),
    )];
    let project_display = paths.project_dir().display();
    if include_project {
        let project_prompts = paths.project_elph_dir().join("prompts");
        if project_prompts.is_dir() {
            entries.push((
                project_prompts.to_string_lossy().to_string(),
                format!("{project_display}/.elph/prompts"),
            ));
        }
        let agents_prompts = cwd.join(".agents").join("prompts");
        if agents_prompts.is_dir() {
            entries.push((
                agents_prompts.to_string_lossy().to_string(),
                format!("{project_display}/.agents/prompts"),
            ));
        }
    }
    for path in extra {
        entries.push((path.clone(), path.clone()));
    }
    dedupe_resource_dirs(entries, &[cwd, paths.project_dir()])
}

pub async fn load_resources(
    paths: &Paths,
    cwd: &Path,
    env: &LocalExecutionEnv,
    settings: &Settings,
) -> LoadResourcesResult {
    let mut warnings = Vec::new();

    let WorkspaceSkills {
        skills,
        conflicts: skill_conflicts,
    } = load_workspace_skills(env, paths, settings).await;

    let WorkspaceAgents {
        agents: _agents,
        conflicts: agent_conflicts,
    } = load_workspace_agents(paths);

    let (prompt_templates, template_conflicts, template_warnings) =
        load_prompt_templates_resolved(env, paths, cwd, settings).await;
    warnings.extend(template_warnings);

    let cross_kind_conflicts = Vec::new(); // intentional: dispatch order (templates before skills) handles priority

    let resources = AgentHarnessResources {
        skills,
        prompt_templates,
    };

    LoadResourcesResult {
        resources,
        skill_conflicts,
        agent_conflicts,
        template_conflicts,
        cross_kind_conflicts,
        warnings,
    }
}

/// Load templates from each directory in priority order; later dirs win on name clash.
async fn load_prompt_templates_resolved(
    env: &LocalExecutionEnv,
    paths: &Paths,
    cwd: &Path,
    settings: &Settings,
) -> (Vec<PromptTemplate>, Vec<TemplateConflict>, Vec<String>) {
    let mut source_by_name: HashMap<String, (String, String)> = HashMap::new();
    let mut by_name: HashMap<String, PromptTemplate> = HashMap::new();
    let mut conflicts = Vec::new();
    let mut warnings = Vec::new();
    let bases = [cwd, paths.project_dir().as_path()];

    for (path, label) in
        prompt_template_dir_entries(paths, cwd, settings.include_project_resources(), &settings.extra_prompt_paths())
    {
        let identity = resource_dir_identity(&path, &bases).to_string_lossy().into_owned();
        let loaded = load_prompt_templates(env, &[path.as_str()]).await;
        for diagnostic in loaded.diagnostics {
            warnings.push(format!("prompt template ({}): {}", diagnostic.path, diagnostic.message));
        }
        for template in loaded.prompt_templates {
            if let Some((previous_id, previous_label)) = source_by_name.get(&template.name)
                && previous_id != &identity
            {
                conflicts.push(TemplateConflict {
                    name: template.name.clone(),
                    overridden_label: previous_label.clone(),
                    winner_label: label.clone(),
                });
            }
            source_by_name.insert(template.name.clone(), (identity.clone(), label.clone()));
            by_name.insert(template.name.clone(), template);
        }
    }

    let mut prompt_templates: Vec<PromptTemplate> = by_name.into_values().collect();
    prompt_templates.sort_by(|a, b| a.name.cmp(&b.name));
    conflicts.sort_by(|a, b| a.name.cmp(&b.name));
    (prompt_templates, conflicts, warnings)
}

/// User-facing multi-section notice for all resource name conflicts.
///
/// Prefer sending this via [`crate::agent::AgentUiEvent::TranscriptNotice`] so it
/// appends to the transcript and is not replaced by later status Meta lines.
pub fn format_resource_conflict_notice(result: &LoadResourcesResult) -> Option<String> {
    conflict_notice::format_name_conflicts(
        &result.skill_conflicts,
        &result.agent_conflicts,
        &result.template_conflicts,
        &result.cross_kind_conflicts,
    )
}

/// Non-fatal load warnings for transcript (parse errors, unreadable dirs).
pub fn format_resource_load_warnings(result: &LoadResourcesResult) -> Option<String> {
    if result.warnings.is_empty() {
        return None;
    }
    let mut lines = vec!["Resource load warnings:".to_string()];
    for w in &result.warnings {
        lines.push(format!("  • {w}"));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_paths(tmp: &TempDir) -> Paths {
        Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), tmp.path().join("project"))
    }

    #[test]
    fn format_notice_includes_all_sections() {
        let result = LoadResourcesResult {
            skill_conflicts: vec![SkillConflict {
                name: "review".into(),
                overridden_label: "bundled".into(),
                winner_label: "user".into(),
            }],
            agent_conflicts: vec![AgentConflict {
                name: "planner".into(),
                overridden_label: "a".into(),
                winner_label: "b".into(),
            }],
            template_conflicts: vec![TemplateConflict {
                name: "ship".into(),
                overridden_label: "home".into(),
                winner_label: "project".into(),
            }],
            cross_kind_conflicts: vec![CrossKindConflict {
                name: "debug".into(),
                slash_winner: "prompt template",
            }],
            ..Default::default()
        };
        let text = format_resource_conflict_notice(&result).expect("notice");
        assert!(text.contains("Skills:"));
        assert!(text.contains("review"));
        assert!(text.contains("Agents:"));
        assert!(text.contains("Prompt templates:"));
        assert!(text.contains("ship"));
        assert!(text.contains("/debug"));
    }

    #[test]
    fn format_notice_none_when_empty() {
        assert!(format_resource_conflict_notice(&LoadResourcesResult::default()).is_none());
    }

    #[tokio::test]
    async fn template_last_wins_records_conflict() {
        let tmp = TempDir::new().unwrap();
        let paths = test_paths(&tmp);
        let home = paths.prompts_dir();
        let project = paths.project_elph_dir().join("prompts");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(home.join("ship.md"), "---\ndescription: home\n---\nHome body\n").unwrap();
        std::fs::write(project.join("ship.md"), "---\ndescription: project\n---\nProject body\n").unwrap();

        let env = LocalExecutionEnv::new(paths.project_dir());
        let mut settings = Settings::defaults();
        settings.project_layer_loaded = true;
        let loaded = load_resources(&paths, paths.project_dir(), &env, &settings).await;
        assert_eq!(loaded.template_count(), 1);
        assert_eq!(loaded.resources.prompt_templates[0].description, "project");
        assert_eq!(loaded.template_conflicts.len(), 1);
        assert_eq!(loaded.template_conflicts[0].name, "ship");
    }

    #[tokio::test]
    async fn extra_relative_prompt_dir_is_not_a_conflict() {
        let tmp = TempDir::new().unwrap();
        let paths = test_paths(&tmp);
        let agents_prompts = paths.project_dir().join(".agents").join("prompts");
        std::fs::create_dir_all(&agents_prompts).unwrap();
        std::fs::write(agents_prompts.join("ship.md"), "---\ndescription: once\n---\nBody\n").unwrap();

        let env = LocalExecutionEnv::new(paths.project_dir());
        let mut settings = Settings::defaults();
        settings.project_layer_loaded = true;
        settings.resources.prompts = vec![".agents/prompts".into()];
        let loaded = load_resources(&paths, paths.project_dir(), &env, &settings).await;
        assert_eq!(loaded.template_count(), 1);
        assert!(loaded.template_conflicts.is_empty());
    }
}
