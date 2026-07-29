//! Model store interface for persisting and restoring dynamic provider catalogs.
//!
//! Ported from `@earendil-works/pi-ai` (`packages/ai/src/models-store.ts`).
//!
//! A `ModelsStore` allows dynamic providers to persist their model catalogs
//! across restarts. The default in-memory implementation is always available;
//! host applications may provide a persistent backend (e.g. file-based or DB).

use std::sync::Arc;

use crate::auth::types::BoxFuture;
use crate::types::Model;

/// A stored model catalog entry for a single provider.
#[derive(Debug, Clone)]
pub struct ModelsStoreEntry {
    /// The resolved models for this provider.
    pub models: Vec<Model>,
    /// Remote ETag validator for conditional refresh (optional).
    pub etag: Option<String>,
}

/// Provider-scoped model store handle.
///
/// Providers receive a `ProviderStore` that scopes reads and writes to their
/// own provider ID, preventing one provider from accessing another's catalog.
#[derive(Clone)]
pub struct ProviderStore {
    inner: Arc<dyn ModelsStore>,
    provider_id: String,
}

impl ProviderStore {
    pub fn new(inner: Arc<dyn ModelsStore>, provider_id: impl Into<String>) -> Self {
        Self {
            inner,
            provider_id: provider_id.into(),
        }
    }

    /// Read this provider's stored catalog entry.
    pub async fn read(&self) -> Option<ModelsStoreEntry> {
        self.inner.read(&self.provider_id).await
    }

    /// Write this provider's catalog entry.
    pub async fn write(&self, entry: ModelsStoreEntry) {
        self.inner.write(&self.provider_id, entry).await;
    }
}

/// Abstract model store for dynamic provider catalogs.
///
/// Implementations must scope reads and writes per `provider_id` so providers
/// cannot access other providers' catalogs.
pub trait ModelsStore: Send + Sync {
    /// Read the stored entry for a provider.
    fn read<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, Option<ModelsStoreEntry>>;

    /// Write a provider's catalog entry.
    fn write<'a>(&'a self, provider_id: &'a str, entry: ModelsStoreEntry) -> BoxFuture<'a, ()>;

    /// Remove a provider's stored entry.
    fn delete<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, ()>;

    /// List all provider IDs that have stored entries.
    fn list<'a>(&'a self) -> BoxFuture<'a, Vec<String>>;
}

/// In-memory implementation of `ModelsStore`.
///
/// Default store used when no persistent backend is configured.
pub struct InMemoryModelsStore {
    entries: tokio::sync::Mutex<std::collections::HashMap<String, ModelsStoreEntry>>,
}

impl Default for InMemoryModelsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryModelsStore {
    pub fn new() -> Self {
        Self {
            entries: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl ModelsStore for InMemoryModelsStore {
    fn read<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, Option<ModelsStoreEntry>> {
        let provider_id = provider_id.to_string();
        Box::pin(async move { self.entries.lock().await.get(&provider_id).cloned() })
    }

    fn write<'a>(&'a self, provider_id: &'a str, entry: ModelsStoreEntry) -> BoxFuture<'a, ()> {
        let provider_id = provider_id.to_string();
        Box::pin(async move {
            self.entries.lock().await.insert(provider_id, entry);
        })
    }

    fn delete<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, ()> {
        let provider_id = provider_id.to_string();
        Box::pin(async move {
            self.entries.lock().await.remove(&provider_id);
        })
    }

    fn list<'a>(&'a self) -> BoxFuture<'a, Vec<String>> {
        Box::pin(async move { self.entries.lock().await.keys().cloned().collect() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_in_memory_store_roundtrip() {
        let store = InMemoryModelsStore::new();
        let model = Model {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            api: "test".to_string(),
            provider: "test-provider".to_string(),
            base_url: "https://api.example.com".to_string(),
            reasoning: false,
            thinking_level_map: None,
            input: vec!["text".to_string()],
            cost: crate::types::ModelCost::default(),
            context_window: 4096,
            max_tokens: 1024,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        };

        let entry = ModelsStoreEntry {
            models: vec![model],
            etag: Some("\"abc123\"".to_string()),
        };

        store.write("test-provider", entry.clone()).await;
        let read_back = store.read("test-provider").await;
        assert!(read_back.is_some());
        assert_eq!(read_back.unwrap().etag, Some("\"abc123\"".to_string()));

        let providers = store.list().await;
        assert_eq!(providers, vec!["test-provider".to_string()]);

        store.delete("test-provider").await;
        assert!(store.read("test-provider").await.is_none());
    }

    #[tokio::test]
    async fn test_provider_store_scoping() {
        let inner = Arc::new(InMemoryModelsStore::new());
        let provider_a = ProviderStore::new(inner.clone(), "provider-a");
        let provider_b = ProviderStore::new(inner.clone(), "provider-b");

        provider_a
            .write(ModelsStoreEntry {
                models: vec![],
                etag: None,
            })
            .await;
        provider_b
            .write(ModelsStoreEntry {
                models: vec![],
                etag: None,
            })
            .await;

        assert!(provider_a.read().await.is_some());
        assert!(provider_b.read().await.is_some());
    }
}
