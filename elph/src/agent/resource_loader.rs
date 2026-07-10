//! Load skills, prompts, and project context into harness resources.

use elph_agent::AgentHarnessResources;
use elph_core::utils::path::AppPaths;
use std::path::Path;

use super::system_prompt::load_skills_metadata;
use crate::platform::Paths;

pub fn load_resources(paths: &Paths, _cwd: &Path) -> AgentHarnessResources {
    let mut resources = AgentHarnessResources::default();
    resources.skills.extend(load_skills_metadata(&paths.skills_dir()));
    if paths.project_elph_dir().join("skills").is_dir() {
        resources
            .skills
            .extend(load_skills_metadata(&paths.project_elph_dir().join("skills")));
    }
    resources
}
