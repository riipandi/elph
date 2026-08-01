//! Process panic → dated crash log under the app logs directory.

use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Prefix for crash log files under the logs directory (`crash.log-YYYYMMDD`).
pub const CRASH_LOG_PREFIX: &str = "crash.log";

static CRASH_LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Install a process-wide panic hook that appends to `{logs_dir}/crash.log-YYYYMMDD`.
///
/// Safe to call once at process start (e.g. after path resolution). Subsequent
/// calls update the target directory if it was not set yet; the hook itself is
/// only installed once.
pub fn install_panic_hook(logs_dir: impl Into<PathBuf>) {
    let dir = logs_dir.into();
    let _ = CRASH_LOG_DIR.set(dir.clone());

    // Only install once — re-entrant set_hook would nest previous hooks.
    static HOOK_INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if HOOK_INSTALLED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = CRASH_LOG_DIR.get()
            && let Err(err) = write_crash_report(dir, info)
        {
            // Last-resort stderr if disk write fails (TUI may already be torn down).
            let _ = writeln!(io::stderr(), "elph: failed to write crash log under {}: {err}", dir.display(),);
        }
        previous(info);
    }));
}

/// Full path to today's crash log for a logs directory (`crash.log-YYYYMMDD`).
pub fn crash_log_path(logs_dir: &Path) -> PathBuf {
    logs_dir.join(crash_log_filename_for(SystemTime::now()))
}

/// `crash.log-YYYYMMDD` for the given instant (UTC).
pub fn crash_log_filename_for(now: SystemTime) -> String {
    let Ok(dur) = now.duration_since(UNIX_EPOCH) else {
        return format!("{CRASH_LOG_PREFIX}-unknown");
    };
    let (y, mo, d, _, _, _) = unix_to_utc_parts(dur.as_secs());
    format!("{CRASH_LOG_PREFIX}-{y:04}{mo:02}{d:02}")
}

/// Legacy alias used by re-exports / tests that expected a fixed basename.
pub const CRASH_LOG_FILE: &str = CRASH_LOG_PREFIX;

fn write_crash_report(logs_dir: &Path, info: &PanicHookInfo<'_>) -> io::Result<()> {
    fs::create_dir_all(logs_dir)?;
    let path = crash_log_path(logs_dir);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

    let ts = format_timestamp_utc();
    let payload = panic_payload(info);
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());

    // Force capture so we get a stack even without RUST_BACKTRACE=1.
    let backtrace = Backtrace::force_capture();

    writeln!(file, "========== crash {} ==========", ts)?;
    writeln!(file, "version: {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(file, "location: {location}")?;
    writeln!(file, "message: {payload}")?;
    writeln!(file, "thread: {}", std::thread::current().name().unwrap_or("<unnamed>"))?;
    writeln!(file, "backtrace:\n{backtrace}")?;
    writeln!(file, "========== end crash ==========\n")?;
    file.flush()?;

    // Also surface via log crate when the bridge is still alive.
    log::error!("panic recorded in {}: {payload} ({location})", path.display());

    Ok(())
}

fn panic_payload(info: &PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = info.payload().downcast_ref::<String>() {
        return s.clone();
    }
    "Box<dyn Any>".to_string()
}

fn format_timestamp_utc() -> String {
    let Ok(dur) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "unknown-time".to_string();
    };
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let (y, mo, d, h, mi, s) = unix_to_utc_parts(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{millis:03}Z")
}

/// Minimal civil date/time from Unix seconds (UTC), enough for crash stamps.
fn unix_to_utc_parts(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = secs % 60;
    let mins = secs / 60;
    let mi = mins % 60;
    let hours = mins / 60;
    let h = hours % 24;
    let days = hours / 24;

    // Civil from days since 1970-01-01 (Howard Hinnant algorithm).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d, h, mi, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn crash_log_filename_is_dated() {
        // 2020-01-01 00:00:00 UTC
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_577_836_800);
        assert_eq!(crash_log_filename_for(t), "crash.log-20200101");
    }

    #[test]
    fn crash_log_path_joins_dated_filename() {
        let p = crash_log_path(Path::new("/tmp/elph-logs"));
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(name.starts_with("crash.log-"), "got {name}");
        assert_eq!(name.len(), "crash.log-YYYYMMDD".len());
    }

    #[test]
    fn write_crash_report_appends_readable_block() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "elph-crash-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        let path = crash_log_path(&dir);
        {
            let mut f = OpenOptions::new().create(true).append(true).open(&path).expect("open");
            writeln!(f, "========== crash test ==========").unwrap();
            writeln!(f, "message: test panic").unwrap();
            writeln!(f, "========== end crash ==========\n").unwrap();
        }
        let body = fs::read_to_string(&path).expect("read");
        assert!(body.contains("test panic"));
        assert!(body.contains("end crash"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unix_to_utc_parts_known_epoch() {
        let (y, m, d, h, mi, s) = unix_to_utc_parts(1_577_836_800);
        assert_eq!((y, m, d, h, mi, s), (2020, 1, 1, 0, 0, 0));
    }
}
