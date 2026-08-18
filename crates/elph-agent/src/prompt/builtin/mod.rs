//! Built-in prompt constants and formatters shipped with elph-agent.

pub mod compaction;
pub mod plan;
pub mod session_name;

pub use compaction::SUMMARIZATION_SYSTEM_PROMPT;
pub use plan::{implement_prompt, plan_mode_reentry_prompt, plan_mode_system_prompt};
