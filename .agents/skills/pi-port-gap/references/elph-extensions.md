# Elph extension scan hints

Starting checklist for **Phase 3 (Elph implementation delta)**. Always verify in
code; the list grows over time. These are **not** port gaps.

For each item found, write **In pi / In Elph / Implications** — not a one-line badge.

---

## elph-ai (usually no pi equivalent)

- **Hyper** — `models/hyper.json`, provider + OAuth; re-add after `generate-models`
- **Faux provider** — deterministic tests (`faux_*`)
- **Catalog tooling** — `bin/generate_models`, `define_catalog!` macros
- **Session resource cleanup** — `src/session_resources.rs` (confirm vs pi if later shared)

## elph-agent (product / runtime extensions)

- **MCP client** — `src/mcp/`  
  - config merge (home + project), schema validate  
  - transports: stdio, streamable HTTP, SSE  
  - auth: env/token vs OAuth store, conflict policy  
  - crypto: AES-256-GCM `enc:`, `auth.json` + `auth.key`  
  - registry, session pool, policy, truncate, events/progress  
  - tool names: `mcp_{server}__{tool}`
- **Goals** — `src/goals/`
- **Subagent** — `src/subagent/`
- **Plugins (WASM)** — `src/plugins/` (feature `extensions`)
- **Built-in tools** — `src/tools/` (read, bash, grep, web, …)
- **Mode / plan** — `src/mode/`
- **Sandbox** — `src/sandbox/`
- **Datastore / Turso** — `src/datastore/`, session Turso backends
- **Prompt encoding (TOON)** — `src/runtime/` / prompt encoding env
- **Harness extras** — richer than pi-agent-core (session hooks, compaction wiring)
- **Skills / prompt templates** — `src/skills/`, `src/prompt_templates/`

## How to confirm “missing in pi”

```bash
# From pi clone
ls packages/agent/src packages/ai/src
rg -n "mcp|MCP" packages/agent packages/ai --glob '!**/node_modules/**' | head
# From elph
ls crates/elph-agent/src
rg -n "pub mod" crates/elph-agent/src/lib.rs
```

If pi later adds a similar concept (e.g. native MCP), reclassify from
`[Elph delta]` toward `[Partial]` / convergence notes under **Parity and nuance**.
