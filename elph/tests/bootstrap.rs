//! Integration tests for home/platform bootstrap.

use elph::platform::{self, Paths};
use elph::utils::path::AppPaths;

#[tokio::test]
async fn ensure_creates_full_home() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join("config");
    let data = tmp.path().join("data");
    let project = tmp.path().join("repo");
    let paths = Paths::from_dirs(config, data, project);

    platform::bootstrap::ensure_with_paths(&paths, "0.0.10-test")
        .await
        .expect("ensure home");

    assert!(paths.settings_path().exists());
    assert!(paths.trust_path().exists());
    assert!(paths.version_path().exists());
    assert!(paths.bundled_manifest_path().exists());

    platform::datastore::ensure(&paths).await.expect("ensure datastore");
    assert!(paths.metadata_db_path().exists());
    assert!(paths.memory_db_path().exists());
    assert!(paths.project_gitignore_path().exists());
    assert!(paths.bundled_dir().join("agents").is_dir());
    assert!(paths.bundled_dir().join("personas").is_dir());
    assert!(paths.bundled_dir().join("skills").is_dir());
    assert!(paths.bundled_dir().join("user-guide").is_dir());
    assert!(paths.prompts_dir().is_dir());
    assert!(paths.providers_dir().is_dir());
    assert!(paths.sessions_dir().is_dir());
    assert!(paths.skills_dir().is_dir());
    assert!(paths.worktrees_dir().is_dir());
    assert!(paths.attachments_dir().is_dir());
    assert!(paths.downloads_dir().is_dir());
    assert!(paths.logs_dir().is_dir());
    assert!(paths.vendor_dir().is_dir());
    assert!(paths.global_extensions_dir().is_dir());
}
