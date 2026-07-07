#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::Path;

use elph_agent::env::LocalExecutionEnv;

use elph_agent::harness::types::PromptTemplate;
use elph_agent::prompt_templates::{
    PromptTemplateDiagnosticCode, format_prompt_template_invocation, load_prompt_templates,
    load_sourced_prompt_templates, parse_command_args, substitute_args,
};
use tempfile::TempDir;

fn join_path(root: &Path, parts: &[&str]) -> String {
    parts
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part))
        .to_string_lossy()
        .replace('\\', "/")
}

#[tokio::test]
async fn load_prompt_templates_non_recursively() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir("a/nested", true).await.expect("create dir");
    env.create_dir("b", true).await.expect("create dir");
    env.write_file("a/one.md", "---\ndescription: One template\n---\nHello $1")
        .await
        .expect("write one");
    env.write_file("a/nested/ignored.md", "Ignored")
        .await
        .expect("write ignored");
    env.write_file("b/two.md", "First line description\nBody")
        .await
        .expect("write two");

    let result = load_prompt_templates(&env, &["a", "b"]).await;

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.prompt_templates.len(), 2);
    assert_eq!(result.prompt_templates[0].name, "one");
    assert_eq!(result.prompt_templates[0].description, "One template");
    assert_eq!(result.prompt_templates[0].content, "Hello $1");
    assert_eq!(result.prompt_templates[1].name, "two");
    assert_eq!(result.prompt_templates[1].description, "First line description");
    assert_eq!(result.prompt_templates[1].content, "First line description\nBody");
}

#[tokio::test]
async fn load_sourced_prompt_templates_preserves_source() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Source {
        kind: &'static str,
    }

    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.create_dir("prompts", true).await.expect("create dir");
    env.write_file("prompts/example.md", "---\ndescription: Example\n---\nExample body")
        .await
        .expect("write file");

    let result = load_sourced_prompt_templates(&env, &[("prompts".to_string(), Source { kind: "project" })]).await;

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.prompt_templates.len(), 1);
    assert_eq!(result.prompt_templates[0].prompt_template.name, "example");
    assert_eq!(result.prompt_templates[0].source, Source { kind: "project" });
}

#[tokio::test]
async fn load_sourced_prompt_templates_attaches_source_to_diagnostics() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Source {
        kind: &'static str,
    }

    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.write_file("broken.md", "---\ndescription: [unterminated\n---\nBody")
        .await
        .expect("write file");

    let result = load_sourced_prompt_templates(&env, &[("broken.md".to_string(), Source { kind: "user" })]).await;

    assert!(result.prompt_templates.is_empty());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, PromptTemplateDiagnosticCode::ParseFailed);
    assert_eq!(result.diagnostics[0].path, join_path(&root, &["broken.md"]));
    assert_eq!(result.diagnostics[0].source, Source { kind: "user" });
}

#[tokio::test]
async fn load_prompt_templates_from_files_and_symlinks() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().to_path_buf();
    let env = LocalExecutionEnv::new(&root);

    env.write_file("target.md", "---\ndescription: Target\n---\nTarget body")
        .await
        .expect("write target");
    symlink(root.join("target.md"), root.join("link.md")).expect("symlink");

    let result = load_prompt_templates(&env, &["target.md", "link.md"]).await;

    assert_eq!(result.prompt_templates.len(), 2);
    assert_eq!(result.prompt_templates[0].name, "target");
    assert_eq!(result.prompt_templates[1].name, "link");
}

#[test]
fn parse_command_args_handles_quotes() {
    assert_eq!(
        parse_command_args(r#""hello world" test 'quoted'"#),
        vec!["hello world".to_string(), "test".to_string(), "quoted".to_string()]
    );
}

#[test]
fn substitute_args_replaces_placeholders() {
    let content = "$1 ${@:2} $ARGUMENTS";
    assert_eq!(
        substitute_args(content, &["hello world".to_string(), "test".to_string()]),
        "hello world test hello world test"
    );
}

#[test]
fn format_prompt_template_invocation_substitutes_arguments() {
    let template = PromptTemplate {
        name: "one".to_string(),
        description: String::new(),
        content: "$1 ${@:2} $ARGUMENTS".to_string(),
    };
    assert_eq!(
        format_prompt_template_invocation(&template, &["hello world".to_string(), "test".to_string()],),
        "hello world test hello world test"
    );
}
