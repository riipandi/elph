use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::auth::{
    AuthContext, AuthModel, AuthResult, CredentialStore, InMemoryCredentialStore, ProviderAuth, ProviderAuthHolder,
    resolve::{AuthResolutionOverrides, ModelsError, ModelsErrorCode, resolve_provider_auth},
};
use crate::types::{AssistantMessage, Context, Model, ProviderHeaders, SimpleStreamOptions, StreamOptions};
use crate::utils::event_stream::AssistantMessageEventStream;

pub trait ProviderStreamsDyn: Send + Sync {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream;

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream;
}

pub enum ProviderApi {
    Single(Arc<dyn ProviderStreamsDyn>),
    Map(HashMap<String, Arc<dyn ProviderStreamsDyn>>),
}

pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: Option<String>,
    pub headers: Option<ProviderHeaders>,
    pub auth: ProviderAuth,
    models: Vec<Model>,
    refresh: Option<RefreshFn>,
    api: ProviderApi,
}

type RefreshFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Model>>> + Send>> + Send + Sync>;

impl Provider {
    pub fn get_models(&self) -> &[Model] {
        &self.models
    }

    pub fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: Option<StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.dispatch(model, |streams| streams.stream(model, context, options))
    }

    pub fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        self.dispatch(model, |streams| streams.stream_simple(model, context, options))
    }

    pub async fn refresh_models(&self) -> Result<(), ModelsError> {
        let Some(refresh) = &self.refresh else {
            return Ok(());
        };
        refresh().await.map_err(|e| {
            ModelsError::with_cause(ModelsErrorCode::ModelSource, format!("Model refresh failed for {}", self.id), e)
        })?;
        Ok(())
    }

    fn api_for(&self, model: &Model) -> Option<Arc<dyn ProviderStreamsDyn>> {
        match &self.api {
            ProviderApi::Single(api) => Some(api.clone()),
            ProviderApi::Map(map) => map.get(&model.api).cloned(),
        }
    }

    fn dispatch(
        &self,
        model: &Model,
        run: impl FnOnce(Arc<dyn ProviderStreamsDyn>) -> AssistantMessageEventStream,
    ) -> AssistantMessageEventStream {
        match self.api_for(model) {
            Some(api) => run(api),
            None => AssistantMessageEventStream::failed(format!(
                "Provider {} has no API implementation for \"{}\"",
                self.id, model.api
            )),
        }
    }
}

pub struct CreateProviderOptions {
    pub id: String,
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub headers: Option<ProviderHeaders>,
    pub auth: ProviderAuth,
    pub models: Vec<Model>,
    pub refresh_models: Option<RefreshFn>,
    pub api: ProviderApi,
}

pub fn create_provider(input: CreateProviderOptions) -> Provider {
    let id = input.id.clone();
    Provider {
        id: input.id,
        name: input.name.unwrap_or(id),
        base_url: input.base_url,
        headers: input.headers,
        auth: input.auth,
        models: input.models,
        refresh: input.refresh_models,
        api: input.api,
    }
}

pub struct CreateModelsOptions {
    pub credentials: Option<Arc<dyn CredentialStore>>,
    pub auth_context: Option<Arc<dyn AuthContext>>,
}

pub struct Models {
    providers: HashMap<String, Provider>,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
}

pub struct MutableModels {
    inner: Models,
}

impl Models {
    pub fn get_providers(&self) -> Vec<&Provider> {
        self.providers.values().collect()
    }

    pub fn get_provider(&self, id: &str) -> Option<&Provider> {
        self.providers.get(id)
    }

    pub fn get_models(&self, provider: Option<&str>) -> Vec<Model> {
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

    pub fn get_model(&self, provider: &str, id: &str) -> Option<Model> {
        self.get_models(Some(provider)).into_iter().find(|m| m.id == id)
    }

    pub async fn refresh(&self, provider: Option<&str>) -> Result<(), ModelsError> {
        match provider {
            Some(id) => {
                let p = self
                    .providers
                    .get(id)
                    .ok_or_else(|| ModelsError::new(ModelsErrorCode::Provider, format!("Unknown provider: {id}")))?;
                p.refresh_models().await
            }
            None => {
                let mut errors = vec![];
                for p in self.providers.values() {
                    if let Err(e) = p.refresh_models().await {
                        errors.push(e);
                    }
                }
                if let Some(e) = errors.into_iter().next() {
                    return Err(e);
                }
                Ok(())
            }
        }
    }

    pub async fn get_auth(&self, model: &Model) -> Result<Option<AuthResult>, ModelsError> {
        let provider = self.providers.get(&model.provider).ok_or_else(|| {
            ModelsError::new(ModelsErrorCode::Provider, format!("Unknown provider: {}", model.provider))
        })?;
        resolve_provider_auth(
            &ProviderAuthHolder {
                id: provider.id.clone(),
                auth: provider.auth.clone(),
            },
            AuthModel::Chat(model.clone()),
            self.credentials.as_ref(),
            self.auth_context.clone(),
            None,
        )
        .await
    }

    pub fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: Option<StreamOptions>,
    ) -> AssistantMessageEventStream {
        let inner = self.clone_for_stream();
        let model = model.clone();
        let context = context.clone();
        lazy_stream(model.clone(), move || async move {
            let provider = inner.require_provider(&model)?;
            let (request_model, request_options) = inner.apply_auth(&model, options).await?;
            Ok(provider.stream(&request_model, &context, request_options))
        })
    }

    pub async fn complete(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessage {
        self.stream(model, context, options).result().await
    }

    pub fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let inner = self.clone_for_stream();
        let model = model.clone();
        let context = context.clone();
        lazy_stream(model.clone(), move || async move {
            let provider = inner.require_provider(&model)?;
            let (request_model, request_options) = inner.apply_auth_simple(&model, options).await?;
            Ok(provider.stream_simple(&request_model, &context, request_options))
        })
    }

    pub async fn complete_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessage {
        self.stream_simple(model, context, options).result().await
    }

    fn clone_for_stream(&self) -> Models {
        Models {
            providers: self.providers.clone(),
            credentials: self.credentials.clone(),
            auth_context: self.auth_context.clone(),
        }
    }

    fn require_provider(&self, model: &Model) -> Result<&Provider, ModelsError> {
        self.providers
            .get(&model.provider)
            .ok_or_else(|| ModelsError::new(ModelsErrorCode::Provider, format!("Unknown provider: {}", model.provider)))
    }

    async fn apply_auth(
        &self,
        model: &Model,
        options: Option<StreamOptions>,
    ) -> Result<(Model, Option<StreamOptions>), ModelsError> {
        let provider = self.require_provider(model)?;
        let overrides = options.as_ref().map(|o| AuthResolutionOverrides {
            api_key: o.api_key.clone(),
            env: o.env.clone(),
        });
        let resolution = resolve_provider_auth(
            &ProviderAuthHolder {
                id: provider.id.clone(),
                auth: provider.auth.clone(),
            },
            AuthModel::Chat(model.clone()),
            self.credentials.as_ref(),
            self.auth_context.clone(),
            overrides,
        )
        .await?;
        Ok(merge_auth(model, options, resolution, provider))
    }

    async fn apply_auth_simple(
        &self,
        model: &Model,
        options: Option<SimpleStreamOptions>,
    ) -> Result<(Model, Option<SimpleStreamOptions>), ModelsError> {
        let stream_opts = options.as_ref().map(|o| o.base.clone());
        let (request_model, stream_opts) = self.apply_auth(model, stream_opts).await?;
        let request_options = stream_opts.map(|base| SimpleStreamOptions {
            base,
            reasoning: options.as_ref().and_then(|o| o.reasoning),
            thinking_budgets: options.as_ref().and_then(|o| o.thinking_budgets.clone()),
        });
        Ok((request_model, request_options))
    }
}

impl Clone for Provider {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            headers: self.headers.clone(),
            auth: self.auth.clone(),
            models: self.models.clone(),
            refresh: self.refresh.clone(),
            api: match &self.api {
                ProviderApi::Single(s) => ProviderApi::Single(s.clone()),
                ProviderApi::Map(m) => ProviderApi::Map(m.clone()),
            },
        }
    }
}

fn merge_auth(
    model: &Model,
    options: Option<StreamOptions>,
    resolution: Option<AuthResult>,
    provider: &Provider,
) -> (Model, Option<StreamOptions>) {
    let mut request_model = model.clone();
    let mut request_options = options.unwrap_or_default();

    if let Some(res) = resolution {
        if let Some(url) = res.auth.base_url {
            request_model.base_url = url;
        }
        if request_options.api_key.is_none() {
            request_options.api_key = res.auth.api_key;
        }
        if let Some(headers) = res.auth.headers {
            let mut merged = provider.headers.clone().unwrap_or_default();
            merged.extend(headers);
            if let Some(opts) = &request_options.headers {
                merged.extend(opts.clone());
            }
            request_options.headers = Some(merged);
        }
        if let Some(env) = res.env {
            let mut merged = request_options.env.unwrap_or_default();
            merged.extend(env);
            request_options.env = Some(merged);
        }
    }

    (request_model, Some(request_options))
}

fn lazy_stream<F, Fut>(model: Model, setup: F) -> AssistantMessageEventStream
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<AssistantMessageEventStream, ModelsError>> + Send + 'static,
{
    let stream = AssistantMessageEventStream::new();
    let output = stream.clone_handle();
    let trace_model = model.clone();
    crate::trace::spawn_stream(&trace_model, async move {
        match setup().await {
            Ok(mut inner) => {
                while let Some(event) = inner.next_event().await {
                    let terminal = matches!(
                        &event,
                        crate::types::AssistantMessageEvent::Done { .. }
                            | crate::types::AssistantMessageEvent::Error { .. }
                    );
                    output.push(event);
                    if terminal {
                        break;
                    }
                }
            }
            Err(e) => {
                let mut partial = crate::types::AssistantMessage::empty(&model);
                partial.stop_reason = crate::types::StopReason::Error;
                partial.error_message = Some(e.message);
                output.push(crate::types::AssistantMessageEvent::Error {
                    reason: crate::types::StopReason::Error,
                    error: partial,
                });
            }
        }
        output.end();
    });
    stream
}

pub fn create_models(options: Option<CreateModelsOptions>) -> MutableModels {
    MutableModels {
        inner: Models {
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

impl MutableModels {
    /// Consume the mutable wrapper and share the underlying [`Models`] via [`Arc`].
    pub fn into_arc(self) -> std::sync::Arc<Models> {
        std::sync::Arc::new(self.inner)
    }

    pub fn set_provider(&mut self, provider: Provider) {
        self.inner.providers.insert(provider.id.clone(), provider);
    }

    pub fn delete_provider(&mut self, id: &str) {
        self.inner.providers.remove(id);
    }

    pub fn clear_providers(&mut self) {
        self.inner.providers.clear();
    }

    pub fn inner(&self) -> &Models {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut Models {
        &mut self.inner
    }
}

impl std::ops::Deref for MutableModels {
    type Target = Models;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub fn has_api(model: &Model, api: &str) -> bool {
    model.api == api
}

pub fn models_are_equal(a: Option<&Model>, b: Option<&Model>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.id == b.id && a.provider == b.provider,
        _ => false,
    }
}

pub fn get_supported_thinking_levels(model: &Model) -> Vec<crate::types::ThinkingLevel> {
    if !model.reasoning {
        return vec![];
    }
    let levels = [
        crate::types::ThinkingLevel::Minimal,
        crate::types::ThinkingLevel::Low,
        crate::types::ThinkingLevel::Medium,
        crate::types::ThinkingLevel::High,
        crate::types::ThinkingLevel::Xhigh,
        crate::types::ThinkingLevel::Max,
    ];
    levels
        .into_iter()
        .filter(|level| {
            if let Some(map) = &model.thinking_level_map {
                let key = crate::models::thinking_level_to_str(*level);
                if map.get(key) == Some(&None) {
                    return false;
                }
                // xhigh/max are opt-in via thinkingLevelMap; other levels default on.
                if matches!(level, crate::types::ThinkingLevel::Xhigh | crate::types::ThinkingLevel::Max) {
                    return map.contains_key(key);
                }
            } else if matches!(level, crate::types::ThinkingLevel::Xhigh | crate::types::ThinkingLevel::Max) {
                return false;
            }
            true
        })
        .collect()
}

pub fn clamp_thinking_level(model: &Model, level: crate::types::ThinkingLevel) -> crate::types::ThinkingLevel {
    let available = get_supported_thinking_levels(model);
    if available.contains(&level) {
        return level;
    }
    let all = [
        crate::types::ThinkingLevel::Minimal,
        crate::types::ThinkingLevel::Low,
        crate::types::ThinkingLevel::Medium,
        crate::types::ThinkingLevel::High,
        crate::types::ThinkingLevel::Xhigh,
        crate::types::ThinkingLevel::Max,
    ];
    let idx = all.iter().position(|l| *l == level).unwrap_or(0);
    for &candidate in &all[idx..] {
        if available.contains(&candidate) {
            return candidate;
        }
    }
    for &candidate in all[..idx].iter().rev() {
        if available.contains(&candidate) {
            return candidate;
        }
    }
    crate::types::ThinkingLevel::High
}
