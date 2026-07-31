//! MCP server notification events (list_changed, progress, tasks, etc.).

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::handler::client::ClientHandler;
use rmcp::model::{
    CancelledNotificationParam, ClientCapabilities, ClientInfo, ElicitRequestParams, ElicitResult, ElicitationAction,
    ElicitationCapability, ErrorData as McpError, FormElicitationCapability, Implementation, ProgressNotificationParam,
    ResourceUpdatedNotificationParam, TASKS_EXTENSION_ID, TaskStatusNotificationParams,
};
use rmcp::service::{NotificationContext, RequestContext, RoleClient};
use tokio::sync::mpsc;

use super::config::McpMrtrElicitationPolicy;

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
    /// SEP-2663 `notifications/tasks`.
    TaskStatus {
        server: String,
        task_id: String,
        status: String,
        status_message: Option<String>,
    },
}

/// Client-side handler that forwards notifications to a channel.
#[derive(Clone)]
pub struct McpClientService {
    server_name: String,
    events: Option<mpsc::UnboundedSender<McpServerEvent>>,
    mrtr_elicitation: McpMrtrElicitationPolicy,
}

impl McpClientService {
    pub fn new(server_name: impl Into<String>, events: Option<mpsc::UnboundedSender<McpServerEvent>>) -> Self {
        Self {
            server_name: server_name.into(),
            events,
            mrtr_elicitation: McpMrtrElicitationPolicy::Decline,
        }
    }

    pub fn with_mrtr_elicitation(mut self, policy: McpMrtrElicitationPolicy) -> Self {
        self.mrtr_elicitation = policy;
        self
    }

    pub fn noop() -> Self {
        Self {
            server_name: String::new(),
            events: None,
            mrtr_elicitation: McpMrtrElicitationPolicy::Decline,
        }
    }

    fn elph_client_info() -> ClientInfo {
        let mut caps = ClientCapabilities::default();
        // Advertise form elicitation so servers may attempt MRTR; policy decides accept/decline.
        caps.elicitation = Some(ElicitationCapability::new().with_form(FormElicitationCapability::new()));
        // Advertise Tasks extension support so servers may return task handles.
        let mut extensions = BTreeMap::new();
        extensions.insert(TASKS_EXTENSION_ID.to_string(), serde_json::Map::new());
        caps.extensions = Some(extensions);

        ClientInfo::new(
            caps,
            Implementation::new("elph", env!("CARGO_PKG_VERSION"))
                .with_title("Elph")
                .with_description("Elph MCP client (2026-07-28)"),
        )
        .with_protocol_version(rmcp::model::ProtocolVersion::V_2026_07_28)
    }
}

impl ClientHandler for McpClientService {
    fn get_info(&self) -> ClientInfo {
        Self::elph_client_info()
    }

    fn create_elicitation(
        &self,
        request: ElicitRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<ElicitResult, McpError>> + Send + '_ {
        let server = self.server_name.clone();
        let policy = self.mrtr_elicitation;
        async move {
            let message = match &request {
                ElicitRequestParams::FormElicitationParams { message, .. } => message.as_str(),
                ElicitRequestParams::UrlElicitationParams { message, url, .. } => {
                    log::info!("MCP elicitation URL mode: server={server} url={url}");
                    message.as_str()
                }
                _ => "(unknown elicitation mode)",
            };
            match policy {
                McpMrtrElicitationPolicy::Decline => {
                    log::info!("MCP elicitation declined (mrtrElicitation=decline): server={server} message={message}");
                    Ok(ElicitResult::new(ElicitationAction::Decline))
                }
                McpMrtrElicitationPolicy::Error => {
                    log::warn!("MCP elicitation rejected (mrtrElicitation=error): server={server} message={message}");
                    Err(McpError::invalid_request(
                        format!(
                            "MCP server \"{server}\" requested user elicitation during a tool call, \
                             but Elph mrtrElicitation=error. Use interactive auth/UI outside the agent, \
                             or set mrtrElicitation to \"decline\"."
                        ),
                        None,
                    ))
                }
            }
        }
    }

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

    fn on_task_status(
        &self,
        params: TaskStatusNotificationParams,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let server = self.server_name.clone();
        let events = self.events.clone();
        async move {
            let task_id = params.task.task.task_id.clone();
            let status = format!("{:?}", params.task.task.status);
            let status_message = params.task.task.status_message.clone();
            log::debug!("MCP task status: server={server} task_id={task_id} status={status}");
            if let Some(tx) = events {
                let _ = tx.send(McpServerEvent::TaskStatus {
                    server,
                    task_id,
                    status,
                    status_message,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_info_advertises_tasks_and_elicitation() {
        let info = McpClientService::elph_client_info();
        assert_eq!(info.client_info.name, "elph");
        assert!(info.capabilities.elicitation.is_some());
        assert!(info.capabilities.supports_tasks());
        assert_eq!(
            info.protocol_version.as_str(),
            rmcp::model::ProtocolVersion::V_2026_07_28.as_str()
        );
    }

    #[test]
    fn elicitation_policy_labels() {
        assert_eq!(McpMrtrElicitationPolicy::Decline.as_str(), "decline");
        assert_eq!(McpMrtrElicitationPolicy::Error.as_str(), "error");
    }
}
