# Changelog

All notable changes to this project will be documented in this file.
This project adheres to [Semantic Versioning](http://semver.org/).

## 0.0.28

### Library contract

- Crate root is an explicit prelude (types, `Models`, faux, images, validation, estimate). OAuth helpers live under `elph_ai::auth`. `anyhow::Result` is no longer re-exported.
- `ModelsError.source` is `Option<Box<dyn Error + Send + Sync>>` (no public `anyhow` cause).
- `CreateModelsOptions.identity` / `ClientIdentity` drive Codex originator, xAI referrer, and `{PREFIX}_*` env keys.
- Cargo features: `bedrock`, `oauth-callback`, `generate-models`, `tracing`. Default is HTTP chat only.
- Published package includes `src/`, `models/*.json`, README, and CHANGELOG (not the generator binary or examples).
- Consumer contract: [`docs/elph-ai.md`](../../docs/elph-ai.md).
