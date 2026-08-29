//! Process panic → hour-grained JSONL crash log under the app logs directory.

use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_jsonlines::JsonLinesWriter;

/// Prefix for crash log files (`crash-YYMMDDhh.jsonl`).
pub const CRASH_LOG_PREFIX: &str = "crash";

/// Legacy alias kept for re-exports that expected a basename constant.
pub const CRASH_LOG_FILE: &str = CRASH_LOG_PREFIX;

static CRASH_LOG_DIR: OnceLock<Mutex<PathBuf>> = OnceLock::new();
static CRASH_WRITE_LOCK: Mutex<()> = Mutex::new(());
static HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

/// One panic recorded as a JSON line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashRecord {
    pub ts: String,
    pub version: String,
    pub thread: String,
    pub location: String,
    pub message: String,
    pub backtrace: String,
}

/// Install a process-wide panic hook that appends to `{logs_dir}/crash-YYMMDDhh.jsonl`.
pub fn install_panic_hook(logs_dir: impl Into<PathBuf>) {
    let dir = logs_dir.into();
    let crash_dir = CRASH_LOG_DIR.get_or_init(|| Mutex::new(dir.clone()));
    if let Ok(mut configured_dir) = crash_dir.lock() {
        *configured_dir = dir;
    } else {
        // A poisoned directory lock should not prevent the original panic
        // hook from rendering its diagnostic.
        let _ = writeln!(io::stderr(), "elph: crash log configuration is unavailable");
    }

    HOOK_INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let dir = CRASH_LOG_DIR.get().map(|dir| match dir.lock() {
                Ok(configured_dir) => configured_dir.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            });
            if let Some(dir) = dir
                && let Err(err) = write_crash_report(&dir, info)
            {
                let _ = writeln!(io::stderr(), "elph: failed to write crash log under {}: {err}", dir.display(),);
            }
            previous(info);
        }));
    });
}

/// Full path to this UTC hour's crash log (`crash-YYMMDDhh.jsonl`).
pub fn crash_log_path(logs_dir: &Path) -> PathBuf {
    logs_dir.join(crash_log_filename_for(Utc::now()))
}

/// `crash-YYMMDDhh.jsonl` for the given UTC instant.
pub fn crash_log_filename_for(now: chrono::DateTime<Utc>) -> String {
    format!("{CRASH_LOG_PREFIX}-{}.jsonl", now.format("%y%m%d%H"))
}

fn write_crash_report(logs_dir: &Path, info: &PanicHookInfo<'_>) -> io::Result<()> {
    // A panic can be raised concurrently by more than one worker. Serialize
    // the complete JSONL write so records cannot interleave on the same file.
    let _write_guard = CRASH_WRITE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    fs::create_dir_all(logs_dir)?;
    let path = crash_log_path(logs_dir);
    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    let record = CrashRecord {
        ts: Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        thread: std::thread::current().name().unwrap_or("<unnamed>").to_string(),
        location: info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "<unknown>".to_string()),
        message: panic_payload(info),
        backtrace: Backtrace::force_capture().to_string(),
    };

    let mut writer = JsonLinesWriter::new(file);
    writer.write(&record)?;
    writer.flush()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn crash_log_filename_is_hour_grained_utc() {
        let t = Utc.with_ymd_and_hms(2026, 8, 18, 14, 7, 0).unwrap();
        assert_eq!(crash_log_filename_for(t), "crash-26081814.jsonl");
    }

    #[test]
    fn crash_log_path_joins_hour_filename() {
        let p = crash_log_path(Path::new("/tmp/elph-logs"));
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(name.starts_with("crash-"), "got {name}");
        assert!(name.ends_with(".jsonl"), "got {name}");
        assert_eq!(name.len(), "crash-YYMMDDhh.jsonl".len());
    }

    #[test]
    fn crash_record_writes_jsonl_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("crash-26081814.jsonl");
        let record = CrashRecord {
            ts: "2026-08-18T14:07:00.000Z".into(),
            version: "0.0.1".into(),
            thread: "main".into(),
            location: "foo.rs:1:1".into(),
            message: "test panic".into(),
            backtrace: "stack".into(),
        };
        {
            let file = OpenOptions::new().create(true).append(true).open(&path).expect("open");
            let mut writer = JsonLinesWriter::new(file);
            writer.write(&record).expect("write");
            writer.flush().expect("flush");
        }
        let body = fs::read_to_string(&path).expect("read");
        let parsed: CrashRecord = serde_json::from_str(body.trim()).expect("json");
        assert_eq!(parsed.message, "test panic");
        assert_eq!(parsed.location, "foo.rs:1:1");
    }

    #[test]
    fn panic_hook_records_caught_panic_in_configured_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        install_panic_hook(dir.path());

        let result = std::panic::catch_unwind(|| panic!("hook test panic"));
        assert!(result.is_err());

        let path = fs::read_dir(dir.path())
            .expect("crash log directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
            .expect("crash log file");
        let body = fs::read_to_string(path).expect("crash log");
        let record: CrashRecord = serde_json::from_str(body.trim()).expect("json");
        assert_eq!(record.message, "hook test panic");
        assert!(!record.backtrace.is_empty());
    }
}
