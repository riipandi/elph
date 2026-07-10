//! Shared state for the Owly TUI and async command dispatch.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};

use crate::config::Config;
use crate::session::SessionStore;
use crate::ui_events::AgentUiEvent;

/// Thread-safe shell state used by the Owly TUI and command worker.
#[derive(Clone)]
pub struct AppContext {
    inner: Arc<AppContextInner>,
}

struct AppContextInner {
    config: Mutex<Config>,
    cwd: PathBuf,
    stream: bool,
    verbose: bool,
    session: Mutex<SessionStore>,
}

impl AppContext {
    pub fn new(config: Config, cwd: PathBuf, stream: bool, verbose: bool, session: SessionStore) -> Self {
        Self {
            inner: Arc::new(AppContextInner {
                config: Mutex::new(config),
                cwd,
                stream,
                verbose,
                session: Mutex::new(session),
            }),
        }
    }

    pub fn cwd(&self) -> &Path {
        &self.inner.cwd
    }

    pub async fn config_snapshot(&self) -> Config {
        self.inner.config.lock().await.clone()
    }

    pub async fn replace_config(&self, config: Config) {
        *self.inner.config.lock().await = config;
    }

    pub async fn dispatch(
        &self,
        input: String,
        ui_events: Option<mpsc::UnboundedSender<AgentUiEvent>>,
    ) -> anyhow::Result<crate::shell::HandleInputResult> {
        let mut session = self.inner.session.lock().await;
        let config = self.inner.config.lock().await;
        crate::shell::handle_user_input(
            &config,
            &self.inner.cwd,
            self.inner.stream,
            self.inner.verbose,
            &mut session,
            &input,
            ui_events,
        )
        .await
    }

    pub fn verbose(&self) -> bool {
        self.inner.verbose
    }
}
