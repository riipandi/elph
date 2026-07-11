//! Harness utilities — shell execution, file system, truncation.
//!
//! Demonstrates: `LocalExecutionEnv`, `Shell` trait, `FileSystem` trait,
//! `execute_shell_with_capture`, `truncate_head`/`truncate_tail`, `format_size`,
//! `format_skills_for_system_prompt`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_harness
//! ```

use std::sync::Arc;

use elph_agent::{
    LocalExecutionEnv, Skill, format_skills_for_system_prompt,
    harness::types::{FileSystem, Result as HResult, Shell, ShellExecOptions},
    harness::utils::{
        TruncationOptions, execute_shell_with_capture, format_size, truncate_head, truncate_line, truncate_tail,
    },
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env = Arc::new(LocalExecutionEnv::new(std::env::current_dir()?));

    // ── Shell execution ──
    println!("=== Shell Execution ===");
    let result = env
        .exec(
            "echo 'hello from harness' && echo 'second line' && echo 'third line'",
            Some(ShellExecOptions {
                cwd: None,
                env: None,
                timeout: None,
                abort_token: None,
                on_stdout: None,
                on_stderr: None,
            }),
        )
        .await;
    match result {
        HResult::Ok(r) => {
            println!("Exit code: {:?}", r.exit_code);
            println!("Output:\n{}", r.stdout);
        }
        HResult::Err(e) => println!("Shell error: {e}"),
    }

    // ── Shell with capture + truncation ──
    println!("\n=== Shell Capture with Truncation ===");
    let captured = execute_shell_with_capture(
        env.as_ref(),
        "echo 'line 1'; echo 'line 2'; echo 'line 3'; echo 'line 4'; echo 'line 5'",
        None,
    )
    .await;
    match captured {
        HResult::Ok(r) => println!("Captured output:\n{}", r.output),
        HResult::Err(e) => println!("Capture error: {e}"),
    }

    // ── FileSystem operations ──
    println!("\n=== FileSystem Operations ===");
    let cwd = env.cwd();
    println!("CWD: {cwd}");

    match env.file_info("Cargo.toml", None).await {
        HResult::Ok(info) => println!("File info: kind={:?}, size={}", info.kind, info.size),
        HResult::Err(e) => println!("file_info error: {e}"),
    }

    match env.file_info("crates", None).await {
        HResult::Ok(info) => println!("Dir info: kind={:?}", info.kind),
        HResult::Err(e) => println!("dir_info error: {e}"),
    }

    match env.exists("nonexistent.xyz", None).await {
        HResult::Ok(exists) => println!("nonexistent.xyz exists: {exists}"),
        HResult::Err(_) => println!("nonexistent.xyz: not found"),
    }

    // ── Truncation utilities ──
    println!("\n=== Truncation ===");
    let long_text = (0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    println!("Original: {} lines", long_text.lines().count());

    let truncated = truncate_head(
        &long_text,
        TruncationOptions {
            max_lines: Some(5),
            max_bytes: None,
        },
    );
    println!("Truncated head ({} lines):\n{}", truncated.output_lines, truncated.content);
    println!("Truncated by: {:?}", truncated.truncated_by);

    let tail = truncate_tail(
        &long_text,
        TruncationOptions {
            max_lines: Some(3),
            max_bytes: None,
        },
    );
    println!("Truncated tail ({} lines):\n{}", tail.output_lines, tail.content);

    let (line, was_truncated) = truncate_line("a".repeat(200).as_str(), 50);
    println!("Single line truncated: {was_truncated}, len={}", line.len());

    // ── Format size ──
    println!("\n=== Format Size ===");
    println!("1024 bytes → {}", format_size(1024));
    println!("1048576 bytes → {}", format_size(1048576));
    println!("1073741824 bytes → {}", format_size(1073741824));

    // ── Skills formatting ──
    println!("\n=== Skills for System Prompt ===");
    let skills = vec![
        Skill {
            name: "diagnose".into(),
            description: "Debug hard bugs".into(),
            content: "skill content here".into(),
            file_path: "skills/diagnose/SKILL.md".into(),
            disable_model_invocation: false,
            license: None,
            compatibility: None,
            metadata: None,
            allowed_tools: None,
        },
        Skill {
            name: "internal-tool".into(),
            description: "Internal tool".into(),
            content: "internal content".into(),
            file_path: "skills/internal/SKILL.md".into(),
            disable_model_invocation: true,
            license: None,
            compatibility: None,
            metadata: None,
            allowed_tools: None,
        },
    ];

    let formatted = format_skills_for_system_prompt(&skills);
    println!("Formatted skills:\n{formatted}");

    Ok(())
}
