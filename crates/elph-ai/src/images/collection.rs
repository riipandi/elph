use std::collections::HashMap;
use std::sync::Arc;

use crate::api::OpenRouterImagesApi;
use crate::auth::{
    AuthContext, AuthModel, AuthResolutionOverrides, AuthResult, CredentialStore, InMemoryCredentialStore, ModelsError,
    ModelsErrorCode, ProviderAuth, ProviderAuthHolder, env_api_key_auth, resolve_provider_auth,
};
use crate::images::models::OPENROUTER_IMAGE_MODELS;
use crate::types::{AssistantImages, ImagesContext, ImagesModel, ImagesOptions, ProviderImages};

pub struct ImagesProvider {
    pub id: String,
    pub name: String,
    pub auth: ProviderAuth,
    models: Vec<ImagesModel>,
    api: Arc<dyn ProviderImages>,
}

impl ImagesProvider {
    pub fn get_models(&self) -> &[ImagesModel] {
        &self.models
    }
}

pub struct CreateImagesProviderOptions {
    pub id: String,
    pub name: Option<String>,
    pub auth: ProviderAuth,
    pub models: Vec<ImagesModel>,
    pub api: Arc<dyn ProviderImages>,
}

pub fn create_images_provider(input: CreateImagesProviderOptions) -> ImagesProvider {
    let id = input.id.clone();
    ImagesProvider {
        id: input.id,
        name: input.name.unwrap_or(id),
        auth: input.auth,
        models: input.models,
        api: input.api,
    }
}

pub struct CreateImagesModelsOptions {
    pub credentials: Option<Arc<dyn CredentialStore>>,
    pub auth_context: Option<Arc<dyn AuthContext>>,
}

pub struct ImagesModels {
    providers: HashMap<String, ImagesProvider>,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
}

pub struct MutableImagesModels {
    inner: ImagesModels,
}

impl ImagesModels {
    pub fn get_providers(&self) -> Vec<&ImagesProvider> {
        self.providers.values().collect()
    }

    pub fn get_provider(&self, id: &str) -> Option<&ImagesProvider> {
        self.providers.get(id)
    }

    pub fn get_models(&self, provider: Option<&str>) -> Vec<ImagesModel> {
        match provider {
            Some(id) => self
                .providers
                .get(id)
                .map(|p| p.get_models().to_vec())
                .unwrap_or_default(),
            None => self
                .providers
                .values()
                .flat_map(|p| p.get_models().iter().cloned())
                .collect(),
        }
    }

    pub fn get_model(&self, provider: &str, id: &str) -> Option<ImagesModel> {
        self.get_models(Some(provider)).into_iter().find(|m| m.id == id)
    }

    pub async fn get_auth(&self, model: &ImagesModel) -> Result<Option<AuthResult>, ModelsError> {
        let provider = self.providers.get(&model.provider).ok_or_else(|| {
            ModelsError::new(ModelsErrorCode::Provider, format!("Unknown provider: {}", model.provider))
        })?;
        resolve_provider_auth(
            &ProviderAuthHolder {
                id: provider.id.clone(),
                auth: provider.auth.clone(),
            },
            AuthModel::Images(model.clone()),
            self.credentials.as_ref(),
            self.auth_context.clone(),
            None,
        )
        .await
    }

    pub async fn generate_images(
        &self,
        model: &ImagesModel,
        context: &ImagesContext,
        options: Option<ImagesOptions>,
    ) -> AssistantImages {
        let Some(provider) = self.providers.get(&model.provider) else {
            return AssistantImages {
                api: model.api.clone(),
                provider: model.provider.clone(),
                model: model.id.clone(),
                output: vec![],
                response_id: None,
                usage: None,
                stop_reason: crate::types::StopReason::Error,
                error_message: Some(format!("Unknown provider: {}", model.provider)),
                timestamp: chrono::Utc::now().timestamp_millis(),
            };
        };

        let overrides = options.as_ref().map(|o| AuthResolutionOverrides {
            api_key: o.api_key.clone(),
            env: o.env.clone(),
        });
        let resolution = match resolve_provider_auth(
            &ProviderAuthHolder {
                id: provider.id.clone(),
                auth: provider.auth.clone(),
            },
            AuthModel::Images(model.clone()),
            self.credentials.as_ref(),
            self.auth_context.clone(),
            overrides,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return AssistantImages {
                    api: model.api.clone(),
                    provider: model.provider.clone(),
                    model: model.id.clone(),
                    output: vec![],
                    response_id: None,
                    usage: None,
                    stop_reason: crate::types::StopReason::Error,
                    error_message: Some(e.message),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
            }
        };

        let mut opts = options.unwrap_or(ImagesOptions {
            api_key: None,
            signal: None,
            env: None,
            headers: None,
            timeout_ms: None,
            max_retries: None,
            on_payload: None,
            on_response: None,
        });
        if let Some(res) = resolution {
            if opts.api_key.is_none() {
                opts.api_key = res.auth.api_key;
            }
            if let Some(headers) = res.auth.headers {
                let mut merged = headers;
                if let Some(request_headers) = opts.headers.take() {
                    merged.extend(request_headers);
                }
                opts.headers = Some(merged);
            }
            if res.env.is_some() || opts.env.is_some() {
                let mut merged = res.env.unwrap_or_default();
                if let Some(request_env) = opts.env.take() {
                    merged.extend(request_env);
                }
                opts.env = Some(merged);
            }
        }

        provider.api.generate_images(model, context, Some(opts)).await
    }
}

pub fn create_images_models(options: Option<CreateImagesModelsOptions>) -> MutableImagesModels {
    MutableImagesModels {
        inner: ImagesModels {
            providers: HashMap::new(),
            credentials: options
                .as_ref()
                .and_then(|o| o.credentials.clone())
                .unwrap_or_else(|| Arc::new(InMemoryCredentialStore::new())),
            auth_context: options
                .as_ref()
                .and_then(|o| o.auth_context.clone())
                .unwrap_or_else(|| Arc::new(crate::auth::DefaultAuthContext::new())),
        },
    }
}

impl MutableImagesModels {
    pub fn set_provider(&mut self, provider: ImagesProvider) {
        self.inner.providers.insert(provider.id.clone(), provider);
    }
}

impl std::ops::Deref for MutableImagesModels {
    type Target = ImagesModels;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub fn openrouter_images_provider() -> ImagesProvider {
    create_images_provider(CreateImagesProviderOptions {
        id: "openrouter".to_string(),
        name: Some("OpenRouter".to_string()),
        auth: ProviderAuth {
            api_key: Some(env_api_key_auth("OpenRouter API key", vec!["OPENROUTER_API_KEY"])),
            oauth: None,
        },
        models: OPENROUTER_IMAGE_MODELS.to_vec(),
        api: Arc::new(OpenRouterImagesApi),
    })
}

pub fn builtin_images_models(options: Option<CreateImagesModelsOptions>) -> MutableImagesModels {
    let mut models = create_images_models(options);
    models.set_provider(openrouter_images_provider());
    models
}

pub async fn generate_images(
    model: &ImagesModel,
    context: &ImagesContext,
    options: Option<ImagesOptions>,
) -> AssistantImages {
    let collection = builtin_images_models(None);
    collection.generate_images(model, context, options).await
}
