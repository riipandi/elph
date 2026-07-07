pub mod adapter;
pub mod builtin;
pub mod cloudflare_auth;
pub mod faux;

pub use crate::models::catalog::{get_builtin_model, get_builtin_models, get_builtin_providers};
pub use builtin::{
    amazon_bedrock_provider, anthropic_provider, builtin_models, builtin_providers, cloudflare_ai_gateway_provider,
    cloudflare_workers_ai_provider, google_vertex_provider, openai_provider,
};
pub use faux::faux_provider;
