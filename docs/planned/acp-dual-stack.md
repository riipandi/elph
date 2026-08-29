# ACP v1 stable + v2 experimental — production-ready

Official guidance ([Migrating from v1](https://agentclientprotocol.com/protocol/v2/migration)): keep v1; treat v2 as additive and **gate it** until it stabilizes. One process speaks **exactly one** protocol version. We do **not** use `protocol_router` to mix versions on one stdio stream — the operator picks the version on the CLI.

## CLI

```sh
elph acp --stdio                  # ACP v1 (stable)
elph acp --stdio --experimental   # ACP v2 (draft / experimental)
```

- `--stdio` is the only transport (required). Missing it: clap error + usage (no silent default to a second transport).
- Bare `elph acp` (no flags) is an **alias of** `elph acp --stdio` so existing editor configs keep working.
- `--experimental` without `--stdio` is invalid.
- Each process answers only that version: v1 `initialize` against `--experimental` fails; v2 initialize against stable `--stdio` fails. No silent downgrade.

Current `elph acp` is v2-only and incomplete. This work: (1) CLI version split, (2) shared host, (3) production-ready v1 (default), (4) hardened v2 behind `--experimental`, (5) tests + honest docs.

No `fs/*` or Client `terminal/*` execution. Auth omitted (`authMethods` empty).

---

## Code structure (clean / maintainable)

Shared application logic once. Version modules are thin wire adapters only — no harness calls in `v1/` or `v2/` except through `host`.

```text
crates/coding-agent/src/cli/acp.rs     clap: --stdio, --experimental → AcpMode
crates/coding-agent/src/platform/acp/
  mod.rs          AcpMode { V1, V2 } + run_agent_stdio(mode)
  host.rs         sessions, open/resume/close/delete/list, MCP attach,
                  extra roots, prompt run, cancel, slash
  content.rs      extract text/image/resource
  slash.rs        advertised commands + dispatch
  replay.rs       typed history (not Debug)
  stop.rs         harness stop → StopReason
  emit.rs         trait SessionSink { … }
  v1/
    mod.rs        Agent.builder() + connect Stdio
    sink.rs       V1Sink: schema::v1 updates / prompt held
    handlers.rs   initialize, new, load, resume, prompt, cancel, …
  v2/
    mod.rs        Agent.v2() + connect Stdio
    sink.rs       V2Sink: schema::v2 updates / prompt ack
    handlers.rs   initialize, new, resume, prompt, cancel, …
```

Move today’s flat `platform/acp/*.rs` into `host` + `v2/`. Do not leave a third copy of slash/MCP/session maps.

`emit::SessionSink` is the only place v1 vs v2 wire shapes diverge:

| Event            | v1 wire                                              | v2 wire                                          |
| ---------------- | ---------------------------------------------------- | ------------------------------------------------ |
| Prompt RPC       | Hold until turn end; `PromptResponse { stopReason }` | Immediate `{}`; idle `state_update` + stopReason |
| User message     | implicit / optional chunk                            | `user_message` + `messageId` **MUST**            |
| Agent text       | `agent_message_chunk` (`messageId` optional)         | chunk **or** upsert; `messageId` **MUST**        |
| Tools            | `tool_call` then `tool_call_update`                  | first `tool_call_update` creates                 |
| Plan             | `plan` (flat entries)                                | `plan_update` `{ type: items, planId }`          |
| Mode             | `session/set_mode` + `current_mode_update`           | `set_config_option` + `config_option_update`     |
| Permission       | `toolCall` in params                                 | required `title` + `subject`                     |
| Load/replay      | `session/load`                                       | `session/resume` + `replayFrom.start`            |
| Resume no replay | `session/resume`                                     | `session/resume` omit `replayFrom`               |
| Slash input      | `{ hint }`                                           | `{ type: text, hint }`                           |
| Diff             | `oldText`/`newText`                                  | `changes` + `git_patch`                          |
| Terminal         | omit (no Client terminal API)                        | Agent-owned `terminal_update` / chunks           |
| Cancel confirm   | prompt result `cancelled`                            | idle `state_update` `cancelled`                  |

Capabilities advertised **only** for implemented behavior (migration rule 1).

---

## Shared host (production holes in today’s v2)

### Prompt accept vs validate

- **v2:** validate session + content **before** `respond({})`. Unknown session / unavertised content → JSON-RPC error, no ack.
- **v1:** same validation; then hold the responder until idle.

### Images

If advertised, pass `extracted.images` into the user turn (`elph_ai::ContentBlock::Image`). If the selected model has no image input, reject the prompt (`-32602`) instead of silently dropping.

### Client MCP

`mcp::map_servers` must **attach**: merge into session MCP registry / `harness.set_tools` (same path as `mcp_bootstrap::apply_mcp_tools_to_harness`). v1 stdio configs have no `type`; v2 requires `type: stdio` / `http`. Do not advertise SSE. Log + skip unknown `_` transports.

### additionalDirectories

Advertise only after extra absolute roots are actually applied (env root set / tool policy allow-list). Resume: omit/`[]` = no extra roots (spec). `cwd` on resume **MUST** match stored cwd.

### Replay

Walk `session_entries()`. Emit real user/assistant text (not `Debug`). Replay tool snapshots and last plan. v2: `user_message` / `agent_message` upserts + `tool_call_update` snapshots. v1: `session/load` uses `user_message_chunk` / `agent_message_chunk` / `tool_call` as the v1 load docs require. Do this **before** the load/resume response.

### Diffs

Build real diffs from `ToolEnd.details` (`old_content` / `new_content` / path):

- v2: `ToolCallContent::Diff` with `changes` + `patch.git_patch` (absolute paths, no commit envelope).
- v1: `oldText` / `newText` on the tool-call content.

### Tools / terminals

- Kind from exact tool name table (`read_file` → read, `shell_exec`/`shell_use` → execute), not substring `exec`.
- Locations from structured args/details, not “first token starting with `/`”.
- Terminal stream **only** for execute tools.
- Track open `toolCallId`s; on cancel mark them `cancelled` (v2) or failed/cancelled (v1) **before** the stop reason.

### Stop reasons

Map harness `StopReason` / errors: `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`. Never hardcode `end_turn`.

### Cancel

Set cancelled flag, `abort()`, flush pending tool updates, resolve permission waiters as cancelled, then:

- v1: complete the pending `session/prompt` with `stopReason: cancelled`
- v2: idle `state_update` + `cancelled`

Do not emit idle/response until abort finishes.

### Config

Expose `mode`, `model` (full registry list, not only current), `thought_level` when the model supports it. v1 also implements `session/set_mode` + `current_mode_update` (v1 modes API). v2 only config options.

### Message IDs

Unique per user/agent/thought message. Slash results get a fresh id (today `"msg_slash"` overwrites).

### Plan confirm

Honor allow/reject (today the outcome is ignored).

### Honest capabilities

**v2** (`capabilities.session: {}` + extras we actually do):

- `prompt.image`, `prompt.embeddedContext` (after image wiring)
- `mcp.stdio`, `mcp.http` (after attach)
- `delete`, `additionalDirectories` (after roots)

**v1** (`agentCapabilities`):

- `loadSession: true`
- `promptCapabilities.image` / `embeddedContext`
- `mcpCapabilities.http` + stdio (no sse)
- `sessionCapabilities.list` / `resume` / `close`
- `agentInfo` name/title/version

Do not advertise image/MCP/extra roots until the host path exists.

---

## v1 surface to implement

Baseline + the optional methods Elph can honor:

| Method                              | Behavior                                                  |
| ----------------------------------- | --------------------------------------------------------- |
| `initialize`                        | Echo v1; return capabilities above                        |
| `session/new`                       | Shared host open; `mcpServers` (v1 required even if `[]`) |
| `session/load`                      | Open + full replay + ready                                |
| `session/resume`                    | Open, no replay                                           |
| `session/list` / `close` / `delete` | Shared host                                               |
| `session/prompt`                    | Hold RPC; stream updates; respond `stopReason`            |
| `session/cancel`                    | Abort; prompt result `cancelled`                          |
| `session/set_mode`                  | Map ask/plan/build/brave; `current_mode_update`           |
| `session/set_config_option`         | Same host as v2                                           |
| `session/request_permission`        | v1 params (`toolCall`); same allow/reject options         |

No `authenticate` unless we add `authMethods`.

---

## Tests (`tests/acp.rs` + unit tests)

Use `Client.builder()` (v1) and `Client.v2()` against in-process agents (or a piped `elph acp --stdio` / `--stdio --experimental`), **not** a live LLM.

**CLI / mode**

1. `elph acp --stdio` initialize → `protocolVersion == 1`
2. `elph acp --stdio --experimental` initialize → `protocolVersion == 2`
3. v2 initialize against v1 process is rejected (and the reverse)
4. `--experimental` without `--stdio` fails clap

**Shared / v2 (fix the holes)** 4. Prompt to unknown session → error, **no** empty ack 5. Valid prompt → `{}` then `user_message` + `running` + idle 6. Cancel → idle `cancelled`; in-flight tool `cancelled` 7. Permission grant → tool completes 8. `replayFrom.start` uses real text, not `Debug` 9. Resume cwd mismatch → error 10. Client `mcpServers` stdio/http appear as tools (or documented attach failure) 11. Image prompt accepted only when model supports it 12. Edit details → structured v2 diff 13. `/help` advertised; prompt RPC not held

**v1** 14. Prompt RPC stays pending until `stopReason` 15. Cancel → that response is `cancelled` 16. `session/load` replays then returns 17. `session/set_mode` changes mode

`make check-elph`, `make lint-elph`, targeted `cargo test -p elph --test acp` (plus lib unit tests). Do not require a full `make test-elph` unless already cheap.

---

## Docs

Update `docs/acp.md`: `elph acp --stdio` = v1 stable, `--experimental` = v2 draft; capability tables; what we do not implement (`fs/*`, auth, audio). Changelog + user-guide match the flags. Editor configs should use `--stdio` unless they speak v2.

---

## Implementation order

1. Clap: `Acp { stdio, experimental }` → `AcpMode`; `run_agent_stdio(mode)`.
2. Extract `host` + `SessionSink`; move current files into `v2/` + host.
3. Fix host holes (validate-before-ack, images, MCP attach, roots, replay, diffs, kinds, cancel, stop reasons, plan confirm, config).
4. Harden `v2/` (`Agent.v2()`), selected only by `--experimental`.
5. Add `v1/` (`Agent.builder()`, held prompt, `session/load`, `set_mode`) for `--stdio`.
6. Tests + `docs/acp.md` + changelog.

---

## Out of scope

- Unstable RFDs (fork, NES, MCP-over-ACP, extra plan types)
- Implementing Client `fs/*` / `terminal/*` execution
- ACP-driven `auth/login`
- Audio prompts
- Moving ACP into `elph-agent`
- `Agent.protocol_router()` on one stdio stream (version is a process flag, not negotiated mix)
