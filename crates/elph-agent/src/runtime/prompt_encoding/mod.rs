//! Optional TOON encoding for structured prompt payloads (tool results).

mod apply;
mod config;
mod decode;
mod encode;
mod extract;
mod fence;
mod heuristic;

pub use apply::apply_to_tool_result;
pub use config::{PromptEncodingConfig, PromptEncodingDelimiter, PromptEncodingMode, PromptEncodingTargets};
pub use decode::{ToonDecodeError, decode_toon_fence};
pub use encode::encode_value;
pub use extract::extract_json_value;
pub use fence::parse_toon_fence;
