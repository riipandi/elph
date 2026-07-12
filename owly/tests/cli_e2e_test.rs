//! Integration E2E tests: invoke the owly binary for CLI paths that do not need an LLM.

use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn owly_bin() -> &'static str {
    static BIN: OnceLock<String> = OnceLock::new();
    BIN.get_or_init(|| {
        if let Ok(path) = std::env::var("OWLY_BIN") {
            return path;
        }
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        root.join("target")
            .join(&profile)
            .join("owly")
            .to_string_lossy()
            .into_owned()
    })
}

fn isolated_home() -> &'static Mutex<tempfile::TempDir> {
    static HOME: OnceLock<Mutex<tempfile::TempDir>> = OnceLock::new();
    HOME.get_or_init(|| Mutex::new(tempdir().unwrap()))
}

fn run(args: &[&str]) -> (i32, String) {
    let home = isolated_home().lock().unwrap();
    let out = Command::new(owly_bin())
        .args(args)
        .env("HOME", home.path())
        .output()
        .expect("spawn owly");
    let code = out.status.code().unwrap_or(-1);
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (code, text)
}

#[test]
fn e2e_bare_invocation() {
    let (code, out) = run(&[]);
    assert_eq!(code, 0);
    assert!(out.contains("Interactive mode not yet implemented"));
}

#[test]
fn e2e_help_lists_personal_mode() {
    let (code, out) = run(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("owly personal"));
}

#[test]
fn e2e_init_requires_mode() {
    let (code, out) = run(&["--init"]);
    assert_ne!(code, 0);
    assert!(out.contains("requires a mode"));
}

#[test]
fn e2e_personal_init_dry_run_trailing_flags() {
    let (code, out) = run(&["personal", "--init", "--dry-run"]);
    assert_eq!(code, 0);
    assert!(out.contains("action:"));
    assert!(out.contains("init"));
}

#[test]
fn e2e_dry_run_personal_init_mixed_order() {
    let (code, out) = run(&["--dry-run", "personal", "--init"]);
    assert_eq!(code, 0);
    assert!(out.contains("init"));
}

#[test]
fn e2e_auth_list() {
    let (code, out) = run(&["auth", "list"]);
    assert_eq!(code, 0);
    assert!(out.contains("auth configure"));
}

#[test]
fn e2e_ngrok_rejected() {
    let (code, out) = run(&["ngrok"]);
    assert_ne!(code, 0);
    assert!(out.contains("not supported"));
}
