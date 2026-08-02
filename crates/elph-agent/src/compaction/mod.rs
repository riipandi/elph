//! Context compaction — elph-agent module.

mod branch_summarization;
mod compact;
mod estimation;
mod preparation;
mod summarization;
mod types;
mod utils;

pub use crate::agent::harness::types::DEFAULT_COMPACTION_SETTINGS;
pub use crate::agent::harness::types::FileOperations;
pub use crate::agent::harness::types::{CompactionPreparation, CompactionSettings};
pub use crate::prompt::builtin::compaction::SUMMARIZATION_SYSTEM_PROMPT;
pub use branch_summarization::BranchPreparation;
pub use branch_summarization::BranchSummaryDetails;
pub use branch_summarization::CollectEntriesResult;
pub use branch_summarization::GenerateBranchSummaryOptions;
pub use branch_summarization::{collect_entries_for_branch_summary, generate_branch_summary, prepare_branch_entries};
pub use compact::compact;
pub use estimation::calculate_context_tokens;
pub use estimation::estimate_context_tokens;
pub use estimation::estimate_tokens;
pub use estimation::estimate_tokens_with_system_prompt;
pub use estimation::find_cut_point;
pub use estimation::find_turn_start_index;
pub use estimation::get_last_assistant_usage;
pub use estimation::should_compact;
pub use estimation::{ContextUsageEstimate, CutPointResult};
pub use preparation::prepare_compaction;
pub use summarization::generate_summary;
pub use types::{CompactionDetails, CompactionResult};
pub use utils::compute_file_lists;
pub use utils::create_file_ops;
pub use utils::extract_file_ops_from_message;
pub use utils::format_file_operations;
pub use utils::serialize_conversation;
