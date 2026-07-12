//! Integration tests for personal mode paths and metadata.

use owly::metadata::metadata_path;
use owly::mode::{RunMode, WikiContext};
use owly::onboarding_config::{complete_personal_onboarding, read_personal_instructions};
use tempfile::tempdir;

#[test]
fn personal_metadata_path_is_under_wiki_root() {
    let ctx = WikiContext::personal("/tmp/any");
    let path = metadata_path(&ctx);
    assert!(path.ends_with(".last-update.json"));
    assert!(path.to_string_lossy().contains("wiki"));
}

#[test]
fn personal_update_noop_always_proceeds() {
    let ctx = WikiContext::personal("/tmp/any");
    assert!(!owly::metadata::is_update_noop_ctx(&ctx));
}

#[test]
fn personal_snapshot_uses_wiki_root() {
    let dir = tempdir().unwrap();
    let wiki = dir.path().join("wiki");
    std::fs::create_dir_all(&wiki).unwrap();
    std::fs::write(wiki.join("quickstart.md"), "# Hi\n").unwrap();

    let snapshot = owly::docs::create_snapshot_at(&wiki).unwrap();
    assert!(snapshot.exists);
}

#[test]
fn complete_personal_onboarding_writes_instructions() {
    let dir = tempdir().unwrap();
    // SAFETY: test-only HOME override; no concurrent env reads in this test.
    unsafe {
        std::env::set_var("HOME", dir.path());
    }

    complete_personal_onboarding("Track research and commitments.").unwrap();
    let goal = read_personal_instructions().unwrap();
    assert_eq!(goal.as_deref(), Some("Track research and commitments."));

    let config = owly::onboarding_config::read_onboarding_config().unwrap();
    assert_eq!(config.mode_id.as_deref(), Some(RunMode::Personal.as_str()));
    assert!(config.completed_at.is_some());
}

#[test]
fn personal_prepare_init_mentions_quickstart() {
    let ctx = WikiContext::personal("/tmp/any");
    let (system, user) = owly::agent::prepare_init_command(&ctx, None, "big-pickle");
    assert!(system.contains("personal"));
    assert!(user.contains("/quickstart.md"));
}
