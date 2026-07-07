//! Skill invocation formatting — elph-agent module.

use crate::env::{basename_env_path, dirname_env_path};
use crate::harness::types::Skill;

/// Format a skill invocation prompt, optionally appending additional user instructions.
pub fn format_skill_invocation(skill: &Skill, additional_instructions: Option<&str>) -> String {
    let skill_dir = dirname_env_path(&skill.file_path);
    let skill_block = format!(
        "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
        skill.name, skill.file_path, skill_dir, skill.content
    );
    match additional_instructions {
        Some(instructions) if !instructions.is_empty() => format!("{skill_block}\n\n{instructions}"),
        _ => skill_block,
    }
}

#[allow(dead_code)]
pub(crate) fn skill_parent_dir_name(file_path: &str) -> String {
    basename_env_path(&dirname_env_path(file_path))
}
