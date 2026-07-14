use anyhow::Result;

use super::store::MemoryStore;
use super::types::{
    ContradictResult, EndTaskWithDecayResult, MemoryCategory, MemoryReportInput, MemoryReportType,
    ReportCorrectionInput, ReportUserInput, TaskEndInput,
};

impl MemoryStore {
    /// Unified memory report (correction, user input, or insight).
    pub async fn report(&self, input: MemoryReportInput) -> Result<String> {
        match input.report_type {
            MemoryReportType::Correction => {
                let what_failed = input.what_failed.unwrap_or_default();
                let what_worked = input.what_worked.unwrap_or_default();
                self.report_correction(ReportCorrectionInput {
                    lesson: input.lesson,
                    what_failed,
                    what_worked,
                    tokens_wasted: input.tokens_wasted,
                    tools_wasted: input.tools_wasted,
                })
                .await
            }
            MemoryReportType::UserInput => {
                let source = input
                    .source
                    .ok_or_else(|| anyhow::anyhow!("user_input report requires `source`"))?;
                self.report_user_input(ReportUserInput {
                    lesson: input.lesson,
                    source,
                })
                .await
            }
            MemoryReportType::Insight => {
                self.insert_raw_memory(&input.lesson, MemoryCategory::Insight, 1.0)
                    .await
            }
        }
    }

    /// End task and run weight decay.
    pub async fn end_task_with_decay(&self, task_id: &str, input: TaskEndInput) -> Result<EndTaskWithDecayResult> {
        self.end_task(task_id, input).await?;
        let decay = self.decay().await?;
        Ok(EndTaskWithDecayResult { decay })
    }

    /// Flag a memory as wrong and optionally store a correction.
    pub async fn contradict(&self, memory_id: &str, correction: Option<&str>) -> Result<ContradictResult> {
        let (deleted, correction_id) = self.contradict_memory(memory_id, correction).await?;
        Ok(ContradictResult { deleted, correction_id })
    }
}

impl MemoryReportInput {
    pub fn correction(
        lesson: impl Into<String>,
        what_failed: impl Into<String>,
        what_worked: impl Into<String>,
    ) -> Self {
        Self {
            report_type: MemoryReportType::Correction,
            lesson: lesson.into(),
            what_failed: Some(what_failed.into()),
            what_worked: Some(what_worked.into()),
            tokens_wasted: None,
            tools_wasted: None,
            source: None,
        }
    }

    pub fn user_input(lesson: impl Into<String>, source: super::types::UserInputSource) -> Self {
        Self {
            report_type: MemoryReportType::UserInput,
            lesson: lesson.into(),
            what_failed: None,
            what_worked: None,
            tokens_wasted: None,
            tools_wasted: None,
            source: Some(source),
        }
    }

    pub fn insight(lesson: impl Into<String>) -> Self {
        Self {
            report_type: MemoryReportType::Insight,
            lesson: lesson.into(),
            what_failed: None,
            what_worked: None,
            tokens_wasted: None,
            tools_wasted: None,
            source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_input_builds_correctly() {
        let input = MemoryReportInput::correction("learned something", "query failed", "bash worked");
        assert_eq!(input.lesson, "learned something");
        assert_eq!(input.what_failed, Some("query failed".into()));
        assert_eq!(input.what_worked, Some("bash worked".into()));
    }

    #[test]
    fn user_input_builds_correctly() {
        let input = MemoryReportInput::user_input("hello", crate::floppy::types::UserInputSource::UserInput);
        assert_eq!(input.lesson, "hello");
    }
}
