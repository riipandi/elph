use sha2::{Digest, Sha256};
use std::path::Path;

use crate::checkpoint::{CheckpointConfigurable, RunnableConfig};

use super::{ASK_TOOL_NAMES, TOOL_CHANNEL_PREFIX};

pub fn is_ask_tool(tool_name: &str) -> bool {
    ASK_TOOL_NAMES.contains(&tool_name)
}

pub fn tool_write_channel(tool_name: &str) -> String {
    format!("{TOOL_CHANNEL_PREFIX}{tool_name}")
}

/// Stable session thread id for a repository root (OpenWiki-style).
pub fn create_session_thread_id(cwd: &Path, run_id: Option<&str>) -> String {
    let resolved = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let digest = Sha256::digest(resolved.to_string_lossy().as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    match run_id {
        Some(run) => format!("owly-{}-{run}", &hex[..32]),
        None => format!("owly-{}-interactive", &hex[..32]),
    }
}

pub fn interactive_config(thread_id: impl Into<String>) -> RunnableConfig {
    RunnableConfig {
        configurable: CheckpointConfigurable {
            thread_id: thread_id.into(),
            checkpoint_ns: String::new(),
            checkpoint_id: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_write_channel_formats_name() {
        assert_eq!(tool_write_channel("bash"), "tool:bash");
    }
}
