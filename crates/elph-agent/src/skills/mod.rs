//! Skill discovery and formatting.

mod args;
mod format;
mod load;

pub use args::{
    argument_hint_requires_args, format_skill_missing_args_notice, metadata_requires_arguments,
    skill_args_validation_notice, skill_requires_arguments,
};
pub use format::format_skill_invocation;
pub use load::{
    LoadSkillsResult, LoadSourcedSkillsResult, SkillDiagnostic, SkillDiagnosticCode, SourcedSkill,
    SourcedSkillDiagnostic, load_skills, load_skills_with_options, load_sourced_skills,
    load_sourced_skills_with_options,
};
