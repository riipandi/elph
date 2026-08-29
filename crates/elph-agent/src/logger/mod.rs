mod crash;
mod options;
mod record;

pub use crash::{CRASH_LOG_FILE, CrashRecord, crash_log_filename_for, crash_log_path, install_panic_hook};
pub use options::{LogRotation, LoggingOptions, LoggingOptionsBuilder, LoggingSettings};
pub use record::{JsonlLayout, LogRecord};

use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

use logforth::Filter;
use logforth::append;
use logforth::append::asynchronous::AsyncBuilder;
use logforth::append::file::FileBuilder;
use logforth::bridge::log::LogBridge;
use logforth::filter::RustLogFilter;
use logforth::layout::TextLayout;

use options::max_level_from_spec;

/// Bounded queue for the async file writer. Caps memory under sustained log bursts.
const FILE_WRITER_BUFFER_LINES: usize = 16_384;

const DEFAULT_SIZE_ROTATION_BYTES: u64 = 10 * 1024 * 1024;

/// Keeps the global logforth bridge alive so async appenders can flush on shutdown.
pub struct LogGuard {
    bridge: Arc<LogBridge>,
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        self.bridge.flush();
        crate::trace::flush();
    }
}

/// Initializes the global logforth logger bridged to the `log` crate.
///
/// The returned [`LogGuard`] must be kept alive for the process lifetime so
/// async appenders can flush buffered records. File-open failure degrades to
/// no file appender (a line is written to stderr) instead of panicking.
pub fn init(options: LoggingOptions) -> LogGuard {
    // Install before initializing any logger/tracing backend so a panic
    // during setup still has a durable last-resort report.
    install_panic_hook(options.logs_dir.clone());
    let trace_enabled = options.trace_enabled;
    crate::trace::init(&options);
    #[cfg(feature = "tracing")]
    elph_ai::trace::set_enabled(trace_enabled && !cfg!(test));
    RESOLVED_LOGS_DIR.get_or_init(|| options.logs_dir.clone());
    install_logger(&options, trace_enabled)
}

/// Resolved logs directory, recorded during [`init`] so other subsystems can
/// persist redirected stderr next to the application log.
static RESOLVED_LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Returns the active logs directory, if logging has been initialized.
pub fn logs_dir() -> Option<PathBuf> {
    RESOLVED_LOGS_DIR.get().cloned()
}

/// Redirects the process stderr (fd 2) to a file inside the logs directory so
/// third-party libraries that write directly to fd 2 — the MCP (rmcp) client —
/// do not corrupt the TUI. Output is persisted under `<logs_dir>/mcp.log`
/// instead of being discarded.
///
/// Safe to call from multiple subsystems; the first call wins and subsequent
/// calls are no-ops (fd 2 is process-global).
#[cfg(unix)]
pub fn redirect_stderr_to_file() {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    static REDIRECTED: OnceLock<std::fs::File> = OnceLock::new();
    if REDIRECTED.get().is_some() {
        return;
    }

    let dir = logs_dir().unwrap_or_else(default_logs_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("mcp.log");
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
        unsafe extern "C" {
            fn dup2(oldfd: std::os::raw::c_int, newfd: std::os::raw::c_int) -> std::os::raw::c_int;
        }
        unsafe {
            dup2(file.as_raw_fd(), 2);
        }
        let _ = REDIRECTED.set(file);
    }
}

#[cfg(not(unix))]
pub fn redirect_stderr_to_file() {}

/// Best-effort logs directory when logging has not been initialized.
#[cfg(unix)]
fn default_logs_dir() -> PathBuf {
    default_logs_dir_for("elph-agent", "ELPH")
}

#[cfg(unix)]
fn default_logs_dir_for(app_name: &str, env_prefix: &str) -> PathBuf {
    let data_key = format!("{env_prefix}_DATA_DIR");
    if let Some(data) = std::env::var_os(&data_key) {
        return PathBuf::from(data).join("logs");
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join(app_name).join("logs");
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/share").join(app_name).join("logs"))
        .unwrap_or_else(|| PathBuf::from(format!(".local/share/{app_name}/logs")))
}

fn level_filter(level: &str) -> Box<dyn Filter> {
    Box::new(RustLogFilter::from(level))
}

fn file_appender(options: &LoggingOptions) -> Result<append::Async, logforth::Error> {
    let mut builder = FileBuilder::new(&options.logs_dir, options.app_name)
        .layout(JsonlLayout)
        .filename_suffix("jsonl");

    builder = match options.rotation {
        LogRotation::Hourly => builder.rollover_hourly(),
        LogRotation::Daily => builder.rollover_daily(),
        LogRotation::Size => builder,
    };

    let size_cap = match options.rotation {
        LogRotation::Size => options.max_bytes.unwrap_or(DEFAULT_SIZE_ROTATION_BYTES),
        _ => options.max_bytes.unwrap_or(0),
    };
    if let Some(max_bytes) = NonZeroUsize::new(size_cap as usize) {
        builder = builder.rollover_size(max_bytes);
    }

    if let Some(max_files) = options.max_files.and_then(NonZeroUsize::new) {
        builder = builder.max_log_files(max_files);
    }

    let file = builder.build()?;

    Ok(AsyncBuilder::new(format!("{}-log-writer", options.app_name))
        .overflow_drop_incoming()
        .buffered_lines_limit(Some(FILE_WRITER_BUFFER_LINES))
        .append(file)
        .build())
}

#[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
fn install_logger(options: &LoggingOptions, trace_enabled: bool) -> LogGuard {
    let filter = level_filter(&options.level);
    let mut starter = logforth::starter_log::builder();

    if options.file_enabled {
        match file_appender(options) {
            Ok(file) => {
                let file_filter = level_filter(&options.level);
                starter = starter.dispatch(|d| d.filter(file_filter).append(file));
            }
            Err(err) => {
                let _ = writeln!(
                    io::stderr(),
                    "elph: failed to initialize rolling log writer under {}: {err}",
                    options.logs_dir.display(),
                );
            }
        }
    }

    if options.console_enabled {
        let stderr = append::Stderr::default().with_layout(TextLayout::default());
        let console_filter = level_filter(&options.level);
        starter = starter.dispatch(|d| d.filter(console_filter).append(stderr));
    }

    #[cfg(feature = "tracing")]
    if trace_enabled {
        let fastrace = append::FastraceEvent::default();
        starter = starter.dispatch(|d| d.filter(filter).append(fastrace));
    } else {
        let _ = filter;
    }
    #[cfg(not(feature = "tracing"))]
    let _ = filter;

    let logger = starter.build();
    let bridge = Arc::new(LogBridge::new(logger));
    let _ = log::set_boxed_logger(Box::new(bridge.clone()));
    log::set_max_level(max_level_from_spec(&options.level));

    log::info!(
        "logging ready level={} file={} console={} trace={} dir={}",
        options.level,
        options.file_enabled,
        options.console_enabled,
        options.trace_enabled,
        options.logs_dir.display(),
    );

    LogGuard { bridge }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_guard_flushes_on_drop() {
        let bridge = Arc::new(LogBridge::new(logforth::starter_log::builder().build()));
        let guard = LogGuard { bridge: bridge.clone() };
        drop(guard);
    }

    #[test]
    fn file_appender_uses_jsonl_suffix_without_double_dot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let options = LoggingOptions::builder()
            .app_name("elph")
            .logs_dir(dir.path().to_path_buf())
            .file_enabled(true)
            .console_enabled(false)
            .trace_enabled(false)
            .build();
        let appender = file_appender(&options).expect("file appender");
        drop(appender);
        let live = dir.path().join("elph.jsonl");
        assert!(live.exists(), "expected {} to exist", live.display());
        assert!(!dir.path().join("elph..jsonl").exists());
    }
}
