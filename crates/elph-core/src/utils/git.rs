//! Git repository introspection via libgit2.

use std::path::Path;

/// Current branch name, if the path is inside a git repository.
pub fn read_branch(cwd: &Path) -> Option<String> {
    let repo = git2::Repository::discover(cwd).ok()?;
    let head = repo.head().ok()?;
    if head.is_branch() {
        head.shorthand().ok().map(str::to_string)
    } else {
        let oid = head.target()?;
        Some(oid.to_string()[..8.min(oid.to_string().len())].to_string())
    }
}

/// Count staged/unstaged line additions and deletions in the working tree.
pub fn read_diff_stats(cwd: &Path) -> (u32, u32) {
    let Ok(repo) = git2::Repository::discover(cwd) else {
        return (0, 0);
    };
    let Ok(diff) = repo.diff_index_to_workdir(None, None) else {
        return (0, 0);
    };
    let Ok(stats) = diff.stats() else {
        return (0, 0);
    };
    (stats.insertions() as u32, stats.deletions() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn run_git(dir: &Path, args: &[&str]) {
        let status = Command::new("git").current_dir(dir).args(args).status();
        assert!(status.is_ok_and(|s| s.success()), "git {:?} failed", args);
    }

    #[test]
    fn reads_branch_and_diff_stats() {
        let dir = tempfile::tempdir().expect("tempdir");
        run_git(dir.path(), &["init", "-b", "main"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("file.txt"), "hello\n").expect("write");
        run_git(dir.path(), &["add", "file.txt"]);
        run_git(dir.path(), &["commit", "-m", "init"]);

        std::fs::write(dir.path().join("file.txt"), "hello\nworld\n").expect("write");

        assert_eq!(read_branch(dir.path()), Some("main".to_string()));
        let (add, del) = read_diff_stats(dir.path());
        assert!(add > 0 || del > 0);
    }
}
