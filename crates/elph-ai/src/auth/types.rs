use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::types::{ImagesModel, Model, ProviderEnv, ProviderHeaders};

#[derive(Debug, Clone)]
pub struct ModelAuth {
    pub api_key: Option<String>,
    pub headers: Option<ProviderHeaders>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiKeyCredential {
    #[serde(rename = "type")]
    pub kind: String,
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<ProviderEnv>,
}

impl ApiKeyCredential {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            kind: "api_key".to_string(),
            key: Some(key.into()),
            env: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthCredential {
    #[serde(rename = "type")]
    pub kind: String,
    pub access: String,
    pub refresh: String,
    pub expires: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enterprise_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_model_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Credential {
    #[serde(rename = "api_key")]
    ApiKey(ApiKeyCredential),
    #[serde(rename = "oauth")]
    OAuth(OAuthCredential),
}

#[async_trait::async_trait]
pub trait CredentialStore: Send + Sync {
    async fn read(&self, provider_id: &str) -> Option<Credential>;
    async fn modify(
        &self,
        provider_id: &str,
        f: Box<dyn FnOnce(Option<Credential>) -> Pin<Box<dyn Future<Output = Option<Credential>> + Send>> + Send>,
    ) -> Option<Credential>;
    async fn delete(&self, provider_id: &str);
}

#[async_trait::async_trait]
pub trait AuthContext: Send + Sync {
    async fn env(&self, name: &str) -> Option<String>;
    async fn file_exists(&self, path: &str) -> bool;
}

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub auth: ModelAuth,
    pub env: Option<ProviderEnv>,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuthPrompt {
    Text {
        message: String,
        placeholder: Option<String>,
    },
    Secret {
        message: String,
        placeholder: Option<String>,
    },
    Select {
        message: String,
        options: Vec<AuthSelectOption>,
    },
    ManualCode {
        message: String,
        placeholder: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct AuthSelectOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuthEvent {
    AuthUrl {
        url: String,
        instructions: Option<String>,
    },
    DeviceCode {
        user_code: String,
        verification_uri: String,
        interval_seconds: Option<u32>,
        expires_in_seconds: Option<u32>,
    },
    Progress {
        message: String,
    },
}

#[async_trait::async_trait]
pub trait AuthLoginCallbacks: Send + Sync {
    async fn prompt(&self, prompt: AuthPrompt) -> anyhow::Result<String>;
    fn notify(&self, event: AuthEvent);
}

pub type ApiKeyResolveFn =
    Arc<dyn Fn(AuthResolveInput) -> Pin<Box<dyn Future<Output = Option<AuthResult>> + Send>> + Send + Sync>;
pub type ApiKeyLoginFn = Arc<
    dyn Fn(Arc<dyn AuthLoginCallbacks>) -> Pin<Box<dyn Future<Output = anyhow::Result<ApiKeyCredential>> + Send>>
        + Send
        + Sync,
>;

pub struct AuthResolveInput {
    pub model: AuthModel,
    pub ctx: Arc<dyn AuthContext>,
    pub credential: Option<ApiKeyCredential>,
}

#[derive(Clone)]
pub enum AuthModel {
    Chat(Model),
    Images(ImagesModel),
}

#[derive(Clone)]
pub struct ApiKeyAuth {
    pub name: String,
    pub resolve: ApiKeyResolveFn,
    pub login: Option<ApiKeyLoginFn>,
}

pub type OAuthLoginFn = Arc<
    dyn Fn(Arc<dyn AuthLoginCallbacks>) -> Pin<Box<dyn Future<Output = anyhow::Result<OAuthCredential>> + Send>>
        + Send
        + Sync,
>;
pub type OAuthRefreshFn =
    Arc<dyn Fn(OAuthCredential) -> Pin<Box<dyn Future<Output = anyhow::Result<OAuthCredential>> + Send>> + Send + Sync>;
pub type OAuthToAuthFn =
    Arc<dyn Fn(OAuthCredential) -> Pin<Box<dyn Future<Output = anyhow::Result<ModelAuth>> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct OAuthAuth {
    pub name: String,
    pub login: OAuthLoginFn,
    pub refresh: OAuthRefreshFn,
    pub to_auth: OAuthToAuthFn,
}

#[derive(Clone, Default)]
pub struct ProviderAuth {
    pub api_key: Option<ApiKeyAuth>,
    pub oauth: Option<OAuthAuth>,
}
