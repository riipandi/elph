use std::process::Command;

#[test]
fn help_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_eclaw"))
        .arg("--help")
        .output()
        .expect("failed to run eclaw --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("eclaw"));
    assert!(stdout.contains("usage") || stdout.contains("Usage"));
}

#[test]
fn version_flag_prints_something() {
    let output = Command::new(env!("CARGO_BIN_EXE_eclaw"))
        .arg("--version")
        .output()
        .expect("failed to run eclaw --version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("eclaw"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_subcommand_prints_something() {
    let output = Command::new(env!("CARGO_BIN_EXE_eclaw"))
        .arg("version")
        .output()
        .expect("failed to run eclaw version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("eclaw"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}
