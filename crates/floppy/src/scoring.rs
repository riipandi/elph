use super::types::{MemoryCategory, TaskBaseline, UserInputSource};

/// Welford's online algorithm — update running mean/variance. Returns new baseline (no mutation).
pub fn update_baseline(baseline: &TaskBaseline, tokens: f64, errors: f64, user_corrections: f64) -> TaskBaseline {
    let n = baseline.count + 1;
    let d_tokens = tokens - baseline.mean_tokens;
    let d_errors = errors - baseline.mean_errors;
    let d_user_corr = user_corrections - baseline.mean_user_corrections;

    let mean_tokens = baseline.mean_tokens + d_tokens / n as f64;
    let mean_errors = baseline.mean_errors + d_errors / n as f64;
    let mean_user_corrections = baseline.mean_user_corrections + d_user_corr / n as f64;

    TaskBaseline {
        count: n,
        mean_tokens,
        mean_errors,
        mean_user_corrections,
        m2_tokens: baseline.m2_tokens + d_tokens * (tokens - mean_tokens),
        m2_errors: baseline.m2_errors + d_errors * (errors - mean_errors),
        m2_user_corrections: baseline.m2_user_corrections + d_user_corr * (user_corrections - mean_user_corrections),
    }
}

fn stddev(m2: f64, count: u32) -> f64 {
    if count < 2 {
        return 1.0; // avoid div-by-zero; z-score = raw delta
    }
    let v = (m2 / (count - 1) as f64).sqrt();
    if v == 0.0 { 1.0 } else { v }
}

/// Composite task score vs running baseline (z-score based). Positive = better than avg.
/// Cold start (<10 tasks): simple normalized deltas instead of z-scores.
pub fn compute_task_score(
    baseline: &TaskBaseline,
    tokens: f64,
    errors: f64,
    user_corrections: f64,
    completed: bool,
) -> f64 {
    let completed_signal = if completed { 1.0 } else { -1.0 };

    if baseline.count < 10 {
        let token_delta = if baseline.count > 0 {
            (baseline.mean_tokens - tokens) / baseline.mean_tokens.max(1.0)
        } else {
            0.0
        };
        let error_delta = if baseline.count > 0 {
            (baseline.mean_errors - errors) / baseline.mean_errors.max(1.0)
        } else {
            0.0
        };
        return token_delta + error_delta - user_corrections * 0.5 + completed_signal;
    }

    let stddev_tokens = stddev(baseline.m2_tokens, baseline.count);
    let stddev_errors = stddev(baseline.m2_errors, baseline.count);
    let stddev_user_corr = stddev(baseline.m2_user_corrections, baseline.count);

    let z_tokens = (tokens - baseline.mean_tokens) / stddev_tokens;
    let z_errors = (errors - baseline.mean_errors) / stddev_errors;
    let z_user_corr = (user_corrections - baseline.mean_user_corrections) / stddev_user_corr;

    -z_tokens - z_errors - z_user_corr + completed_signal
}

/// Credit for a single memory given task outcome + self-report.
pub fn compute_credit(task_score: f64, self_report_score: f64, num_memories_retrieved: u32) -> f64 {
    task_score * (self_report_score / 3.0) * (1.0 / num_memories_retrieved.max(1) as f64)
}

/// EMA weight update, clamped to [0.1, 5.0].
pub fn update_weight(old_weight: f64, credit: f64, learning_rate: f64) -> f64 {
    let raw = (1.0 - learning_rate) * old_weight + learning_rate * credit;
    raw.clamp(0.1, 5.0)
}

/// Initial weight for a new memory based on category/source.
pub fn initial_weight(
    category: MemoryCategory,
    source: Option<UserInputSource>,
    tokens_wasted: Option<f64>,
    avg_tokens_per_task: Option<f64>,
) -> f64 {
    match category {
        MemoryCategory::Correction => {
            let avg = avg_tokens_per_task.unwrap_or(10_000.0);
            let cost = tokens_wasted.unwrap_or(0.0);
            1.0 + (cost / avg)
        }
        MemoryCategory::User => match source {
            Some(UserInputSource::UserDenial) => 2.0,
            Some(UserInputSource::UserCorrection) => 2.5,
            Some(UserInputSource::UserInput) => 2.0,
            None => 2.0,
        },
        MemoryCategory::Insight => 1.0,
        MemoryCategory::Consolidated => 1.0, // caller sets avg of sources
        MemoryCategory::Discovery => 1.0,
    }
}

/// Fresh empty baseline for z-score tracking.
pub fn empty_baseline() -> TaskBaseline {
    TaskBaseline {
        count: 0,
        mean_tokens: 0.0,
        mean_errors: 0.0,
        mean_user_corrections: 0.0,
        m2_tokens: 0.0,
        m2_errors: 0.0,
        m2_user_corrections: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_baseline_starts_at_zero() {
        let b = empty_baseline();
        assert_eq!(b.count, 0);
        assert_eq!(b.mean_tokens, 0.0);
        assert_eq!(b.m2_tokens, 0.0);
    }

    #[test]
    fn update_baseline_tracks_running_mean() {
        let b0 = empty_baseline();
        let b1 = update_baseline(&b0, 1000.0, 2.0, 1.0);
        assert_eq!(b1.count, 1);
        assert_eq!(b1.mean_tokens, 1000.0);
        assert_eq!(b1.mean_errors, 2.0);
        assert_eq!(b1.mean_user_corrections, 1.0);

        let b2 = update_baseline(&b1, 2000.0, 4.0, 0.0);
        assert_eq!(b2.count, 2);
        assert!((b2.mean_tokens - 1500.0).abs() < f64::EPSILON);
        assert!((b2.mean_errors - 3.0).abs() < f64::EPSILON);
        assert!((b2.mean_user_corrections - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn cold_start_score_rewards_completion_and_lower_usage() {
        let baseline = update_baseline(&empty_baseline(), 1000.0, 2.0, 1.0);
        let good = compute_task_score(&baseline, 500.0, 0.0, 0.0, true);
        let bad = compute_task_score(&baseline, 2000.0, 5.0, 2.0, false);
        assert!(good > bad);
        assert!(good > 0.0);
        assert!(bad < 0.0);
    }

    #[test]
    fn first_task_cold_start_uses_completion_only() {
        let baseline = empty_baseline();
        let completed = compute_task_score(&baseline, 500.0, 0.0, 0.0, true);
        let failed = compute_task_score(&baseline, 500.0, 0.0, 0.0, false);
        assert_eq!(completed, 1.0);
        assert_eq!(failed, -1.0);
    }

    #[test]
    fn z_score_mode_after_ten_tasks() {
        let mut baseline = empty_baseline();
        for i in 0..10 {
            baseline = update_baseline(&baseline, 1000.0 + i as f64 * 10.0, 1.0, 0.0);
        }
        let better = compute_task_score(&baseline, 800.0, 0.0, 0.0, true);
        let worse = compute_task_score(&baseline, 1500.0, 5.0, 2.0, false);
        assert!(better > worse);
    }

    #[test]
    fn compute_credit_scales_by_self_report_and_retrieval_count() {
        let full = compute_credit(2.0, 3.0, 3);
        let partial = compute_credit(2.0, 1.0, 3);
        let solo = compute_credit(2.0, 3.0, 1);
        assert!((full - 2.0 / 3.0).abs() < f64::EPSILON);
        assert!((partial - 2.0 / 9.0).abs() < f64::EPSILON);
        assert!((solo - 2.0).abs() < f64::EPSILON);
        assert_eq!(compute_credit(1.0, 0.0, 5), 0.0);
    }

    #[test]
    fn update_weight_clamps_to_bounds() {
        assert_eq!(update_weight(1.0, 10.0, 0.5), 5.0);
        assert_eq!(update_weight(1.0, -10.0, 0.5), 0.1);
        let mid = update_weight(2.0, 1.5, 0.1);
        assert!((mid - 1.95).abs() < 1e-9);
    }

    #[test]
    fn initial_weight_correction_scales_with_tokens_wasted() {
        let w = initial_weight(MemoryCategory::Correction, None, Some(5000.0), Some(10_000.0));
        assert!((w - 1.5).abs() < f64::EPSILON);
        let w_default = initial_weight(MemoryCategory::Correction, None, None, None);
        assert!((w_default - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn initial_weight_user_sources() {
        assert_eq!(
            initial_weight(MemoryCategory::User, Some(UserInputSource::UserDenial), None, None),
            2.0
        );
        assert_eq!(
            initial_weight(MemoryCategory::User, Some(UserInputSource::UserCorrection), None, None),
            2.5
        );
        assert_eq!(
            initial_weight(MemoryCategory::User, Some(UserInputSource::UserInput), None, None),
            2.0
        );
        assert_eq!(initial_weight(MemoryCategory::User, None, None, None), 2.0);
    }

    #[test]
    fn initial_weight_other_categories_default_to_one() {
        assert_eq!(initial_weight(MemoryCategory::Insight, None, None, None), 1.0);
        assert_eq!(initial_weight(MemoryCategory::Consolidated, None, None, None), 1.0);
        assert_eq!(initial_weight(MemoryCategory::Discovery, None, None, None), 1.0);
    }

    #[test]
    fn task_baseline_serializes_with_camel_case_for_ts_compat() {
        let baseline = TaskBaseline {
            count: 3,
            mean_tokens: 1200.0,
            mean_errors: 1.5,
            mean_user_corrections: 0.5,
            m2_tokens: 90.0,
            m2_errors: 2.0,
            m2_user_corrections: 0.25,
        };
        let json = serde_json::to_string(&baseline).expect("serialize");
        assert!(json.contains("meanTokens"));
        assert!(json.contains("m2UserCorrections"));

        let parsed: TaskBaseline = serde_json::from_str(
            r#"{"count":3,"meanTokens":1200,"meanErrors":1.5,"meanUserCorrections":0.5,"m2Tokens":90,"m2Errors":2,"m2UserCorrections":0.25}"#,
        )
        .expect("deserialize");
        assert_eq!(parsed.count, baseline.count);
        assert_eq!(parsed.mean_tokens, baseline.mean_tokens);
    }
}
