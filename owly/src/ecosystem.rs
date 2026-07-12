//! Repository ecosystem hooks (AGENTS.md / CLAUDE.md + optional CI).
//!
//! Thin re-export of [`crate::code_mode`] so existing call sites keep working.

use anyhow::Result;
use std::path::Path;

pub use crate::code_mode::{
    LEGACY_OPENWIKI_CONTEXT, OWLY_SNIPPET_END, OWLY_SNIPPET_START, create_code_mode_agents_snippet,
    create_code_mode_workflow, ensure_code_mode_repo_setup, ensure_code_mode_repo_setup_with_options,
    merge_agent_snippet,
};

/// Sync agent guidance files (and create the Owly CI workflow if missing) when the wiki exists.
pub fn sync_agent_guidance_files(cwd: &Path) -> Result<()> {
    ensure_code_mode_repo_setup(cwd)
}
