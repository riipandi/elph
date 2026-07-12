//! Agent integration using elph-agent and elph-ai.

mod commands;
mod listeners;
mod model;
mod run;
mod shared_models;
mod tools;

pub use commands::{prepare_chat_command, prepare_init_command, prepare_update_command};
pub use run::{RunAgentOptions, RunAgentResult, run_agent};
pub use shared_models::{credential_store, shared_models};
