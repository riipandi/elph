#[path = "reporter.rs"]
mod reporter;

use std::time::Duration;

pub use fastrace::collector::SpanContext;
pub use fastrace::prelude::{LocalSpan, Span};
pub use fastrace::{flush, set_reporter};
pub use fastrace_reqwest::traceparent_headers;
pub use reporter::JsonlReporter;

use crate::logger::LoggingOptions;

/// Holds a root span and its local parent guard for the current task.
pub struct RootSpanGuard {
    _span: Span,
    _local: fastrace::local::LocalParentGuard,
}

/// Initialize the global fastrace reporter. No-op when tracing is disabled.
pub fn init(options: &LoggingOptions) {
    if cfg!(test) || !options.trace_enabled {
        return;
    }

    let reporter = match JsonlReporter::new(&options.logs_dir, options.app_name) {
        Ok(reporter) => reporter,
        Err(error) => {
            log::warn!("failed to initialize trace reporter: {error}");
            return;
        }
    };

    set_reporter(
        reporter,
        fastrace::collector::Config::default().report_interval(Duration::from_secs(1)),
    );
}

/// Start a new root span and install it as the local parent for the current task.
pub fn root_span(name: &'static str) -> RootSpanGuard {
    let span = Span::root(name, SpanContext::random());
    let guard = span.set_local_parent();
    RootSpanGuard {
        _span: span,
        _local: guard,
    }
}
