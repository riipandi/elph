//! Optional TOON encoding for structured prompt payloads (tool results).

mod apply;
mod config;
mod encode;
mod heuristic;

pub use apply::apply_to_tool_result;
pub use config::{PromptEncodingConfig, PromptEncodingMode, PromptEncodingTargets};
pub use encode::encode_value;
