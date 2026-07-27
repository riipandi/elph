//! Circuit breaker wrapper around the `failsafe` crate.
//!
//! Provides per-provider circuit breaking to stop hammering a failing provider
//! and allow it time to recover.
//!
//! The `failsafe::StateMachine` has complex generic parameters that are hard to
//! name. This wrapper stores the state machine operations as closures, providing
//! a clean, object-safe interface.

use std::sync::Arc;

use super::config::ResilienceConfig;

/// A thread-safe circuit breaker for a single provider.
///
/// Wraps `failsafe::StateMachine` operations as closures to avoid
/// naming the complex concrete generic type.
#[derive(Clone)]
pub struct ProviderCircuitBreaker {
    inner: Arc<CircuitBreakerInner>,
}

/// Internal state machine operations, stored as closures.
struct CircuitBreakerInner {
    is_call_possible: Box<dyn Fn() -> bool + Send + Sync>,
    on_success: Box<dyn Fn() + Send + Sync>,
    on_error: Box<dyn Fn() + Send + Sync>,
}

impl ProviderCircuitBreaker {
    /// Create a new circuit breaker from a config.
    pub fn new(config: &ResilienceConfig) -> Self {
        Self::with_params(config.failure_threshold, config.recovery_timeout)
    }

    /// Create a circuit breaker with explicit parameters.
    pub fn with_params(failure_threshold: u32, recovery_timeout: std::time::Duration) -> Self {
        use failsafe::{Config, backoff, failure_policy};

        // Exponential backoff: starts at 1s, grows to recovery_timeout
        let bo = backoff::exponential(std::time::Duration::from_secs(1), recovery_timeout);

        // Trip after N consecutive failures
        let policy = failure_policy::consecutive_failures(failure_threshold, bo);

        let sm = Config::new().failure_policy(policy).build();

        // Wrap in closures to avoid naming the concrete StateMachine type
        let sm_success = sm.clone();
        let sm_error = sm.clone();

        Self {
            inner: Arc::new(CircuitBreakerInner {
                is_call_possible: Box::new(move || sm.is_call_permitted()),
                on_success: Box::new(move || sm_success.on_success()),
                on_error: Box::new(move || sm_error.on_error()),
            }),
        }
    }

    /// Check if a call is allowed.
    ///
    /// Returns `true` if the circuit is closed or half-open (probe allowed).
    /// Returns `false` if the circuit is open (fail fast).
    pub fn is_call_possible(&self) -> bool {
        (self.inner.is_call_possible)()
    }

    /// Record a successful call.
    ///
    /// If the circuit was half-open, this closes it.
    pub fn on_success(&self) {
        (self.inner.on_success)()
    }

    /// Record a failed call.
    ///
    /// If failures exceed the threshold, the circuit opens.
    pub fn on_error(&self) {
        (self.inner.on_error)()
    }

    /// Execute a closure with circuit breaker protection.
    ///
    /// Returns `Err(CircuitBreakerOpen)` if the circuit is open.
    /// On success, records success. On error, records failure and returns the error.
    pub fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if !self.is_call_possible() {
            return Err(CircuitBreakerError::Open);
        }

        match f() {
            Ok(val) => {
                self.on_success();
                Ok(val)
            }
            Err(e) => {
                self.on_error();
                Err(CircuitBreakerError::Inner(e))
            }
        }
    }

    /// Execute an async closure with circuit breaker protection.
    pub async fn call_async<F, Fut, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if !self.is_call_possible() {
            return Err(CircuitBreakerError::Open);
        }

        match f().await {
            Ok(val) => {
                self.on_success();
                Ok(val)
            }
            Err(e) => {
                self.on_error();
                Err(CircuitBreakerError::Inner(e))
            }
        }
    }
}

/// Error type for circuit breaker protected calls.
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// The circuit is open — call was rejected (fail fast).
    Open,
    /// The call was made but returned an error.
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerError::Open => write!(f, "circuit breaker is open"),
            CircuitBreakerError::Inner(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CircuitBreakerError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CircuitBreakerError::Open => None,
            CircuitBreakerError::Inner(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_closed() {
        let cb = ProviderCircuitBreaker::with_params(3, std::time::Duration::from_secs(10));
        assert!(cb.is_call_possible());
    }

    #[test]
    fn call_records_success() {
        let cb = ProviderCircuitBreaker::with_params(3, std::time::Duration::from_secs(10));
        let result = cb.call(|| Ok::<_, String>("ok"));
        assert!(result.is_ok());
        assert!(cb.is_call_possible());
    }

    #[test]
    fn call_records_failure() {
        let cb = ProviderCircuitBreaker::with_params(3, std::time::Duration::from_secs(10));
        let _ = cb.call(|| Err::<(), _>("fail"));
        // Still closed because threshold not reached
        assert!(cb.is_call_possible());
    }

    #[test]
    fn trips_after_threshold() {
        let cb = ProviderCircuitBreaker::with_params(2, std::time::Duration::from_secs(60));
        let _ = cb.call(|| Err::<(), _>("fail1"));
        let _ = cb.call(|| Err::<(), _>("fail2"));
        // After 2 failures, should be open
        assert!(!cb.is_call_possible());
    }

    #[test]
    fn open_returns_rejected() {
        let cb = ProviderCircuitBreaker::with_params(1, std::time::Duration::from_secs(60));
        let _ = cb.call(|| Err::<(), _>("fail"));
        // Circuit should be open now
        let result = cb.call(|| Ok::<_, String>("ok"));
        assert!(matches!(result, Err(CircuitBreakerError::Open)));
    }

    #[test]
    fn success_resets_failures() {
        let cb = ProviderCircuitBreaker::with_params(3, std::time::Duration::from_secs(60));
        let _ = cb.call(|| Err::<(), _>("fail1"));
        let _ = cb.call(|| Err::<(), _>("fail2"));
        // One success resets the counter
        let _ = cb.call(|| Ok::<_, String>("ok"));
        // Two more failures won't trip (counter reset)
        let _ = cb.call(|| Err::<(), _>("fail3"));
        let _ = cb.call(|| Err::<(), _>("fail4"));
        assert!(cb.is_call_possible());
    }
}
