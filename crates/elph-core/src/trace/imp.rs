#[path = "reporter.rs"]
mod reporter;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub use fastrace::collector::SpanContext;
pub use fastrace::prelude::{LocalSpan, Span};
pub use fastrace::{flush as fastrace_flush, set_reporter as fastrace_set_reporter};
pub use fastrace_reqwest::traceparent_headers;
pub use reporter::JsonlReporter;

use crate::logger::LoggingOptions;

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

/// Initialize the global fastrace reporter. No-op when tracing is disabled.
pub fn init(options: &LoggingOptions) {
    let enabled = !cfg!(test) && options.trace_enabled;
    TRACING_ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        return;
    }

    let reporter = match JsonlReporter::new(&options.logs_dir, options.app_name) {
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
