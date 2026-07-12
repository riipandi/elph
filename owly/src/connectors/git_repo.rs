use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

use super::io::{self, create_run_id, update_state_with_run, write_raw_json};
use super::types::{ConnectorId, ConnectorIngestOptions, ConnectorIngestResult, ConnectorRunRecord, IngestStatus};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GitRepoConfig {
    #[serde(default)]
    repos: Vec<GitRepoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitRepoEntry {
    id: String,
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitRepoManifest {
    id: String,
    path: String,
    branch: String,
    head: String,
    status: String,
    changed_files: Vec<String>,
    recent_commits: Vec<String>,
}

pub fn ingest(options: ConnectorIngestOptions) -> Result<ConnectorIngestResult> {
    let run_id = create_run_id();
    let config: GitRepoConfig = io::read_connector_config(ConnectorId::GitRepo)?;
    let mut state = io::read_connector_state(ConnectorId::GitRepo)?;
    let mut warnings = Vec::new();
    let mut raw_files = Vec::new();

    if config.repos.is_empty() {
        return Ok(skipped(
            run_id,
            "No local repositories configured. Add repos to ~/.owly/connectors/git-repo/config.json.",
            warnings,
        ));
    }

    let limit = options.limit.unwrap_or(config.repos.len());
    let mut manifests = Vec::new();

    for repo in config.repos.iter().take(limit) {
        if !is_safe_repo_id(&repo.id) {
            warnings.push(format!("Skipped repo with unsafe id: {}", repo.id));
            continue;
        }
        let repo_path = Path::new(&repo.path);
        match create_repo_manifest(&repo.id, repo_path) {
            Ok(manifest) => manifests.push(manifest),
            Err(err) => warnings.push(format!("{}: {err}", repo.id)),
        }
    }

    if !manifests.is_empty() {
        let path = write_raw_json(
            ConnectorId::GitRepo,
            &run_id,
            "manifest.json",
            &serde_json::json!({
                "generatedAt": chrono::Utc::now().to_rfc3339(),
                "repos": manifests,
            }),
        )?;
        raw_files.push(path);
    }

    let status = if manifests.is_empty() {
        IngestStatus::Skipped
    } else {
        IngestStatus::Success
    };

    state = update_state_with_run(
        state,
        ConnectorRunRecord {
            at: chrono::Utc::now().to_rfc3339(),
            run_id: run_id.clone(),
            status: status.as_str().to_string(),
            raw_files: raw_files.clone(),
            warnings: warnings.clone(),
        },
    );
    io::write_connector_state(ConnectorId::GitRepo, &state)?;

    Ok(ConnectorIngestResult {
        connector_id: ConnectorId::GitRepo,
        message: format!("Wrote {} local git repo manifest(s).", manifests.len()),
        raw_files,
        run_id,
        status,
        warnings,
    })
}

fn create_repo_manifest(id: &str, repo_path: &Path) -> Result<GitRepoManifest> {
    if !repo_path.is_dir() {
        anyhow::bail!("not a directory: {}", repo_path.display());
    }
    Ok(GitRepoManifest {
        id: id.to_string(),
        path: repo_path.display().to_string(),
        branch: run_git(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])?,
        head: run_git(repo_path, &["rev-parse", "HEAD"])?,
        status: run_git(repo_path, &["status", "--short"])?,
        changed_files: run_git(repo_path, &["diff", "--name-status", "HEAD"])?
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        recent_commits: run_git(repo_path, &["log", "--max-count=20", "--name-status", "--oneline"])?
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
    })
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("git {:?} in {}", args, cwd.display()))?;
    if !output.status.success() {
        anyhow::bail!("git failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_safe_repo_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80 && value.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
}

fn skipped(run_id: String, message: &str, warnings: Vec<String>) -> ConnectorIngestResult {
    ConnectorIngestResult {
        connector_id: ConnectorId::GitRepo,
        message: message.to_string(),
        raw_files: Vec::new(),
        run_id,
        status: IngestStatus::Skipped,
        warnings,
    }
}
