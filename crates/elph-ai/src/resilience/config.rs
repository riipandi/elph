//! Per-provider resilience configuration.
//!
//! Each LLM provider can have different rate limits and error thresholds.
//! Configuration can be loaded from environment variables or set programmatically.

use std::time::Duration;

/// Per-provider resilience configuration.
///
/// Controls rate limiting, circuit breaking, and retry behavior for a specific
/// LLM provider (e.g., "anthropic", "openai").
#[derive(Debug, Clone)]
pub struct ResilienceConfig {
    /// Provider identifier (e.g., "anthropic", "openai").
    pub provider_id: String,

    /// Rate limiter: requests per second allowed.
    pub requests_per_second: u64,

    /// Rate limiter: burst size (max consecutive requests without waiting).
    pub burst_size: u32,

    /// Circuit breaker: number of consecutive failures before tripping open.
    pub failure_threshold: u32,

    /// Circuit breaker: how long to wait before trying again (half-open).
    pub recovery_timeout: Duration,

    /// Retry: maximum number of retry attempts.
    pub max_retries: u32,

    /// Retry: initial backoff delay.
    pub initial_backoff: Duration,

    /// Retry: maximum backoff delay.
    pub max_backoff: Duration,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            requests_per_second: 10,
            burst_size: 5,
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            max_retries: 5, // Increased from 3 to 5 for better handling of transient transport errors
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
        }
    }
}

impl ResilienceConfig {
    /// Create a config for a specific provider with sensible defaults.
    pub fn for_provider(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            ..Default::default()
        }
    }

    /// Builder-style: set requests per second.
    pub fn with_rps(mut self, rps: u64) -> Self {
        self.requests_per_second = rps;
        self
    }

    /// Builder-style: set burst size.
    pub fn with_burst(mut self, burst: u32) -> Self {
        self.burst_size = burst;
        self
    }

    /// Builder-style: set failure threshold for circuit breaker.
    pub fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Builder-style: set recovery timeout.
    pub fn with_recovery_timeout(mut self, timeout: Duration) -> Self {
        self.recovery_timeout = timeout;
        self
    }

    /// Builder-style: set max retries.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Builder-style: set backoff parameters.
    pub fn with_backoff(mut self, initial: Duration, max: Duration) -> Self {
        self.initial_backoff = initial;
        self.max_backoff = max;
        self
    }

    /// Load configuration from `{prefix}_RATE_LIMIT_*` using the process identity prefix
    /// ([`crate::client_identity`], default `ELPH`).
    pub fn from_env(provider_id: impl Into<String>) -> Self {
        Self::from_env_prefixed(provider_id, &crate::types::client_identity().env_prefix)
    }

    /// Load configuration using `{prefix}_RATE_LIMIT_*` / `{prefix}_CIRCUIT_BREAKER_*`.
    pub fn from_env_prefixed(provider_id: impl Into<String>, prefix: &str) -> Self {
        let provider = provider_id.into();
        let upper = provider.to_uppercase().replace('-', "_");

        let requests_per_second = std::env::var(format!("{prefix}_RATE_LIMIT_{upper}_RPS"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let burst_size = std::env::var(format!("{prefix}_RATE_LIMIT_{upper}_BURST"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let failure_threshold = std::env::var(format!("{prefix}_CIRCUIT_BREAKER_{upper}_THRESHOLD"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let recovery_timeout_ms = std::env::var(format!("{prefix}_CIRCUIT_BREAKER_{upper}_TIMEOUT_MS"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30_000);

        let max_retries = std::env::var(format!("{prefix}_MAX_RETRIES"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let max_backoff_ms = std::env::var(format!("{prefix}_MAX_RETRY_DELAY_MS"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30_000);

        Self {
            provider_id: provider,
            requests_per_second,
            burst_size,
            failure_threshold,
            recovery_timeout: Duration::from_millis(recovery_timeout_ms),
            max_retries,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_millis(max_backoff_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sane_values() {
        let cfg = ResilienceConfig::default();
        assert_eq!(cfg.requests_per_second, 10);
        assert_eq!(cfg.burst_size, 5);
        assert_eq!(cfg.failure_threshold, 5);
        assert_eq!(cfg.recovery_timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_retries, 5); // Updated to 5
    }

    #[test]
    fn builder_sets_values() {
        let cfg = ResilienceConfig::for_provider("openai")
            .with_rps(20)
            .with_burst(10)
            .with_failure_threshold(3)
            .with_recovery_timeout(Duration::from_secs(60))
            .with_max_retries(5)
            .with_backoff(Duration::from_millis(100), Duration::from_secs(10));

        assert_eq!(cfg.provider_id, "openai");
        assert_eq!(cfg.requests_per_second, 20);
        assert_eq!(cfg.burst_size, 10);
        assert_eq!(cfg.failure_threshold, 3);
        assert_eq!(cfg.recovery_timeout, Duration::from_secs(60));
        assert_eq!(cfg.max_retries, 5);
        assert_eq!(cfg.initial_backoff, Duration::from_millis(100));
        assert_eq!(cfg.max_backoff, Duration::from_secs(10));
    }

    #[test]
    fn for_provider_sets_id() {
        let cfg = ResilienceConfig::for_provider("anthropic");
        assert_eq!(cfg.provider_id, "anthropic");
    }
}
