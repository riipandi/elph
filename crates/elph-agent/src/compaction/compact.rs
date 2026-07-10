//! Compaction execution.

use elph_ai::{Model, Models, ThinkingLevel};
use tokio_util::sync::CancellationToken;

use crate::compaction::utils::{compute_file_lists, format_file_operations};
use crate::harness::types::{CompactionError, CompactionErrorCode};

pub use crate::harness::types::CompactionPreparation;

use super::summarization::{generate_summary, generate_turn_prefix_summary};
use super::types::{CompactionDetails, CompactionResult};

/// Generate compaction summary data from prepared session history.
pub async fn compact(
    preparation: CompactionPreparation,
    models: &Models,
    model: &Model,
    custom_instructions: Option<&str>,
    signal: Option<CancellationToken>,
    thinking_level: Option<ThinkingLevel>,
) -> std::result::Result<CompactionResult, CompactionError> {
    let CompactionPreparation {
        first_kept_entry_id,
        messages_to_summarize,
        turn_prefix_messages,
        is_split_turn,
        tokens_before,
        previous_summary,
        file_ops,
        settings,
    } = preparation;

    if first_kept_entry_id.is_empty() {
        return Err(CompactionError::new(
            CompactionErrorCode::InvalidSession,
            "First kept entry has no TSID - session may need migration",
        ));
    }

    let summary = if is_split_turn && !turn_prefix_messages.is_empty() {
        let history_result = if messages_to_summarize.is_empty() {
            Ok("No prior history.".to_string())
        } else {
            generate_summary(
                &messages_to_summarize,
                models,
                model,
                settings.reserve_tokens,
                signal.clone(),
                custom_instructions,
                previous_summary.as_deref(),
                thinking_level,
            )
            .await
        };
        let history = match history_result {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let turn_prefix = generate_turn_prefix_summary(
            &turn_prefix_messages,
            models,
            model,
            settings.reserve_tokens,
            signal,
            thinking_level,
        )
        .await;
        let turn_prefix = match turn_prefix {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        format!("{history}\n\n---\n\n**Turn Context (split turn):**\n\n{turn_prefix}")
    } else {
        match generate_summary(
            &messages_to_summarize,
            models,
            model,
            settings.reserve_tokens,
            signal,
            custom_instructions,
            previous_summary.as_deref(),
            thinking_level,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => return Err(error),
        }
    };

    let (read_files, modified_files) = compute_file_lists(&file_ops);
    let mut summary_with_files = summary;
    summary_with_files.push_str(&format_file_operations(&read_files, &modified_files));

    Ok(CompactionResult {
        summary: summary_with_files,
        first_kept_entry_id,
        tokens_before,
        details: Some(CompactionDetails {
            read_files,
            modified_files,
        }),
    })
}
