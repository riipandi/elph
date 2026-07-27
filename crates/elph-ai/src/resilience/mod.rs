//! Resilience primitives for provider API calls.
//!
//! Provides rate limiting, circuit breaking, and retry with exponential backoff
//! for outbound HTTP calls to LLM providers.
//!
//! # Architecture
//!
//! Three layers of resilience, applied from outermost to innermost:
//!
//! 1. **Rate limiter** (`governor`): Token bucket per provider. Prevents hitting
//!    provider RPM/TPM limits. Non-blocking check; if denied, the caller waits.
//!
//! 2. **Circuit breaker** (`failsafe`): Trips open after N consecutive failures.
//!    Gives a failing provider time to recover. Half-open probes test recovery.
//!
//! 3. **Retry** (`backon`): Exponential backoff with jitter for transient errors.
//!    Automatically retries 5xx, 429, timeout, and connection errors.
//!
//! # Usage
//!
//! ```rust,no_run
//! use elph_ai::resilience::{ResilienceManager, ResilienceConfig};
//!
//! let manager = ResilienceManager::new(
//!     ResilienceConfig::for_provider("anthropic")
//!         .with_rps(10)
//!         .with_burst(5)
//! );
//!
//! // Check before a call
//! manager.check("anthropic").unwrap();
//!
//! // Record result
//! manager.record_success("anthropic");
//! ```

pub mod circuit_breaker;
pub mod config;
pub mod manager;
pub mod rate_limiter;
pub mod retry;

pub use circuit_breaker::{CircuitBreakerError, ProviderCircuitBreaker};
pub use config::ResilienceConfig;
pub use manager::{ResilienceError, ResilienceManager};
pub use rate_limiter::ProviderRateLimiter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn full_lifecycle() {
        let manager = ResilienceManager::new(
            ResilienceConfig::for_provider("test-provider")
                .with_rps(100)
                .with_burst(10)
                .with_failure_threshold(3)
                .with_recovery_timeout(Duration::from_secs(60))
                .with_max_retries(2),
        );

        // First request should be allowed
        assert!(manager.check("test-provider").is_ok());

        // Success resets failure count
        manager.record_success("test-provider");
        assert!(manager.check("test-provider").is_ok());

        // Two failures
        manager.record_failure("test-provider");
        manager.record_failure("test-provider");
        assert!(manager.check("test-provider").is_ok());

        // Third failure trips the breaker
        manager.record_failure("test-provider");
        assert!(matches!(manager.check("test-provider"), Err(ResilienceError::CircuitOpen)));
    }
}
