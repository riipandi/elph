//! Shared elph-ai [`Models`] instance with Owly credential store and auth context.

use std::sync::{Arc, OnceLock};

use anyhow::Result;
use elph_ai::{CreateModelsOptions, Models, builtin_models};

use crate::credentials::{OwlyAuthContext, OwlyCredentialStore};

static SHARED_MODELS: OnceLock<Arc<Models>> = OnceLock::new();
static SHARED_CREDENTIALS: OnceLock<Arc<OwlyCredentialStore>> = OnceLock::new();

fn init() -> Result<(Arc<OwlyCredentialStore>, Arc<Models>)> {
    let credentials = OwlyCredentialStore::load_default()?;
    let models = builtin_models(Some(CreateModelsOptions {
        credentials: Some(credentials.clone()),
        auth_context: Some(Arc::new(OwlyAuthContext)),
    }))
    .into_arc();
    Ok((credentials, models))
}

/// Owly credential store (OAuth tokens), loaded once per process.
pub fn credential_store() -> Arc<OwlyCredentialStore> {
    SHARED_CREDENTIALS
        .get_or_init(|| init().expect("initialize Owly credential store").0)
        .clone()
}

/// Shared elph-ai models registry with Owly auth wiring.
pub fn shared_models() -> Arc<Models> {
    SHARED_MODELS
        .get_or_init(|| init().expect("initialize Owly shared models").1)
        .clone()
}
