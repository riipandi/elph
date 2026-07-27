//! Resilience metrics counters for observability.
//!
//! Tracks rate limiting, circuit breaker events, and request outcomes.
//! Use `snapshot()` to get a point-in-time view of all counters.

use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics counter for resilience events.
///
/// Thread-safe and lock-free using atomic operations.
#[derive(Debug, Default)]
pub struct ResilienceMetrics {
    /// Total rate-limited requests.
    pub rate_limited_total: AtomicU64,
    /// Total circuit breaker open rejections.
    pub circuit_open_total: AtomicU64,
    /// Total successful requests.
    pub request_success_total: AtomicU64,
    /// Total failed requests (recorded to circuit breaker).
    pub request_failure_total: AtomicU64,
}

/// Point-in-time snapshot of all metrics counters.
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub rate_limited: u64,
    pub circuit_open: u64,
    pub success: u64,
    pub failure: u64,
}

impl ResilienceMetrics {
    /// Create a new metrics instance with all counters at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a rate-limited request.
    pub fn record_rate_limited(&self) {
        self.rate_limited_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a circuit breaker open rejection.
    pub fn record_circuit_open(&self) {
        self.circuit_open_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a successful request.
    pub fn record_success(&self) {
        self.request_success_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed request.
    pub fn record_failure(&self) {
        self.request_failure_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a point-in-time snapshot of all counters.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            rate_limited: self.rate_limited_total.load(Ordering::Relaxed),
            circuit_open: self.circuit_open_total.load(Ordering::Relaxed),
            success: self.request_success_total.load(Ordering::Relaxed),
            failure: self.request_failure_total.load(Ordering::Relaxed),
        }
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.rate_limited_total.store(0, Ordering::Relaxed);
        self.circuit_open_total.store(0, Ordering::Relaxed);
        self.request_success_total.store(0, Ordering::Relaxed);
        self.request_failure_total.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_metrics_all_zero() {
        let m = ResilienceMetrics::new();
        let s = m.snapshot();
        assert_eq!(s.rate_limited, 0);
        assert_eq!(s.circuit_open, 0);
        assert_eq!(s.success, 0);
        assert_eq!(s.failure, 0);
    }

    #[test]
    fn record_increments_counters() {
        let m = ResilienceMetrics::new();
        m.record_rate_limited();
        m.record_rate_limited();
        m.record_circuit_open();
        m.record_success();
        m.record_failure();
        m.record_failure();
        m.record_failure();

        let s = m.snapshot();
        assert_eq!(s.rate_limited, 2);
        assert_eq!(s.circuit_open, 1);
        assert_eq!(s.success, 1);
        assert_eq!(s.failure, 3);
    }

    #[test]
    fn reset_clears_counters() {
        let m = ResilienceMetrics::new();
        m.record_success();
        m.record_failure();
        m.reset();

        let s = m.snapshot();
        assert_eq!(s.success, 0);
        assert_eq!(s.failure, 0);
    }

    #[test]
    fn snapshot_is_point_in_time() {
        let m = ResilienceMetrics::new();
        m.record_success();
        let s1 = m.snapshot();
        m.record_success();
        let s2 = m.snapshot();

        assert_eq!(s1.success, 1);
        assert_eq!(s2.success, 2);
    }
}
