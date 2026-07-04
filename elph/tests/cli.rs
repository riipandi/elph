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
fn version_flag_prints_something() {
    let output = Command::new(env!("CARGO_BIN_EXE_elph"))
        .arg("--version")
        .output()
        .expect("failed to run elph --version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
}
