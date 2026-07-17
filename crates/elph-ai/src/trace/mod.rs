//! Optional fastrace helpers for provider HTTP and streaming.

#[cfg(feature = "tracing")]
mod imp;

#[cfg(feature = "tracing")]
mod reporter;

#[cfg(not(feature = "tracing"))]
mod stub;

#[cfg(feature = "tracing")]
pub use imp::*;

#[cfg(feature = "tracing")]
pub use reporter::JsonlReporter;

#[cfg(not(feature = "tracing"))]
pub use stub::*;
