//! Code-mode repository setup (agent guidance files + optional CI workflow).
//!
//! Behavioral port of OpenWiki `src/code-mode.ts`, adapted for Owly:
//! - Uses elph-friendly markers (`OWLY:START` / `OWLY:END`) while remaining compatible
//!   with the legacy `openwiki-context` marker used in this monorepo.
//! - Writes `owly-update.yml` (not `openwiki-update.yml`) that runs `owly --update --print`.

use anyhow::{Context, Result};
use std::path::Path;

use crate::constants::OWLY_DIR;

/// Refreshable block markers (OpenWiki-style START/END).
pub const OWLY_SNIPPET_START: &str = "<!-- OWLY:START -->";
pub const OWLY_SNIPPET_END: &str = "<!-- OWLY:END -->";

/// Legacy monorepo marker (still recognized for upgrade/refresh).
pub const LEGACY_OPENWIKI_CONTEXT: &str = "<!-- openwiki-context -->";

const DEFAULT_CODE_MODE_CRON: &str = "0 8 * * *";
const CODE_MODE_AGENT_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

const AGENT_INSTRUCTION: &str = r#"When searching for repository context, read `openwiki/quickstart.md` first and follow links to the relevant section pages under `openwiki/`. Prefer those docs over re-exploring the entire codebase when they already answer the question."#;

/// Ensure agent guidance snippets (and optionally the GitHub Actions workflow) after a successful docs run.
pub fn ensure_code_mode_repo_setup(cwd: &Path) -> Result<()> {
    ensure_code_mode_repo_setup_with_options(cwd, true)
}

/// Same as [`ensure_code_mode_repo_setup`], with control over writing the CI workflow file.
pub fn ensure_code_mode_repo_setup_with_options(cwd: &Path, write_workflow: bool) -> Result<()> {
    let wiki_entry = cwd.join(OWLY_DIR).join("quickstart.md");
    if !wiki_entry.exists() {
        return Ok(());
    }
    if write_workflow {
        write_code_mode_workflow(cwd, DEFAULT_CODE_MODE_CRON)?;
    }
    write_code_mode_agent_snippets(cwd)?;
    Ok(())
}

/// Build the refreshable agent-guidance snippet (no trailing file newline required by callers).
pub fn create_code_mode_agents_snippet() -> String {
    format!(
        "{OWLY_SNIPPET_START}\n\
         {LEGACY_OPENWIKI_CONTEXT}\n\
         ## OpenWiki Documentation\n\
         \n\
         {AGENT_INSTRUCTION}\n\
         \n\
         Entry point: `{OWLY_DIR}/quickstart.md`.\n\
         {OWLY_SNIPPET_END}"
    )
}

/// GitHub Actions workflow body for scheduled / manual Owly updates.
pub fn create_code_mode_workflow(cron_expression: &str) -> String {
    // Double braces become single braces in the written workflow (`${{ secrets... }}`).
    format!(
        r#"name: Owly Update

on:
  workflow_dispatch:
  schedule:
    - cron: "{cron_expression}"

permissions:
  contents: write
  pull-requests: write

jobs:
  update-docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install owly
        run: cargo install --locked owly

      - name: Update documentation
        env:
          OWLY_PROVIDER: ${{{{ vars.OWLY_PROVIDER || 'opencode' }}}}
          OWLY_MODEL_ID: ${{{{ vars.OWLY_MODEL_ID || 'big-pickle' }}}}
          OPENCODE_API_KEY: ${{{{ secrets.OPENCODE_API_KEY }}}}
          OPENROUTER_API_KEY: ${{{{ secrets.OPENROUTER_API_KEY }}}}
          ANTHROPIC_API_KEY: ${{{{ secrets.ANTHROPIC_API_KEY }}}}
          OPENAI_API_KEY: ${{{{ secrets.OPENAI_API_KEY }}}}
        run: owly --update --print

      - name: Create pull request
        uses: peter-evans/create-pull-request@v7
        with:
          add-paths: |
            openwiki
            AGENTS.md
            CLAUDE.md
            .github/workflows/owly-update.yml
          commit-message: "docs: update openwiki via owly"
          title: "docs: Owly openwiki update"
          body: |
            Automated documentation refresh from `owly --update --print`.
          branch: owly/update-docs
"#
    )
}

fn write_code_mode_workflow(cwd: &Path, cron_expression: &str) -> Result<()> {
    let workflow_path = cwd.join(".github").join("workflows").join("owly-update.yml");
    if let Some(parent) = workflow_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    // Do not clobber a customized workflow if it already exists.
    if workflow_path.exists() {
        return Ok(());
    }
    std::fs::write(&workflow_path, create_code_mode_workflow(cron_expression))
        .with_context(|| format!("write {}", workflow_path.display()))?;
    Ok(())
}

fn write_code_mode_agent_snippets(cwd: &Path) -> Result<()> {
    let snippet = create_code_mode_agents_snippet();
    for name in CODE_MODE_AGENT_FILES {
        write_code_mode_agent_snippet(&cwd.join(name), &snippet)?;
    }
    Ok(())
}

fn write_code_mode_agent_snippet(path: &Path, snippet: &str) -> Result<()> {
    let current = if path.exists() {
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?
    } else {
        String::new()
    };

    let next = merge_agent_snippet(
        &current,
        snippet,
        path.file_name().and_then(|s| s.to_str()).unwrap_or("AGENTS.md"),
    );
    if next != current {
        std::fs::write(path, next).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

/// Merge or replace the Owly guidance block inside an agent instruction file.
pub fn merge_agent_snippet(current: &str, snippet: &str, filename: &str) -> String {
    let start = current.find(OWLY_SNIPPET_START);
    let end = current.find(OWLY_SNIPPET_END);

    if let (Some(start_idx), Some(end_idx)) = (start, end)
        && end_idx > start_idx
    {
        let after_end = end_idx + OWLY_SNIPPET_END.len();
        return format!("{}{}{}", &current[..start_idx], snippet, &current[after_end..]);
    }

    // Upgrade legacy openwiki-context-only block: replace from marker through end of file section.
    if let Some(legacy_idx) = current.find(LEGACY_OPENWIKI_CONTEXT)
        && start.is_none()
    {
        // Keep content before the legacy marker; append fresh START/END snippet.
        let before = current[..legacy_idx].trim_end();
        if before.is_empty() {
            return format!("{snippet}\n");
        }
        return format!("{before}\n\n{snippet}\n");
    }

    if current.trim().is_empty() {
        return format!("# {filename}\n\n{snippet}\n");
    }

    format!("{}\n\n{snippet}\n", current.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn merge_inserts_when_empty() {
        let next = merge_agent_snippet("", &create_code_mode_agents_snippet(), "AGENTS.md");
        assert!(next.contains(OWLY_SNIPPET_START));
        assert!(next.contains(OWLY_SNIPPET_END));
        assert!(next.contains(LEGACY_OPENWIKI_CONTEXT));
    }

    #[test]
    fn merge_refreshes_existing_block() {
        let initial = format!("# Keep me\n\n{OWLY_SNIPPET_START}\nold\n{OWLY_SNIPPET_END}\n\n# After\n");
        let next = merge_agent_snippet(&initial, &create_code_mode_agents_snippet(), "AGENTS.md");
        assert!(next.contains("# Keep me"));
        assert!(next.contains("# After"));
        assert!(next.contains(AGENT_INSTRUCTION));
        assert!(!next.contains("\nold\n"));
    }

    #[test]
    fn merge_upgrades_legacy_marker() {
        let initial = format!("# Agents\n\n{LEGACY_OPENWIKI_CONTEXT}\nold guidance\n");
        let next = merge_agent_snippet(&initial, &create_code_mode_agents_snippet(), "AGENTS.md");
        assert!(next.contains("# Agents"));
        assert!(next.contains(OWLY_SNIPPET_START));
        assert!(!next.contains("old guidance"));
    }

    #[test]
    fn ensure_writes_workflow_once() {
        let dir = tempdir().unwrap();
        let cwd = dir.path();
        std::fs::create_dir_all(cwd.join(OWLY_DIR)).unwrap();
        std::fs::write(cwd.join(OWLY_DIR).join("quickstart.md"), "# hi\n").unwrap();
        ensure_code_mode_repo_setup(cwd).unwrap();
        let wf = cwd.join(".github/workflows/owly-update.yml");
        assert!(wf.exists());
        let content = std::fs::read_to_string(&wf).unwrap();
        assert!(content.contains("owly --update --print"));
        // Second call must not overwrite customizations
        std::fs::write(&wf, "custom\n").unwrap();
        ensure_code_mode_repo_setup(cwd).unwrap();
        assert_eq!(std::fs::read_to_string(&wf).unwrap(), "custom\n");
    }
}
