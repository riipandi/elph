use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub use super::reporter::JsonlReporter;
pub use fastrace::collector::SpanContext;
pub use fastrace::prelude::{LocalSpan, Span};
pub use fastrace::{flush as fastrace_flush, set_reporter as fastrace_set_reporter};
pub use fastrace_reqwest::traceparent_headers;

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether runtime tracing is active (`{PREFIX}_TRACE` and successful init).
pub fn is_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Holds a root span and its local parent guard for the current task.
pub struct RootSpanGuard {
    #[allow(dead_code)]
    inner: Option<(Span, fastrace::local::LocalParentGuard)>,
}

/// Set the process-wide enabled flag without installing a reporter.
///
/// The `elph` binary installs the reporter from `elph-agent` and only needs
/// this flag so stream/HTTP helpers attach spans.
pub fn set_enabled(enabled: bool) {
    TRACING_ENABLED.store(enabled && !cfg!(test), Ordering::Relaxed);
}

/// Initialize the global fastrace reporter. No-op when tracing is disabled.
pub fn init(logs_dir: &std::path::Path, app_name: &str, enabled: bool) {
    set_enabled(enabled);
    if !is_enabled() {
        return;
    }

    let reporter = match JsonlReporter::new(logs_dir, app_name) {
        Ok(reporter) => reporter,
        Err(error) => {
            TRACING_ENABLED.store(false, Ordering::Relaxed);
            log::warn!("failed to initialize trace reporter: {error}");
            return;
        }
    };

    set_reporter(
        reporter,
        fastrace::collector::Config::default().report_interval(Duration::from_secs(1)),
    );
}

/// Install a custom fastrace reporter (tests and advanced embeds).
pub fn set_reporter(reporter: JsonlReporter, config: fastrace::collector::Config) {
    TRACING_ENABLED.store(true, Ordering::Relaxed);
    fastrace_set_reporter(reporter, config);
}

/// Flush pending spans. No-op when tracing is disabled.
pub fn flush() {
    if is_enabled() {
        fastrace_flush();
        super::reporter::flush_writer();
    }
}

/// Start a new root span and install it as the local parent for the current task.
pub fn root_span(name: &'static str) -> RootSpanGuard {
    if !is_enabled() {
        return RootSpanGuard { inner: None };
    }

    let span = Span::root(name, SpanContext::random());
    let guard = span.set_local_parent();
    RootSpanGuard {
        inner: Some((span, guard)),
    }
}
