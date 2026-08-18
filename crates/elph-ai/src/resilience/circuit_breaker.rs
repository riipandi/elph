//! Per-provider circuit breaker.
//!
//! Consecutive failures trip the circuit open so a failing provider is not
//! hammered. After `recovery_timeout` a single probe is allowed (half-open);
//! success closes the circuit, failure re-opens it.

use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

use super::config::ResilienceConfig;

/// A thread-safe circuit breaker for a single provider.
#[derive(Clone)]
pub struct ProviderCircuitBreaker {
    inner: Arc<Mutex<BreakerState>>,
}

struct BreakerState {
    failure_threshold: u32,
    recovery_timeout: std::time::Duration,
    phase: Phase,
}

enum Phase {
    Closed { failures: u32 },
    Open { until: Instant },
    HalfOpen,
}

impl ProviderCircuitBreaker {
    /// Create a new circuit breaker from a config.
    pub fn new(config: &ResilienceConfig) -> Self {
        Self::with_params(config.failure_threshold, config.recovery_timeout)
    }

    /// Create a circuit breaker with explicit parameters.
    pub fn with_params(failure_threshold: u32, recovery_timeout: std::time::Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BreakerState {
                failure_threshold,
                recovery_timeout,
                phase: Phase::Closed { failures: 0 },
            })),
        }
    }

    /// Check if a call is allowed.
    ///
    /// Returns `true` if the circuit is closed or half-open (probe allowed).
    /// Returns `false` if the circuit is open (fail fast).
    pub fn is_call_possible(&self) -> bool {
        let mut state = self.inner.lock();
        match state.phase {
            Phase::Closed { .. } | Phase::HalfOpen => true,
            Phase::Open { until } if Instant::now() >= until => {
                state.phase = Phase::HalfOpen;
                true
            }
            Phase::Open { .. } => false,
        }
    }

    /// Record a successful call.
    ///
    /// If the circuit was half-open, this closes it.
    pub fn on_success(&self) {
        let mut state = self.inner.lock();
        state.phase = Phase::Closed { failures: 0 };
    }

    /// Record a failed call.
    ///
    /// If failures exceed the threshold, the circuit opens.
    pub fn on_error(&self) {
        let mut state = self.inner.lock();
        let until = Instant::now() + state.recovery_timeout;
        match state.phase {
            Phase::HalfOpen => {
                state.phase = Phase::Open { until };
            }
            Phase::Closed { failures } => {
                let next = failures.saturating_add(1);
                if next >= state.failure_threshold {
                    state.phase = Phase::Open { until };
                } else {
                    state.phase = Phase::Closed { failures: next };
                }
            }
            Phase::Open { .. } => {}
        }
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
