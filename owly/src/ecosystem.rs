//! Repository ecosystem hooks (AGENTS.md / CLAUDE.md).

use anyhow::Result;
use std::path::Path;

use crate::constants::OWLY_DIR;

const MARKER: &str = "<!-- openwiki-context -->";

const AGENT_INSTRUCTION: &str = r#"When searching for repository context, read `openwiki/quickstart.md` first and follow links to the relevant section pages under `openwiki/`. Prefer those docs over re-exploring the entire codebase when they already answer the question."#;

/// Append Owly context instructions to agent guidance files when documentation exists.
pub fn sync_agent_guidance_files(cwd: &Path) -> Result<()> {
    let wiki_entry = cwd.join(OWLY_DIR).join("quickstart.md");
    if !wiki_entry.exists() {
        return Ok(());
    }

    for name in ["AGENTS.md", "CLAUDE.md"] {
        sync_agent_file(cwd, name)?;
    }
    Ok(())
}

fn sync_agent_file(cwd: &Path, filename: &str) -> Result<()> {
    let path = cwd.join(filename);
    let block = format!(
        "\n\n{MARKER}\n## Owly documentation\n\n{AGENT_INSTRUCTION}\n\nEntry point: `{OWLY_DIR}/quickstart.md`.\n"
    );

    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        if content.contains(MARKER) {
            return Ok(());
        }
        std::fs::write(&path, format!("{content}{block}"))?;
    } else {
        let content = format!(
            "# {filename}\n\n{MARKER}\n## Owly documentation\n\n{AGENT_INSTRUCTION}\n\nEntry point: `{OWLY_DIR}/quickstart.md`.\n"
        );
        std::fs::write(&path, content)?;
    }

    Ok(())
}
