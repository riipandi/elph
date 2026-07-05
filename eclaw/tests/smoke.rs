use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn eclaw_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_eclaw"))
}

fn with_isolated_home(mut cmd: Command) -> (Command, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let data = tmp.path().join("data");
    cmd.env("ECLAW_HOME", &home);
    cmd.env("ECLAW_DATA_DIR", &data);
    (cmd, tmp)
}

#[test]
fn default_run_starts_server() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind ephemeral port");
        listener.local_addr().expect("failed to read ephemeral port").port()
    };

    let (mut cmd, _tmp) = with_isolated_home(eclaw_cmd());
    let mut child = cmd
        .args(["--port", &port.to_string()])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn eclaw");

    let stdout = child.stdout.take().expect("failed to capture stdout");
    let reader = BufReader::new(stdout);

    let started = Instant::now();
    let mut saw_listening = false;
    for line in reader.lines() {
        let line = line.expect("failed to read eclaw stdout");
        if line.contains("listening") {
            saw_listening = true;
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out waiting for eclaw to start"
        );
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(saw_listening);
}

#[test]
fn doctor_exits_successfully() {
    let (mut cmd, _tmp) = with_isolated_home(eclaw_cmd());
    let output = cmd.arg("doctor").output().expect("failed to run eclaw doctor");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("not yet implemented"));
}

#[test]
fn unknown_subcommand_fails() {
    let (mut cmd, _tmp) = with_isolated_home(eclaw_cmd());
    let output = cmd
        .arg("not-a-command")
        .output()
        .expect("failed to run eclaw not-a-command");
    assert!(!output.status.success());
}
