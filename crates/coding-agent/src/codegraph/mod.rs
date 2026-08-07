//! Codegraph host wiring: CLI, store open, agent tools, startup onboarding.

mod cmd;
mod onboard;
mod store;
pub mod tools;

pub use cmd::run;
pub use onboard::maybe_offer_index;
