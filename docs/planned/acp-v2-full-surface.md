# ACP v2-only — full stable surface

Replace the v1 3-method adapter with a **v2-only** agent that implements the entire _stable_ ACP v2 surface from [overview](https://agentclientprotocol.com/protocol/v2/overview) and the linked pages (initialization, session setup/list/delete, prompt lifecycle, content, tool calls, plans, slash commands, config options, elicitation, cancellation, transports).

No v1, no `protocol_router`, no dual schema, no shims.

ACP stays in `coding-agent`. `elph-agent` stays protocol-agnostic; add host-facing hooks only when the adapter cannot attach client MCP servers, expand workspace roots, or replay history.

Unstable RFDs (session fork, NES, MCP-over-ACP, custom LLM providers, plan operations beyond `type: items`) stay out.

## Current v1 code to delete

- `Agent.builder()` + `schema::v1::*` in `crates/coding-agent/src/platform/acp/`
- Echo of `initialize.protocol_version`
- Empty `AgentCapabilities::new()`
- `session/prompt` held until `RunCompleted` + `PromptResponse { stopReason: end_turn }`
- Only `AgentMessageChunk`; all other `AgentUiEvent` dropped
- Silent drop of non-text prompt blocks
- Slash path that returns JSON-RPC errors for TUI-only commands
- ACP sessions with `headless: false` that hang on `ToolApprovalRequired`

Files: `platform/acp/{mod,handler,util}.rs`. Keep `cli/acp.rs` as the stdio entry.

## SDK

```toml
agent-client-protocol = { version = "2.0.0", features = ["unstable_protocol_v2"] }
```

`Agent.v2()` + `schema::v2` + `ProtocolVersion::V2` only. SDK rejects v1 initialize when no v1 impl is registered.

Enable `unstable_elicitation` **if** the crate gates `elicitation/create` types behind it. Do not enable `unstable_session_fork`, `unstable_mcp_over_acp`, `unstable_nes`, `unstable_auth_methods`.

---

## Capability matrix (this is the work)

Every row is required unless marked _N/A (honest omit)_. Advertise only implemented capabilities.

### 1. Initialization

| Spec                              | Implementation                                                                                  |
| --------------------------------- | ----------------------------------------------------------------------------------------------- |
| `initialize`                      | `Agent.v2()` handler. Always `protocolVersion: 2`.                                              |
| Required `info`                   | `name: "elph"`, `title: "Elph"`, `version: CARGO_PKG_VERSION`                                   |
| `capabilities.session: {}`        | Present → commits to baseline methods below                                                     |
| `authMethods`                     | Omit / `[]`. Elph auth is file/env, not ACP. Do **not** implement `auth/login` / `auth/logout`. |
| Client `capabilities.elicitation` | Store on the connection; use form/url only when advertised                                      |

### 2. Session lifecycle (baseline + delete + extra roots)

| Method           | Implementation                                                                                                                                                                                                                                                                           |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `session/new`    | Absolute `cwd` required. `create_coding_session_with_events(resume_id: None)`. Attach `mcpServers`. Honor `additionalDirectories` (advertise `session.additionalDirectories`). Return `sessionId` + full `configOptions`. Then push `available_commands_update` + `session_info_update`. |
| `session/list`   | `SessionManager` / Turso list. Filter by `cwd`. Cursor pagination (opaque offset). `SessionInfo`: `sessionId`, absolute `cwd`, `title`, `updatedAt`, `additionalDirectories` if any.                                                                                                     |
| `session/resume` | Open existing id. Request `cwd` must match stored cwd. `mcpServers` + `additionalDirectories` re-applied in full (omit/`[]` = no extra roots). `replayFrom` omitted/`null` = no replay. `{ type: "start" }` = emit history as `session/update` **before** the resume response.           |
| `session/close`  | Abort turn, cancel pending permissions, drop in-memory session, release lease. Persist store rows stay.                                                                                                                                                                                  |
| `session/delete` | `delete_by_id`. Advertise `session.delete: {}`. Already-deleted / unknown → empty success. Active session → close then delete.                                                                                                                                                           |
| `session/cancel` | See §5                                                                                                                                                                                                                                                                                   |

**additionalDirectories:** treat `[cwd, ...dirs]` as the effective root set. Pass extra absolute roots into the session env / tool policy as allowed roots (extend `LocalExecutionEnv` or a thin host wrapper if it is cwd-only today). Relative paths in tools still resolve against `cwd`.

**Client MCP:** map v2 `type: stdio` / `type: http` to `McpServerConfig`. Merge with Elph local MCP and hot-attach. Reject SSE. Advertise `session.mcp.stdio` + `session.mcp.http`.

### 3. Prompt lifecycle (breaking vs current code)

`session/prompt` **MUST** return `{}` as soon as the prompt is accepted. Completion is only `state_update: idle` + `stopReason`.

Sequence per spec:

1. Validate session + content types vs advertised prompt capabilities.
2. `respond(PromptResponse {})` immediately.
3. `user_message` upsert with agent-owned `messageId` (source of truth).
4. `state_update: running` when foreground work starts.
5. Stream output (messages, thoughts, plan, tools, terminals, usage).
6. Permission / elicitation as needed → `requires_action` while blocked, then `running`.
7. Idle + `stopReason` (`end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`). Never hardcode `end_turn`.

Second prompt while `running` → enqueue as steer (`submit_prompt(..., true)`); still ack `{}` immediately.

`messageId` is required on every message update and chunk. One id per user / agent / thought message; chunks reuse it.

### 4. Content (prompt + output)

Advertise `session.prompt.image` and `session.prompt.embeddedContext`. Baseline `text` + `resource_link` always on.

| Block                 | Prompt in                                          | Output out                    |
| --------------------- | -------------------------------------------------- | ----------------------------- |
| `text`                | Join into user turn                                | `agent_message` / `_chunk`    |
| `resource_link`       | URI + name + mime into prompt (MUST)               | forward if a tool returns one |
| `resource` (embedded) | Inline text; blob as a short note                  | same                          |
| `image`               | `elph_ai::ContentBlock::Image` (already supported) | tool/image content blocks     |
| `audio`               | **Do not advertise.** Reject with `-32602` if sent | N/A                           |

Unknown non-`_` content types: preserve when proxying internally; do not fail the whole prompt if they appear only in tool output. On `session/prompt`, reject unavertised types instead of silent drop.

### 5. Cancellation

- Handle `session/cancel`: `CodingAgentSession::abort`, mark in-flight tools `cancelled`, resolve pending `request_permission` waiters as `cancelled` (client should also respond `cancelled`; we still abort locally).
- Then idle `state_update` with `stopReason: cancelled`. Catch abort errors so they are not generic failures.
- Updates MAY continue after cancel until that idle notification.
- Honor `$/cancel_request` on in-flight Agent→Client requests (permissions/elicitation) via the SDK; on `session/cancel` also drop our side of those requests.

### 6. Tool calls (full)

Map `ToolStart` / `ToolUpdate` / `ToolEnd` / `ToolApprovalRequired`:

- First sight of `toolCallId` → `tool_call_update` with `title`, `kind`, `status: pending`, `rawInput`, `locations` when a path is known (absolute, 1-based line).
- Start exec → `in_progress`.
- Incremental output → `tool_call_content_chunk`.
- Done → `completed` / `failed` / `cancelled` + `rawOutput`. `content` replace vs chunk append per spec.

**Kinds:** `read` (read_file, list_dir), `edit` (edit_file, write_file), `delete`, `move` (move/copy), `search` (grep, find, web_search), `execute` (shell), `fetch` (web_fetch/extract), `think` (todos / plan tools), else `other`. Custom `_` only if we invent Elph-only kinds.

**Locations:** every file tool includes `{ path, line? }` so IDEs can follow-along.

**Diffs:** `edit_file` / `write_file` / `delete_path` / `move_path` emit `ToolCallContent::diff` with v2 `changes` (`add`/`delete`/`modify`/`move`/`copy`, absolute paths, `fileType`) and `patch: { format: git_patch, text }` when both sides are text.

**Display-only terminals (required, not optional):** shell tools (`shell_exec` / `shell_use`) also:

1. Allocate a session-unique `terminalId` (do not reuse).
2. `terminal_update` with `command`, absolute `cwd`.
3. Stream `terminal_output_chunk` (independently base64-encoded bytes).
4. Final `terminal_update.exitStatus` (`exitCode` / `signal`).
5. A `terminal` content ref on the tool call.

This is Agent-owned display only — no Client `terminal/*` (removed in v2).

**Permission:** `session/request_permission` with required `title`, optional `description`, `subject`:

- File/MCP tools → `{ type: "tool_call", toolCall: { toolCallId, title, kind, status, rawInput } }`
- Shell → `{ type: "command", command, cwd (absolute), toolCallId, terminalId? }`

Options: `allow_once` (Approve), `allow_always` (Allow session / Allow all), `reject_once`, `reject_always`. Unknown outcome **MUST NOT** approve. While waiting: `requires_action`.

Do not auto-approve by setting `headless: true`. Brave mode still uses existing policy.

### 7. Agent plan (full `items`)

`TodoUpdated` → `plan_update`:

```json
{ "sessionUpdate": "plan_update",
  "plan": { "type": "items", "planId": "<session-plan-id>",
            "entries": [{ "content", "priority", "status" }] } }
```

- One `planId` per session todo list (stable). Replace the full `entries` array each time (v2 rule).
- Map todo status → `pending` / `in_progress` / `completed` / `cancelled`.
- Priority: Elph todo priority if present, else `medium`.
- `PlanConfirmationRequired` → permission (`title` = confirm plan) then continue; on reject, mark entries `cancelled`.

No unstable plan types (markdown / file-backed).

### 8. Slash commands (advertise + run)

After `session/new` and `session/resume`, send `available_commands_update`.

Advertise every command the ACP session can actually run:

- Builtins that produce text or session actions: `help`, `tools`, `session`, `rename`, `compact`, `continue`, `reload`, `goal`, `settings`, `changelog`, `hotkeys`, `workers`, `tree`, `export`, `import`, `trust`, `fork`, `clone`, `aside`, `mcp` (list/logout), `provider` (list), plus **skills / prompt templates / extensions** (they already load via `create_coding_session_with_events`).
- `input: { type: "text", hint }` when the command takes args (v2 discriminator is required).

Do **not** advertise TUI-only commands: confetti, overlays, `/intercom`, `/feedback`, `/provider connect|disconnect|update`, `/mcp auth`, `/handover`, `/new`, `/resume` (clients have `session/new` / `session/resume`).

Running: command text arrives as a normal `session/prompt` (`/name args`). Dispatch like today, but:

- Always ack `{}` first, emit `user_message`, then result as `agent_message`, then idle `end_turn`.
- Unknown `/foo` that is not a slash → send to the model (current `None` branch).
- TUI-only if typed anyway → **text explanation**, never JSON-RPC error.

Update the list if skills/MCP tools change mid-session (reload).

### 9. Session config options

Return on `session/new` and `session/resume`. Implement `session/set_config_option`. Push `config_option_update` (full array) on agent-initiated changes.

| configId        | category        | type   | values                                |
| --------------- | --------------- | ------ | ------------------------------------- |
| `mode`          | `mode`          | select | `ask`, `plan`, `build`, `brave`       |
| `model`         | `model`         | select | live registry (`provider/model_id`)   |
| `thought_level` | `thought_level` | select | model-supported levels (omit if none) |

Defaults always present so a client that ignores config still works. Response to set is the **complete** option list (model change may rewrite thought_level options). Wire `type: "id"` vs `"boolean"` as in the spec.

### 10. Elicitation (Client optional method)

When Client advertised `capabilities.elicitation.form`, map `UserQuestionRequired` / `ask_user_question` to `elicitation/create` (`mode: form`, `sessionId`, `toolCallId` if any, restricted JSON Schema from the question steps).

Fallback if form is not advertised: `session/request_permission` with the choices as options.

URL mode: use only if advertised, and only for real out-of-band OAuth (`/mcp auth`, provider connect). After the user accepts the URL, listen for `elicitation/complete`. Never put secrets in ACP or the model context. If URL is not advertised, fail those flows with a text message pointing at `elph mcp auth` / `elph provider` — do **not** fall back to form for secrets.

### 11. Other `session/update` variants

| Update                      | When                                                   |
| --------------------------- | ------------------------------------------------------ |
| `usage_update`              | `RunCompleted.usage` → `used`, `size`, optional `cost` |
| `session_info_update`       | Title generation / rename (`title`, `updatedAt`)       |
| `config_option_update`      | Mode/model/thought changes from the agent              |
| `available_commands_update` | Session start + after `/reload`                        |

### 12. Transports / errors / conventions

- stdio JSON-RPC 2.0. SDK already accepts batches; do not batch lifecycle methods ourselves.
- All paths absolute; lines 1-based.
- camelCase properties, snake_case discriminators.
- JSON-RPC errors for real failures (`session not found`, invalid cwd, unavertised content). Notifications never get responses.
- `_meta` allowed on updates we emit (e.g. Elph session extras); no custom `_` methods in this task.

---

## Module layout

Rewrite in place:

```
crates/coding-agent/src/platform/acp/
  mod.rs           Agent.v2() registration, run_agent_stdio
  capabilities.rs  info + advertised capabilities (from client init)
  state.rs         sessions, message/tool/terminal ids, foreground state, cancel
  session.rs       new / resume / close / list / delete / additional roots
  prompt.rs        accept, slash vs turn, cancel, stop reasons
  updates.rs       AgentUiEvent → SessionUpdate
  tools.rs         tool_call_update, kinds, locations, diffs
  terminals.rs     display-only terminal_update / chunks
  plan.rs          plan_update from todos
  permission.rs    request_permission (tool_call + command subjects)
  elicitation.rs   form/url create + complete
  content.rs       ContentBlock conversion
  commands.rs      available_commands_update + ACP slash set
  config.rs        configOptions + set_config_option
  mcp.rs           client mcpServers → McpServerConfig
  replay.rs        history → session/update
```

`elph-agent` only if needed: extra MCP servers per session; extra workspace roots; iterate persisted messages for replay.

## Tests (`crates/coding-agent/tests/acp.rs`)

Use `Client.v2()` against in-process `Agent.v2()` (or piped stdio). Cover:

1. initialize → v2 + `info` + `capabilities.session`
2. v1 initialize rejected
3. prompt ack `{}` before idle; `user_message` + `running` + `messageId` chunks + idle `end_turn`
4. `session/cancel` → idle `cancelled`; in-flight tool → `cancelled`
5. mutating tool → `request_permission` (client grants) → `tool_call_update` + locations
6. shell → `terminal_update` / `terminal_output_chunk` + command permission subject
7. edit → v2 `diff` (`changes` + `git_patch`)
8. todos → `plan_update` items + `planId`
9. `available_commands_update` includes `/help`; `/help` does not hang the prompt RPC
10. `session/list` + resume no-replay + `replayFrom.start`
11. `session/close` then prompt → not found; `session/delete` drops from list
12. `set_config_option` mode/model returns full options
13. image + embedded resource accepted when advertised; audio rejected
14. ask-user: elicitation form when client advertises it; permission fallback otherwise

`make test-elph` (not raw cargo). Then `make check` / `make lint`.

## Docs (public protocol change)

- New `docs/acp.md`: `elph acp`, v2-only, full method/capability table, config options, slash subset, permission, terminals, elicitation, editor requirements.
- `assets/user-guide/01-getting-started.md` — ACP v2, not v1.
- `docs/session-persistence.md` — resume/list/delete via ACP.
- `crates/coding-agent/CHANGELOG.md` — breaking: v1 clients will not connect.

## Implementation order

1. Feature flag + delete v1 types; `Agent.v2()` initialize + stub session methods that compile
2. `session/new` + prompt accept/idle + text/thought streaming + `messageId`
3. `session/cancel` + permission bridge (unblocks tools)
4. Full tool mapping: kinds, locations, diffs, terminals
5. Plans + slash advertise/run + config options
6. list / resume / replay / close / delete + additionalDirectories + client MCP
7. Elicitation form (+ URL only for MCP/provider OAuth if client supports it)
8. Tests + `docs/acp.md` + changelog
9. `make check` / `make lint` / `make test-elph`

## Honest omissions (do not advertise)

| Surface                      | Why                                                         |
| ---------------------------- | ----------------------------------------------------------- |
| `auth/login` / `auth/logout` | No ACP auth methods; credentials stay file/env              |
| `session.prompt.audio`       | No audio pipeline                                           |
| Unstable RFDs                | fork, NES, MCP-over-ACP, custom providers, extra plan types |

Everything else on the v2 overview and its child pages is in scope.

## Risk

v2 schema is still draft behind `unstable_protocol_v2`. Pin the crate. Most production editors still speak v1 — this is an explicit break, documented in the changelog.
