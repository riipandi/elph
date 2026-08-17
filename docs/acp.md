# Agent Client Protocol (ACP)

Elph speaks ACP JSON-RPC 2.0 over stdio. **One process = one protocol version.**

```sh
elph acp --stdio                  # ACP v1 (stable)
elph acp --stdio --experimental   # ACP v2 (draft)
elph acp                          # alias of `elph acp --stdio`
elph acp --setup                  # Terminal Auth: interactive provider login
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
- **Authentication (required for the [ACP Registry](https://agentclientprotocol.com/get-started/registry)):** initialize advertises `authMethods`. v1: `authenticate` + `logout` (`agentCapabilities.auth.logout`). v2: `auth/login` + `auth/logout`. Methods: `existing-credentials` plus `openai`, `anthropic`, `xai`, `openrouter`, `github-copilot`. Login succeeds when the matching env var or `auth.json` entry is present. Only **privileged** methods need credentials: `session/new`, `session/load`, `session/resume`, `session/prompt`, `session/set_mode`, `session/set_config_option`. `initialize`, login/logout, `session/list`, `session/close`, `session/delete`, and `session/cancel` do not. If the connection has not logged out and keys already exist, privileged methods succeed **without** a separate authenticate call. Logout is **connection-scoped** (does not delete `auth.json`); open sessions are aborted and later privileged methods return `auth_required` until the client logs in again.
- No audio prompts.
- File reads: `file://` prompt links are hydrated from disk (v1 also uses client `fs/read_text_file` when advertised). Writes stay local. Elph does **not** call client `fs/write_text_file`.
- Shell runs **locally** (`shell_exec` / `shell_use`). The agent streams display-only `terminal_update` / `terminal_output_chunk` (command, cwd, base64 output, exit code or `SIGINT` on cancel). It does **not** advertise or call client `terminal/*`. Non-shell tools never get a terminal id.
- Prompt images are accepted and forwarded to the model when the session model supports image input.
- Slash commands advertised after session setup are a **headless subset** of builtins (no TUI pickers such as `/model`, `/resume`, `/memory`, `/hotkeys`) plus prompt templates and skills.
- Skills are advertised as `/skill:NAME` so they do not collide with prompt templates. Templates keep their raw name (e.g. `/review`). Invoking a skill runs `harness.skill` (full SKILL.md), not a raw `/skill:…` user prompt.
- After session open, MCP is attached **before** `available_commands_update`. The `list_available_tools` / `list_skills` catalogs are rebuilt from the live registry so lazy MCP tools and skills stay discoverable. `/tools` lists inactive (lazy) tools and loaded skills. `/reload` re-sends the slash catalog.
- `configOptions` advertise **model** (full live session catalog plus disk/embedded providers), **thought_level** (reasoning), and **mode** (ask/plan/build/brave).
- On v1, Zed-style `modes` / `session/set_mode` are thinking levels (`off` … `max`), matching pi-acp. Agent permission mode stays on `configOptions.mode`.
- Tool approval uses `session/request_permission` (allow once / session / all / reject).
- Agent `request_mode_change` uses `session/request_permission`, applies the mode, then replies `true`/`false` to the tool (same contract as the TUI).
- `ask_user_question` uses v2 `elicitation/create` forms when the client advertises form elicitation; otherwise each step uses `session/request_permission`.
- Client `mcpServers` on `session/new`, `session/load`, and `session/resume` are overlaid on `mcp.json` (home ← project ← client; same name wins) and bound into the session tool registry.
- `session/cancel` aborts the harness turn **and** marks in-flight tool calls cancelled (v2 status `cancelled`; v1 `failed` with output `cancelled`).
- A harness `RunCompleted` mid-turn (auto-retry after a stream cut, compact-then-resume) does **not** end the ACP prompt. The client stays in `running` until `submit_prompt` finishes. A failed `session/update` (tool chunk, terminal) is logged and ignored so the turn does not die mid tool-call.
- One turn owns the UI stream (`stream_gate`). A second `session/prompt` while a turn is running still **submits immediately** (steer) and does not steal `ui_rx`. `/aside` answers via a side completion and does not lock the event channel.
- `session/cancel`, `session/close`, and logout notify in-flight `request_permission` / elicitation so the harness is not left waiting. Stale UI drain keeps `*Required` events (never drops a pending tool approval).
- `session/list`, `session/close`, `session/delete`, `set_config`/`set_mode`, and logout run on a spawned task (same as `session/new`). MCP attach and slash catalog run **before** the `session/new` / `session/resume` / `session/load` response.
- ACP sessions are created `headless: true` (no TUI worker-inbox noise on the event channel).
- Agent/tool/terminal `session/update` text is capped (~16k characters) so a large dump cannot drop the client.
- Auto-retry after `RunCompleted` uses a new agent/thought message id.
- `session/cancel` notifies waiters on the I/O task, then aborts the harness on a spawned task.
- After a v2 prompt has been acked, a later failure still emits `idle` so the client is not left in `running`.

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
| `session/new` | Spawned off I/O. MCP attach and slash catalog run **before** the response; `config_option_update` may follow. |
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
- `session.prompt.embeddedContext` / `session.prompt.image`
- `session.mcp.stdio` / `session.mcp.http`
- `session.delete`
- `session.additionalDirectories`

Methods:

| Method | Notes |
|---|---|
| `initialize` | Protocol version 2 only |
| `session/new` | Spawned off I/O. MCP attach and slash catalog run **before** the response; full `config_option_update` may follow. |
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

## Later / client limitations

- **Extensions** — WASM extension slash commands are not listed in `available_commands_update`.
- **Client `terminal/*`** — not used. Local shell + display-only terminal updates only.
- **In-band OAuth on `authenticate`** — not used. First-run uses **Terminal Auth** (`elph acp --setup` → `elph provider connect`). After that, `authenticate` / `auth/login` with `existing-credentials` (or a provider id) checks env/`auth.json`.

Wire tests: `crates/coding-agent/tests/acp_wire.rs` (initialize, capability ads, relative-cwd, login, `session/new` list/close). Unit tests cover slash catalog, permission option mapping, and display-only terminal encoding. CLI flag tests: `tests/acp.rs`. There is no automated Zed UI smoke.

Registry submission: `docs/acp-registry/`.

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

Wire tests: `crates/coding-agent/tests/acp_wire.rs`. CLI flag tests: `tests/acp.rs`.
