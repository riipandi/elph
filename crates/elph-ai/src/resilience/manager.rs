//! Central resilience manager that coordinates rate limiting, circuit breaking,
//! and retry for all providers.
//!
//! The `ResilienceManager` holds per-provider rate limiters and circuit breakers,
//! lazily initializing them on first use. It is designed to be shared across
//! async tasks via `Arc<ResilienceManager>`.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::circuit_breaker::ProviderCircuitBreaker;
use super::config::ResilienceConfig;
use super::rate_limiter::ProviderRateLimiter;

/// Current epoch time in seconds.
fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Errors that can occur when checking resilience before a call.
#[derive(Debug, Clone)]
pub enum ResilienceError {
    /// Rate limited: too many requests, caller should wait.
    RateLimited,
    /// Circuit breaker open: provider is failing, call rejected.
    CircuitOpen,
}

impl std::fmt::Display for ResilienceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResilienceError::RateLimited => write!(f, "rate limited"),
            ResilienceError::CircuitOpen => write!(f, "circuit breaker is open"),
        }
    }
}

impl std::error::Error for ResilienceError {}

/// Per-provider state: a rate limiter and circuit breaker pair.
struct ProviderState {
    limiter: ProviderRateLimiter,
    breaker: ProviderCircuitBreaker,
    last_used: std::sync::atomic::AtomicU64,
}

/// Central manager for rate limiting and circuit breaking across all providers.
pub struct ResilienceManager {
    providers: RwLock<HashMap<String, Arc<ProviderState>>>,
    default_config: ResilienceConfig,
}

impl ResilienceManager {
    /// Create a new manager with a default config.
    pub fn new(default_config: ResilienceConfig) -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            default_config,
        }
    }

    /// Create a manager with sensible defaults for common providers.
    pub fn with_defaults() -> Self {
        Self::new(ResilienceConfig::default())
    }

    /// Get or initialize the state for a provider.
    fn get_or_init(&self, provider_id: &str) -> Arc<ProviderState> {
        // Fast path: read lock
        {
            let providers = self.providers.read();
            if let Some(state) = providers.get(provider_id) {
                return Arc::clone(state);
            }
        }

        // Slow path: write lock to insert
        let mut providers = self.providers.write();
        // Double-check after acquiring write lock
        if let Some(state) = providers.get(provider_id) {
            return Arc::clone(state);
        }

        let config = ResilienceConfig::from_env(provider_id)
            .with_rps(self.default_config.requests_per_second)
            .with_burst(self.default_config.burst_size)
            .with_failure_threshold(self.default_config.failure_threshold)
            .with_recovery_timeout(self.default_config.recovery_timeout)
            .with_max_retries(self.default_config.max_retries)
            .with_backoff(self.default_config.initial_backoff, self.default_config.max_backoff);

        let state = Arc::new(ProviderState {
            limiter: ProviderRateLimiter::new(&config),
            breaker: ProviderCircuitBreaker::new(&config),
            last_used: std::sync::atomic::AtomicU64::new(epoch_secs()),
        });

        providers.insert(provider_id.to_string(), Arc::clone(&state));
        state
    }

    /// Get the config for a provider (useful for retry backoff parameters).
    pub fn config_for(&self, provider_id: &str) -> ResilienceConfig {
        ResilienceConfig::from_env(provider_id)
            .with_rps(self.default_config.requests_per_second)
            .with_burst(self.default_config.burst_size)
            .with_failure_threshold(self.default_config.failure_threshold)
            .with_recovery_timeout(self.default_config.recovery_timeout)
            .with_max_retries(self.default_config.max_retries)
            .with_backoff(self.default_config.initial_backoff, self.default_config.max_backoff)
    }

    /// Check if a request to the given provider is allowed.
    ///
    /// Checks both rate limiter and circuit breaker. Returns `Ok(())` if allowed,
    /// or `Err(ResilienceError)` if blocked.
    pub fn check(&self, provider_id: &str) -> Result<(), ResilienceError> {
        let state = self.get_or_init(provider_id);

        // Update last_used timestamp
        state
            .last_used
            .store(epoch_secs(), std::sync::atomic::Ordering::Relaxed);

        // 1. Check rate limiter (non-blocking)
        if !state.limiter.check() {
            return Err(ResilienceError::RateLimited);
        }

        // 2. Check circuit breaker
        if !state.breaker.is_call_possible() {
            return Err(ResilienceError::CircuitOpen);
        }

        Ok(())
    }

    /// Wait until a request to the given provider is allowed (async).
    ///
    /// Waits for rate limiter token, then checks circuit breaker.
    pub async fn wait_until_ready(&self, provider_id: &str) -> Result<(), ResilienceError> {
        let state = self.get_or_init(provider_id);

        // Wait for rate limiter token
        state.limiter.until_ready().await;

        // Check circuit breaker
        if !state.breaker.is_call_possible() {
            return Err(ResilienceError::CircuitOpen);
        }

        Ok(())
    }

    /// Record a successful call to the given provider.
    pub fn record_success(&self, provider_id: &str) {
        let state = self.get_or_init(provider_id);
        state
            .last_used
            .store(epoch_secs(), std::sync::atomic::Ordering::Relaxed);
        state.breaker.on_success();
    }

    /// Record a failed call to the given provider.
    pub fn record_failure(&self, provider_id: &str) {
        let state = self.get_or_init(provider_id);
        state
            .last_used
            .store(epoch_secs(), std::sync::atomic::Ordering::Relaxed);
        state.breaker.on_error();
    }

    /// Cleanup provider state that hasn't been used for longer than `max_age`.
    pub fn cleanup_stale(&self, max_age: std::time::Duration) {
        let mut providers = self.providers.write();
        let now = epoch_secs();
        let max_age_secs = max_age.as_secs();
        providers.retain(|_, state| {
            let last = state.last_used.load(std::sync::atomic::Ordering::Relaxed);
            now.saturating_sub(last) < max_age_secs
        });
    }

    /// Check, execute, and record in one call.
    ///
    /// This is the high-level API for wrapping provider calls.
    pub fn check_and_execute<F, T, E>(&self, provider_id: &str, f: F) -> Result<T, ResilienceError>
    where
        F: FnOnce() -> Result<T, E>,
    {
        self.check(provider_id)?;

        match f() {
            Ok(val) => {
                self.record_success(provider_id);
                Ok(val)
            }
            Err(_) => {
                self.record_failure(provider_id);
                Err(ResilienceError::CircuitOpen)
            }
        }
    }

    /// Execute with automatic retry on retryable errors.
    ///
    /// Combines rate limiter check, circuit breaker, and retry logic.
    pub async fn execute_with_retry<F, Fut, T>(&self, provider_id: &str, f: F) -> Result<T, anyhow::Error>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        use super::retry::with_anyhow_retry;

        // Wait for rate limiter + check circuit breaker
        self.wait_until_ready(provider_id).await?;

        let config = self.config_for(provider_id);
        let provider_id = provider_id.to_string();
        let manager = Arc::new(self);

        with_anyhow_retry(
            || {
                let provider_id = provider_id.clone();
                let manager = manager.clone();
                let fut = f();
                async move {
                    // Re-check circuit breaker before each attempt
                    if !manager.get_or_init(&provider_id).breaker.is_call_possible() {
                        return Err(anyhow::anyhow!("circuit breaker is open"));
                    }

                    match fut.await {
                        Ok(val) => {
                            manager.record_success(&provider_id);
                            Ok(val)
                        }
                        Err(e) => {
                            manager.record_failure(&provider_id);
                            Err(e)
                        }
                    }
                }
            },
            &config,
        )
        .await
    }
}

impl Default for ResilienceManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_allows_first_request() {
        let manager = ResilienceManager::with_defaults();
        assert!(manager.check("anthropic").is_ok());
    }

    #[test]
    fn record_failure_trips_breaker() {
        let config = ResilienceConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let manager = ResilienceManager::new(config);

        let _ = manager.check("openai");
        manager.record_failure("openai");
        let _ = manager.check("openai");
        manager.record_failure("openai");

        // After 2 failures, should be open
        assert!(matches!(manager.check("openai"), Err(ResilienceError::CircuitOpen)));
    }

    #[test]
    fn different_providers_independent() {
        let config = ResilienceConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let manager = ResilienceManager::new(config);

        // Trip "openai"
        manager.record_failure("openai");
        assert!(manager.check("openai").is_err());

        // "anthropic" should still be fine
        assert!(manager.check("anthropic").is_ok());
    }

    #[test]
    fn config_for_returns_defaults() {
        let config = ResilienceConfig {
            max_retries: 5,
            ..Default::default()
        };
        let manager = ResilienceManager::new(config);
        let cfg = manager.config_for("anthropic");
        assert_eq!(cfg.max_retries, 5);
        assert_eq!(cfg.provider_id, "anthropic");
    }
}
