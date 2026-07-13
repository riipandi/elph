//! Optional fastrace helpers for the agent runtime.

#[cfg(feature = "tracing")]
mod imp;

#[cfg(not(feature = "tracing"))]
mod stub;

#[cfg(feature = "tracing")]
pub use imp::*;

#[cfg(not(feature = "tracing"))]
pub use stub::*;
