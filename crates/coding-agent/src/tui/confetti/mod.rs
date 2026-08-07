//! Confetti and fireworks easter egg (`/confetti`).

mod array;
mod fireworks;
mod overlay;
mod physics;
mod rain;
mod simulation;

pub use overlay::{
    ConfettiOverlay, ConfettiRuntime, OpenConfettiArgs, PendingConfetti, close_confetti, confetti_mode_from_slash_args,
    open_confetti,
};
pub use simulation::ConfettiMode;
