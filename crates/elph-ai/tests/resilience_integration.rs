//! Integration tests for the resilience module.

use elph_ai::resilience::{ResilienceConfig, ResilienceError, ResilienceManager};
use std::time::Duration;

#[tokio::test]
async fn test_circuit_breaker_trips_after_failures() {
    let manager = ResilienceManager::new(ResilienceConfig::for_provider("test").with_failure_threshold(2));

    // Two failures should trip the breaker
    manager.record_failure("test");
    manager.record_failure("test");

    assert!(matches!(manager.check("test"), Err(ResilienceError::CircuitOpen)));
}

#[tokio::test]
async fn test_success_resets_failure_count() {
    let manager = ResilienceManager::new(ResilienceConfig::for_provider("test").with_failure_threshold(3));

    manager.record_failure("test");
    manager.record_failure("test");
    manager.record_success("test"); // Reset

    // Two more failures won't trip (counter reset)
    manager.record_failure("test");
    manager.record_failure("test");
    assert!(manager.check("test").is_ok());
}

#[tokio::test]
async fn test_different_providers_independent() {
    let manager = ResilienceManager::new(ResilienceConfig::for_provider("test").with_failure_threshold(1));

    manager.record_failure("provider-a");
    assert!(manager.check("provider-a").is_err());
    assert!(manager.check("provider-b").is_ok()); // Independent
}

#[tokio::test]
async fn test_rate_limiter_enforces_limit() {
    let config = ResilienceConfig::for_provider("test").with_rps(1).with_burst(1);
    let manager = ResilienceManager::new(config);

    // First request allowed
    assert!(manager.check("test").is_ok());

    // Second request rate limited (burst exhausted)
    assert!(matches!(manager.check("test"), Err(ResilienceError::RateLimited)));
}

#[tokio::test]
async fn test_cleanup_stale_removes_old_entries() {
    let manager = ResilienceManager::new(ResilienceConfig::for_provider("test").with_failure_threshold(5));

    // Create some provider states
    let _ = manager.check("provider-a");
    let _ = manager.check("provider-b");

    // Cleanup with 0 duration should remove all
    manager.cleanup_stale(Duration::ZERO);

    // After cleanup, providers should be re-initialized
    assert!(manager.check("provider-a").is_ok());
    assert!(manager.check("provider-b").is_ok());
}

#[tokio::test]
async fn test_cleanup_stale_keeps_recent() {
    let manager = ResilienceManager::new(ResilienceConfig::for_provider("test").with_failure_threshold(5));

    // Create a provider state
    let _ = manager.check("provider-a");

    // Cleanup with large duration should keep it
    manager.cleanup_stale(Duration::from_secs(3600));

    // Provider should still exist (no re-init needed)
    assert!(manager.check("provider-a").is_ok());
}

#[tokio::test]
async fn test_config_from_env_defaults() {
    elph_ai::set_client_identity(elph_ai::ClientIdentity::default());
    // Clear any existing env vars
    unsafe { std::env::remove_var("ELPH_RATE_LIMIT_TEST_RPS") };
    unsafe { std::env::remove_var("ELPH_CIRCUIT_BREAKER_TEST_THRESHOLD") };

    let config = ResilienceConfig::from_env("test");
    assert_eq!(config.requests_per_second, 10);
    assert_eq!(config.failure_threshold, 5);
    assert_eq!(config.max_retries, 5);
}

#[tokio::test]
async fn test_config_from_env_overrides() {
    elph_ai::set_client_identity(elph_ai::ClientIdentity::default());
    unsafe { std::env::set_var("ELPH_RATE_LIMIT_MYPROVIDER_RPS", "20") };
    unsafe { std::env::set_var("ELPH_CIRCUIT_BREAKER_MYPROVIDER_THRESHOLD", "10") };

    let config = ResilienceConfig::from_env("myprovider");
    assert_eq!(config.requests_per_second, 20);
    assert_eq!(config.failure_threshold, 10);

    // Cleanup
    unsafe { std::env::remove_var("ELPH_RATE_LIMIT_MYPROVIDER_RPS") };
    unsafe { std::env::remove_var("ELPH_CIRCUIT_BREAKER_MYPROVIDER_THRESHOLD") };
}

#[test]
fn test_metrics_counters() {
    let m = elph_ai::resilience::ResilienceMetrics::new();

    m.record_rate_limited();
    m.record_rate_limited();
    m.record_circuit_open();
    m.record_success();
    m.record_failure();
    m.record_failure();

    let s = m.snapshot();
    assert_eq!(s.rate_limited, 2);
    assert_eq!(s.circuit_open, 1);
    assert_eq!(s.success, 1);
    assert_eq!(s.failure, 2);
}

#[test]
fn test_metrics_reset() {
    let m = elph_ai::resilience::ResilienceMetrics::new();
    m.record_success();
    m.record_failure();
    m.reset();

    let s = m.snapshot();
    assert_eq!(s.success, 0);
    assert_eq!(s.failure, 0);
}
