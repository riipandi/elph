//! Map client `mcpServers` onto Elph MCP configs (best-effort attach).

use agent_client_protocol::schema::v2::McpServer;
use elph_agent::McpServerConfig;

pub fn map_servers(servers: &[McpServer]) -> Vec<(String, McpServerConfig)> {
    let mut out = Vec::new();
    for server in servers {
        match server {
            McpServer::Stdio(stdio) => {
                out.push((
                    stdio.name.clone(),
                    McpServerConfig::stdio(stdio.command.0.display().to_string(), stdio.args.clone()),
                ));
            }
            McpServer::Http(http) => {
                out.push((http.name.clone(), McpServerConfig::http(http.url.clone())));
            }
            McpServer::Other(other) => {
                log::warn!("ignoring unsupported ACP MCP transport '{}'", other.type_);
            }
            _ => {}
        }
    }
    out
}
