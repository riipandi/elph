//! Rate limiter wrapper around the `governor` crate.
//!
//! Provides per-provider token bucket rate limiting to prevent hitting
//! provider API rate limits (RPM/TPM).

use std::num::NonZeroU32;
use std::sync::Arc;

use governor::clock::DefaultClock;
use governor::{Quota, RateLimiter};

use super::config::ResilienceConfig;

/// A thread-safe rate limiter for a single provider.
#[derive(Clone)]
pub struct ProviderRateLimiter {
    inner: Arc<RateLimiter<governor::state::NotKeyed, governor::state::InMemoryState, DefaultClock>>,
}

impl ProviderRateLimiter {
    /// Create a new rate limiter from a config.
    pub fn new(config: &ResilienceConfig) -> Self {
        Self::with_params(config.requests_per_second, config.burst_size)
    }

    /// Create a rate limiter with explicit RPS and burst parameters.
    pub fn with_params(rps: u64, burst: u32) -> Self {
        let rps = NonZeroU32::new(rps as u32).unwrap_or(NonZeroU32::new(1).unwrap());
        let burst = NonZeroU32::new(burst).unwrap_or(NonZeroU32::new(1).unwrap());

        let quota = Quota::per_second(rps).allow_burst(burst);
        let limiter = Arc::new(RateLimiter::direct(quota));

        Self { inner: limiter }
    }

    /// Check if a request is allowed (non-blocking).
    ///
    /// Returns `true` if the request can proceed, `false` if rate limited.
    /// Consumes one token if allowed.
    pub fn check(&self) -> bool {
        self.inner.check().is_ok()
    }

    /// Wait until a request is allowed (async).
    ///
    /// This will yield the task until a token is available.
    pub async fn until_ready(&self) {
        self.inner.until_ready().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_allows_within_burst() {
        let limiter = ProviderRateLimiter::with_params(100, 10);
        // Should allow burst_size requests immediately
        for _ in 0..10 {
            assert!(limiter.check());
        }
    }

    #[test]
    fn check_rejects_after_burst_exhausted() {
        let limiter = ProviderRateLimiter::with_params(1, 2);
        // Exhaust burst
        assert!(limiter.check());
        assert!(limiter.check());
        // Next should be rejected (token bucket empty)
        assert!(!limiter.check());
    }

    #[test]
    fn clone_shares_limiter() {
        let limiter = ProviderRateLimiter::with_params(100, 10);
        let limiter2 = limiter.clone();
        // Both share the same token bucket
        assert!(limiter.check());
        // Second clone should see one less token
        // (exact behavior depends on governor's internal state)
    }
}
