# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

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
