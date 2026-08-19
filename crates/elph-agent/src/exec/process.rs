//! Process-tree termination helpers.
//!
//! `child.kill()` in tokio only terminates the *direct* child (the `sh -c`
//! wrapper). Commands with grandchildren (`npm test`, `cargo build`, scripts
//! that spawn children) keep those alive — and because they still hold the
//! stdout/stderr pipes or PTY master, every `.await` on the child/wait/read
//! hangs until they exit. Abort and timeout therefore appeared to "freeze".
//!
//! These helpers kill the whole process group (graceful SIGTERM first, then
//! SIGKILL) like a terminal Ctrl+C would, using `rustix` (already a
//! dependency) instead of invoking a second binary.

use std::time::Duration;

use tokio::time;

/// How long to wait after SIGTERM before escalating to SIGKILL.
#[cfg(unix)]
const TERM_GRACE: Duration = Duration::from_millis(1500);

/// How long to wait after SIGTERM when the user explicitly asked to abort.
#[cfg(unix)]
const ABORT_GRACE: Duration = Duration::from_millis(100);

/// Best-effort termination of the whole process group of `child`.
///
/// Unix only: the child was spawned with `process_group(0)` so its pid is the
/// group id. Killing `-pgid` reaches the shell and every grandchild. Returns
/// immediately; the caller reaps the child (bounded `wait`) so pipe waiters
/// resolve.
#[cfg(unix)]
pub(crate) async fn terminate_child_tree(child: &mut tokio::process::Child, force: bool) {
    let Some(pid) = child.id().and_then(|raw| rustix::process::Pid::from_raw(raw as i32)) else {
        // Child already reaped; nothing to signal.
        return;
    };
    let grace = if force { ABORT_GRACE } else { TERM_GRACE };

    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::TERM);
    // Give the group a moment to exit on its own before SIGKILL, so processes
    // that trap SIGTERM (shells, test runners) can clean up.
    time::sleep(grace).await;
    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    // Backstop: if for some reason the child was not a group leader, kill it
    // directly. `start_kill` is non-blocking.
    let _ = child.start_kill();
}

/// Windows: `taskkill /T` ends the shell and its descendants (no POSIX process groups).
#[cfg(windows)]
pub(crate) async fn terminate_child_tree(child: &mut tokio::process::Child, _force: bool) {
    if let Some(pid) = child.id() {
        let mut kill = tokio::process::Command::new("taskkill");
        kill.args(["/PID", &pid.to_string(), "/T", "/F"]);
        kill.stdout(std::process::Stdio::null());
        kill.stderr(std::process::Stdio::null());
        // Avoid a flash of `taskkill.exe` console on GUI hosts.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        kill.creation_flags(CREATE_NO_WINDOW);
        let _ = kill.status().await;
    }
    let _ = child.start_kill();
}

/// Other non-Unix: kill the direct child only.
#[cfg(not(any(unix, windows)))]
pub(crate) async fn terminate_child_tree(child: &mut tokio::process::Child, _force: bool) {
    let _ = child.start_kill();
}

/// Bounded wait after termination so pipe-holding grandchildren can't hang us.
///
/// Returns `Some(status)` if the child was reaped within `timeout`, else `None`
/// (the caller should still return/treat as stopped).
pub(crate) async fn reap_bounded(
    child: &mut tokio::process::Child,
    timeout: Duration,
) -> Option<std::process::ExitStatus> {
    match time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => Some(status),
        _ => None,
    }
}
