use crate::logger::LoggingOptions;

/// No-op root span guard when the `tracing` feature is disabled.
pub struct RootSpanGuard;

/// Initialize tracing. No-op without the `tracing` feature.
pub fn init(_options: &LoggingOptions) {}

/// Start a root span. No-op without the `tracing` feature.
pub fn root_span(_name: &'static str) -> RootSpanGuard {
    RootSpanGuard
}
