//! Comprehensive example demonstrating elph-agent skills functionality.
//!
//! This example shows:
//! - Loading skills from multiple directories
//! - Parsing all agentskills.io spec fields
//! - Conflict resolution (last-wins)
//! - Strict vs lenient validation
//! - Directory resolution with app name
//! - Formatting skills for system prompt and invocation

use std::collections::HashMap;

use elph_agent::agent::harness::format_skills_for_system_prompt;
use elph_agent::agent::harness::types::{Skill, SkillLoadOptions, SkillValidationSettings};
use elph_agent::agent::harness::types::{resolve_project_skills_dirs, resolve_user_skills_dirs};
use elph_agent::runtime::local_env::LocalExecutionEnv;
use elph_agent::skills::{format_skill_invocation, load_skills_with_options};
use tempfile::TempDir;

#[tokio::main]
async fn main() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path();
    let env = LocalExecutionEnv::new(root);

    // ── 1. Create example skills with all spec fields ──

    // Skill 1: Code review skill with all optional fields
    env.create_dir(".agents/skills/code-review", true)
        .await
        .expect("create dir");
    env.write_file(
        ".agents/skills/code-review/SKILL.md",
        r#"---
name: code-review
description: Review code for quality, security, and best practices. Use when asked to review code changes.
license: MIT
compatibility: Requires git and rust-analyzer for Rust projects
metadata:
  author: elph-team
  version: "1.0"
  category: development
allowed-tools: read grep git shell_exec
---
# Code Review Skill

When reviewing code, follow these steps:

1. **Understand the context**: Read the PR description and related issues
2. **Check for security issues**: Look for SQL injection, XSS, path traversal
3. **Verify error handling**: Ensure errors are properly handled
4. **Check for performance**: Look for N+1 queries, unnecessary allocations
5. **Review tests**: Verify test coverage and quality

## Output Format

```markdown
## Summary
Brief overview of changes

## Issues Found
- [Critical] Description
- [Warning] Description
- [Info] Description

## Recommendations
1. Recommendation
2. Recommendation
```
"#,
    )
    .await
    .expect("write file");

    // Skill 2: Minimal skill with only required fields
    env.create_dir(".agents/skills/quick-fix", true)
        .await
        .expect("create dir");
    env.write_file(
        ".agents/skills/quick-fix/SKILL.md",
        r#"---
name: quick-fix
description: Apply quick fixes to common issues. Use for simple bug fixes.
---
Fix the issue with minimal changes. Don't refactor unrelated code.
"#,
    )
    .await
    .expect("write file");

    // Skill 3: App-specific skill (simulating elph app)
    env.create_dir(".elph/skills/elph-expert", true)
        .await
        .expect("create dir");
    env.write_file(
        ".elph/skills/elph-expert/SKILL.md",
        r#"---
name: elph-expert
description: Expert knowledge about Elph agent runtime. Use for elph-specific questions.
compatibility: Designed for Elph coding assistant
metadata:
  author: elph-agent
  internal: "true"
allowed-tools: read grep
---
# Elph Expert

You are an expert in the Elph agent runtime.

## Key Concepts

- **Agent Harness**: Orchestrates tool execution and message flow
- **Skills**: Reusable instructions loaded from SKILL.md files
- **Prompt Templates**: Slash commands with argument substitution

## Common Patterns

```rust
// Loading skills
let skills = load_skills(&env, &dirs).await;

// Using skills in system prompt
let prompt = format_skills_for_system_prompt(&skills);
```
"#,
    )
    .await
    .expect("write file");

    // Skill 4: Bundled skill (simulating app bundles)
    env.create_dir(".elph/bundled/skills/debugger", true)
        .await
        .expect("create dir");
    env.write_file(
        ".elph/bundled/skills/debugger/SKILL.md",
        r#"---
name: debugger
description: Debug issues systematically. Use when troubleshooting problems.
license: Apache-2.0
allowed-tools: shell_exec read grep
---
# Debugger Skill

## Debugging Process

1. **Reproduce**: Create a minimal reproduction case
2. **Isolate**: Narrow down the problem area
3. **Hypothesize**: Form theories about root cause
4. **Test**: Verify hypothesis with experiments
5. **Fix**: Implement and verify the fix

## Tools

- Use `shell_exec` for running commands
- Use `read` for examining files
- Use `grep` for searching code
"#,
    )
    .await
    .expect("write file");

    println!("=== 1. Loading Skills from Multiple Directories ===\n");

    // ── 2. Load skills from user-level directories ──
    let user_dirs = resolve_user_skills_dirs("elph");
    println!("User skill directories:");
    for dir in &user_dirs {
        println!("  - {}", dir);
    }

    // For this example, we'll use relative paths
    let result =
        load_skills_with_options(&env, &[".agents/skills", ".elph/skills", ".elph/bundled/skills"], None).await;

    println!("\nLoaded {} skills:", result.skills.len());
    for skill in &result.skills {
        println!("  - {} ({})", skill.name, skill.file_path);
    }

    if !result.diagnostics.is_empty() {
        println!("\nDiagnostics:");
        for diag in &result.diagnostics {
            println!("  - {:?}: {} ({})", diag.code, diag.message, diag.path);
        }
    }

    println!("\n=== 2. Skill Details ===\n");

    // ── 3. Inspect loaded skills ──
    for skill in &result.skills {
        println!("Skill: {}", skill.name);
        println!("  Description: {}", skill.description);
        println!("  License: {:?}", skill.license);
        println!("  Compatibility: {:?}", skill.compatibility);
        println!("  Allowed Tools: {:?}", skill.allowed_tools);
        if let Some(ref metadata) = skill.metadata {
            println!("  Metadata:");
            for (key, value) in metadata {
                println!("    {}: {}", key, value);
            }
        }
        println!();
    }

    println!("=== 3. Conflict Resolution ===\n");

    // ── 4. Demonstrate conflict resolution ──
    // Create a skill in .agents/skills with one description
    env.create_dir(".agents/skills/conflict-demo", true)
        .await
        .expect("create dir");
    env.write_file(
        ".agents/skills/conflict-demo/SKILL.md",
        "---\nname: conflict-demo\ndescription: First version\n---\nFirst content",
    )
    .await
    .expect("write file");

    // Create same skill in .elph/skills with different description
    env.create_dir(".elph/skills/conflict-demo", true)
        .await
        .expect("create dir");
    env.write_file(
        ".elph/skills/conflict-demo/SKILL.md",
        "---\nname: conflict-demo\ndescription: Second version (wins!)\n---\nSecond content",
    )
    .await
    .expect("write file");

    let result = load_skills_with_options(&env, &[".agents/skills", ".elph/skills"], None).await;

    let conflict_skill = result.skills.iter().find(|s| s.name == "conflict-demo");
    if let Some(skill) = conflict_skill {
        println!("Conflict demo skill description: {}", skill.description);
        println!("(Last-wins: .elph/skills overrides .agents/skills)");
    }

    println!("\n=== 4. Strict Validation Mode ===\n");

    // ── 5. Demonstrate strict validation ──
    env.create_dir(".agents/skills/long-compat", true)
        .await
        .expect("create dir");
    let long_compat_content = format!(
        "---\nname: long-compat\ndescription: Skill with long compatibility\ncompatibility: {}\n---\nContent",
        "x".repeat(501)
    );
    env.write_file(".agents/skills/long-compat/SKILL.md", &long_compat_content)
        .await
        .expect("write file");

    // Lenient mode (default) - no diagnostic
    let result = load_skills_with_options(&env, &[".agents/skills/long-compat"], None).await;
    println!("Lenient mode diagnostics: {}", result.diagnostics.len());

    // Strict mode - diagnostic
    let options = SkillLoadOptions {
        validation: SkillValidationSettings { strict_mode: true },
    };
    let result = load_skills_with_options(&env, &[".agents/skills/long-compat"], Some(&options)).await;
    println!("Strict mode diagnostics: {}", result.diagnostics.len());
    if let Some(diag) = result.diagnostics.first() {
        println!("  Diagnostic: {}", diag.message);
    }

    println!("\n=== 5. Formatting Skills ===\n");

    // ── 6. Format skills for system prompt ──
    let system_prompt = format_skills_for_system_prompt(&result.skills);
    println!("System prompt skills section:");
    if system_prompt.len() > 500 {
        println!("{}...", &system_prompt[..500]);
    } else {
        println!("{}", system_prompt);
    }

    println!("\n=== 6. Skill Invocation Format ===\n");

    // ── 7. Format skill invocation ──
    if let Some(skill) = result.skills.first() {
        let invocation = format_skill_invocation(skill, Some("Focus on security issues."));
        println!("Skill invocation:");
        println!("{}", invocation);
    }

    println!("\n=== 7. Directory Resolution ===\n");

    // ── 8. Show directory resolution ──
    println!("User directories for 'elph':");
    for dir in resolve_user_skills_dirs("elph") {
        println!("  - {}", dir);
    }

    println!("\nUser directories for 'acme':");
    for dir in resolve_user_skills_dirs("acme") {
        println!("  - {}", dir);
    }

    println!("\nProject directories for 'elph':");
    for dir in resolve_project_skills_dirs("/my/project", "elph") {
        println!("  - {}", dir);
    }

    println!("\n=== 8. Custom Metadata Usage ===\n");

    // ── 9. Demonstrate metadata usage ──
    let mut metadata = HashMap::new();
    metadata.insert("author".to_string(), serde_json::Value::String("custom".to_string()));
    metadata.insert("version".to_string(), serde_json::Value::String("2.0".to_string()));
    metadata.insert("tags".to_string(), serde_json::Value::String("testing,qa".to_string()));

    let custom_skill = Skill {
        name: "custom".to_string(),
        description: "Custom skill with metadata".to_string(),
        content: "Custom content".to_string(),
        file_path: "/custom/SKILL.md".to_string(),
        disable_model_invocation: false,
        license: Some("GPL-3.0".to_string()),
        compatibility: Some("Requires Python 3.10+".to_string()),
        metadata: Some(metadata),
        allowed_tools: Some(vec!["python".to_string(), "pytest".to_string()]),
        argument_hint: None,
    };

    println!("Custom skill formatted for invocation:");
    println!("{}", format_skill_invocation(&custom_skill, None));

    println!("\n=== Summary ===\n");
    println!("✓ Loaded {} skills from multiple directories", result.skills.len());
    println!("✓ Demonstrated conflict resolution (last-wins)");
    println!("✓ Showed strict vs lenient validation");
    println!("✓ Formatted skills for system prompt and invocation");
    println!("✓ Resolved directories based on app name");
    println!("✓ Used custom metadata");
}
