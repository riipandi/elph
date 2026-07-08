//! Skill invocation formatting — elph-agent module.

use crate::env::{basename_env_path, dirname_env_path};
use crate::harness::types::Skill;

/// Format a skill invocation prompt, optionally appending additional user instructions.
pub fn format_skill_invocation(skill: &Skill, additional_instructions: Option<&str>) -> String {
    let skill_dir = dirname_env_path(&skill.file_path);
    let mut skill_block = format!("<skill name=\"{}\" location=\"{}\">", skill.name, skill.file_path);

    // Add optional fields
    if let Some(ref license) = skill.license {
        skill_block.push_str(&format!("\n<license>{}</license>", license));
    }
    if let Some(ref compatibility) = skill.compatibility {
        skill_block.push_str(&format!("\n<compatibility>{}</compatibility>", compatibility));
    }
    if let Some(ref allowed_tools) = skill.allowed_tools
        && !allowed_tools.is_empty()
    {
        skill_block.push_str(&format!("\n<allowed-tools>{}</allowed-tools>", allowed_tools.join(" ")));
    }
    if let Some(ref metadata) = skill.metadata {
        for (key, value) in metadata {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            skill_block.push_str(&format!("\n<meta key=\"{}\" value=\"{}\" />", key, value_str));
        }
    }

    skill_block.push_str(&format!(
        "\nReferences are relative to {}.\n\n{}\n</skill>",
        skill_dir, skill.content
    ));

    match additional_instructions {
        Some(instructions) if !instructions.is_empty() => format!("{skill_block}\n\n{instructions}"),
        _ => skill_block,
    }
}

#[allow(dead_code)]
pub(crate) fn skill_parent_dir_name(file_path: &str) -> String {
    basename_env_path(&dirname_env_path(file_path))
}
