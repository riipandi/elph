//! Agent builder and initialization — configure app identity, logging, and built-in tools.
//!
//! Demonstrates: `AgentBuilder`, `AgentInit`, `BuiltinToolsBuilder` (with feature gates),
//! `InitProgress`, `LoggingOptions`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_init --features builtin-tools
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use elph_agent::{
    AgentBuilder, AgentInit, BuiltinToolsBuilder, InitProgress, LocalExecutionEnv, LogRotation, LoggingOptions,
};

fn main() {
    // ── 1. AgentBuilder: compose init settings ──
    println!("=== AgentBuilder ===");

    let init: AgentInit = AgentBuilder::new("0.1.0-dev")
        .env_prefix("MYAPP")
        .app_name("myapp")
        .logs_dir(PathBuf::from("/tmp/myapp-logs"))
        .console_enabled(true)
        .build();

    println!("  app_version: {}", init.app_version);
    println!("  quiet_env: {:?}", init.quiet_env);
    println!(
        "  logging: level={}, file={}, console={}, rotation={:?}",
        init.logging.level, init.logging.file_enabled, init.logging.console_enabled, init.logging.rotation,
    );

    // ── 2. Minimal builder (defaults) ──
    println!("\n=== Minimal Builder (defaults) ===");
    let minimal = AgentBuilder::new("1.0.0").build();
    println!(
        "  version={}, file_enabled={}, level={}",
        minimal.app_version, minimal.logging.file_enabled, minimal.logging.level,
    );

    // ── 3. Console-disabled builder (file-only logging) ──
    println!("\n=== File-Only Logging ===");
    let file_only = AgentBuilder::new("2.0.0")
        .app_name("daemon")
        .console_enabled(false)
        .build();
    println!(
        "  console_enabled={}, file_enabled={}",
        file_only.logging.console_enabled, file_only.logging.file_enabled,
    );

    // ── 4. BuiltinToolsBuilder: assemble compile-time enabled tools ──
    println!("\n=== BuiltinToolsBuilder ===");
    let env = Arc::new(LocalExecutionEnv::new(std::env::current_dir().unwrap().as_path()));
    let tools = BuiltinToolsBuilder::new(env.clone()).with_web().build();
    println!("  {} built-in tools compiled in:", tools.len());
    for tool in &tools {
        let mode = tool.execution_mode.map(|m| format!("{m:?}")).unwrap_or_default();
        println!("    - {} ({}) [mode={mode}]", tool.tool.name, tool.label);
    }

    // ── 5. Without-web tools ──
    let no_web = BuiltinToolsBuilder::new(env).without_web().build();
    println!("\n  Without web: {} tools", no_web.len());

    // ── 6. InitProgress: progress bar for startup phases ──
    println!("\n=== InitProgress ===");
    let progress = InitProgress::new(3);
    progress.advance("Loading config...");
    progress.advance("Initializing databases...");
    progress.advance("Starting runtime...");
    progress.finish();
    println!("  (progress bar advanced through 3 steps)");

    // ── 7. LoggingOptions::resolve directly ──
    println!("\n=== LoggingOptions resolusi langsung ===");
    let opts = LoggingOptions::resolve("MYAPP", "demo", Some(PathBuf::from("/tmp/demo-logs")), true);
    println!(
        "  app_name={}, logs_dir={:?}, rotation={:?}",
        opts.app_name, opts.logs_dir, opts.rotation,
    );

    // ── 8. LogRotation variants ──
    println!("\n=== LogRotation ===");
    println!("  Hourly: {:?}", LogRotation::Hourly);
    println!("  Daily:  {:?}", LogRotation::Daily);
    println!("  Weekly: {:?}", LogRotation::Weekly);

    println!("\nDone.");
}
