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
    // Goals, platform schema, and sessions share the project store DB
    // (`.elph/store.db`); ensure() creates the file with the platform band.
    assert!(paths.memory_db_path().exists());
    // Floppy memory band is applied by MemoryStore on first use.
    assert!(paths.project_elph_dir().exists());
    assert!(paths.project_gitignore_path().exists());
    assert!(paths.bundled_dir().join("agents").is_dir());
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
    // Embedded user-guide + built-in skills extracted under bundled/.
    assert!(
        paths.bundled_dir().join("user-guide/01-getting-started.md").is_file(),
        "bootstrap should unpack user-guide"
    );
    assert!(
        paths.bundled_dir().join("skills/create-skill/SKILL.md").is_file(),
        "bootstrap should unpack create-skill"
    );
    assert!(!paths.config_dir().join("AGENTS.md").exists());
    assert!(!paths.project_extensions_dir().exists());
    assert!(!paths.plans_dir().exists());
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
    assert!(paths.host_mcp_cache_dir().starts_with(paths.data_dir()));
    assert!(paths.worktrees_dir().starts_with(paths.data_dir()));
    assert!(paths.sessions_dir().starts_with(paths.data_dir()));
    // No leftover legacy projects/ root required at bootstrap.
    assert!(
        !paths.data_dir().join("projects").is_dir() || {
            // Empty legacy dir is ok until migration removes it.
            std::fs::read_dir(paths.data_dir().join("projects"))
                .map(|mut d| d.next().is_none())
                .unwrap_or(true)
        }
    );
}
