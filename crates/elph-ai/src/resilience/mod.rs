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
pub mod metrics;
pub mod rate_limiter;
pub mod retry;

pub use circuit_breaker::{CircuitBreakerError, ProviderCircuitBreaker};
pub use config::ResilienceConfig;
pub use manager::{ResilienceError, ResilienceManager};
pub use metrics::{MetricsSnapshot, ResilienceMetrics};
pub use rate_limiter::ProviderRateLimiter;

// ---------------------------------------------------------------------------
// Global convenience functions (backed by a static ResilienceManager)
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

/// Global resilience manager — initialized once, then immutable.
///
/// Use `init_global_manager()` to set custom config at startup.
/// If not initialized, defaults are used on first access.
static GLOBAL_MANAGER: OnceLock<ResilienceManager> = OnceLock::new();

/// Initialize the global resilience manager with custom defaults.
///
/// Call this once at app startup (before any provider calls).
/// If called multiple times, only the first call takes effect.
pub fn init_global_manager(manager: ResilienceManager) {
    log::info!("resilience: initializing global manager");
    let _ = GLOBAL_MANAGER.set(manager);
}

/// Get the global resilience manager, initializing with defaults if needed.
fn global_manager() -> &'static ResilienceManager {
    GLOBAL_MANAGER.get_or_init(ResilienceManager::with_defaults)
}

/// Check rate limiter and circuit breaker before sending a request.
pub fn check_provider_resilience(provider_id: &str) -> Result<(), ResilienceError> {
    match global_manager().check(provider_id) {
        Ok(()) => Ok(()),
        Err(ResilienceError::RateLimited) => {
            log::warn!("resilience: rate limited — {provider_id}");
            Err(ResilienceError::RateLimited)
        }
        Err(ResilienceError::CircuitOpen) => {
            log::warn!("resilience: circuit breaker open — {provider_id}");
            Err(ResilienceError::CircuitOpen)
        }
    }
}

/// Record a successful call to a provider.
pub fn record_provider_success(provider_id: &str) {
    global_manager().record_success(provider_id);
}

/// Record a failed call to a provider.
pub fn record_provider_failure(provider_id: &str) {
    log::debug!("resilience: recording failure — {provider_id}");
    global_manager().record_failure(provider_id);
}

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
