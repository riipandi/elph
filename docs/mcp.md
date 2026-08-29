# MCP CLI

Elph stores MCP server definitions as JSON in two layers:

- `CONFIG_DIR/mcp.json` is the user-wide configuration.
- `<project>/.elph/mcp.json` is the project configuration and overrides a
  user definition with the same name.

## Commands

```sh
elph mcp list [--json] [--home|--project]
elph mcp add [OPTIONS] <NAME> [COMMAND_OR_URL] [ARGS]...
elph mcp remove <NAME> [--scope user|project] [--all]
elph mcp enable <NAME> [--scope user|project]
elph mcp disable <NAME> [--scope user|project]
elph mcp doctor [--json] [NAME]
elph mcp auth <NAME> [--scopes SCOPE...]
elph mcp logout <NAME>
```

`mcp add` defaults to a stdio server. Put the executable and its arguments
after `--` so server flags are not parsed by Elph:

```sh
elph mcp add filesystem -- npx -y @modelcontextprotocol/server-filesystem /tmp
elph mcp add postgres -e DATABASE_URL=postgres://localhost/db -- npx server-postgres
```

Remote servers can use streamable HTTP or legacy SSE:

```sh
elph mcp add --transport http sentry https://mcp.sentry.dev/mcp
elph mcp add --transport sse legacy https://example.test/sse
elph mcp add --transport http api https://example.test/mcp \
  --header 'Authorization: Bearer TOKEN'
```

`doctor` performs a live MCP discovery with a ten-second per-server timeout.
It exits non-zero when a configured and enabled server cannot be reached or
does not complete discovery. Disabled servers are reported without a
connection attempt. `--json` produces a report suitable for automation; it
does not include header or environment-variable values.

OAuth credentials are stored in the Elph auth store. Configure a remote server
with `"oauth": true` when it should use credentials created by `mcp auth`.
