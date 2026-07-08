use std::process::Command;

#[test]
fn help_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_elph"))
        .arg("--help")
        .output()
        .expect("failed to run elph --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("elph"));
    assert!(stdout.contains("usage") || stdout.contains("Usage"));
}

#[test]
fn memory_help_lists_subcommands() {
    let output = Command::new(env!("CARGO_BIN_EXE_elph"))
        .args(["memory", "--help"])
        .output()
        .expect("failed to run elph memory --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for sub in ["status", "list", "tasks", "log", "search", "purge"] {
        assert!(stdout.contains(sub), "missing subcommand {sub} in:\n{stdout}");
    }
}

#[test]
fn memory_status_on_empty_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_elph"))
        .env("ELPH_PROJECT_DIR", dir.path())
        .args(["memory", "status"])
        .output()
        .expect("failed to run elph memory status");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("floppy status"));
    assert!(stdout.contains("Memories:  0"));

    let memory_db = dir.path().join(".elph/memory.db");
    assert!(memory_db.is_file(), "expected floppy DB at {}", memory_db.display());
    assert!(
        !dir.path().join(".elph/floppy/memory.db").exists(),
        "floppy DB must not use legacy .elph/floppy/ path"
    );
}

#[test]
fn version_flag_prints_something() {
    let output = Command::new(env!("CARGO_BIN_EXE_elph"))
        .arg("--version")
        .output()
        .expect("failed to run elph --version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
}
