//! Shared ACP connection state.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use agent_client_protocol::schema::v2::SessionId;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::agent::{AgentUiEvent, CodingAgentSession};
use crate::platform::{Paths, Settings};
use crate::types::AgentMode;

pub type SessionContext = (
    Arc<CodingAgentSession>,
    Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
    PathBuf,
);

pub struct AcpSessionState {
    pub session: Arc<CodingAgentSession>,
    pub ui_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
    pub cwd: PathBuf,
    pub additional_directories: Vec<PathBuf>,
    pub running: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
    pub ids: MessageIds,
    pub open_tools: Arc<Mutex<HashSet<String>>>,
    /// Tool-call ids whose local shell is mirrored as a display-only ACP terminal.
    pub open_shells: Arc<Mutex<HashSet<String>>>,
}

pub struct AcpAgentState {
    pub sessions: HashMap<String, AcpSessionState>,
    pub paths: Paths,
    pub settings: Settings,
    pub client_fs_read: bool,
    pub client_elicitation_form: bool,
    /// Connection-scoped ACP login. Cleared by logout; does not delete `auth.json`.
    pub authenticated: bool,
}

#[derive(Clone)]
pub struct MessageIds {
    next: Arc<AtomicU64>,
}

impl MessageIds {
    pub fn new() -> Self {
        Self {
            next: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn next(&self, prefix: &str) -> String {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}_{n}")
    }
}

impl Default for MessageIds {
    fn default() -> Self {
        Self::new()
    }
}

pub fn session_key(session_id: &SessionId) -> String {
    session_id.0.as_ref().to_owned()
}

pub fn lookup_session(state: &Arc<Mutex<AcpAgentState>>, key: &str) -> anyhow::Result<SessionContext> {
    let guard = state.lock();
    let entry = guard
        .sessions
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("ACP session not found"))?;
    Ok((Arc::clone(&entry.session), entry.ui_rx.clone(), entry.cwd.clone()))
}

pub fn current_mode(session: &CodingAgentSession) -> AgentMode {
    session.mode_state().try_lock().map(|g| *g).unwrap_or(AgentMode::Build)
}
