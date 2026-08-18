/// No-op root span guard when the `tracing` feature is disabled.
pub struct RootSpanGuard;

/// Runtime tracing is never active without the `tracing` feature.
pub fn is_enabled() -> bool {
    false
}

/// No-op enabled flag without the `tracing` feature.
pub fn set_enabled(_enabled: bool) {}

/// Initialize tracing. No-op without the `tracing` feature.
pub fn init(_logs_dir: &std::path::Path, _app_name: &str, _enabled: bool) {}

/// Flush pending spans. No-op without the `tracing` feature.
pub fn flush() {}

/// Start a root span. No-op without the `tracing` feature.
pub fn root_span(_name: &'static str) -> RootSpanGuard {
    RootSpanGuard
}
