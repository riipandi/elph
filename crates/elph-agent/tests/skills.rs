#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::Path;

use elph_agent::env::LocalExecutionEnv;
use elph_agent::harness::types::{
    SkillLoadOptions, SkillValidationSettings, resolve_project_skills_dirs, resolve_user_skills_dirs,
};
use elph_agent::skills::{SkillDiagnosticCode, load_skills, load_skills_with_options, load_sourced_skills};
use tempfile::TempDir;

fn join_path(root: &Path, parts: &[&str]) -> String {
    parts
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part))
        .to_string_lossy()
        .replace('\\', "/")
}

#[tokio::test]
async fn load_skills_from_skill_md() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir(".agents/skills/example", true)
        .await
        .expect("create dir");
    env.write_file(
        ".agents/skills/example/SKILL.md",
        "---\nname: example\ndescription: Example skill\ndisable-model-invocation: true\n---\nUse this skill.\n",
    )
    .await
    .expect("write file");

    let result = load_skills(&env, &[".agents/skills"]).await;

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name, "example");
    assert_eq!(result.skills[0].description, "Example skill");
    assert_eq!(result.skills[0].content, "Use this skill.");
    assert_eq!(
        result.skills[0].file_path,
        join_path(&root, &[".agents", "skills", "example", "SKILL.md"])
    );
    assert!(result.skills[0].disable_model_invocation);
}

#[tokio::test]
async fn load_skills_through_symlinked_directories() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir("actual/example", true).await.expect("create dir");
    env.write_file(
        "actual/example/SKILL.md",
        "---\nname: example\ndescription: Example skill\n---\nUse this skill.",
    )
    .await
    .expect("write file");
    symlink(root.join("actual"), root.join("skills-link")).expect("symlink");

    let result = load_skills(&env, &["skills-link"]).await;

    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name, "example");
    assert_eq!(
        result.skills[0].file_path,
        join_path(&root, &["skills-link", "example", "SKILL.md"])
    );
}

#[tokio::test]
async fn load_sourced_skills_preserves_source() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Source {
        kind: &'static str,
    }

    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir("user/example", true).await.expect("create dir");
    env.write_file(
        "user/example/SKILL.md",
        "---\nname: example\ndescription: Example skill\n---\nUse this skill.",
    )
    .await
    .expect("write file");

    let result = load_sourced_skills(&env, &[("user".to_string(), Source { kind: "user" })]).await;

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].skill.name, "example");
    assert_eq!(result.skills[0].source, Source { kind: "user" });
}

#[tokio::test]
async fn load_sourced_skills_attaches_source_to_diagnostics() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Source {
        kind: &'static str,
    }

    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir("user/broken", true).await.expect("create dir");
    env.write_file("user/broken/SKILL.md", "---\nname: broken\n---\nMissing description.")
        .await
        .expect("write file");

    let result = load_sourced_skills(&env, &[("user".to_string(), Source { kind: "user" })]).await;

    assert!(result.skills.is_empty());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, SkillDiagnosticCode::InvalidMetadata);
    assert_eq!(result.diagnostics[0].message, "description is required");
    assert_eq!(
        result.diagnostics[0].path,
        join_path(&root, &["user", "broken", "SKILL.md"])
    );
    assert_eq!(result.diagnostics[0].source, Source { kind: "user" });
}

#[tokio::test]
async fn load_skills_loads_direct_markdown_children_only_from_root() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir("skills/nested", true).await.expect("create dir");
    env.write_file("skills/root.md", "---\ndescription: Root skill\n---\nRoot content")
        .await
        .expect("write root");
    env.write_file(
        "skills/nested/ignored.md",
        "---\ndescription: Ignored\n---\nIgnored content",
    )
    .await
    .expect("write nested");

    let result = load_skills(&env, &["skills"]).await;

    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name, "skills");
    assert_eq!(result.skills[0].content, "Root content");
}

#[tokio::test]
async fn load_skills_with_optional_fields() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir(".agents/skills/example", true)
        .await
        .expect("create dir");
    env.write_file(
        ".agents/skills/example/SKILL.md",
        "---\nname: example\ndescription: Example skill\nlicense: MIT\ncompatibility: Requires bash\nmetadata:\n  author: test\n  version: '1.0'\nallowed-tools: bash read write\n---\nUse this skill.",
    )
    .await
    .expect("write file");

    let result = load_skills(&env, &[".agents/skills"]).await;

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name, "example");
    assert_eq!(result.skills[0].license, Some("MIT".to_string()));
    assert_eq!(result.skills[0].compatibility, Some("Requires bash".to_string()));
    assert!(result.skills[0].metadata.is_some());
    let metadata = result.skills[0].metadata.as_ref().unwrap();
    assert_eq!(metadata.get("author").unwrap(), "test");
    assert_eq!(metadata.get("version").unwrap(), "1.0");
    assert_eq!(
        result.skills[0].allowed_tools,
        Some(vec!["bash".to_string(), "read".to_string(), "write".to_string()])
    );
}

#[tokio::test]
async fn load_skills_conflict_resolution_last_wins() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    // First directory
    env.create_dir("dir1/example", true).await.expect("create dir");
    env.write_file(
        "dir1/example/SKILL.md",
        "---\nname: example\ndescription: First skill\n---\nFirst content",
    )
    .await
    .expect("write file");

    // Second directory (should override first)
    env.create_dir("dir2/example", true).await.expect("create dir");
    env.write_file(
        "dir2/example/SKILL.md",
        "---\nname: example\ndescription: Second skill\n---\nSecond content",
    )
    .await
    .expect("write file");

    let result = load_skills(&env, &["dir1", "dir2"]).await;

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name, "example");
    assert_eq!(result.skills[0].description, "Second skill");
    assert_eq!(result.skills[0].content, "Second content");
}

#[tokio::test]
async fn load_skills_strict_mode_validates_compatibility() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir(".agents/skills/example", true)
        .await
        .expect("create dir");
    let content = format!(
        "---\nname: example\ndescription: Example\ncompatibility: {}\n---\nContent",
        "x".repeat(501)
    );
    env.write_file(".agents/skills/example/SKILL.md", &content)
        .await
        .expect("write file");

    // Lenient mode - no diagnostic
    let result = load_skills(&env, &[".agents/skills"]).await;
    assert!(result.diagnostics.is_empty());

    // Strict mode - diagnostic
    let options = SkillLoadOptions {
        validation: SkillValidationSettings { strict_mode: true },
    };
    let result = load_skills_with_options(&env, &[".agents/skills"], Some(&options)).await;
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, SkillDiagnosticCode::InvalidMetadata);
    assert!(result.diagnostics[0].message.contains("compatibility exceeds"));
}

#[test]
fn resolve_user_skills_dirs_uses_app_name() {
    let dirs = resolve_user_skills_dirs("elph");
    assert_eq!(dirs.len(), 3);
    assert!(dirs[0].ends_with("/.agents/skills"));
    assert!(dirs[1].ends_with("/.elph/skills"));
    assert!(dirs[2].ends_with("/.elph/bundled/skills"));

    let dirs = resolve_user_skills_dirs("eclaw");
    assert!(dirs[1].ends_with("/.eclaw/skills"));
    assert!(dirs[2].ends_with("/.eclaw/bundled/skills"));
}

#[test]
fn resolve_project_skills_dirs_uses_app_name() {
    let dirs = resolve_project_skills_dirs("/project", "elph");
    assert_eq!(dirs.len(), 2);
    assert_eq!(dirs[0], "/project/.agents/skills");
    assert_eq!(dirs[1], "/project/.elph/skills");

    let dirs = resolve_project_skills_dirs("/project", "eclaw");
    assert_eq!(dirs[1], "/project/.eclaw/skills");
}
