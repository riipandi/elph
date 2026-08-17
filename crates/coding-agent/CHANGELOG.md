# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **ACP:** `elph acp --stdio` speaks **v1 (stable)**; `elph acp --stdio --experimental`
  speaks **v2 (draft)**. Bare `elph acp` aliases `--stdio`. Each process speaks one
  version. v1 holds `session/prompt` until `stopReason` and supports `session/load`.
  Slash advertisement includes prompt templates and skills. v1 `modes` / `thought_level`
  expose reasoning effort (pi-acp convention); `configOptions` also expose model and
  agent mode. Client `mcpServers` are attached to the session registry. `session/cancel`
  cancels in-flight tool calls. v2 keeps accept-then-`state_update`. See `docs/acp.md`.

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
