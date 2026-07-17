# Porting status: pi-coding-agent → elph

**Last audited:** 2026-07-11T12:14:13Z
**Upstream:** `@earendil-works/pi-coding-agent` · `packages/coding-agent` · **v0.80.6** + Unreleased
**Upstream commit:** `4c18610` (2026-07-11)
**Local clone:** `/Users/ariss/Developer/github.com/earendil-works/pi`
**Elph crate:** `elph/` (binary + library; product shell)
**Depends on:** `elph-agent`, `elph-ai`, `elph-tui`, `floppy` — see [pi-ai.md](./pi-ai.md), [pi-agent.md](./pi-agent.md)

---

## Purpose

Track how far the **Elph coding-agent product** (`elph` crate) lags or leads mainstream **pi-coding-agent**.

This is **not** the same as `elph-agent` / `elph-ai` (runtime libraries). Those map to `packages/agent` and `packages/ai`.
`elph` maps to the **product shell**: CLI, interactive TUI, session UX, slash commands, settings, export, extensions host, print/RPC modes, and so on.

Elph deliberately **diverges** in product design (memory, codegraph, ACP, WASM extensions, goals). Treat those as **[Elph delta]**, not failures to port pi.

**Style:** status is written as tagged bullets and short paragraphs so the page stays scannable without wide comparison tables.

---

## At a glance

- Module layout / product intent — **[Partial]** — `elph/src/agent/` is the declared pi-coding-agent equivalent; many CLI/TUI surfaces are stubs
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
- Memory / codegraph / server — **[Elph delta]**

---

## Timeline

### 2026-07-11T12:14:13Z @ `4c18610` (v0.80.6 + Unreleased)

Initial product gap audit: tree compare `packages/coding-agent` vs `elph/`, design docs, CLI stubs, slash registry, modes. **Analysis only — no product code changes.**

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
- Memory / floppy, codegraph, local server — **[Elph delta]** (server often stub)
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
- `/model`, `/tree`, selectors — **[Partial]** (overlays partially stubbed)
- `/login` / `/logout` — **[Partial]** (CLI `provider` + oauth infra)
- `/scoped-models` — **[Partial]** (editor + Ctrl+P cycle; no keybinding remaps / null=all semantics)
- `/share` — **[Gap]**
- `/goal` — **[Elph delta]** / **[Partial]** in elph (design + goal_slash)
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
- `export` / `import` — **stub** vs pi export/import/HTML
- `mcp` — **[Partial]** stubs (pi MCP packaging differs)
- `plugin` / extensions — **[Partial]** vs pi extensions + package manager
- `doctor`, `stats`, `update` — **stubs**
- `acp`, `memory` — present, **[Elph delta]**
- `codegraph`, `server` — **stubs**, **[Elph delta]**
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
- Codegraph CLI surface (often stubs)
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

15. Memory, codegraph, server, goals polish on their own roadmaps.

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
# - elph/src/agent/**
# - elph/src/shell/**
# - elph/src/cli/**
# - docs/slash-commands.md, docs/cli.md, docs/tui.md
```

Update **Last audited**, append a **Timeline** entry, and refresh status bullets. Prefer new timeline entries over rewriting history.

---

## Related docs

- Product design: [docs/README.md](../README.md), [cli.md](../cli.md), [slash-commands.md](../slash-commands.md), [tui.md](../tui.md), [codebase-layout.md](../codebase-layout.md)
- Library ports: [pi-ai.md](./pi-ai.md), [pi-agent.md](./pi-agent.md)
- Porting index: [README.md](./README.md)
