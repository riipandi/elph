//! App-agnostic agent runtime primitives shared by Elph applications.

pub mod builder;
pub mod datastore;
pub mod init;
pub mod migration;
pub mod runtime;
pub mod utils;

pub use builder::{AgentBuilder, AgentInit, LogRotation, LoggingOptions};
pub use datastore::{DatabaseSpec, ensure_database, ensure_databases, ensure_databases_once};
pub use init::{InitProgress, ensure_dirs, write_file_if_missing, write_json_file, write_private_file};
pub use migration::Migration;
pub use runtime::{block_on, try_block_on};
