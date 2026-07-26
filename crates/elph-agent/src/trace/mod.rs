//! Optional fastrace helpers for the agent runtime.

#[cfg(feature = "tracing")]
mod core_imp;
#[cfg(feature = "tracing")]
mod reporter;

#[cfg(not(feature = "tracing"))]
mod core_stub;

#[cfg(feature = "tracing")]
mod imp;

#[cfg(not(feature = "tracing"))]
mod stub;

#[cfg(feature = "tracing")]
pub use core_imp::*;

#[cfg(not(feature = "tracing"))]
pub use core_stub::*;

#[cfg(feature = "tracing")]
pub use imp::{model_stream_span, spawn_stream, with_trace_headers};

#[cfg(not(feature = "tracing"))]
pub use stub::{model_stream_span, spawn_stream, with_trace_headers};
