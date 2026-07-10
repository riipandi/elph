//! Context compaction — elph-agent module.

mod branch_summarization;
mod compact;
mod estimation;
mod preparation;
mod summarization;
mod types;
mod utils;

pub use crate::harness::types::FileOperations;
pub use crate::harness::types::{CompactionPreparation, CompactionSettings, DEFAULT_COMPACTION_SETTINGS};
pub use branch_summarization::{
    BranchPreparation, BranchSummaryDetails, CollectEntriesResult, GenerateBranchSummaryOptions,
    collect_entries_for_branch_summary, generate_branch_summary, prepare_branch_entries,
};
pub use compact::compact;
pub use estimation::{ContextUsageEstimate, CutPointResult};
pub use estimation::{
    calculate_context_tokens, estimate_context_tokens, estimate_tokens, find_cut_point, find_turn_start_index,
    get_last_assistant_usage, should_compact,
};
pub use preparation::prepare_compaction;
pub use summarization::generate_summary;
pub use types::{CompactionDetails, CompactionResult, SUMMARIZATION_SYSTEM_PROMPT};
pub use utils::{
    compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations, serialize_conversation,
};
