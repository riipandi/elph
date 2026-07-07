//! Integration tests for fff-search backed grep and find tools.

use std::sync::Arc;

use elph_agent::env::LocalExecutionEnv;
use elph_agent::harness::types::{FileSystem, get_or_throw};
use elph_agent::tools::{create_find_tool, create_grep_tool};
use elph_agent::types::ToolResultContent;
use serde_json::json;
use tempfile::TempDir;

fn env_in_temp() -> (TempDir, Arc<LocalExecutionEnv>) {
    let temp = TempDir::new().expect("temp dir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    (temp, env)
}

fn tool_text(result: elph_agent::types::AgentToolResult) -> String {
    match result.content.first() {
        Some(ToolResultContent::Text(text)) => text.text.clone(),
        _ => String::new(),
    }
}

#[tokio::test]
async fn find_tool_matches_glob_pattern_recursively() {
    let (_temp, env) = env_in_temp();
    get_or_throw(env.create_dir("src/nested", true).await);
    get_or_throw(env.write_file("src/main.rs", "fn main() {}\n").await);
    get_or_throw(env.write_file("src/nested/lib.rs", "pub fn lib() {}\n").await);
    get_or_throw(env.write_file("readme.md", "# readme\n").await);

    let tool = create_find_tool(env.clone());
    let result = (tool.execute)("find-1".into(), json!({ "pattern": "*.rs" }), None, None)
        .await
        .expect("find tool");

    let text = tool_text(result);
    assert!(text.contains("src/main.rs"), "expected src/main.rs in:\n{text}");
    assert!(text.contains("src/nested/lib.rs"), "expected nested lib in:\n{text}");
    assert!(!text.contains("readme.md"), "readme should not match:\n{text}");
}

#[tokio::test]
async fn grep_tool_finds_literal_pattern_in_directory() {
    let (_temp, env) = env_in_temp();
    get_or_throw(env.write_file("alpha.txt", "hello world\n").await);
    get_or_throw(env.write_file("beta.txt", "goodbye world\n").await);

    let tool = create_grep_tool(env.clone());
    let result = (tool.execute)(
        "grep-1".into(),
        json!({ "pattern": "hello", "literal": true }),
        None,
        None,
    )
    .await
    .expect("grep tool");

    let text = tool_text(result);
    assert!(text.contains("alpha.txt:1:hello world"), "unexpected output:\n{text}");
    assert!(!text.contains("beta.txt"), "beta should not match:\n{text}");
}

#[tokio::test]
async fn grep_tool_scopes_search_to_single_file() {
    let (_temp, env) = env_in_temp();
    get_or_throw(env.write_file("one.txt", "needle here\n").await);
    get_or_throw(env.write_file("two.txt", "needle there\n").await);

    let path = format!("{}/one.txt", env.cwd().replace('\\', "/"));

    let tool = create_grep_tool(env.clone());
    let result = (tool.execute)(
        "grep-2".into(),
        json!({ "pattern": "needle", "path": path, "literal": true }),
        None,
        None,
    )
    .await
    .expect("grep tool");

    let text = tool_text(result);
    assert!(text.contains("one.txt:1:needle here"), "unexpected output:\n{text}");
    assert!(!text.contains("two.txt"), "two.txt should be excluded:\n{text}");
}

#[tokio::test]
async fn grep_tool_supports_regex_pattern() {
    let (_temp, env) = env_in_temp();
    get_or_throw(env.write_file("data.txt", "foo123 bar\n").await);

    let tool = create_grep_tool(env.clone());
    let result = (tool.execute)("grep-3".into(), json!({ "pattern": "foo\\d+" }), None, None)
        .await
        .expect("grep tool");

    let text = tool_text(result);
    assert!(text.contains("data.txt:1:foo123 bar"), "unexpected output:\n{text}");
}
