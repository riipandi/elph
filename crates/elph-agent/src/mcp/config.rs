//! MCP server configuration types.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpServerConfig {
    #[serde(rename = "stdio")]
    Stdio(McpStdioConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpStdioConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl McpServerConfig {
    pub fn stdio(command: impl Into<String>, args: Vec<String>) -> Self {
        Self::Stdio(McpStdioConfig {
            command: command.into(),
            args,
            env: BTreeMap::new(),
        })
    }
}
