use anyhow::Result;

use crate::session::SessionStore;

use super::help::history_limit;
use super::output::ShellWriter;

pub(super) async fn write_checkpoint_history(
    session: &SessionStore,
    input: &str,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    let limit = history_limit(input);
    let summaries = session.list_checkpoint_history(limit).await?;
    writer.blank();
    if summaries.is_empty() {
        writer.line("No checkpoints for this thread.");
    } else {
        writer.line(format!("Checkpoints (newest first, showing up to {limit}):"));
        for (index, summary) in summaries.iter().enumerate() {
            let short_id = summary.checkpoint_id.get(..8).unwrap_or(summary.checkpoint_id.as_str());
            writer.line(format!(
                "  #{} step={} source={} id={}… ({} message(s))",
                index + 1,
                summary.step,
                summary.source,
                short_id,
                summary.message_count
            ));
        }
        writer.line("Use /restore <#> or /restore <id-prefix> to rewind.");
    }
    writer.blank();
    Ok(())
}

pub(super) async fn write_checkpoint_restore(
    session: &mut SessionStore,
    input: &str,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    let arg = input
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: /restore <#|checkpoint_id>"))?;
    let checkpoint_id = session.resolve_checkpoint_id(arg).await?;
    let restored = session.restore_checkpoint(&checkpoint_id).await?;
    writer.blank();
    writer.line(format!(
        "Restored {restored} message(s) from checkpoint {}…",
        checkpoint_id.get(..8).unwrap_or(checkpoint_id.as_str())
    ));
    writer.line("Next turn will fork from this checkpoint.");
    writer.blank();
    Ok(())
}
