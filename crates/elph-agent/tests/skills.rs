#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::Path;

use elph_agent::env::LocalExecutionEnv;

use elph_agent::skills::{SkillDiagnosticCode, load_skills, load_sourced_skills};
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
