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
    #[cfg(windows)]
    {
        pid_alive_windows(pid as u32)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        // Without a cheap probe, assume alive so we do not reclaim early.
        true
    }
}

#[cfg(windows)]
mod win {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn GetExitCodeProcess(handle: *mut core::ffi::c_void, exit_code: *mut u32) -> i32;
    }

    pub(super) fn pid_alive(pid: u32) -> bool {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(handle, &mut code);
            CloseHandle(handle);
            ok != 0 && code == STILL_ACTIVE
        }
    }
}

/// `OpenProcess` + `GetExitCodeProcess`: STILL_ACTIVE (259) means the pid is live.
#[cfg(windows)]
fn pid_alive_windows(pid: u32) -> bool {
    win::pid_alive(pid)
}
