use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fastrace::collector::SpanContext;
use fastrace::future::FutureExt;
use fastrace::prelude::Span;
use reqwest::RequestBuilder;

use crate::types::Model;

pub use fastrace::prelude::LocalSpan;
pub use fastrace::{flush as fastrace_flush, set_reporter as fastrace_set_reporter};
pub use fastrace_reqwest::traceparent_headers;

use super::reporter::JsonlReporter;

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether runtime tracing is active.
pub fn is_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Holds a root span and its local parent guard for the current task.
pub struct RootSpanGuard {
    #[allow(dead_code)]
    inner: Option<(Span, fastrace::local::LocalParentGuard)>,
}

/// Initialize the global fastrace reporter. No-op when tracing is disabled.
pub fn init(logs_dir: &Path, app_name: &str, enabled: bool) {
    let enabled = !cfg!(test) && enabled;
    TRACING_ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
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

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    if !is_enabled() {
        return request;
    }
    request.headers(traceparent_headers())
}

pub fn model_stream_span(model: &Model) -> Span {
    let span = Span::root("elph.ai.stream", SpanContext::random());
    span.add_property(|| ("model.id", model.id.clone()));
    span.add_property(|| ("model.provider", model.provider.clone()));
    span.add_property(|| ("model.api", model.api.clone()));
    span
}

pub fn spawn_stream<F>(model: &Model, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    if !is_enabled() {
        return tokio::spawn(fut);
    }
    tokio::spawn(fut.in_span(model_stream_span(model)))
}
