//! Coding-agent system prompt templates and builders.

mod agents_md;
mod builder;
mod context;
mod modes;
mod template;

pub use agents_md::agents_md_for_cwd;
pub use builder::build_coding_system_prompt;
