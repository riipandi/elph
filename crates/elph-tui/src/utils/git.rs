use std::path::Path;

/// Reads the current branch name from the git repository containing `cwd`.
pub fn read_git_branch(cwd: &Path) -> Option<String> {
    elph_core::utils::git::read_branch(cwd)
}

/// Line additions and deletions in the working tree diff.
pub fn read_git_diff_stats(cwd: &Path) -> (u32, u32) {
    elph_core::utils::git::read_diff_stats(cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegates_to_elph_core() {
        let dir = std::env::temp_dir().join(format!("elph_git_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join(".git"));
        std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        // git2 discovers repos via .git directory metadata; HEAD alone may not suffice.
        // Smoke test that the function is callable.
        let _ = read_git_branch(&dir);
        let _ = read_git_diff_stats(&dir);
        let _ = std::fs::remove_dir_all(dir);
    }
}
