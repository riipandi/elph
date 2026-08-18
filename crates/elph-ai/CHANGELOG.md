# Changelog

All notable changes to this project will be documented in this file.
This project adheres to [Semantic Versioning](http://semver.org/).

## 0.0.28

### Library contract

- Crate root is an explicit prelude (types, `Models`, faux, images, validation, estimate). OAuth helpers live under `elph_ai::auth`. `anyhow::Result` is no longer re-exported.
- `ModelsError.source` is `Option<Box<dyn Error + Send + Sync>>` (no public `anyhow` cause).
- `CreateModelsOptions.identity` / `set_client_identity` drive Codex originator, xAI referrer, `{PREFIX}_CACHE_RETENTION`, `{PREFIX}_GITHUB_HOST`, and resilience `{PREFIX}_RATE_LIMIT_*` / `{PREFIX}_CIRCUIT_BREAKER_*` / `{PREFIX}_MAX_RETRIES`.
- Public OAuth helpers (`oauth_provider_login`, `refresh_oauth_token`, `get_oauth_api_key`, `oauth_provider_to_auth`) return `Result<_, ModelsError>`.
- Package metadata: `documentation = "https://docs.rs/elph-ai"`; docs.rs `all-features`.
- Cargo features: `bedrock`, `oauth-callback`, `generate-models`, `tracing`. Default is HTTP chat only.
- Published package includes `src/`, `models/*.json`, README, and CHANGELOG (not the generator binary or examples).
- Consumer contract: [`docs/elph-ai.md`](../../docs/elph-ai.md).
