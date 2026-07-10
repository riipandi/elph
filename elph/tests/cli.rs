use std::process::Command;

fn elph_command() -> (tempfile::TempDir, Command) {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("home dir");
    let mut command = Command::new(env!("CARGO_BIN_EXE_elph"));
    command.env("ELPH_HOME", &home);
    command.env("ELPH_DATA_DIR", dir.path().join("data"));
    (dir, command)
}

#[test]
fn help_exits_successfully() {
    let (_dir, mut command) = elph_command();
    let output = command.arg("--help").output().expect("failed to run elph --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("elph"));
    assert!(stdout.contains("usage") || stdout.contains("Usage"));
}

#[test]
fn memory_help_lists_subcommands() {
    let (_dir, mut command) = elph_command();
    let output = command
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
    let (_home, mut command) = elph_command();
    let output = command
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

    let memory_db = dir.path().join(".elph/store.db");
    assert!(memory_db.is_file(), "expected floppy DB at {}", memory_db.display());
    assert!(
        !dir.path().join(".elph/floppy/store.db").exists(),
        "floppy DB must not use legacy .elph/floppy/ path"
    );
}

#[test]
fn completions_generates_bash_script() {
    let (_dir, mut command) = elph_command();
    let output = command
        .args(["completions", "--shell", "bash"])
        .output()
        .expect("failed to run elph completions --shell bash");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("elph"), "missing bin name in:\n{stdout}");
    assert!(stdout.contains("extensions"), "missing extensions in:\n{stdout}");
    assert!(stdout.contains("ext"), "missing ext alias in:\n{stdout}");
}

#[test]
fn completions_writes_to_output_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("elph.bash");
    let (_home, mut command) = elph_command();
    let output = command
        .args([
            "completions",
            "--shell",
            "zsh",
            "--output",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("failed to run elph completions --output");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let script = std::fs::read_to_string(&path).expect("read completion file");
    assert!(script.contains("#compdef elph"), "expected zsh compdef in:\n{script}");
}

#[test]
fn version_flag_prints_something() {
    let (_dir, mut command) = elph_command();
    let output = command.arg("--version").output().expect("failed to run elph --version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
}
