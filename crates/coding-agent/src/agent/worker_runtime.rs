//! Multi-worker host lifecycle: register, heartbeat, tools, shutdown.
//!
//! Inbound peer prompts (`worker_send` / `worker_ask`, or chat replies) land in
//! the durable mailbox. The session's inbox poller
//! (`CodingAgentSession::start_worker_inbox_poller` + `deliver_worker_inbound`)
//! owns delivery: claiming marks the mailbox message `delivered` **before** the
//! peer's ask can be answered, so a delivery failure can never replay/loop and
//! the peer's ask times out instead of wedging either agent. Inbound messages
//! never steal the current turn — while the harness is busy the answer is
//! enqueued as a follow-up (never a steer), and once idle the poller starts a
//! real agent turn that answers with `worker_reply`.
//!
//! **Loop guard:** only *new* messages (no `parent_msg_id`) trigger an answer
//! turn. Threaded replies (`worker_reply` / TUI chat answers) land in the inbox
//! and resolve the asker's pending ask via the mailbox rows (see `MailboxStore`),
//! so peers polling `worker_get` / `worker_await` unblock — but they never spawn
//! another reply turn. Without this, two idle workers replying to each other
//! would ping-pong forever.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use elph_agent::types::AgentTool;
use elph_agent::{
    FileLeaseStore, MailboxStore, SessionLeaseStore, WorkerRegistry, WorkerStatus, WorkerToolContext, create_worker_id,
    create_worker_tools,
};
use parking_lot::Mutex;
use tokio::task::JoinHandle;
use turso::Database;

/// Live multi-worker coordination for one coding-agent process.
pub struct WorkerRuntime {
    pub worker_id: String,
    pub session_id: String,
    pub project_key: String,
    pub name: String,
    stale_secs: u64,
    ask_timeout_ms: u64,
    max_hops: i64,
    tui_show_peers: bool,
    file_leases_enabled: bool,
    lease: SessionLeaseStore,
    registry: Arc<WorkerRegistry>,
    mailbox: Arc<MailboxStore>,
    file_leases: FileLeaseStore,
    stop: Arc<AtomicBool>,
    live_count: Arc<AtomicUsize>,
    heartbeat_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Interior mut so inbox can attach after session is behind `Arc`.
    inbox_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    inbox_poll_ms: u64,
}

pub struct WorkerRuntimeStart {
    pub database: Arc<Database>,
    pub db_path: std::path::PathBuf,
    pub worker_id: String,
    pub session_id: String,
    pub project_key: String,
    pub desired_name: String,
    pub purpose: String,
    pub model: Option<String>,
    pub heartbeat_secs: u64,
    pub stale_secs: u64,
    pub ask_timeout_ms: u64,
    pub max_hops: u32,
    pub tui_show_peers: bool,
    pub file_leases: bool,
    pub inbox_poll_ms: u64,
}

impl WorkerRuntime {
    /// Mint a process-lifetime worker id (share with session lease).
    pub fn new_worker_id() -> String {
        create_worker_id()
    }

    /// Register presence and start the heartbeat loop.
    pub async fn start(opts: WorkerRuntimeStart) -> Result<Self> {
        let stale_secs = opts.stale_secs.max(1);
        let heartbeat_secs = opts.heartbeat_secs.max(1);
        let ask_timeout_ms = opts.ask_timeout_ms.max(1);

        let lease = SessionLeaseStore::new(&opts.db_path).with_database(opts.database.clone());
        let registry = Arc::new(WorkerRegistry::new(&opts.db_path).with_database(opts.database.clone()));
        let mailbox = Arc::new(MailboxStore::new(&opts.db_path).with_database(opts.database.clone()));
        let file_leases = FileLeaseStore::new(&opts.db_path).with_database(opts.database.clone());

        let record = registry
            .register(
                &opts.worker_id,
                &opts.session_id,
                &opts.project_key,
                &opts.desired_name,
                &opts.purpose,
                opts.model.as_deref(),
                stale_secs,
            )
            .await
            .context("register worker")?;

        let stop = Arc::new(AtomicBool::new(false));
        let live_count = Arc::new(AtomicUsize::new(1));

        let hb_lease = lease.clone();
        let hb_registry = Arc::clone(&registry);
        let hb_mailbox = Arc::clone(&mailbox);
        let hb_files = file_leases.clone();
        let hb_stop = Arc::clone(&stop);
        let hb_live = Arc::clone(&live_count);
        let worker_id = opts.worker_id.clone();
        let session_id = opts.session_id.clone();
        let project_key = opts.project_key.clone();
        let model = opts.model.clone();
        let refresh_files = opts.file_leases;

        let heartbeat_handle = tokio::spawn(async move {
            let interval = Duration::from_secs(heartbeat_secs);
            let reaper = Duration::from_secs(heartbeat_secs.clamp(1, 2));
            let mut since_hb = Duration::ZERO;
            loop {
                if hb_stop.load(Ordering::Relaxed) {
                    break;
                }
                if let Err(err) = hb_registry.demote_stale(&project_key, stale_secs).await {
                    log::debug!("worker demote_stale: {err:#}");
                }
                match hb_registry.count_live(&project_key, stale_secs).await {
                    Ok(n) => hb_live.store(n, Ordering::Relaxed),
                    Err(err) => log::debug!("worker count: {err:#}"),
                }
                // Sweep timed-out asks project-wide so waiters unblock without relying only on await loops.
                if let Err(err) = hb_mailbox.sweep_timeouts(&project_key, ask_timeout_ms).await {
                    log::debug!("mailbox timeout sweep: {err:#}");
                }

                if since_hb == Duration::ZERO || since_hb >= interval {
                    if let Err(err) = hb_lease.heartbeat(&session_id, &worker_id).await {
                        log::warn!("session lease heartbeat: {err:#}");
                    }
                    if let Err(err) = hb_registry
                        .heartbeat(&worker_id, WorkerStatus::Online, None, model.as_deref())
                        .await
                    {
                        log::warn!("worker registry heartbeat: {err:#}");
                    }
                    if refresh_files && let Err(err) = hb_files.refresh_worker(&worker_id).await {
                        log::debug!("file lease refresh: {err:#}");
                    }
                    since_hb = Duration::ZERO;
                }

                tokio::select! {
                    _ = tokio::time::sleep(reaper) => {
                        since_hb = since_hb.saturating_add(reaper);
                    }
                    _ = async {
                        while !hb_stop.load(Ordering::Relaxed) {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                    } => break,
                }
            }
        });

        if let Ok(n) = registry.count_live(&opts.project_key, stale_secs).await {
            live_count.store(n, Ordering::Relaxed);
        }

        Ok(Self {
            worker_id: opts.worker_id,
            session_id: opts.session_id,
            project_key: opts.project_key,
            name: record.name,
            stale_secs,
            ask_timeout_ms,
            max_hops: opts.max_hops.max(1) as i64,
            tui_show_peers: opts.tui_show_peers,
            file_leases_enabled: opts.file_leases,
            lease,
            registry,
            mailbox,
            file_leases,
            stop,
            live_count,
            heartbeat_handle: Arc::new(Mutex::new(Some(heartbeat_handle))),
            inbox_handle: Arc::new(Mutex::new(None)),
            inbox_poll_ms: opts.inbox_poll_ms.max(100),
        })
    }

    pub fn registry(&self) -> Arc<WorkerRegistry> {
        Arc::clone(&self.registry)
    }

    pub fn mailbox(&self) -> Arc<MailboxStore> {
        Arc::clone(&self.mailbox)
    }

    pub fn file_leases(&self) -> FileLeaseStore {
        self.file_leases.clone()
    }

    pub fn file_leases_enabled(&self) -> bool {
        self.file_leases_enabled
    }

    pub fn live_count(&self) -> usize {
        self.live_count.load(Ordering::Relaxed)
    }

    /// Count to show in TUI: full live count when peers enabled and ≥2, else 0 (hidden).
    pub fn tui_peer_badge_count(&self) -> usize {
        if !self.tui_show_peers {
            return 0;
        }
        let n = self.live_count();
        if n >= 2 { n } else { 0 }
    }

    pub fn stale_secs(&self) -> u64 {
        self.stale_secs
    }

    pub fn inbox_poll_ms(&self) -> u64 {
        self.inbox_poll_ms
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    /// Comma-separated live peer names (excludes self), for system prompt injection.
    pub async fn peer_names_summary(&self) -> String {
        match self
            .registry
            .list_live_peers(&self.project_key, &self.worker_id, self.stale_secs)
            .await
        {
            Ok(peers) => peers
                .into_iter()
                .filter(|p| !p.is_self)
                .map(|p| p.name)
                .collect::<Vec<_>>()
                .join(", "),
            Err(_) => String::new(),
        }
    }

    /// Agent tools for peer coordination.
    pub fn create_tools(&self) -> Vec<AgentTool> {
        let ctx = Arc::new(WorkerToolContext {
            registry: Arc::clone(&self.registry),
            mailbox: Arc::clone(&self.mailbox),
            worker_id: self.worker_id.clone(),
            session_id: self.session_id.clone(),
            project_key: self.project_key.clone(),
            stale_secs: self.stale_secs,
            ask_timeout_ms: self.ask_timeout_ms,
            max_hops: self.max_hops,
        });
        create_worker_tools(ctx)
    }

    /// Attach a host-provided inbox poller handle (called after session Arc is ready).
    pub fn set_inbox_handle(&self, handle: JoinHandle<()>) {
        if let Some(old) = self.inbox_handle.lock().replace(handle) {
            old.abort();
        }
    }

    /// Stop heartbeat and release coordination rows (best-effort). Safe from `Arc` session.
    pub async fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.heartbeat_handle.lock().take() {
            handle.abort();
        }
        if let Some(handle) = self.inbox_handle.lock().take() {
            handle.abort();
        }
        if let Err(err) = self.file_leases.release_all_for_worker(&self.worker_id).await {
            log::warn!("release file leases on shutdown: {err:#}");
        }
        if let Err(err) = self.lease.release(&self.session_id, &self.worker_id).await {
            log::warn!("release session lease on shutdown: {err:#}");
        }
        if let Err(err) = self
            .registry
            .mark_offline_with_reason(&self.worker_id, "clean_exit")
            .await
        {
            log::warn!("mark worker offline on shutdown: {err:#}");
        }
    }
}

impl Drop for WorkerRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.heartbeat_handle.lock().take() {
            handle.abort();
        }
        if let Some(handle) = self.inbox_handle.lock().take() {
            handle.abort();
        }
        let lease = self.lease.clone();
        let registry = Arc::clone(&self.registry);
        let files = self.file_leases.clone();
        let session_id = self.session_id.clone();
        let worker_id = self.worker_id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = files.release_all_for_worker(&worker_id).await;
                let _ = lease.release(&session_id, &worker_id).await;
                let _ = registry.mark_offline_with_reason(&worker_id, "process_drop").await;
            });
        }
    }
}
