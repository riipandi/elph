//! Retry with exponential backoff using `backon`.
//!
//! Wraps `backon::ExponentialBuilder` for convenient retry of provider API calls
//! with configurable retry counts and backoff delays.

use backon::{ExponentialBuilder, Retryable};

use super::config::ResilienceConfig;

#[cfg(test)]
use std::time::Duration;

/// Build a backon exponential backoff from a resilience config.
pub fn backoff_from_config(config: &ResilienceConfig) -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_max_times(config.max_retries as usize)
        .with_min_delay(config.initial_backoff)
        .with_max_delay(config.max_backoff)
        .with_jitter()
}

/// Execute a fallible async closure with retry and exponential backoff.
///
/// Retries on errors where `is_retryable` returns `true`.
/// Returns the first successful result, or the last error after all retries exhausted.
pub async fn with_retry<F, Fut, T, E>(
    f: F,
    is_retryable: impl Fn(&E) -> bool,
    config: &ResilienceConfig,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let backoff = backoff_from_config(config);

    let result = (|| f()).retry(backoff).when(is_retryable).await;

    result
}

/// Execute a fallible async closure with retry, accepting `anyhow::Error`.
///
/// Uses the project-wide `is_retryable` classification from `utils::retry`.
pub async fn with_anyhow_retry<F, Fut, T>(f: F, config: &ResilienceConfig) -> anyhow::Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let backoff = backoff_from_config(config);

    let result = (|| f())
        .retry(backoff)
        .when(|e: &anyhow::Error| is_anyhow_retryable(e))
        .await;

    result
}

/// Simple heuristic for retryable anyhow errors.
///
/// This duplicates some logic from `utils::retry::is_retryable` but works on
/// raw `anyhow::Error` instead of `AssistantMessage`.
pub fn is_anyhow_retryable(error: &anyhow::Error) -> bool {
    let msg = error.to_string().to_lowercase();

    // Non-retryable: billing/quota issues
    if msg.contains("quota exceeded")
        || msg.contains("insufficient_quota")
        || msg.contains("billing")
        || msg.contains("out of budget")
        || msg.contains("monthly usage limit")
        || msg.contains("free usage limit")
    {
        return false;
    }

    // Retryable: transient server errors
    msg.contains("overloaded")
        || msg.contains("rate limit")
        || msg.contains("too many requests")
        || msg.contains("429")
        || msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("524")
        || msg.contains("service unavailable")
        || msg.contains("server error")
        || msg.contains("internal error")
        || msg.contains("network error")
        || msg.contains("connection refused")
        || msg.contains("connection lost")
        || msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("socket hang up")
        || msg.contains("fetch failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_config_applies_settings() {
        let config = ResilienceConfig {
            max_retries: 5,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(10),
            ..Default::default()
        };
        let backoff = backoff_from_config(&config);
        // backon ExponentialBuilder stores these internally;
        // we just verify it builds without panicking
        let _ = backoff;
    }

    #[test]
    fn anyhow_retryable_classifies_errors() {
        assert!(is_anyhow_retryable(&anyhow::anyhow!("503 service unavailable")));
        assert!(is_anyhow_retryable(&anyhow::anyhow!("rate limit exceeded")));
        assert!(is_anyhow_retryable(&anyhow::anyhow!("overloaded")));
        assert!(is_anyhow_retryable(&anyhow::anyhow!("connection refused")));
        assert!(is_anyhow_retryable(&anyhow::anyhow!("timeout")));

        assert!(!is_anyhow_retryable(&anyhow::anyhow!("quota exceeded")));
        assert!(!is_anyhow_retryable(&anyhow::anyhow!("billing error")));
        assert!(!is_anyhow_retryable(&anyhow::anyhow!("invalid api key")));
    }

    #[tokio::test]
    async fn with_retry_succeeds_after_transient_failure() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let config = ResilienceConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(50),
            ..Default::default()
        };

        let result = with_retry(
            || {
                let attempts = attempts_clone.clone();
                async move {
                    let n = attempts.fetch_add(1, Ordering::SeqCst);
                    if n < 2 { Err("transient error") } else { Ok("success") }
                }
            },
            |e: &&str| *e == "transient error",
            &config,
        )
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn with_retry_exhausts_retries() {
        let config = ResilienceConfig {
            max_retries: 2,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(50),
            ..Default::default()
        };

        let result = with_retry(
            || async { Err::<(), _>("persistent error") },
            |e: &&str| *e == "persistent error",
            &config,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "persistent error");
    }
}
