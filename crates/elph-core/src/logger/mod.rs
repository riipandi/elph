mod options;

pub use options::{LogRotation, LoggingOptions};

use std::num::NonZeroUsize;
use std::sync::Arc;

use logforth::Filter;
use logforth::append;
use logforth::append::asynchronous::AsyncBuilder;
use logforth::append::file::FileBuilder;
use logforth::bridge::log::LogBridge;
use logforth::filter::RustLogFilter;
use logforth::layout::JsonLayout;
use logforth::layout::TextLayout;

/// Bounded queue for the async file writer. Caps memory under sustained log bursts.
const FILE_WRITER_BUFFER_LINES: usize = 16_384;

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
/// Returns a [`LogGuard`] that must be kept alive for the process lifetime so
/// async appenders can flush buffered records.
pub fn init(options: LoggingOptions) -> Option<LogGuard> {
    if cfg!(test) {
        return None;
    }

    let trace_enabled = options.trace_enabled;
    crate::trace::init(&options);
    install_logger(&options, trace_enabled)
}

fn level_filter(level: &str) -> Box<dyn Filter> {
    Box::new(RustLogFilter::from(level))
}

fn parse_max_level(level: &str) -> log::LevelFilter {
    match level {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => log::LevelFilter::Info,
    }
}

fn file_appender(options: &LoggingOptions) -> append::Async {
    let mut builder = FileBuilder::new(&options.logs_dir, options.app_name)
        .layout(JsonLayout::default())
        .filename_suffix(".jsonl");

    builder = match options.rotation {
        LogRotation::Hourly => builder.rollover_hourly(),
        LogRotation::Daily | LogRotation::Weekly => builder.rollover_daily(),
    };

    if let Some(max_files) = options.max_files.and_then(NonZeroUsize::new) {
        builder = builder.max_log_files(max_files);
    }

    let file = builder.build().expect("failed to initialize rolling log writer");

    AsyncBuilder::new(format!("{}-log-writer", options.app_name))
        .overflow_drop_incoming()
        .buffered_lines_limit(Some(FILE_WRITER_BUFFER_LINES))
        .append(file)
        .build()
}

#[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
fn install_logger(options: &LoggingOptions, trace_enabled: bool) -> Option<LogGuard> {
    let filter = level_filter(&options.level);
    let mut starter = logforth::starter_log::builder();

    if options.file_enabled {
        let file = file_appender(options);
        let file_filter = level_filter(&options.level);
        starter = starter.dispatch(|d| d.filter(file_filter).append(file));
    }

    if options.console_enabled {
        let stdout = append::Stdout::default().with_layout(TextLayout::default());
        let console_filter = level_filter(&options.level);
        starter = starter.dispatch(|d| d.filter(console_filter).append(stdout));
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
    log::set_boxed_logger(Box::new(bridge.clone())).expect("failed to set global logger");
    log::set_max_level(parse_max_level(&options.level));

    Some(LogGuard { bridge })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_max_level_valid() {
        assert_eq!(parse_max_level("trace"), log::LevelFilter::Trace);
        assert_eq!(parse_max_level("debug"), log::LevelFilter::Debug);
        assert_eq!(parse_max_level("info"), log::LevelFilter::Info);
        assert_eq!(parse_max_level("warn"), log::LevelFilter::Warn);
        assert_eq!(parse_max_level("error"), log::LevelFilter::Error);
    }

    #[test]
    fn parse_max_level_defaults_to_info() {
        assert_eq!(parse_max_level("unknown"), log::LevelFilter::Info);
        assert_eq!(parse_max_level(""), log::LevelFilter::Info);
    }

    #[test]
    fn log_guard_flushes_on_drop() {
        let bridge = Arc::new(LogBridge::new(logforth::starter_log::builder().build()));
        let guard = LogGuard { bridge: bridge.clone() };
        drop(guard);
        // No panic = flush succeeded
    }

    #[test]
    fn init_returns_none_in_test_mode() {
        let options = LoggingOptions {
            app_name: "test",
            logs_dir: PathBuf::from("/tmp"),
            level: "info".to_string(),
            rotation: LogRotation::Daily,
            max_files: None,
            file_enabled: true,
            console_enabled: true,
            trace_enabled: true,
        };
        assert!(init(options).is_none());
    }
}
