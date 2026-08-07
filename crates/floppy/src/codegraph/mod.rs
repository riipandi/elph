//! Semantic codebase index (chunk + FTS + vector + thin graph).
//!
//! Gated behind the `codegraph` feature.

mod chunk;
mod graph;
mod index;
mod merkle;
pub mod migrations;
mod search;
mod store;
mod types;
mod walk;

pub use store::CodegraphStore;
pub use types::{
    ChunkHit, CodegraphConfig, CodegraphStatus, ImpactNode, IndexPhase, IndexProgress, ProgressFn, ScanStats,
    SearchOptions,
};
