# Porting status: pi-coding-agent → elph

| Field               | Value                                                                                                       |
| ------------------- | ----------------------------------------------------------------------------------------------------------- |
| **Last audited**    | 2026-07-11T12:14:13Z                                                                                        |
| **Upstream**        | `@earendil-works/pi-coding-agent` · `packages/coding-agent` · **v0.80.6** + Unreleased                      |
| **Upstream commit** | `4c18610` (2026-07-11)                                                                                      |
| **Local clone**     | `/Users/ariss/Developer/github.com/earendil-works/pi`                                                       |
| **Elph crate**      | `elph/` (binary + library; product shell)                                                                   |
| **Depends on**      | `elph-agent`, `elph-ai`, `elph-tui`, `elph-core` — see [pi-ai.md](./pi-ai.md), [pi-agent.md](./pi-agent.md) |

---

## Purpose of this document

Track how far the **Elph coding-agent product** (`elph` crate) lags or leads mainstream **pi-coding-agent**.

This is **not** the same as `elph-agent` / `elph-ai` (runtime libraries). Those map to `packages/agent` and `packages/ai`.
`elph` maps to the **product shell**: CLI, interactive TUI, session UX, slash commands, settings, export, extensions host, print/RPC modes, etc.

Elph deliberately **diverges** in product design (memory, codegraph, ACP, WASM extensions, goals). Treat those as Elph extensions, not failures to port pi.

---

## At a glance

| Area                                    | Assessment                                                                                                  |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Module layout / product intent          | **Partial** — `elph/src/agent/` is the declared pi-coding-agent equivalent; many CLI/TUI surfaces are stubs |
| Session orchestration above harness     | **Partial** — `CodingAgentSession`, wiring, session manager exist; UX completeness lags                     |
| Interactive TUI mode                    | **Partial** — shell/TUI wired; overlays and slash handlers largely stubbed                                  |
| Print / non-interactive mode            | **Partial** — `elph run` exists; flags incomplete (fork, files)                                             |
| RPC / JSON automation mode              | **Missing in elph** (pi has dedicated RPC mode); Elph has **ACP** instead (different protocol)              |
| Public SDK (`createAgentSession`)       | **Missing in elph** as a first-class TS-style SDK; library is `elph` + crates, not a pi-compatible SDK API  |
| Built-in tools (read/bash/edit/write/…) | **Parity** via `elph-agent` tools (+ Elph web/multi-agent extras)                                           |
| Extensions                              | **Partial / different** — pi: JS/TS extension host; elph: WASM Component Model                              |
| Skills + prompt templates               | **Partial** — load paths exist in agent crate; product wiring incomplete                                    |
| Themes / keybindings editor             | **Missing in elph** (or minimal)                                                                            |
| Project trust                           | **Partial** — slash/registry names exist; full trust UX may lag                                             |
| Login / OAuth UX                        | **Partial** — provider CLI + oauth in `elph-ai`; interactive login dialogs lag                              |
| Export HTML / share gist                | **Missing in elph** (CLI export stub)                                                                       |
| Memory / codegraph / server             | **Missing in pi** (Elph product)                                                                            |

---

## Audit log

| Timestamp (UTC)      | Pi version / commit               | Notes                                                                                                                                                           |
| -------------------- | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-07-11T12:14:13Z | `0.80.6` + Unreleased @ `4c18610` | Initial gap audit: tree compare `packages/coding-agent` vs `elph/`, design docs, CLI stubs, slash registry, modes. **Analysis only — no product code changes.** |

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

| pi package area          | elph location                            | Status                                                       |
| ------------------------ | ---------------------------------------- | ------------------------------------------------------------ |
| CLI entry + arg parse    | `cli/mod.rs`, `main.rs`                  | **Partial** — clap subcommands vs pi flag-oriented UX        |
| Interactive mode         | `shell/`, `tui/`                         | **Partial**                                                  |
| Print mode               | `cli/run.rs`, `agent/run_mode.rs`        | **Partial**                                                  |
| RPC mode                 | —                                        | **Missing in elph**                                          |
| ACP                      | `cli/acp.rs`, `platform/acp.rs`          | **Missing in pi**                                            |
| Agent session core       | `agent/session`, `runtime`               | **Partial**                                                  |
| Session manager          | `agent/session_manager.rs`               | **Partial**                                                  |
| Model registry / resolve | `agent/model_registry.rs`, `provider.rs` | **Partial**                                                  |
| Resource loader          | `agent/resource_loader.rs`               | **Partial**                                                  |
| Slash commands           | `agent/slash_commands.rs`                | **Partial** — registry wide; dispatch mostly stubs           |
| System prompt assembly   | `agent/system_prompt.rs`                 | **Partial**                                                  |
| Settings                 | `platform/settings.rs`                   | **Partial** — smaller settings surface than pi               |
| Project trust            | platform / slash `trust`                 | **Partial**                                                  |
| Extensions               | `extensions/` + agent plugins            | **Partial** (WASM ≠ JS)                                      |
| Tools                    | `elph-agent` tools                       | **Parity+** (web, multi-agent extra)                         |
| Export / import          | `cli/export`, `cli/import`               | **Missing in elph** (stubs)                                  |
| HTML export / gist share | pi `export-html`, `/share`               | **Missing in elph**                                          |
| Package manager CLI      | `package-manager-cli.ts`                 | **Missing in elph** (elph has `plugin` / extensions instead) |
| Themes                   | interactive theme controller             | **Missing in elph**                                          |
| Keybindings system       | `core/keybindings.ts`                    | **Partial** / minimal in TUI                                 |
| Telemetry / timings      | `core/telemetry.ts`, `timings.ts`        | **Missing in elph** (or not product-exposed)                 |
| Diagnostics              | `core/diagnostics.ts`                    | **Partial**                                                  |
| Footers / status         | interactive footer components            | **Partial** (`tui/widget/statusline`)                        |
| Memory / floppy          | `memory/`                                | **Missing in pi**                                            |
| Codegraph                | `cli/codegraph`, `memory/codegraph`      | **Missing in pi**                                            |
| Local server             | `cli/server`                             | **Missing in pi** (stub in elph)                             |
| Worktree admin CLI       | `cli/worktree`                           | **Partial** (stubs); pi may use different workflows          |

---

## Run modes

| Mode                        | pi                                         | elph                      | Status                                         |
| --------------------------- | ------------------------------------------ | ------------------------- | ---------------------------------------------- |
| Interactive TUI             | `modes/interactive` — full component suite | `shell/` + `tui/`         | **Partial**                                    |
| Print / one-shot            | `--print` / print-mode                     | `elph run`                | **Partial** (`--fork`, file attach incomplete) |
| JSON / structured print     | `--mode json`                              | limited                   | **Partial** / **Missing in elph**              |
| RPC JSONL control plane     | `modes/rpc`                                | —                         | **Missing in elph**                            |
| ACP stdio                   | —                                          | `elph acp`                | **Missing in pi**                              |
| First-time setup / trust UI | `cli/startup-ui`, project-trust            | bootstrap / doctor (stub) | **Partial**                                    |
| Session picker at start     | `cli/session-picker`                       | resume flag / session CLI | **Partial**                                    |

---

## Slash commands

pi built-ins (registry in `core/slash-commands.ts`):
`/settings`, `/model`, `/scoped-models`, `/export`, `/import`, `/share`, `/copy`, `/name`, `/session`, `/changelog`, `/hotkeys`, `/fork`, `/clone`, `/tree`, `/trust`, `/login`, `/logout`, `/new`, `/compact`, `/resume`, `/reload`, `/quit`.

elph built-in **names** (registry) largely mirror pi, plus `/provider`, `/help`, `/exit`. Design docs also plan `/goal`, diagnostics, `/commit`, `/diff` (see [slash-commands.md](../slash-commands.md)).

| Command / group              | pi               | elph                                     | Status                                  |
| ---------------------------- | ---------------- | ---------------------------------------- | --------------------------------------- |
| Registry list                | full             | full-ish names                           | **Partial** (names present)             |
| Dispatch / handlers          | implemented      | mostly `slash_stub_message` / incomplete | **Missing in elph** (behavior)          |
| `/model`, `/tree`, selectors | UI components    | overlays partially stubbed               | **Partial**                             |
| `/login` / `/logout`         | interactive auth | CLI `provider` + oauth infra             | **Partial**                             |
| `/scoped-models`             | yes              | not in registry                          | **Missing in elph**                     |
| `/share` (gist)              | yes              | no                                       | **Missing in elph**                     |
| `/goal`                      | no (pi)          | design + goal_slash module               | **Missing in pi** / **Partial** in elph |
| Extension commands           | JS extensions    | WASM extension slash                     | **Partial** (different model)           |
| Prompt templates as `/name`  | yes              | planned                                  | **Partial**                             |

---

## Interactive TUI surface (pi component inventory)

pi ships a large interactive component set under `modes/interactive/components/` (message types, selectors, login, themes, diff, tool execution, tree, etc.).

| Capability                            | pi                | elph                        | Status              |
| ------------------------------------- | ----------------- | --------------------------- | ------------------- |
| Transcript + tool rendering           | rich components   | TUI bridge / widgets        | **Partial**         |
| Model / session / tree selectors      | full              | `overlays.rs` partial       | **Partial**         |
| Thinking selector                     | yes               | settings / incomplete UX    | **Partial**         |
| Login / OAuth dialogs                 | yes               | missing interactive dialogs | **Missing in elph** |
| Theme selector + themes               | yes               | settings `theme` string     | **Partial**         |
| Settings selector                     | yes               | planned                     | **Missing in elph** |
| Diff view                             | components + tool | planned slash               | **Missing in elph** |
| Extension UI (editor/input/selector)  | yes               | WASM slash only (phase 1)   | **Partial**         |
| Image show / clipboard paste image    | yes (utils)       | limited                     | **Partial**         |
| Keybinding hints                      | yes               | `/hotkeys` stub             | **Partial**         |
| Ctrl+X copy last message (Unreleased) | yes               | no                          | **Missing in elph** |
| Cache-miss notices                    | settings          | no                          | **Missing in elph** |

Design snapshot (product docs): _“Elph TUI + coding agent — In progress; Shell wired; overlays partially stubbed.”_

---

## CLI product surface

### pi (flag-oriented)

Typical flags: `--model`, `--provider`, `--thinking`, `--continue`/`-c`, `--resume`/`-r`, `--session`, `--fork`, `--print`, `--mode text|json|rpc`, `--tools` / `--no-tools`, extensions/skills/templates toggles, `--list-models`, `--export`, offline/verbose, file args, system prompt flags, project trust override.

### elph (subcommand-oriented)

| Subcommand            | Implementation       | vs pi                             |
| --------------------- | -------------------- | --------------------------------- |
| (default interactive) | partial              | interactive mode                  |
| `run`                 | partial              | print mode                        |
| `session`             | present              | session management                |
| `models`              | present              | `--list-models`                   |
| `provider`            | partial (many stubs) | `/login` + auth storage           |
| `export` / `import`   | **stub**             | `/export`, `/import`, HTML export |
| `mcp`                 | partial stubs        | pi packages/MCP differ            |
| `plugin` / extensions | partial              | pi extensions + package manager   |
| `doctor`              | **stub**             | diagnostics / config display      |
| `stats`               | **stub**             | `/session` stats                  |
| `update`              | **stub**             | pi self-update utils              |
| `completions`         | present              | shell completion                  |
| `acp`                 | present              | **Missing in pi**                 |
| `memory`              | present              | **Missing in pi**                 |
| `codegraph`           | **all stubs**        | **Missing in pi**                 |
| `server`              | **stub**             | **Missing in pi**                 |
| `worktree`            | **stubs**            | different packaging               |

---

## Core product modules (deeper)

| Module                             | pi                                | elph                                | Status                         |
| ---------------------------------- | --------------------------------- | ----------------------------------- | ------------------------------ |
| AgentSession + events              | rich session facade               | `CodingAgentSession` + wiring       | **Partial**                    |
| Session services / runtime factory | `agent-session-services`, runtime | `create_coding_session_with_events` | **Partial**                    |
| Auth storage + guidance            | `auth-storage`, `auth-guidance`   | `elph-ai` oauth + provider CLI      | **Partial**                    |
| Bash executor (product)            | coding-agent bash + tools         | elph-agent bash tool                | **Parity** (library)           |
| Compaction UX                      | product compaction helpers        | harness compaction                  | **Partial** (UX commands stub) |
| Model registry / scoped models     | full                              | model_registry present              | **Partial**                    |
| Settings manager                   | extensive settings.md surface     | smaller `Settings` struct           | **Partial**                    |
| Project trust                      | trust manager + UI                | trust slash name                    | **Partial**                    |
| Keybindings                        | full                              | incomplete                          | **Missing in elph**            |
| Package manager                    | npm package resources             | extensions install                  | **Partial** (different model)  |
| Export HTML                        | yes                               | no                                  | **Missing in elph**            |
| Event bus                          | product event bus                 | harness/agent events                | **Partial**                    |
| Output guard / stdout takeover     | yes                               | N/A / different                     | **N/A**                        |
| HTTP dispatcher / proxy settings   | yes                               | env/proxy in elph-ai                | **Partial**                    |
| Migrations                         | product migrations                | platform migrations                 | **Partial**                    |
| SDK `createAgentSession`           | first-class                       | no pi-compatible SDK                | **Missing in elph**            |

---

## Upstream coding-agent features (0.80.4–Unreleased) not reflected in elph product

Inherited library fixes (ai/agent) may already be in `elph-ai` / `elph-agent` after the library sprints; **product exposure** may still lag:

| Feature                                          | Layer                    | elph product gap                                        |
| ------------------------------------------------ | ------------------------ | ------------------------------------------------------- |
| Dynamic tool loading for extensions              | ai/agent + extensions.md | WASM extensions may not expose same deferred-load story |
| Thinking `max` / Fable 5                         | catalogs + UI            | library ok; TUI selector / CLI flag completeness TBD    |
| Input pricing tiers                              | models                   | library ok; stats/footer display TBD                    |
| `agent_settled` / idle wait for extensions + RPC | product                  | RPC missing; settled UX TBD                             |
| `before_provider_headers` extension hook         | product                  | JS hooks ≠ WASM                                         |
| Project-local `pi config -l` resources           | product                  | different config model                                  |
| Cache miss notices                               | product settings         | missing                                                 |
| Ctrl+X copy message                              | interactive              | missing                                                 |
| `/login <provider>` autocomplete                 | interactive              | partial CLI only                                        |
| SDK model/scoped-model resolution exports        | SDK                      | missing pi-shaped SDK                                   |

---

## What exists only in elph (not port gaps)

| Feature                                   | Notes                               |
| ----------------------------------------- | ----------------------------------- |
| Goals + nested subagents (product wiring) | Design + agent crate; slash `/goal` |
| MCP product integration                   | `elph-agent` MCP + CLI              |
| Project memory (floppy) + `elph memory`   | design docs                         |
| Codegraph CLI (surface reserved)          | mostly unimplemented stubs          |
| ACP server mode                           | alternative to pi RPC               |
| WASM extensions                           | vs pi JS extensions                 |
| Local REST/WS server (planned)            | stub                                |
| Web tools (search/fetch)                  | agent crate                         |
| Hyper provider                            | elph-ai only                        |

---

## Prioritized product gaps (tracking only)

### P0 — interactive product usable parity

1. **Slash command dispatch** — implement handlers behind existing registry (model, compact, tree, new, resume, reload, quit/help).
2. **Interactive overlays** — model / session / tree selectors end-to-end (stop stubbing).
3. **`elph run` completeness** — fork, file attachments, thinking level, tool filters, continue/session flags aligned with design.
4. **Provider login UX** — connect interactive or documented CLI path equivalent to `/login`.

### P1 — session lifecycle & power-user UX

5. Export / import sessions (JSONL minimum; HTML optional).
6. Fork / clone / name / session stats.
7. Compaction command + status feedback.
8. Settings UI or complete settings file surface (cache notices, thinking display, etc.).
9. Project trust first-run flow.

### P2 — modes & ecosystem

10. Decide RPC vs ACP strategy (document; implement chosen automation plane fully).
11. Themes + keybindings (if product wants pi-like customizability).
12. Prompt templates as `/name` end-to-end.
13. Extension story: deferred tools + entry renderers equivalent (WASM).
14. Doctor / stats / update CLI beyond stubs.

### Product (Elph-only — do not measure as pi lag)

15. Memory, codegraph, server, goals polish on their own roadmaps.

---

## Dependency note

Coding-agent product gaps often **depend on library parity** but are not solved by libraries alone:

| Product need             | Library prerequisite                               |
| ------------------------ | -------------------------------------------------- |
| Thinking `max` in UI/CLI | `elph-ai` / `elph-agent` levels (**library done**) |
| Deferred extension tools | `added_tool_names` + providers (**library done**)  |
| Compaction correctness   | harness estimate (**library done**)                |
| Session tree navigation  | session backends (**largely done in agent**)       |

Re-audit this file after product milestones; re-audit [pi-ai.md](./pi-ai.md) / [pi-agent.md](./pi-agent.md) when library mainstream moves.

---

## How to re-audit

```bash
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

Update **Last audited**, **Audit log**, and status cells. Prefer new audit rows over rewriting history.

---

## Related docs

- Product design: [docs/README.md](../README.md), [cli.md](../cli.md), [slash-commands.md](../slash-commands.md), [tui.md](../tui.md), [codebase-layout.md](../codebase-layout.md)
- Library ports: [pi-ai.md](./pi-ai.md), [pi-agent.md](./pi-agent.md)
- Porting index: [README.md](./README.md)
