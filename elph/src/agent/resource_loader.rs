//! Load skills, prompts, and project context into harness resources.

use elph_agent::{AgentHarnessResources, LocalExecutionEnv, load_prompt_templates};
use elph_core::utils::path::AppPaths;
use std::path::Path;

use super::skills_load::{WorkspaceSkills, load_workspace_skills};
use crate::platform::Paths;

pub struct LoadResourcesResult {
    pub resources: AgentHarnessResources,
    pub skill_conflicts: Vec<super::skills_load::SkillConflict>,
}

pub fn prompt_template_search_paths(paths: &Paths, cwd: &Path) -> Vec<String> {
    let mut search_paths = vec![paths.prompts_dir().to_string_lossy().to_string()];
    let project_prompts = paths.project_elph_dir().join("prompts");
    if project_prompts.is_dir() {
        search_paths.push(project_prompts.to_string_lossy().to_string());
    }
    let agents_prompts = cwd.join(".agents").join("prompts");
    if agents_prompts.is_dir() {
        search_paths.push(agents_prompts.to_string_lossy().to_string());
    }
    search_paths
}

pub async fn load_resources(paths: &Paths, cwd: &Path, env: &LocalExecutionEnv) -> LoadResourcesResult {
    let WorkspaceSkills { skills, conflicts } = load_workspace_skills(env, paths).await;
    let mut resources = AgentHarnessResources::default();
    resources.skills = skills;

    let search_paths = prompt_template_search_paths(paths, cwd);
    let path_refs: Vec<&str> = search_paths.iter().map(String::as_str).collect();
    let loaded = load_prompt_templates(env, &path_refs).await;
    for diagnostic in loaded.diagnostics {
        log::warn!("prompt template load warning ({}): {}", diagnostic.path, diagnostic.message);
    }
    resources.prompt_templates = loaded.prompt_templates;
    LoadResourcesResult {
        resources,
        skill_conflicts: conflicts,
    }
}
