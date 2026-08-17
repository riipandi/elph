# Agent Client Protocol (ACP)

Elph speaks ACP JSON-RPC 2.0 over stdio. **One process = one protocol version.**

```sh
elph acp --stdio                  # ACP v1 (stable)
elph acp --stdio --experimental   # ACP v2 (draft)
elph acp                          # alias of `elph acp --stdio`
```

`--experimental` requires `--stdio`. There is no mixed-version stream and no `protocol_router` on one connection.

## When to use which

| Command | Protocol | Typical clients |
|---|---|---|
| `--stdio` | v1 (stable) | Zed and editors that speak ACP 1 |
| `--stdio --experimental` | v2 (draft) | Clients that send `protocolVersion: 2` |

A v2 `initialize` against a v1 process is rejected, and the reverse.

Elph’s wire shape follows the same product conventions as [pi-acp](https://github.com/svkozak/pi-acp): advertise only implemented capabilities, send `available_commands_update` after session setup, expose model + thinking as session configuration, and treat v1 `modes` as thinking levels for Zed’s mode picker.

## Shared behavior

- Working directory must be an **absolute** path.
- Resume `cwd` must match the stored session cwd.
- No ACP auth methods (credentials stay file/env).
- No audio prompts.
- No Client `fs/*` or `terminal/*` execution (Elph uses its own tools).
- Slash commands advertised after session setup are ACP-safe builtins **plus** the session’s prompt templates and skills (same catalog as the TUI palette). Dispatch uses that catalog.
- `configOptions` advertise **model**, **thought_level** (reasoning), and **mode** (ask/plan/build/brave).
- On v1, Zed-style `modes` / `session/set_mode` are thinking levels (`off` … `max`), matching pi-acp. Agent permission mode stays on `configOptions.mode`.
- Tool approval uses `session/request_permission`.
- Client `mcpServers` on `session/new`, `session/load`, and `session/resume` are overlaid on `mcp.json` (home ← project ← client; same name wins) and bound into the session tool registry.
- `session/cancel` aborts the harness turn **and** marks in-flight tool calls cancelled (v2 status `cancelled`; v1 `failed` with output `cancelled`).

## v1 capabilities

`agentInfo`: `elph` / `Elph` / crate version.

- `loadSession`
- `promptCapabilities.embeddedContext`
- `mcpCapabilities.http` / `sse` (stdio is required by the spec)
- `sessionCapabilities.list` / `resume` / `close`

Methods:

| Method | Notes |
|---|---|
| `initialize` | Echoes protocol version 1 |
| `session/new` | Answered first, then commands/MCP attach (keeps stdio alive) |
| `session/load` | Replays history, then same extras as new |
| `session/resume` | Reattach without full replay |
| `session/list` | Cursor pagination (page size 50) |
| `session/close` | Abort + drop in-memory session |
| `session/delete` | Close + delete stored session |
| `session/prompt` | **Held** until `PromptResponse { stopReason }` |
| `session/cancel` | Abort + cancel open tools |
| `session/set_mode` | Thinking level |
| `session/set_config_option` | `model`, `thought_level`, or `mode` |

Updates: `agent_message_chunk` / `agent_thought_chunk`, `tool_call` then `tool_call_update`, `plan`, `available_commands_update`, `current_mode_update`.

## v2 capabilities

`info`: same implementation fields.

- `capabilities.session` (baseline methods)
- `session.prompt.embeddedContext`
- `session.mcp.stdio` / `session.mcp.http`
- `session.delete`

Methods:

| Method | Notes |
|---|---|
| `initialize` | Protocol version 2 only |
| `session/new` | Answered first; `available_commands_update` + MCP attach after |
| `session/list` | Same listing as v1 |
| `session/resume` | Optional `replayFrom: start` |
| `session/close` / `session/delete` | Same host as v1 |
| `session/prompt` | Ack `{}` **after** validation, then `state_update` |
| `session/cancel` | Abort + cancel open tools + idle `cancelled` |
| `session/set_config_option` | `model`, `thought_level`, or `mode` |

Updates: `user_message` / `agent_message` (required `messageId`), `tool_call_update` only, `plan_update`, `available_commands_update`, `state_update` (`running` / `requires_action` / `idle`), `config_option_update`.

## Client `mcpServers`

Advertised transports:

- **stdio** — `command`, `args`, `env`
- **http** — `url`, `headers`
- **sse** (v1, or v2 `type: "sse"`) — mapped to Elph’s SSE MCP transport

Unsupported transports (including ACP-over-MCP) are ignored with a warning. A failed attach does not fail `session/new`; the session stays up without those tools.

## Cancellation

1. Client sends `session/cancel`.
2. The session cancel flag is set and `CodingAgentSession::abort` runs.
3. Every tool id still in the open-tool set gets a terminal `tool_call_update`.
4. v2 also emits idle `stopReason: cancelled`. v1 completes the held prompt with `stopReason: cancelled` when the stream notices the flag.

## Later (not advertised)

- **Elicitation** — structured `session/elicitation` forms. User questions currently fall back to `session/request_permission`.
- **Extensions** — WASM extension slash commands are not listed in `available_commands_update`.
- **Images** — not advertised; `submit_prompt` is text-only.
- **`additionalDirectories`** — accepted on the wire but not applied as extra tool roots.
- **ACP `fs/*` / `terminal/*`** — not used.

## Code

`crates/coding-agent/src/platform/acp/`

| Path | Role |
|---|---|
| `mod.rs` | `AcpMode`, stdio / in-process transport, v2 handlers |
| `v1/` | v1 Agent builder |
| `session.rs` | new / load / resume / list / close / delete |
| `mcp.rs` | Map + attach client MCP servers |
| `commands.rs` | Slash catalog + dispatch |
| `config.rs` | model / thought_level / mode |
| `tools.rs` | Tool updates + open-tool cancel |
| `prompt.rs` | v2 prompt ack + cancel |
| `updates.rs` | v2 `session/update` stream |

Wire tests: `crates/coding-agent/tests/acp_wire.rs` (initialize + capability ads + relative-cwd error + cancel-then-new). CLI flag tests: `tests/acp.rs`.
