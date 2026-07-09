//! Update metadata tracking for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/agent/utils.ts`. Original MIT License, Copyright (c) 2026 LangChain.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

use crate::constants::{OWLY_DIR, UPDATE_METADATA_PATH};

/// Metadata about the last successful update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMetadata {
    /// When the last update was performed
    pub updated_at: DateTime<Utc>,

    /// The command that was run (init, update)
    pub command: String,

    /// Git HEAD at the time of update
    pub git_head: Option<String>,

    /// Model used for the update
    pub model: String,
}

/// Status of update noop check
#[derive(Debug)]
pub enum UpdateNoopStatus {
    /// Should skip - no changes detected
    Skip { git_head: String, model: String },
    /// Should proceed with update
    Proceed { reason: String },
}

/// Load the last update metadata
pub fn load_metadata(cwd: &Path) -> Option<UpdateMetadata> {
    let path = cwd.join(UPDATE_METADATA_PATH);
    if !path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save update metadata
pub fn save_metadata(cwd: &Path, metadata: &UpdateMetadata) -> Result<()> {
    let dir = cwd.join(OWLY_DIR);
    std::fs::create_dir_all(&dir).context("Failed to create owly directory")?;

    let path = cwd.join(UPDATE_METADATA_PATH);
    let content = serde_json::to_string_pretty(metadata)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Get the current git HEAD
pub fn get_git_head(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
    } else {
        None
    }
}

/// Run a git command and return stdout
pub fn run_git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("--no-pager")
        .args(args)
        .current_dir(cwd)
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                stdout
            } else if stdout.is_empty() {
                stderr
            } else {
                format!("{stdout}\n{stderr}")
            }
        }
        Err(_) => String::new(),
    }
}

/// Check if update is a no-op (no changes since last update)
pub fn is_update_noop(cwd: &Path) -> bool {
    let noop_status = get_update_noop_status(cwd);
    matches!(noop_status, UpdateNoopStatus::Skip { .. })
}

/// Get detailed update noop status
pub fn get_update_noop_status(cwd: &Path) -> UpdateNoopStatus {
    let Some(last_update) = load_metadata(cwd) else {
        return UpdateNoopStatus::Proceed {
            reason: "missing previous update metadata".to_string(),
        };
    };

    let Some(last_head) = &last_update.git_head else {
        return UpdateNoopStatus::Proceed {
            reason: "missing previous update git head".to_string(),
        };
    };

    let Some(current_head) = get_git_head(cwd) else {
        return UpdateNoopStatus::Proceed {
            reason: "missing current git head".to_string(),
        };
    };

    // Check for uncommitted changes
    let status = run_git(cwd, &["status", "--short", "--untracked-files=all"]);
    let meaningful_status: Vec<&str> = status
        .lines()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .filter(|line| !is_update_metadata_status_line(line))
        .collect();

    if !meaningful_status.is_empty() {
        return UpdateNoopStatus::Proceed {
            reason: "worktree has changes".to_string(),
        };
    }

    // Check if HEAD has changed
    if current_head != *last_head {
        // Check if only owly files changed
        let changed_paths = get_changed_paths_since_last_update(cwd, last_head);
        if changed_paths.is_empty() || changed_paths.iter().any(|path| !is_owly_path(path)) {
            return UpdateNoopStatus::Proceed {
                reason: "git head changed with non-owly modifications".to_string(),
            };
        }
    }

    UpdateNoopStatus::Skip {
        git_head: current_head,
        model: last_update.model.clone(),
    }
}

/// Create git summary for the agent prompt
pub fn create_git_summary(cwd: &Path, last_update: Option<&UpdateMetadata>) -> String {
    let mut sections = Vec::new();

    // git status
    let status = run_git(cwd, &["status", "--short"]);
    sections.push(format_git_section("git status --short", &status));

    // git HEAD
    let head = get_git_head(cwd).unwrap_or_else(|| "(unknown)".to_string());
    sections.push(format_git_section("git rev-parse HEAD", &head));

    // git log based on command type
    if let Some(update) = last_update
        && let Some(ref last_head) = update.git_head
    {
        // Update mode with previous HEAD
        let log = run_git(
            cwd,
            &["log", &format!("{last_head}..HEAD"), "--name-status", "--oneline"],
        );
        sections.push(format_git_section(
            &format!("git log {last_head}..HEAD --name-status --oneline"),
            &log,
        ));
    } else if let Some(update) = last_update {
        // Update mode with timestamp
        let timestamp = update.updated_at.to_rfc3339();
        let log = run_git(cwd, &["log", "--since", &timestamp, "--name-status", "--oneline"]);
        sections.push(format_git_section(
            &format!("git log --since {timestamp} --name-status --oneline"),
            &log,
        ));
    } else {
        // Init mode - recent history
        let log = run_git(cwd, &["log", "--max-count=20", "--name-status", "--oneline"]);
        sections.push(format_git_section(
            "git log --max-count=20 --name-status --oneline",
            &log,
        ));
    }

    // git diff
    let diff = run_git(cwd, &["diff", "--name-status", "HEAD"]);
    sections.push(format_git_section("git diff --name-status HEAD", &diff));

    sections.join("\n\n")
}

fn format_git_section(command: &str, output: &str) -> String {
    let display_output = if output.is_empty() { "(no output)" } else { output };
    format!("$ {command}\n{display_output}")
}

fn is_update_metadata_status_line(line: &str) -> bool {
    let status_path = if line.len() > 3 { line[3..].trim() } else { line.trim() };
    let normalized_path = status_path.replace('\\', "/");
    normalized_path == UPDATE_METADATA_PATH || normalized_path.ends_with(&format!(" -> {UPDATE_METADATA_PATH}"))
}

fn get_changed_paths_since_last_update(cwd: &Path, git_head: &str) -> Vec<String> {
    let diff = run_git(cwd, &["diff", "--name-only", &format!("{git_head}..HEAD")]);
    diff.lines()
        .map(|line| line.trim().replace('\\', "/"))
        .filter(|line| !line.is_empty())
        .collect()
}

fn is_owly_path(path: &str) -> bool {
    path == OWLY_DIR || path.starts_with(&format!("{OWLY_DIR}/"))
}
