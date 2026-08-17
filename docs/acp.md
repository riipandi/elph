# Agent Client Protocol (ACP)

Elph speaks **ACP v2 only** over stdio:

```sh
elph acp
```

The process is a JSON-RPC 2.0 agent. Clients (editors) must negotiate `protocolVersion: 2`. v1 initialize is rejected. There is no v1 fallback.

## Implementation

| Field | Value |
|---|---|
| `info.name` | `elph` |
| `info.title` | `Elph` |
| `info.version` | crate version |
| Transport | stdio |
| Code | `crates/coding-agent/src/platform/acp/` |

## Advertised capabilities

`capabilities.session` is present, so the baseline methods are implemented:

- `session/new`, `session/list`, `session/resume`, `session/close`
- `session/prompt`, `session/cancel`, `session/update`
- `session/delete` (`session.delete: {}`)
- `additionalDirectories` on new/resume
- Prompt: `text`, `resource_link` (baseline), plus `image` and `embeddedContext`
- MCP: `stdio` and `http` (client `mcpServers` on new/resume)
- `session/set_config_option` for `mode` and `model`

Not advertised: `authMethods` (credentials stay file/env), `session.prompt.audio`, unstable RFDs (fork, NES, MCP-over-ACP).

## Prompt lifecycle

1. Client sends `session/prompt`.
2. Agent responds `{}` immediately (acceptance, not turn end).
3. Agent emits `user_message` with an agent-owned `messageId`.
4. `state_update: running` while the turn works.
5. Output arrives as `session/update` (message/thought chunks, tool calls, plans, terminals, usage).
6. Idle `state_update` carries `stopReason` (`end_turn`, `cancelled`, …).

`session/cancel` aborts the harness turn and reports idle `cancelled`.

## Tools, plans, terminals

- File and MCP tools → `tool_call_update` (kind, locations, content).
- Shell tools also emit display-only `terminal_update` / `terminal_output_chunk`.
- Mutating tools request `session/request_permission` (never hang on a TUI oneshot).
- Todos → `plan_update` `{ type: "items", planId: "session-plan" }`.

## Slash commands

After `session/new` and `session/resume`, Elph sends `available_commands_update`. Commands run as ordinary `session/prompt` text (`/help`, `/compact`, skills, …). TUI-only commands are not advertised; if typed, they return an explanation, not a JSON-RPC error. Use `session/new` and `session/resume` instead of `/new` / `/resume`.

## Config options

| configId | category | values |
|---|---|---|
| `mode` | `mode` | `ask`, `plan`, `build`, `brave` |
| `model` | `model` | current `provider/model_id` |

## Breaking change

Previous `elph acp` spoke a thin ACP **v1** subset. Clients that only implement v1 will not connect. Point the editor at a v2 ACP client, or wait for that client to negotiate v2.
