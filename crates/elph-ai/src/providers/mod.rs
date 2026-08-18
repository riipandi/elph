pub mod adapter;
pub mod builtin;
pub mod cloudflare_auth;
pub mod faux;

pub use crate::models::{get_builtin_model, get_builtin_models, get_builtin_providers};
#[cfg(feature = "bedrock")]
pub use builtin::amazon_bedrock_provider;
pub use builtin::{anthropic_provider, builtin_models, builtin_providers};
pub use builtin::{cloudflare_ai_gateway_provider, cloudflare_workers_ai_provider, google_vertex_provider};
pub use builtin::{
    hyper_provider, mistral_provider, neuralwatt_provider, nvidia_provider, openai_provider, sumopod_provider,
    wafer_provider, xai_provider,
};
pub use faux::faux_provider;
