# MCP Servers

MCP (Model Context Protocol) servers extend Elph with external tools.

## Config

Home: `CONFIG_DIR/mcp.json`  
Project: `<project>/.elph/mcp.json` (merged over home)

Typical shape:

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

## CLI

```sh
elph mcp list
elph mcp …
```

Session MCP caches live under `APP_DATA/projects/<SESSION_ID>/mcp_cache/`. Host-level
cache (no session): `APP_DATA/mcp_cache/`.

See repo `docs/mcp.md` for approval policy and tool wiring.
