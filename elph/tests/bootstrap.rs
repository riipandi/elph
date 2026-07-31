//! Integration tests for home/platform bootstrap.

use elph::platform::{self, AppPaths, Paths};

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
    assert!(paths.changelog_md_path().exists());
    assert!(paths.changelog_json_path().exists());
    assert!(paths.bundled_manifest_path().exists());

    platform::datastore::ensure(&paths).await.expect("ensure datastore");
    // Goals + platform metadata share metadata.db; path must remain APP_DATA/metadata.db.
    assert!(paths.metadata_db_path().exists());
    // Memory DB is lazily opened by MemoryStore::init(), not by ensure_datastore.
    assert!(paths.project_elph_dir().exists());
    assert!(paths.project_gitignore_path().exists());
    assert!(paths.bundled_dir().join("agents").is_dir());
    assert!(paths.bundled_dir().join("personas").is_dir());
    assert!(paths.bundled_dir().join("skills").is_dir());
    assert!(paths.bundled_dir().join("user-guide").is_dir());
    assert!(paths.agents_dir().is_dir());
    assert!(paths.hooks_dir().is_dir());
    assert!(paths.prompts_dir().is_dir());
    assert!(paths.providers_dir().is_dir());
    // Built-in provider catalogs unpacked as kebab-case JSON (never overwrites).
    assert!(
        paths.providers_dir().join("anthropic.json").is_file(),
        "bootstrap should unpack anthropic.json"
    );
    assert!(paths.config_dir().join("AGENTS.md").is_file());
    assert!(paths.projects_dir().is_dir());
    assert!(paths.host_mcp_cache_dir().is_dir());
    assert!(paths.sessions_dir().is_dir());
    assert!(paths.skills_dir().is_dir());
    assert!(paths.worktrees_dir().is_dir());
    assert!(paths.attachments_dir().is_dir());
    assert!(paths.downloads_dir().is_dir());
    assert!(paths.logs_dir().is_dir());
    assert!(paths.mcp_logs_dir().is_dir());
    assert!(paths.vendor_dir().is_dir());
    assert!(paths.global_extensions_dir().is_dir());

    // Runtime layout is under APP_DATA/, not CONFIG_DIR.
    assert!(paths.projects_dir().starts_with(paths.data_dir()));
    assert!(paths.host_mcp_cache_dir().starts_with(paths.data_dir()));
    assert!(paths.worktrees_dir().starts_with(paths.data_dir()));
    assert!(paths.sessions_dir().starts_with(paths.data_dir()));
    // No project-hash layout dirs under projects/ at bootstrap.
    assert!(
        std::fs::read_dir(paths.projects_dir())
            .map(|mut d| d.next().is_none())
            .unwrap_or(true)
    );
}
