//! Repository wiki brief (`openwiki/INSTRUCTIONS.md`).
//!
//! Ported from OpenWiki `src/onboarding.ts` (`REPOSITORY_INSTRUCTIONS_FILE`).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::startup::stdin_is_tty;

use crate::constants::OWLY_DIR;
use crate::mode::{RunMode, WikiContext};
use crate::onboarding_config::{self, complete_personal_onboarding};

/// Filename for the user-authored wiki brief (not generated documentation).
pub const INSTRUCTIONS_FILE: &str = "INSTRUCTIONS.md";

/// Path to `openwiki/INSTRUCTIONS.md` under a repository root.
pub fn instructions_path(repo_root: &Path) -> PathBuf {
    repo_root.join(OWLY_DIR).join(INSTRUCTIONS_FILE)
}

/// Read the repository wiki brief when present and non-empty.
pub fn read_repository_instructions(repo_root: &Path) -> Option<String> {
    let path = instructions_path(repo_root);
    let content = std::fs::read_to_string(&path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Persist the wiki brief to `openwiki/INSTRUCTIONS.md`.
pub fn save_repository_instructions(repo_root: &Path, wiki_goal: &str) -> Result<()> {
    let trimmed = wiki_goal.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Wiki brief cannot be empty.");
    }
    let path = instructions_path(repo_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&path, format!("{trimmed}\n")).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Read the wiki brief for the active run mode.
pub fn read_wiki_instructions(ctx: &WikiContext) -> Option<String> {
    match ctx.mode {
        RunMode::Code => read_repository_instructions(&ctx.repo_cwd),
        RunMode::Personal => onboarding_config::read_personal_instructions().ok().flatten(),
    }
}

/// Prompt for a wiki brief when missing (mode-aware).
pub fn prompt_wiki_brief_if_missing(ctx: &WikiContext) -> Result<Option<String>> {
    match ctx.mode {
        RunMode::Code => prompt_repository_wiki_brief_if_missing(&ctx.repo_cwd),
        RunMode::Personal => prompt_personal_wiki_brief_if_missing(),
    }
}

/// Prompt for a repository wiki brief when missing and stdin is a TTY (first init / setup).
pub fn prompt_repository_wiki_brief_if_missing(repo_root: &Path) -> Result<Option<String>> {
    if read_repository_instructions(repo_root).is_some() {
        return Ok(read_repository_instructions(repo_root));
    }
    if !stdin_is_tty() {
        return Ok(None);
    }

    use dialoguer::Input;

    println!();
    println!("\x1b[36;1m>_ Owly wiki brief\x1b[0m");
    println!("Describe what this wiki should understand (scope, priorities, audience).");
    println!(
        "Saved to {}/{} and injected into init/update prompts.",
        OWLY_DIR, INSTRUCTIONS_FILE
    );
    println!();

    let wiki_goal: String = Input::new()
        .with_prompt("Wiki brief")
        .interact_text()
        .context("wiki brief input cancelled")?;

    let trimmed = wiki_goal.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Describe what this wiki should understand.");
    }

    save_repository_instructions(repo_root, trimmed)?;
    Ok(Some(trimmed.to_string()))
}

/// Prompt for a personal wiki brief when missing and stdin is a TTY.
pub fn prompt_personal_wiki_brief_if_missing() -> Result<Option<String>> {
    if let Some(existing) = onboarding_config::read_personal_instructions()? {
        return Ok(Some(existing));
    }
    if !stdin_is_tty() {
        return Ok(None);
    }

    use dialoguer::Input;

    println!();
    println!("\x1b[36;1m>_ Owly personal wiki brief\x1b[0m");
    println!("Describe what your personal wiki should track (scope, priorities, audience).");
    println!("Saved to ~/.owly/INSTRUCTIONS.md and injected into init/update prompts.");
    println!();

    let wiki_goal: String = Input::new()
        .with_prompt("Wiki brief")
        .interact_text()
        .context("wiki brief input cancelled")?;

    let trimmed = wiki_goal.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Describe what your personal wiki should track.");
    }

    complete_personal_onboarding(trimmed)?;
    Ok(Some(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_and_read_round_trip() {
        let dir = tempdir().unwrap();
        save_repository_instructions(dir.path(), "Track API and ops runbooks.").unwrap();
        let read = read_repository_instructions(dir.path()).unwrap();
        assert_eq!(read, "Track API and ops runbooks.");
    }

    #[test]
    fn read_missing_returns_none() {
        let dir = tempdir().unwrap();
        assert!(read_repository_instructions(dir.path()).is_none());
    }

    #[test]
    fn save_rejects_empty() {
        let dir = tempdir().unwrap();
        assert!(save_repository_instructions(dir.path(), "   ").is_err());
    }

    #[test]
    fn instructions_path_joins_openwiki() {
        let dir = tempdir().unwrap();
        assert_eq!(
            instructions_path(dir.path()),
            dir.path().join("openwiki").join("INSTRUCTIONS.md")
        );
    }
}
