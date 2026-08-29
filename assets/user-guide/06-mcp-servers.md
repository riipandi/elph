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
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@example/mcp-deepwiki"]
        }
    }
}
```

## CLI

```sh
elph mcp list                         # merged user + project config
elph mcp add filesystem -- npx -y @modelcontextprotocol/server-filesystem /tmp
elph mcp add --transport http sentry https://mcp.sentry.dev/mcp
elph mcp add --scope project local -- ./scripts/mcp-server
elph mcp enable filesystem
elph mcp disable filesystem
elph mcp remove filesystem
elph mcp doctor                       # connect and list tools from each server
elph mcp auth <name>                  # OAuth for an HTTP/SSE server
elph mcp logout <name>
```

`mcp add` uses stdio by default. A positional `http://` or `https://` URL is
recognized as streamable HTTP; use `--transport sse` for legacy SSE. Repeat
`--env KEY=value` for stdio environment variables and `--header "Name: value"`
for HTTP/SSE headers. Use `--json` with `list` or `doctor` for scripting.

Session MCP caches live under `APP_DATA/sessions/<SESSION_ID>/mcp_cache/`. Host-level
cache (no session): `APP_DATA/mcp_cache/`.

See repo `docs/mcp.md` for approval policy and tool wiring.
