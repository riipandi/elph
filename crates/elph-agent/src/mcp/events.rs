//! MCP server notification events (list_changed, progress, etc.).

use std::sync::Arc;

use rmcp::handler::client::ClientHandler;
use rmcp::model::{CancelledNotificationParam, ProgressNotificationParam, ResourceUpdatedNotificationParam};
use rmcp::service::{NotificationContext, RoleClient};
use tokio::sync::mpsc;

/// Events emitted when an MCP server notifies the client.
#[derive(Debug, Clone)]
pub enum McpServerEvent {
    ToolListChanged {
        server: String,
    },
    ResourceListChanged {
        server: String,
    },
    PromptListChanged {
        server: String,
    },
    ResourceUpdated {
        server: String,
        uri: String,
    },
    Progress {
        server: String,
        progress: f64,
        total: Option<f64>,
        message: Option<String>,
    },
}

/// Client-side handler that forwards notifications to a channel.
#[derive(Clone)]
pub struct McpClientService {
    server_name: String,
    events: Option<mpsc::UnboundedSender<McpServerEvent>>,
}

impl McpClientService {
    pub fn new(server_name: impl Into<String>, events: Option<mpsc::UnboundedSender<McpServerEvent>>) -> Self {
        Self {
            server_name: server_name.into(),
            events,
        }
    }

    pub fn noop() -> Self {
        Self {
            server_name: String::new(),
            events: None,
        }
    }
}

impl ClientHandler for McpClientService {
    fn on_tool_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let server = self.server_name.clone();
        let events = self.events.clone();
        async move {
            log::debug!("MCP tools/list_changed: server={server}");
            if let Some(tx) = events {
                let _ = tx.send(McpServerEvent::ToolListChanged { server });
            }
        }
    }

    fn on_resource_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let server = self.server_name.clone();
        let events = self.events.clone();
        async move {
            log::debug!("MCP resources/list_changed: server={server}");
            if let Some(tx) = events {
                let _ = tx.send(McpServerEvent::ResourceListChanged { server });
            }
        }
    }

    fn on_prompt_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let server = self.server_name.clone();
        let events = self.events.clone();
        async move {
            log::debug!("MCP prompts/list_changed: server={server}");
            if let Some(tx) = events {
                let _ = tx.send(McpServerEvent::PromptListChanged { server });
            }
        }
    }

    fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let server = self.server_name.clone();
        let events = self.events.clone();
        async move {
            log::debug!("MCP resources/updated: server={server} uri={}", params.uri);
            if let Some(tx) = events {
                let _ = tx.send(McpServerEvent::ResourceUpdated {
                    server,
                    uri: params.uri,
                });
            }
        }
    }

    fn on_cancelled(
        &self,
        _params: CancelledNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        std::future::ready(())
    }

    fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let server = self.server_name.clone();
        let events = self.events.clone();
        async move {
            log::debug!(
                "MCP progress: server={server} progress={} total={:?} message={:?}",
                params.progress,
                params.total,
                params.message
            );
            if let Some(tx) = events {
                let _ = tx.send(McpServerEvent::Progress {
                    server,
                    progress: params.progress,
                    total: params.total,
                    message: params.message,
                });
            }
        }
    }
}

/// Shared event bus for pooled MCP sessions.
#[derive(Clone, Default)]
pub struct McpEventBus {
    inner: Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<McpServerEvent>>>>,
}

impl McpEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_sender(&self, tx: mpsc::UnboundedSender<McpServerEvent>) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    }

    pub fn sender(&self) -> Option<mpsc::UnboundedSender<McpServerEvent>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
