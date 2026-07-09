use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::types::{BoxFuture, Credential, CredentialModifyFn, CredentialStore};

/// In-memory credential store with per-provider serialized writes.
pub struct InMemoryCredentialStore {
    credentials: Mutex<HashMap<String, Credential>>,
    chains: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl Default for InMemoryCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCredentialStore {
    pub fn new() -> Self {
        Self {
            credentials: Mutex::new(HashMap::new()),
            chains: Mutex::new(HashMap::new()),
        }
    }

    async fn lock_chain(&self, provider_id: &str) -> Arc<Mutex<()>> {
        let mut chains = self.chains.lock().await;
        chains
            .entry(provider_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn read<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, Option<Credential>> {
        Box::pin(async move { self.credentials.lock().await.get(provider_id).cloned() })
    }

    fn modify<'a>(&'a self, provider_id: &'a str, f: CredentialModifyFn) -> BoxFuture<'a, Option<Credential>> {
        let provider_id = provider_id.to_string();
        Box::pin(async move {
            let chain = self.lock_chain(&provider_id).await;
            let _guard = chain.lock().await;
            let current = self.credentials.lock().await.get(&provider_id).cloned();
            let next = f(current).await;
            let mut credentials = self.credentials.lock().await;
            if let Some(ref cred) = next {
                credentials.insert(provider_id.clone(), cred.clone());
                next
            } else {
                credentials.get(&provider_id).cloned()
            }
        })
    }

    fn delete<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, ()> {
        let provider_id = provider_id.to_string();
        Box::pin(async move {
            let chain = self.lock_chain(&provider_id).await;
            let _guard = chain.lock().await;
            self.credentials.lock().await.remove(&provider_id);
        })
    }
}
