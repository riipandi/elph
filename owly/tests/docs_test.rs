//! Tests for Owly docs module.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `test/constants.test.ts` and related. Original MIT License, Copyright (c) 2026 LangChain.

use tempfile::TempDir;

#[test]
fn test_frontmatter_generation() {
    let temp_dir = TempDir::new().unwrap();
    let cwd = temp_dir.path();

    // Test write_doc_file creates file with frontmatter
    let result = owly::docs::write_doc_file(
        cwd,
        "quickstart.md",
        "Quickstart Guide",
        "quickstart",
        "# Quickstart\n\nThis is a quickstart guide.",
        Some(&["getting-started", "overview"]),
    );

    assert!(result.is_ok());
    let path = result.unwrap();
    assert!(path.exists());

    // Read and verify frontmatter
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("---\n"));
    assert!(content.contains("title: \"Quickstart Guide\""));
    assert!(content.contains("category: quickstart"));
    assert!(content.contains("last_updated:"));
    assert!(content.contains("tags:"));
    assert!(content.contains("  - getting-started"));
    assert!(content.contains("  - overview"));
    assert!(content.contains("status: published"));
    assert!(content.contains("---\n"));
    assert!(content.contains("# Quickstart"));
}

#[test]
fn test_frontmatter_without_tags() {
    let temp_dir = TempDir::new().unwrap();
    let cwd = temp_dir.path();

    let result = owly::docs::write_doc_file(
        cwd,
        "architecture.md",
        "Architecture",
        "architecture",
        "# Architecture\n\nArchitecture details.",
        None,
    );

    assert!(result.is_ok());
    let path = result.unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("---\n"));
    assert!(content.contains("title: \"Architecture\""));
    assert!(content.contains("category: architecture"));
    assert!(!content.contains("tags:"));
}

#[test]
fn test_create_snapshot() {
    let temp_dir = TempDir::new().unwrap();
    let cwd = temp_dir.path();

    // Test snapshot of non-existent directory
    let snapshot = owly::docs::create_snapshot_for_repo(cwd).unwrap();
    assert!(!snapshot.exists);

    // Create some documentation files
    let owly_dir = cwd.join("openwiki");
    std::fs::create_dir_all(&owly_dir).unwrap();
    std::fs::write(owly_dir.join("quickstart.md"), "# Quickstart\n").unwrap();
    std::fs::write(owly_dir.join("architecture.md"), "# Architecture\n").unwrap();

    // Test snapshot of existing directory
    let snapshot = owly::docs::create_snapshot_for_repo(cwd).unwrap();
    assert!(snapshot.exists);
}

#[test]
fn test_snapshot_has_changed() {
    let temp_dir = TempDir::new().unwrap();
    let cwd = temp_dir.path();

    // Create initial documentation
    let owly_dir = cwd.join("openwiki");
    std::fs::create_dir_all(&owly_dir).unwrap();
    std::fs::write(owly_dir.join("quickstart.md"), "# Quickstart v1\n").unwrap();

    let snapshot1 = owly::docs::create_snapshot_for_repo(cwd).unwrap();

    // Same content should not be changed
    let snapshot2 = owly::docs::create_snapshot_for_repo(cwd).unwrap();
    assert!(!owly::docs::has_changed(&snapshot1, &snapshot2));

    // Modify content
    std::fs::write(owly_dir.join("quickstart.md"), "# Quickstart v2\n").unwrap();
    let snapshot3 = owly::docs::create_snapshot_for_repo(cwd).unwrap();
    assert!(owly::docs::has_changed(&snapshot1, &snapshot3));
}

#[test]
fn test_get_git_summary() {
    // Test that git summary doesn't panic even outside a git repo
    let temp_dir = TempDir::new().unwrap();
    let ctx = owly::mode::WikiContext::code(temp_dir.path());
    let summary = owly::docs::get_git_summary(&ctx);
    // Should return empty or partial summary, not panic
    assert!(summary.is_empty() || summary.contains("git"));
}
