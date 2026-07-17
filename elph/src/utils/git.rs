//! Git repository introspection via libgit2.

use std::path::Path;

/// True when `cwd` is inside a git work tree.
pub fn is_worktree(cwd: &Path) -> bool {
    git2::Repository::discover(cwd).is_ok()
}

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

/// Count paths with local changes (modified, staged, untracked, etc.).
///
/// Returns `None` when `cwd` is not inside a git repository.
pub fn count_worktree_changes(cwd: &Path) -> Option<u32> {
    let repo = git2::Repository::discover(cwd).ok()?;
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    opts.include_unmodified(false);
    opts.recurse_untracked_dirs(true);
    opts.renames_head_to_index(true);
    opts.renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut opts)).ok()?;
    let count = statuses
        .iter()
        .filter(|entry| {
            let status = entry.status();
            !(status.is_empty() || status == git2::Status::CURRENT)
        })
        .count() as u32;
    Some(count)
}

/// File and line change counts for the working tree vs `HEAD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GitWorktreeStats {
    /// New or untracked paths (`INDEX_NEW`, `WT_NEW`, renames/copies on the add side).
    pub files_added: u32,
    /// Inserted lines across staged, unstaged, and untracked diffs.
    pub lines_added: u32,
    /// Removed paths (`INDEX_DELETED`, `WT_DELETED`, renames on the delete side).
    pub files_deleted: u32,
    /// Deleted lines across staged, unstaged, and untracked diffs.
    pub lines_deleted: u32,
}

fn count_changed_files(statuses: &git2::Statuses<'_>) -> (u32, u32) {
    let mut files_added = 0u32;
    let mut files_deleted = 0u32;
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_empty() || status == git2::Status::CURRENT {
            continue;
        }
        let renamed = status.intersects(git2::Status::INDEX_RENAMED | git2::Status::WT_RENAMED);
        if renamed {
            files_added = files_added.saturating_add(1);
            files_deleted = files_deleted.saturating_add(1);
            continue;
        }
        if status.intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW) {
            files_added = files_added.saturating_add(1);
        }
        if status.intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED) {
            files_deleted = files_deleted.saturating_add(1);
        }
    }
    (files_added, files_deleted)
}

fn read_line_diff_stats(repo: &git2::Repository) -> Option<(u32, u32)> {
    let head_tree = repo.head().ok()?.peel_to_tree().ok()?;
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    opts.include_typechange(true);
    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&head_tree), Some(&mut opts))
        .ok()?;
    let stats = diff.stats().ok()?;
    Some((stats.insertions() as u32, stats.deletions() as u32))
}

/// File-status and line-diff stats for the working tree vs `HEAD`.
///
/// Returns `None` when `cwd` is not inside a git repository.
pub fn read_worktree_stats(cwd: &Path) -> Option<GitWorktreeStats> {
    let repo = git2::Repository::discover(cwd).ok()?;
    let mut status_opts = git2::StatusOptions::new();
    status_opts.include_untracked(true);
    status_opts.include_unmodified(false);
    status_opts.recurse_untracked_dirs(true);
    status_opts.renames_head_to_index(true);
    status_opts.renames_index_to_workdir(true);
    let statuses = repo.statuses(Some(&mut status_opts)).ok()?;
    let (files_added, files_deleted) = count_changed_files(&statuses);
    let (lines_added, lines_deleted) = read_line_diff_stats(&repo).unwrap_or((0, 0));
    Some(GitWorktreeStats {
        files_added,
        lines_added,
        files_deleted,
        lines_deleted,
    })
}

/// Line additions and deletions in the working tree vs `HEAD` (staged, unstaged, and untracked).
///
/// Returns `None` when `cwd` is not inside a git repository.
pub fn read_diff_stats(cwd: &Path) -> Option<(u32, u32)> {
    read_worktree_stats(cwd).map(|stats| (stats.lines_added, stats.lines_deleted))
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

        assert!(is_worktree(dir.path()));
        assert_eq!(read_branch(dir.path()), Some("main".to_string()));
        let stats = read_worktree_stats(dir.path()).expect("worktree stats");
        assert!(stats.lines_added > 0);
        assert_eq!(stats.lines_deleted, 0);
        assert_eq!(stats.files_added, 0);
        assert_eq!(stats.files_deleted, 0);
        assert_eq!(count_worktree_changes(dir.path()), Some(1));
    }

    #[test]
    fn non_git_dir_has_no_footer_git_info() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!is_worktree(dir.path()));
        assert_eq!(read_branch(dir.path()), None);
        assert_eq!(count_worktree_changes(dir.path()), None);
        assert_eq!(read_worktree_stats(dir.path()), None);
    }

    #[test]
    fn diff_stats_count_staged_and_unstaged_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        run_git(dir.path(), &["init", "-b", "main"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("a.txt"), "one\n").expect("write");
        run_git(dir.path(), &["add", "a.txt"]);
        run_git(dir.path(), &["commit", "-m", "init"]);

        std::fs::write(dir.path().join("b.txt"), "new\nfile\n").expect("write");
        std::fs::write(dir.path().join("a.txt"), "one\ntwo\n").expect("write");
        run_git(dir.path(), &["add", "b.txt"]);

        let stats = read_worktree_stats(dir.path()).expect("worktree stats");
        assert!(stats.lines_added >= 3, "expected staged + unstaged insertions");
        assert_eq!(stats.lines_deleted, 0);
        assert_eq!(stats.files_added, 1, "staged new file b.txt");
        assert_eq!(stats.files_deleted, 0);
    }

    #[test]
    fn worktree_stats_count_new_and_deleted_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        run_git(dir.path(), &["init", "-b", "main"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("keep.txt"), "stay\n").expect("write");
        std::fs::write(dir.path().join("drop.txt"), "gone\n").expect("write");
        run_git(dir.path(), &["add", "."]);
        run_git(dir.path(), &["commit", "-m", "init"]);

        std::fs::write(dir.path().join("new.txt"), "fresh\n").expect("write");
        std::fs::remove_file(dir.path().join("drop.txt")).expect("remove");
        run_git(dir.path(), &["add", "-A"]);

        let stats = read_worktree_stats(dir.path()).expect("worktree stats");
        assert_eq!(stats.files_added, 1);
        assert_eq!(stats.files_deleted, 1);
        assert!(stats.lines_added > 0);
        assert!(stats.lines_deleted > 0);
    }
}
