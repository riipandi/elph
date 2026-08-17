use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::auth::ProviderAuthHolder;
use crate::auth::oauth::oauth_provider_modify_models;
use crate::auth::resolve::resolve_provider_auth;
use crate::auth::resolve::{AuthResolutionOverrides, ModelsError, ModelsErrorCode};
use crate::auth::types::{Credential, OAuthCredential};
use crate::auth::{AuthContext, AuthModel, AuthResult, CredentialStore, InMemoryCredentialStore, ProviderAuth};
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
    /// Full catalog baseline (before OAuth plan filtering).
    catalog_models: Vec<Model>,
    /// Live model list (may be plan-filtered). Interior mutability so credentials
    /// can re-filter after `/provider connect` without rebuilding `Models`.
    models: RwLock<Vec<Model>>,
    refresh: Option<RefreshFn>,
    api: ProviderApi,
}

type RefreshFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Model>>> + Send>> + Send + Sync>;

impl Provider {
    pub fn get_models(&self) -> Vec<Model> {
        self.models.read().clone()
    }

    /// Replace the model list for this provider (keeps auth/api adapters).
    /// Also updates the catalog baseline used for OAuth re-filtering.
    pub fn set_models(&mut self, models: Vec<Model>) {
        self.catalog_models = models.clone();
        *self.models.write() = models;
    }

    /// Re-apply OAuth `modify_models` (base URL + plan filter) onto the catalog baseline.
    pub fn apply_oauth_model_filter(&self, credential: &OAuthCredential) {
        let filtered = oauth_provider_modify_models(&self.id, self.catalog_models.clone(), credential);
        *self.models.write() = filtered;
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
        catalog_models: input.models.clone(),
        models: RwLock::new(input.models),
        refresh: input.refresh_models,
        api: input.api,
    }
}

#[derive(Default)]
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
    /// Shared credential store used by [`Self::get_auth`] / streaming auth resolution.
    ///
    /// Host apps should update this after `/provider connect` so the live session
    /// picks up new OAuth/API keys without a full restart.
    pub fn credentials(&self) -> Arc<dyn CredentialStore> {
        self.credentials.clone()
    }

    /// Insert or replace a credential for `provider_id` (in-memory store used by this
    /// models collection). Does not write `auth.json` — callers persist separately.
    ///
    /// For OAuth providers with `modify_models` (e.g. GitHub Copilot plan gating), also
    /// re-filters the live model list from the catalog baseline.
    pub async fn set_credential(&self, provider_id: &str, credential: crate::auth::Credential) {
        let provider_id = provider_id.to_string();
        let cred = credential;
        let for_filter = cred.clone();
        self.credentials
            .modify(&provider_id, Box::new(move |_| Box::pin(async move { Some(cred) })))
            .await;
        self.apply_oauth_model_filter(provider_id.as_str(), &for_filter);
    }

    /// Apply OAuth plan/baseUrl model filters for every stored OAuth credential.
    pub async fn apply_oauth_model_filters(&self) {
        for info in self.credentials.list().await {
            if let Some(cred) = self.credentials.read(&info.provider_id).await {
                self.apply_oauth_model_filter(&info.provider_id, &cred);
            }
        }
    }

    fn apply_oauth_model_filter(&self, provider_id: &str, credential: &Credential) {
        let Credential::OAuth(oauth) = credential else {
            return;
        };
        let Some(provider) = self.providers.get(provider_id) else {
            return;
        };
        provider.apply_oauth_model_filter(oauth);
    }

    pub fn get_providers(&self) -> Vec<&Provider> {
        self.providers.values().collect()
    }

    pub fn get_provider(&self, id: &str) -> Option<&Provider> {
        self.providers.get(id)
    }

    pub fn get_models(&self, provider: Option<&str>) -> Vec<Model> {
        match provider {
            Some(id) => self.providers.get(id).map(|p| p.get_models()).unwrap_or_default(),
            None => self.providers.values().flat_map(|p| p.get_models()).collect(),
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
            catalog_models: self.catalog_models.clone(),
            models: RwLock::new(self.models.read().clone()),
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

    /// Apply disk catalog overlays onto existing providers' model lists.
    ///
    /// Disk-only provider ids (no built-in adapter) are registered when their models
    /// use a supported API (`openai-completions`, `openai-responses`, `anthropic-messages`,
    /// `google-generative-ai`, `mistral-conversations`, `azure-openai-responses`).
    pub fn apply_model_overlays(&mut self, overlays: &HashMap<String, Vec<Model>>) -> OverlayApplyReport {
        let mut report = OverlayApplyReport::default();
        for (provider_id, overlay) in overlays {
            if let Some(provider) = self.inner.providers.get_mut(provider_id) {
                let merged = crate::models::catalog::merge_model_lists(&provider.get_models(), overlay);
                provider.set_models(merged);
                report.updated += 1;
            } else if let Some(provider) = create_disk_provider(provider_id, overlay.clone()) {
                log::info!(
                    "registered disk-only provider `{provider_id}` ({} model(s)) for streaming",
                    provider.get_models().len()
                );
                self.set_provider(provider);
                report.registered += 1;
            } else {
                log::warn!(
                    "provider catalog `{provider_id}` skipped: no supported API among {} model(s)",
                    overlay.len()
                );
                report.skipped += 1;
            }
        }
        report
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

/// Result of applying disk catalog overlays onto a [`MutableModels`] collection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayApplyReport {
    /// Existing built-in providers whose model lists were merged.
    pub updated: usize,
    /// New disk-only providers registered with streaming adapters.
    pub registered: usize,
    /// Disk providers skipped (unsupported API kinds only).
    pub skipped: usize,
}

/// Build a streaming provider from a disk-only catalog when models use known APIs.
pub fn create_disk_provider(id: &str, models: Vec<Model>) -> Option<Provider> {
    if models.is_empty() {
        return None;
    }
    let api = disk_provider_api_map(&models)?;
    let base_url = models
        .iter()
        .find(|m| !m.base_url.is_empty())
        .map(|m| m.base_url.clone());
    let headers = models.iter().find_map(|m| {
        m.headers.as_ref().map(|h| {
            h.iter()
                .map(|(k, v)| (k.clone(), Some(v.clone())))
                .collect::<crate::types::ProviderHeaders>()
        })
    });
    // Convention: SOME_PROVIDER_API_KEY from kebab id `some-provider`.
    let env_name = format!("{}_API_KEY", id.replace('-', "_").to_ascii_uppercase());
    let display = title_case_provider_id(id);
    let local = base_url
        .as_deref()
        .is_some_and(crate::auth::is_local_or_loopback_base_url)
        || {
            let lower = id.to_ascii_lowercase();
            lower.contains("local") || lower.contains("ollama") || lower.contains("lmstudio") || lower.contains("vllm")
        };
    let api_key_auth = if local {
        // Local OpenAI-compatible endpoints rarely require a real key.
        crate::auth::optional_env_api_key_auth(format!("{display} API key (optional)"), vec![env_name])
    } else {
        crate::auth::flexible_api_key_auth(format!("{display} API key"), vec![env_name])
    };
    Some(create_provider(CreateProviderOptions {
        id: id.to_string(),
        name: Some(display.clone()),
        base_url,
        headers,
        auth: crate::auth::ProviderAuth {
            api_key: Some(api_key_auth),
            oauth: None,
        },
        models,
        refresh_models: None,
        api,
    }))
}

fn disk_provider_api_map(models: &[Model]) -> Option<ProviderApi> {
    use crate::providers::adapter::{
        anthropic_messages_api, azure_openai_responses_api, google_generative_ai_api, mistral_conversations_api,
        openai_completions_api, openai_responses_api,
    };

    let mut map = HashMap::new();
    for model in models {
        let key = model.api.as_str();
        if map.contains_key(key) {
            continue;
        }
        let api = match key {
            "openai-completions" => openai_completions_api(),
            "openai-responses" => openai_responses_api(),
            "anthropic-messages" => anthropic_messages_api(),
            "google-generative-ai" => google_generative_ai_api(),
            "mistral-conversations" => mistral_conversations_api(),
            "azure-openai-responses" => azure_openai_responses_api(),
            _ => continue,
        };
        map.insert(key.to_string(), api);
    }
    if map.is_empty() {
        None
    } else {
        Some(ProviderApi::Map(map))
    }
}

fn title_case_provider_id(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ModelCost;

    fn sample_model(provider: &str, api: &str) -> Model {
        Model {
            id: "m1".into(),
            name: "M1".into(),
            api: api.into(),
            provider: provider.into(),
            base_url: "https://example.com/v1".into(),
            reasoning: false,
            thinking_level_map: None,
            input: vec!["text".into()],
            cost: ModelCost {
                input: 1.0,
                output: 1.0,
                cache_read: 0.0,
                cache_write: 0.0,
                tiers: None,
            },
            context_window: 8_000,
            max_tokens: 1_024,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        }
    }

    #[test]
    fn create_disk_provider_registers_openai_completions() {
        let p = create_disk_provider("my-gateway", vec![sample_model("my-gateway", "openai-completions")])
            .expect("provider");
        assert_eq!(p.id, "my-gateway");
        assert_eq!(p.get_models().len(), 1);
        assert!(p.base_url.as_deref() == Some("https://example.com/v1"));
    }

    #[test]
    fn create_disk_provider_skips_unknown_api() {
        assert!(create_disk_provider("x", vec![sample_model("x", "totally-unknown-api")]).is_none());
    }

    #[test]
    fn apply_overlays_registers_disk_only() {
        let mut models = create_models(None);
        let mut overlays = HashMap::new();
        overlays.insert("custom-llm".into(), vec![sample_model("custom-llm", "openai-completions")]);
        let report = models.apply_model_overlays(&overlays);
        assert_eq!(report.registered, 1);
        assert_eq!(report.skipped, 0);
        assert!(models.get_provider("custom-llm").is_some());
    }
}
