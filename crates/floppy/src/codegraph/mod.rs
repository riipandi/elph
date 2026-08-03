//! Semantic codebase index (chunk + FTS + vector + thin graph).
//!
//! Gated behind the `codegraph` feature.

mod chunk;
mod graph;
mod index;
mod merkle;
mod migrations;
mod search;
mod store;
mod types;

pub use store::CodegraphStore;
pub use types::{ChunkHit, CodegraphConfig, CodegraphStatus, ImpactNode, ScanStats, SearchOptions};
