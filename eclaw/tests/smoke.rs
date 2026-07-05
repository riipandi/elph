use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::time::{Duration, Instant};

fn health_ready(port: u16) -> bool {
    let mut stream = match TcpStream::connect(format!("127.0.0.1:{port}")) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let request = format!("GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut buf = [0u8; 512];
    let Ok(n) = stream.read(&mut buf) else {
        return false;
    };
    std::str::from_utf8(&buf[..n])
        .ok()
        .is_some_and(|response| response.contains("200"))
}

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
        .spawn()
        .expect("failed to spawn eclaw");

    let started = Instant::now();
    let mut ready = false;
    while started.elapsed() < Duration::from_secs(5) {
        if health_ready(port) {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(ready, "timed out waiting for eclaw /api/health");
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
