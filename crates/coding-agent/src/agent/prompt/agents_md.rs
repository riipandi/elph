//! AGENTS.md discovery for coding sessions.

use std::path::Path;

pub fn agents_md_for_cwd(cwd: &Path) -> Option<String> {
    find_agents_md(cwd)
}

fn find_agents_md(mut dir: &Path) -> Option<String> {
    for _ in 0..8 {
        let candidate = dir.join("AGENTS.md");
        if candidate.is_file() {
            return std::fs::read_to_string(candidate).ok();
        }
        dir = dir.parent()?;
    }
    None
}
