use std::process::Command;

#[test]
fn default_run_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_eclaw"))
        .output()
        .expect("failed to run eclaw");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("interactive mode not yet implemented"));
}

#[test]
fn unknown_subcommand_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_eclaw"))
        .arg("not-a-command")
        .output()
        .expect("failed to run eclaw not-a-command");
    assert!(!output.status.success());
}
