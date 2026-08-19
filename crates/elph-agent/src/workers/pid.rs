//! Best-effort process-liveness checks for lease / worker reclaim.

#[cfg(unix)]
use std::path::Path;

/// Returns true when `pid` appears to still be running on this machine.
pub fn pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(unix)]
    {
        if Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
        // macOS / BSD: kill -0 succeeds when the process exists (and is reachable).
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        // Without a cheap cross-platform probe, assume alive so we do not reclaim early.
        true
    }
}
