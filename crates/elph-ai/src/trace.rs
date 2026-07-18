//! Optional fastrace helpers for provider HTTP and streaming.

#[cfg(feature = "tracing")]
mod imp;

#[cfg(not(feature = "tracing"))]
mod stub;

#[cfg(feature = "tracing")]
pub use imp::*;

#[cfg(not(feature = "tracing"))]
pub use stub::*;
