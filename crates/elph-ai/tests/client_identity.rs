use elph_ai::resilience::ResilienceConfig;
use elph_ai::{ClientIdentity, CreateModelsOptions, create_models};

#[test]
fn create_models_stores_identity_on_collection_only() {
    let models = create_models(Some(CreateModelsOptions {
        identity: Some(ClientIdentity::new("acme", "ACME")),
        ..Default::default()
    }));
    let id = models.identity();
    assert_eq!(id.product, "acme");
    assert_eq!(id.env_prefix, "ACME");
    assert_eq!(id.env_key("CACHE_RETENTION"), "ACME_CACHE_RETENTION");
    assert_eq!(id.env_key("GITHUB_HOST"), "ACME_GITHUB_HOST");

    let other = create_models(Some(CreateModelsOptions {
        identity: Some(ClientIdentity::new("beta", "BETA")),
        ..Default::default()
    }));
    assert_eq!(models.identity().env_prefix, "ACME");
    assert_eq!(other.identity().env_prefix, "BETA");
}

#[test]
fn resilience_prefix_is_explicit_not_process_global() {
    unsafe {
        std::env::set_var("OTHER_RATE_LIMIT_DEMO_RPS", "42");
    }
    let config = ResilienceConfig::from_env_prefixed("demo", "OTHER");
    unsafe {
        std::env::remove_var("OTHER_RATE_LIMIT_DEMO_RPS");
    }
    assert_eq!(config.requests_per_second, 42);
}
