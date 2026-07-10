//! Per-turn goal token and wall-clock accounting.

use elph_ai::Usage;
use std::time::Instant;

/// Codex-style token delta: non-cached input + output.
pub fn goal_token_delta(usage: &Usage) -> i64 {
    let input = usage.input.saturating_sub(usage.cache_read);
    let output = usage.output;
    (input + output) as i64
}

#[derive(Debug, Clone, Default)]
pub struct TurnBaseline {
    pub tokens_used: i64,
    pub wall_clock_ms: i64,
    pub started_at: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalAccountingState {
    pub turn_baseline: Option<TurnBaseline>,
}

impl GoalAccountingState {
    pub fn start_turn(&mut self, tokens_used: i64, wall_clock_ms: i64) {
        self.turn_baseline = Some(TurnBaseline {
            tokens_used,
            wall_clock_ms,
            started_at: Some(Instant::now()),
        });
    }

    pub fn finish_turn(&self, usage: Option<&Usage>) -> (i64, i64, i64) {
        let baseline = self.turn_baseline.as_ref();
        let token_delta = usage.map(goal_token_delta).unwrap_or(0);
        let turn_delta = 1i64;
        let wall_delta = baseline
            .and_then(|b| b.started_at)
            .map(|start| start.elapsed().as_millis() as i64)
            .unwrap_or(0);
        (token_delta, turn_delta, wall_delta)
    }
}
