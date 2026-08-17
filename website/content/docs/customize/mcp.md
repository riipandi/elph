# MCP

MCP (Model Context Protocol) servers extend Elph with external tools.

Home: `CONFIG_DIR/mcp.json`  
Project: `<project>/.elph/mcp.json` (merged over home)

```json
{
  "mcpServers": {
    "deepwiki": {
      "command": "npx",
      "args": ["-y", "@example/mcp-deepwiki"]
    }
  }
}
```

```sh
elph mcp list
elph mcp auth <name>
```

Remote MCP servers (`http` / `sse`) use browser PKCE. Credentials are sealed under `auth.json`. Session MCP caches live under `APP_DATA/sessions/<SESSION_ID>/mcp_cache/`.
