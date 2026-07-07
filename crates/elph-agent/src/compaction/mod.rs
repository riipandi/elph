//! Context compaction — elph-agent module.

#![allow(clippy::module_inception)]

mod branch_summarization;
mod compaction;
mod utils;

pub use crate::harness::types::FileOperations;
pub use crate::harness::types::{CompactionPreparation, CompactionSettings, DEFAULT_COMPACTION_SETTINGS};
pub use branch_summarization::{
    BranchPreparation, BranchSummaryDetails, CollectEntriesResult, GenerateBranchSummaryOptions,
    collect_entries_for_branch_summary, generate_branch_summary, prepare_branch_entries,
};
pub use compaction::{
    CompactionDetails, CompactionResult, ContextUsageEstimate, CutPointResult, SUMMARIZATION_SYSTEM_PROMPT,
    calculate_context_tokens, compact, estimate_context_tokens, estimate_tokens, find_cut_point, find_turn_start_index,
    generate_summary, get_last_assistant_usage, prepare_compaction, should_compact,
};
pub use utils::{
    compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations, serialize_conversation,
};
