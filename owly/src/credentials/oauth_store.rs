//! Persistent OAuth credentials for elph-ai (`~/.owly/oauth-credentials.json`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use elph_ai::{BoxFuture, Credential, CredentialStore, OAuthCredential, builtin_oauth_provider_ids};
use tokio::sync::Mutex;

use super::env_dir_internal;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct StoreFile {
    #[serde(default)]
    credentials: HashMap<String, Credential>,
}

/// File-backed credential store shared by Owly agent runs.
pub struct OwlyCredentialStore {
    credentials: Mutex<HashMap<String, Credential>>,
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl OwlyCredentialStore {
    pub fn load_default() -> Result<Arc<Self>> {
        let path = env_dir_internal().join("oauth-credentials.json");
        let file = if path.exists() {
            let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            StoreFile::default()
        };

        Ok(Arc::new(Self {
            credentials: Mutex::new(file.credentials),
            path,
            write_lock: Mutex::new(()),
        }))
    }

    pub async fn store_oauth(&self, provider_id: &str, credential: OAuthCredential) -> Result<()> {
        self.credentials
            .lock()
            .await
            .insert(provider_id.to_string(), Credential::OAuth(credential));
        self.flush().await
    }

    pub async fn has_oauth(&self, provider_id: &str) -> bool {
        matches!(self.credentials.lock().await.get(provider_id), Some(Credential::OAuth(_)))
    }

    async fn flush(&self) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let credentials = self.credentials.lock().await;
        let mut oauth_only = HashMap::new();
        for provider_id in builtin_oauth_provider_ids() {
            if let Some(Credential::OAuth(cred)) = credentials.get(provider_id) {
                oauth_only.insert(provider_id.to_string(), Credential::OAuth(cred.clone()));
            }
        }
        let dir = self.path.parent().context("oauth store parent")?;
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let body = serde_json::to_string_pretty(&StoreFile {
            credentials: oauth_only,
        })?;
        std::fs::write(&self.path, body).with_context(|| format!("write {}", self.path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

impl CredentialStore for OwlyCredentialStore {
    fn read<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, Option<Credential>> {
        Box::pin(async move { self.credentials.lock().await.get(provider_id).cloned() })
    }

    fn modify<'a>(
        &'a self,
        provider_id: &'a str,
        f: elph_ai::auth::types::CredentialModifyFn,
    ) -> BoxFuture<'a, Option<Credential>> {
        Box::pin(async move {
            let current = self.credentials.lock().await.get(provider_id).cloned();
            let next = f(current).await;
            if let Some(ref cred) = next {
                self.credentials
                    .lock()
                    .await
                    .insert(provider_id.to_string(), cred.clone());
            }
            if next.is_some() {
                let _ = self.flush().await;
            }
            next
        })
    }

    fn delete<'a>(&'a self, provider_id: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.credentials.lock().await.remove(provider_id);
            let _ = self.flush().await;
        })
    }
}
