# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **ACP steering:** a second `session/prompt` while a turn is running no longer emits its
  own `idle` state update (and no longer claims the output was lost). The owning turn closes
  the stretch; a mid-turn `running` refresh no longer reopens the idle slot, so a steer or
  retry cannot double-idle the client.
- **ACP diffs:** `git_patch` text is now a valid unified diff — `diff --git` header,
  `--- /dev/null` on add, `+++ /dev/null` on delete, and a hunk header whose line counts
  match the body. Oversized changes ship `changes` without `patch` instead of a truncated
  patch that contradicts its own header.
- **ACP tool locations:** absolute-path detection accepts drive-letter (`C:\…`) and UNC
  (`\\server\share`) paths, so `locations` is no longer always empty on Windows.
- **ACP v1 stop reasons:** a failed `session/prompt` reports the real reason
  (`max_tokens`, `max_turn_requests`, `refusal`) instead of always `end_turn`.
- **ACP error codes:** a relative `cwd` on `session/new` / `load` / `resume` returns
  `invalid_params` (-32602) with the offending path, not `internal_error`. v1 `session/load`
  and `session/resume` validate `cwd` too.
- **ACP Registry entry:** `crates/coding-agent/agent.json` (moved out of `docs/acp-registry/elph/`) used an invalid platform key
  (`linux-arm64`) and archive filenames that no release publishes. Keys and asset names now
  match `release.yml`, every target pins its `sha256`, and the README documents the mapping
  plus the prerelease caveat for the registry's hourly auto-update.

### Changed

- **ACP rate limits:** provider 429 / retry status is shown in the session instead of
  a silent spinner. v1 no longer ignores `Status` events.
- **ACP stream:** do not upsert a full agent message after chunks (duplicate/scrambled
  text). Tool updates replace accumulated output instead of appending snapshots as
  chunks. Always create the tool call before updates (`Tool call not found`).
- **ACP errors:** slash/skill/prompt failures are written into the session (text +
  chunk) and the turn goes idle, instead of a silent hang or a bare JSON-RPC error.
- **ACP `session/new`:** do not attach MCP or run store GC before answering;
  isolate open work from the stdio loop and time out after 25s so a hang/panic
  cannot close the transport (`incoming_transport_closed`).
- **ACP slash catalog:** `available_commands_update` is sent after `session/new` /
  `resume` / `load` is answered (then refreshed after MCP), so Zed and similar
  clients actually register `/help`, `/skill:…`, and the rest.
- **ACP production contract:** v1 advertises `session/delete` and `additionalDirectories`.
  Tools are reported `pending` then `in_progress`. v2 emits at most one idle `state_update`
  per stretch, unique agent `messageId`s for slash/status text, `usage_update.size` as the
  model context window (plus USD cost when known), and mapped stop reasons
  (`max_tokens` / `max_turn_requests` / `refusal`). Wire tests cover `/help` prompt
  lifecycle on both versions.
- **ACP:** `elph acp --stdio` speaks **v1 (stable)**; `elph acp --stdio --experimental`
  speaks **v2 (draft)**. Bare `elph acp` aliases `--stdio`. Each process speaks one
  version. v1 holds `session/prompt` until `stopReason` and supports `session/load`.
  Slash advertisement includes prompt templates and `/skill:NAME` skills (TUI-only
  commands omitted). Model config lists the full provider/model catalog. v1 `modes` / `thought_level`
  expose reasoning effort (pi-acp convention); `configOptions` also expose model and
  agent mode. Client `mcpServers` are attached to the session registry. `session/cancel`
  cancels in-flight tool calls. v2 keeps accept-then-`state_update`. Local shell is
  mirrored as display-only ACP terminals (not client `terminal/*`). ACP **auth**
  advertises `authMethods` and implements v1 `authenticate`/`logout` and v2
  `auth/login`/`auth/logout`. Privileged methods (`session/new`/`load`/`resume`/`prompt`,
  set mode/config) require credentials; list/close/delete/cancel and initialize do not.
  Existing env/`auth.json` keys allow privileged methods without an extra authenticate
  call. After logout, those methods return `auth_required` until an explicit login.
  ACP turns no longer stop on an intermediate `RunCompleted` (retry/compact) or a
  failed tool `session/update`. Concurrent prompt submit is not blocked on `ui_rx`;
  cancel/close/logout abort in-flight permission RPC; list/close/config/logout are
  spawned off the I/O loop; MCP is attached before `session/new` answers.
  ACP update payloads are truncated; retry text uses a new message id; cancel
  aborts off the I/O loop; v2 prompt failures after ack still emit idle.
  Terminal Auth (`elph acp --setup`) is advertised for the ACP Registry.
  Registry submission files live in `docs/acp-registry/`.
  See `docs/acp.md`.

### Added

- Coding agent now follows a `<response_style>` section (Simplified Technical English,
  ASD-STE100) for every response: short active sentences, plain words without jargon,
  hedging, or pleasantries, one consistent term per concept, and no preamble/recap/closing.
  Applies to chat replies and content written to files; non-English prose keeps the style
  rules (the controlled vocabulary applies to English prose). Configurable via the
  `simplifiedTechnicalEnglish` setting (default `true`; `false` omits the section).
  See `docs/design/system-prompt-efficiency.md`.
- `ui.density` setting (default `compact`): collapsed tool-call items pack together in the
  transcript log (grouped/narrow log lines). Expanded (accessed) tool-call items, `Thinking`,
  and AI chat response/assistant items always keep line breaks above and below. Set to `loose`
  for the classic blank-line spacing between every process-log row. The former boolean
  `ui.narrowLogLines` is migrated automatically (`true` → `compact`, `false` → `loose`).
