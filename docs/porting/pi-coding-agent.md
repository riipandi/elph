# Porting status: pi-coding-agent → elph

**Last audited:** 2026-07-11T12:14:13Z
**Upstream:** `@earendil-works/pi-coding-agent` · `packages/coding-agent` · **v0.80.6** + Unreleased
**Upstream commit:** `4c18610` (2026-07-11)
**Local clone:** `~/.local/share/elph/tmp//pi`
**Elph crate:** `crates/coding-agent/` (binary + library; product shell)
**Depends on:** `elph-agent`, `elph-ai`, `elph-tui` — see [pi-ai.md](./pi-ai.md), [pi-agent.md](./pi-agent.md)

---

## Purpose

Track how far the **Elph coding-agent product** (`elph` crate) lags or leads mainstream **pi-coding-agent**.

This is **not** the same as `elph-agent` / `elph-ai` (runtime libraries). Those map to `packages/agent` and `packages/ai`.
`elph` maps to the **product shell**: CLI, interactive TUI, session UX, slash commands, settings, export, extensions host, print/RPC modes, and so on.

Elph deliberately **diverges** in product design (memory, ACP, WASM extensions, goals). Treat those as **[Elph delta]**, not failures to port pi.

**Style:** status is written as tagged bullets and short paragraphs so the page stays scannable without wide comparison tables.

---

## At a glance

- Module layout / product intent — **[Partial]** — `crates/coding-agent/src/agent/` is the declared pi-coding-agent equivalent; many CLI/TUI surfaces are stubs
- Session orchestration above harness — **[Partial]** — `CodingAgentSession`, wiring, session manager exist; UX completeness lags
- Interactive TUI — **[Partial]** — shell/TUI wired; overlays and slash handlers largely stubbed
- Print / non-interactive mode — **[Partial]** — `elph run` exists; flags incomplete (fork, files)
- RPC / JSON automation — **[Gap]** in elph (pi has RPC); Elph has **ACP** instead (**[Elph delta]**, different protocol)
- Public SDK (`createAgentSession`) — **[Gap]** as a first-class TS-style SDK; library is `elph` + crates, not a pi-compatible SDK API
- Built-in tools — **[Parity]** via `elph-agent` tools (+ Elph web/multi-agent extras)
- Extensions — **[Partial]** / different — pi: JS/TS host; elph: WASM Component Model
- Skills + prompt templates — **[Partial]** — load paths in agent crate; product wiring incomplete
- Themes / keybindings editor — **[Gap]** (or minimal)
- Project trust — **[Partial]**
- Login / OAuth UX — **[Partial]** — provider CLI + oauth in `elph-ai`; interactive dialogs lag
- Export HTML / share gist — **[Gap]** (CLI export stub)
- Memory / server — **[Elph delta]**

---

## Timeline

### 2026-08-18 — Settings file surface: filters, trust, host knobs

**Scope:** `crates/coding-agent/src/platform/settings/`, resource loader, session create. Not a TUI `/settings` overlay.

Pi intent (`enabledModels`, resource path arrays, `defaultTools`, `defaultProjectTrust`, thinking budgets, shell, proxy) mapped onto Elph nested groups. No npm `packages`. No legacy settings migration.

- `models.enabled` glob catalog filter; `models.thinkingBudgets` → harness stream options.
- `resources.skills` / `prompts` / `extensions`, `disabledSkills` / `disabledExtensions`, `enableSkillCommands`.
- `tools.default` builtin allowlist (meta tools stay).
- `trust.defaultProjectTrust` + skip project settings/resources when untrusted (`ask` ≡ `never` until a prompt UI exists).
- `shell.path` / `commandPrefix`, `network.httpProxy`, `ui.quietStartup`, `compaction.reserveTokens`.
- Dropped `migrate_settings_value` and `extensions.json` sidecar.

### 2026-08-09 — Busy-state queueing: action dispatch, not raw text (Elph delta)

**Scope:** `crates/coding-agent/` product crate. Follow-up to the `/handover`
resilience pass.

Previously, when the agent was busy (`agent_turn_active`), turn-spawning slash
commands queued their **raw slash text** (`/continue`, `/compact`) to the model
as a follow-up prompt — semantically wrong (the model received the literal
command string), while `goal`/`reload`/`extension` work was silently **dropped**.

Now:

- `handle_slash_submit` **always dispatches** turn-spawning commands (Continue,
  compact, skill, template, goal, reload, extension) on a background task. The
  session's internal `turn_gate` serializes them behind the active turn — the
  same mechanism `/handover` uses. The `spawn_agent_work` field is gone.
- The shell no longer pushes raw slash text as a follow-up. When busy, a clear
  meta notice is shown ("Command /x queued — runs after the current task.");
  when idle, the normal echo/busy flow applies. Normal text prompts are
  unaffected (they still queue/steer as user input).
- **Quiet background commands** — `/reload`, `/goal`, `/extension` (and
  `/handover`) return `SlashOutcome::BackgroundTaskQuiet`: the slash input is
  never echoed as a user card and never enters prompt history; the task reports
  via `AgentUiEvent` (Status / notices) and busy state derives from the agent
  loop, so a failure cannot strand a stale busy UI. (`/memory` keeps the plain
  `BackgroundTask` echo.)
- Consequence: `.jsonl.zst` still unsupported, but the busy-path semantic gap
  (raw text vs. real action) is closed for every turn-spawning command.

### 2026-08-09 — `/handover` resilience hardening (Elph delta)

**Scope:** `crates/coding-agent/` product crate. Follow-up to the Claude+Codex
handover launch.

- **Bounded reads** — both readers cap a transcript at 32 MiB total, 4 MiB per
  JSONL record, and 5000 conversational records; oversized records are counted
  and skipped, over-cap transcripts surface a `transcript_truncated` warning,
  and over-size files are rejected with a clear message instead of being
  buffered whole (previously `fs::read` slurped arbitrary-size files).
- **Background dispatch** — `/handover` now runs resolve+read+prompt-build on a
  `spawn_blocking` background task; the TUI render thread is never blocked. New
  `SlashOutcome::BackgroundTaskQuiet` dispatches the handoff turn without
  echoing the raw slash text as a user card; busy state is derived from the
  agent loop, so a read failure cannot strand a stale "busy" chip. Error/success
  notices flow through normal `AgentUiEvent::Status` events.

### 2026-08-09 — `/handover` Codex resume (Elph delta)

**Scope:** `crates/coding-agent/` product crate. Follow-up to the `/handover`
Claude launch (same session); adds the Codex source.

- **`/handover codex [ref]`** — discovers Codex CLI/VSCode rollout transcripts
  (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`), resolves
  `latest`/UUID/free-text, reads the transcript as **inert history**
  (`session_meta` + `response_item` + `event_msg`; skips developer-role and
  injected AGENTS.md wrappers; applies `compacted`/`thread_rolled_back`
  reduction + duplicate collapse), and injects a Codex handoff prompt
  (`CODEX_HANDOVER_PROMPT_PREFIX`, slim `Handover from Codex…` meta line).
- Reader lives entirely on the **rollout filesystem** — `state_N.sqlite` is
  never opened, so a live Codex process (hot WAL) is never disturbed.
- Codex reader: `crates/coding-agent/src/agent/handover/codex.rs` + tests
  (`codex/tests.rs`, 11 tests).

### 2026-08-09 — `/handover` foreign-session resume (Elph delta)

**Scope:** `crates/coding-agent/` product crate. Not a pi port — mirrors Grok Build's
`foreign_sessions/claude` resume flow (powered by the portable Claude resume
skills) as an **Elph delta**.

- **`/handover claude [ref]`** — discovers Claude Code sessions
  (`~/.claude/projects/<slug>/*.jsonl`), resolves `latest` / UUID / free-text
  title, reads the transcript as **inert history** (leaf chain, preserved/snipped
  compaction segments, parallel siblings; strips meta/sidechain/thinking; caps
  tool I/O and message text; stubs summarized results), and injects a handoff
  prompt into the current Elph session. New module
  `crates/coding-agent/src/agent/handover/` + design doc
  [handover.md](../design/handover.md).
- **`/handover codex …`** — accepted arg (palette completions),
  prints `Codex handover not yet implemented`.
- `SlashDispatch::Handover`, ACP guard (`handler.rs`), TUI slim
  `Handover from Claude Code…` meta line (`tick.rs`).

### 2026-07-29 — Rust verify & harden + dead code cleanup

**Upstream baseline:** unchanged (`4c18610`, v0.80.6 + Unreleased).

Full quality-gate pass across `crates/coding-agent/` + workspace:

- **26 clippy violations fixed** — `manual_clamp`, `collapsible_if` ×2, `collapsible_str_replace`, `clone_on_copy` ×7, `if_same_then_else`, `unnecessary_lazy_evaluations`, 4× `too_many_arguments` suppressed with TODO refactor markers.
- **2 test failures repaired** — `agent_creates_with_custom_initial_state` and `agent_updates_state_with_mutators` used `get_model("openai", "gpt-4o-mini")` which no longer resolves (direct `openai` provider removed from catalog). Changed to `get_models(None).next()`.
- **Dead code removed** — 17 items across provider connect, credential store, plan confirmation, paths, and tool approval modules (see below).

**Provider connect dialog** (`crates/coding-agent/src/tui/provider_connect_dialog.rs`):

Removed dead wrapper functions that were part of the WIP wiring but never called:

- `auth_store_path()`, `save_provider_api_key()`, `load_provider_api_key()`, `remove_provider_api_key()`
- `clamp_selected()`, `transition_to_api_key_step()`, `transition_to_select_provider_step()`
- `ProviderConnectFocus::ApiKeyInput` enum variant

**Credential store** (`crates/coding-agent/src/tui/provider_credential_store.rs`):

Removed three dead credential helpers (`load_provider_credential`, `load_all_provider_credentials`, `remove_provider_credential`) that were only reachable through the dead wrapper chain. `save_provider_credential` retained — used from OAuth flow in `shell/mod.rs`.

**Plan confirmation** (`crates/coding-agent/src/tui/tool_approval.rs`, `status_dialog.rs`, `shell/mod.rs`):

- `PendingPlanConfirmation.plan_id` — stored but never read. Removed from struct, `From` impl, and `shell/mod.rs` constructor.
- `StatusDialogKind::PlanConfirmation.plan_id` — same; removed from variant and builder.
- `MODE_CHANGE_DEFAULT_INDEX` — dead constant removed.

**Paths** (`crates/coding-agent/src/platform/paths.rs`):

- Removed `project_sessions_dir()` and `project_sessions_dir_for()` — never called.
- Fixed misplaced doc comment on `global_extensions_dir()`.
- Removed stale `#[allow(dead_code)]` from `project_dir()` and `global_extensions_dir()` — both actively used.

### 2026-07-11T12:14:13Z @ `4c18610` (v0.80.6 + Unreleased)

Initial product gap audit: tree compare `packages/coding-agent` vs `crates/coding-agent/`, design docs, CLI stubs, slash registry, modes. **Analysis only — no product code changes.**

---

## Architecture mapping

```
packages/coding-agent/                 elph/
├── main.ts / cli.ts                   ├── main.rs + cli/
├── cli/args, session-picker, …        ├── cli/* (subcommands) + default interactive entry
├── core/agent-session*.ts             ├── agent/runtime, session/, session_manager
├── core/model-registry, resolver      ├── agent/model_registry, provider
├── core/resource-loader, skills       ├── agent/resource_loader, skills/
├── core/slash-commands                ├── agent/slash_commands (+ shell/slash)
├── core/system-prompt                 ├── agent/system_prompt
├── core/tools/*                       ├── (lives in crates/elph-agent/tools)
├── core/extensions/*                  ├── extensions/ + elph-agent plugins (WASM)
├── core/settings-manager              ├── platform/settings, paths, bootstrap
├── core/export-html                   ├── cli/export (stub)
├── core/sdk.ts                        ├── lib.rs public modules (not pi-shaped SDK)
├── modes/interactive/*                ├── shell/ + tui/
├── modes/print-mode.ts                ├── cli/run + agent/run_mode
├── modes/rpc/*                        ├── cli/acp (different protocol)
├── config.ts, migrations.ts           ├── platform/migrations, paths
└── utils/*                            ├── platform/*, worktree/, scattered helpers
```

**Status by area**

- CLI entry + arg parse (`cli/mod.rs`, `main.rs`) — **[Partial]** — clap subcommands vs pi flag-oriented UX
- Interactive mode (`shell/`, `tui/`) — **[Partial]**
- Print mode (`cli/run.rs`, `agent/run_mode.rs`) — **[Partial]**
- RPC mode — **[Gap]** in elph
- ACP (`cli/acp.rs`, `platform/acp.rs`) — **[Elph delta]**
- Agent session core (`agent/session`, `runtime`) — **[Partial]**
- Session manager, model registry, resource loader, system prompt, settings — **[Partial]**
- Slash commands — **[Partial]** — wide registry; dispatch mostly stubs
- Extensions — **[Partial]** (WASM ≠ JS)
- Tools — **[Parity+]** via `elph-agent` (web, multi-agent extra)
- Export / import, HTML export / gist share — **[Gap]** (stubs)
- Package manager CLI — **[Gap]** (elph uses `plugin` / extensions instead)
- Themes — **[Gap]**; keybindings — **[Partial]** / minimal
- Telemetry / timings — **[Gap]** or not product-exposed
- Diagnostics, footers/status — **[Partial]**
- Memory / floppy, local server — **[Elph delta]** (server often stub)
- Worktree admin CLI — **[Partial]**

---

## Run modes

- **Interactive TUI** — pi `modes/interactive` vs elph `shell/` + `tui/` — **[Partial]**
- **Print / one-shot** — pi `--print` vs `elph run` — **[Partial]** (`--fork`, file attach incomplete)
- **JSON / structured print** — pi `--mode json` vs limited elph — **[Partial]** / **[Gap]**
- **RPC JSONL control plane** — pi `modes/rpc` — **[Gap]** in elph
- **ACP stdio** — `elph acp` — **[Elph delta]**
- **First-time setup / trust UI** — pi startup-ui vs bootstrap / doctor (stub) — **[Partial]**
- **Session picker** — pi session-picker vs resume flag / session CLI — **[Partial]**

---

## Slash commands

pi built-ins (registry in `core/slash-commands.ts`):
`/settings`, `/model`, `/scoped-models`, `/export`, `/import`, `/share`, `/copy`, `/name`, `/session`, `/changelog`, `/hotkeys`, `/fork`, `/clone`, `/tree`, `/trust`, `/login`, `/logout`, `/new`, `/compact`, `/resume`, `/reload`, `/quit`.

elph built-in **names** largely mirror pi, plus `/provider`, `/help`, `/exit`. Design docs also plan `/goal`, diagnostics, `/commit`, `/diff` (see [slash-commands.md](../slash-commands.md)).

- Registry list — **[Partial]** (names present)
- Dispatch / handlers — **[Gap]** in behavior (mostly `slash_stub_message`)
- `/model`, selectors — **[Partial]** (overlays partially stubbed)
- `/tree` — **[Partial]** (interactive item selector + jump/summary; not full Pi TreeSelector filters/labels)
- `/login` / `/logout` — **[Partial]** (CLI `provider` + oauth infra)
- `/scoped-models` — **[Partial]** (editor + Ctrl+P cycle; no keybinding remaps / null=all semantics)
- `/share` — **[Gap]**
- `/goal` — **[Elph delta]** / **[Partial]** in elph (design + goal_slash)
- `/handover` — **[Elph delta]** foreign-session resume (Claude + Codex
  implemented; see [handover.md](../design/handover.md))
- Extension commands — **[Partial]** (JS vs WASM model)
- Prompt templates as `/name` — **[Partial]** (planned)

---

## Interactive TUI surface

pi ships a large interactive component set under `modes/interactive/components/` (message types, selectors, login, themes, diff, tool execution, tree, and so on).

- Transcript + tool rendering — **[Partial]** (TUI bridge / widgets)
- Model / session / tree selectors — **[Partial]** (`overlays.rs`)
- Thinking selector — **[Partial]**
- Login / OAuth dialogs — **[Gap]**
- Theme selector — **[Gap]** (no settings field; fixed dark palette); settings selector — **[Gap]**
- Diff view — **[Gap]** (planned slash)
- Extension UI (editor/input/selector) — **[Partial]** (WASM slash only, phase 1)
- Image show / clipboard paste — **[Partial]**
- Keybinding hints — **[Partial]** (`/hotkeys` stub)
- Ctrl+X copy last message (Unreleased) — **[Gap]**
- Cache-miss notices — **[Gap]**

Design snapshot: _“Elph TUI + coding agent — In progress; Shell wired; overlays partially stubbed.”_

---

## CLI product surface

### pi (flag-oriented)

Typical flags: `--model`, `--provider`, `--thinking`, `--continue`/`-c`, `--resume`/`-r`, `--session`, `--fork`, `--print`, `--mode text|json|rpc`, `--tools` / `--no-tools`, extensions/skills/templates toggles, `--list-models`, `--export`, offline/verbose, file args, system prompt flags, project trust override.

### elph (subcommand-oriented)

- Default interactive — **[Partial]**
- `run` — **[Partial]** (print mode)
- `session`, `models`, `completions` — present
- `provider` — **[Partial]** (many stubs; login/auth storage)
- `export` / `import` — **[Partial]** JSONL full-tree export + import-to-new-session (Pi intent); HTML export / gist share still gap
- `mcp` — **[Partial]** stubs (pi MCP packaging differs)
- `plugin` / extensions — **[Partial]** vs pi extensions + package manager
- `doctor`, `stats`, `update` — **stubs**
- `acp`, `memory` — present, **[Elph delta]**
- `server` — **stub**, **[Elph delta]**
- `worktree` — **stubs**; packaging differs from pi

---

## Core product modules (deeper)

- AgentSession + events — pi rich facade vs `CodingAgentSession` + wiring — **[Partial]**
- Session services / runtime factory — `create_coding_session_with_events` — **[Partial]**
- Auth storage + guidance — `elph-ai` oauth + provider CLI — **[Partial]**
- Shell executor — library tool in `elph-agent` — **[Parity]**
- Compaction UX — harness compaction; UX commands stub — **[Partial]**
- Model registry / scoped models, settings, project trust — **[Partial]**
- Keybindings — **[Gap]** / incomplete
- Package manager vs extensions install — **[Partial]** (different model)
- Export HTML — **[Gap]**
- Event bus — harness/agent events — **[Partial]**
- Output guard / stdout takeover — **[N/A]** (different product model)
- HTTP dispatcher / proxy — env/proxy in `elph-ai` — **[Partial]**
- Migrations — platform migrations — **[Partial]**
- SDK `createAgentSession` — **[Gap]** (no pi-compatible SDK)

---

## Upstream coding-agent features (0.80.4–Unreleased) vs product exposure

Library fixes may already be in `elph-ai` / `elph-agent` after the library sprints; **product exposure** can still lag:

- Dynamic tool loading for extensions — library may be ready; WASM may not expose the same deferred-load story
- Thinking `max` / Fable 5 — library ok; TUI selector / CLI flag completeness TBD
- Input pricing tiers — library ok; stats/footer display TBD
- `agent_settled` / idle wait for extensions + RPC — RPC missing; settled UX TBD
- `before_provider_headers` extension hook — JS hooks ≠ WASM
- Project-local `pi config -l` resources — different config model
- Cache miss notices, Ctrl+X copy message — missing in product
- `/login <provider>` autocomplete — partial CLI only
- SDK model/scoped-model resolution exports — missing pi-shaped SDK

---

## What exists only in elph (not port gaps)

- Goals + nested subagents (product wiring); slash `/goal`
- MCP product integration (`elph-agent` MCP + CLI)
- Project memory (floppy) + `elph memory`
- ACP server mode (alternative to pi RPC)
- WASM extensions (vs pi JS extensions)
- Local REST/WS server (planned / stub)
- Web tools (search/fetch) in the agent crate
- Hyper provider (`elph-ai` only)

---

## Prioritized product gaps (tracking only)

### P0 — interactive product usable parity

1. **Slash command dispatch** — implement handlers behind the existing registry (model, compact, tree, new, resume, reload, quit/help).
2. **Interactive overlays** — model / session / tree selectors end-to-end (stop stubbing).
3. **`elph run` completeness** — fork, file attachments, thinking level, tool filters, continue/session flags aligned with design.
4. **Provider login UX** — interactive or documented CLI path equivalent to `/login`.

### P1 — session lifecycle and power-user UX

5. Export / import sessions (JSONL minimum; HTML optional).
6. Fork / clone / name / session stats.
7. Compaction command + status feedback.
8. Settings UI or complete settings file surface (cache notices, thinking display, etc.).
9. Project trust first-run flow.

### P2 — modes and ecosystem

10. Decide RPC vs ACP strategy (document; implement the chosen automation plane fully).
11. Themes + keybindings (if product wants pi-like customizability).
12. Prompt templates as `/name` end-to-end.
13. Extension story: deferred tools + entry renderers equivalent (WASM).
14. Doctor / stats / update CLI beyond stubs.

### Product (Elph-only — do not measure as pi lag)

15. Memory, server, goals polish on their own roadmaps.

---

## Dependency note

Coding-agent product gaps often **depend on library parity** but are not solved by libraries alone:

- Thinking `max` in UI/CLI — needs `elph-ai` / `elph-agent` levels (**library done**)
- Deferred extension tools — needs `added_tool_names` + providers (**library done**)
- Compaction correctness — harness estimate (**library done**)
- Session tree navigation — session backends (**largely done in agent**)

Re-audit this file after product milestones; re-audit [pi-ai.md](./pi-ai.md) / [pi-agent.md](./pi-agent.md) when library mainstream moves.

---

## How to re-audit

```sh
cd /path/to/pi && git pull && git rev-parse --short HEAD
head -80 packages/coding-agent/CHANGELOG.md

# Compare:
# - packages/coding-agent/src/core/slash-commands.ts
# - packages/coding-agent/src/cli/args.ts
# - packages/coding-agent/src/modes/**
# - packages/coding-agent/docs/**

# Against:
# - crates/coding-agent/src/agent/**
# - crates/coding-agent/src/shell/**
# - crates/coding-agent/src/cli/**
# - docs/slash-commands.md, docs/cli.md, docs/tui.md
```

Update **Last audited**, append a **Timeline** entry, and refresh status bullets. Prefer new timeline entries over rewriting history.

---

## Related docs

- Product design: [docs/README.md](../README.md), [cli.md](../cli.md), [slash-commands.md](../slash-commands.md), [tui.md](../tui.md), [codebase-layout.md](../codebase-layout.md)
- Library ports: [pi-ai.md](./pi-ai.md), [pi-agent.md](./pi-agent.md)
- Porting index: [README.md](./README.md)
