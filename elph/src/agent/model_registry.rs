//! Model and auth resolution.

use anyhow::{Context, Result};
use elph_ai::{Model, Models, get_builtin_model};
use std::sync::Arc;

use super::provider::resolve_provider_and_model;
use crate::platform::Settings;

#[derive(Clone)]
pub struct ModelSelection {
    pub provider: String,
    pub model_id: String,
    pub model: Model,
    pub models: Arc<Models>,
    pub display_name: String,
}

pub async fn resolve_model(
    settings: &Settings,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<ModelSelection> {
    let (provider, model_id) = resolve_provider_and_model(
        provider_override,
        model_override,
        settings.session.provider_id.as_deref(),
        settings.session.model_id.as_deref(),
    )?;

    let model = get_builtin_model(&provider, &model_id)
        .or_else(|| {
            if model_id.contains('/') {
                let parts: Vec<&str> = model_id.splitn(2, '/').collect();
                if parts.len() == 2 {
                    return get_builtin_model(parts[0], parts[1]);
                }
            }
            None
        })
        .with_context(|| format!("Model not found: {provider}/{model_id}"))?;

    let models = elph_ai::builtin_models(None).into_arc();
    let _auth = models.get_auth(&model).await?;

    let display_name = model.name.clone();
    Ok(ModelSelection {
        provider,
        model_id: model.id.clone(),
        model,
        models,
        display_name,
    })
}
