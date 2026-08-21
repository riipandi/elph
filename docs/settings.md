# Settings

User preferences live in JSON. The host (`elph`) maps them into `elph-agent` / `elph-ai` at session create.

## Files

| File | Path | Role |
| --- | --- | --- |
| Settings | `CONFIG_DIR/settings.json` + `<cwd>/.elph/settings.json` | UI, models, memory, notifications, compaction, session, workers, resources, logging |
| MCP | `CONFIG_DIR/mcp.json` + `<cwd>/.elph/mcp.json` | Servers, policy, **cache TTL / max entries** |
| Trust | `CONFIG_DIR/trust.json` | Trusted directories + `defaultProjectTrust` (WASM extensions only) |
| Auth | `CONFIG_DIR/auth.json` | Credentials (`schemas/auth-schema.json`) |

Merge for settings and MCP: defaults ← home ← **project always** (nested objects deep-merge; arrays replace). Runtime `Settings::save` writes **home** only.

Schema: `schemas/elph-schema.json`, `schemas/mcp-schema.json`, `schemas/auth-schema.json`, `schemas/provider-schema.json`. Generated files stamp `$schema` (`https://elph.space/elph-schema.json`, `https://elph.space/mcp-schema.json`, `https://elph.space/auth-schema.json`, `https://elph.space/provider-schema.json`).

## Shape (one nesting level)

```json
{
  "$schema": "https://elph.space/elph-schema.json",
  "preferredChatLanguage": "english",
  "simplifiedTechnicalEnglish": true,
  "maxRetries": 2,
  "defaultTimeout": "120s",
  "quietStartup": false,
  "defaultTools": null,
  "shellPath": null,
  "shellCommandPrefix": null,
  "httpProxy": null,
  "ui": {
    "theme": "auto",
    "showThinking": true,
    "density": "compact",
    "showHiddenFiles": false,
    "turnStats": true
  },
  "models": {
    "defaultModel": null,
    "defaultThinkingLevel": "high",
    "scopedModels": [],
    "enabled": [],
    "embedModel": "allMiniLML6V2",
    "embedQuantized": true,
    "embedGpuAcceleration": "auto"
  },
  "notifications": {
    "enabled": true,
    "onStartupReady": true,
    "onTurnComplete": true
  },
  "memory": { "enabled": true, "topK": 8 },
  "compaction": { "thresholdPct": 80, "keepRecentTokens": 20000, "reserveTokens": 16384 },
  "session": { "enabled": true, "gcOnOpen": true, "maxSessionsPerCwd": 40 },
  "workers": { "enabled": true },
  "resources": { "skills": [], "prompts": [], "disabledSkills": [], "disabledPrompts": [], "enableSkillCommands": true },
  "logging": { "level": "info", "file": true, "rotation": "daily", "trace": true }
}
```

`notifications.onStartupReady` in a **project** `.elph/settings.json` overrides home. Restart or `/reload` after edits.

`resources.skills` / `resources.prompts` entries may carry a `!` or `-` prefix to exclude a skill/template by name or path (leading `~` expands to the home dir; a bare directory path excludes everything under it; a relative path like `.agents/skills` matches any project at that relative location), and `+` to force-include. `resources.disabledSkills` / `resources.disabledPrompts` drop entries by name glob after discovery.

## Memory (agent)

When `memory.enabled` is true, the harness injects ranked recall at **turn start** (`<memory_context>`, `<recent_work>`, `<project_map>`) and journals successful edits / tool errors at **turn end**. That injection is a seed. The coding system prompt renders the memory tool group and `## Memory` policy only when this flag is on.

The coding prompt also tells the model to call tools **mid-turn**:

- `memory_search` / `memory_recent` when the task pivots or injected blocks are empty/weak
- `memory_report` as soon as a user preference, durable insight, or failed-then-fixed approach appears (do not wait for turn end)
- `memory_contradict` when a recalled item is wrong

Routine file edits stay auto-journaled as `work`; the model must not re-report them.

## Logging

`logging` is applied at process start (restart after edits). Merge: defaults ← `settings.json` `logging` ← `ELPH_LOG_*` / `ELPH_TRACE` (env wins).

| Field | Default | Meaning |
| --- | --- | --- |
| `level` | `info` | rustlog spec (`info` or `elph_agent=debug,elph_ai=warn`) |
| `file` | `true` | Rolling JSONL at `APP_DATA/logs/elph.jsonl` |
| `rotation` | `daily` | `hourly` / `daily` / `size` |
| `maxFiles` | unset | Cap of retained rotated files |
| `maxBytes` | unset | Size trigger (`size` rotation defaults to 10 MiB) |
| `trace` | `true` | `APP_DATA/logs/elph-traces.jsonl` |

Crash reports: `APP_DATA/logs/crash-YYMMDDhh.jsonl` (UTC hour). Console JSONL is not a settings key; the `elph` binary keeps stderr logging off.

See [elph-agent observability](../crates/elph-agent/docs/observability.md).

## Doctor

`elph doctor` reports terminal / multiplexer / SSH / color / clipboard (same class of facts as `grok doctor`), then config health: writable dirs, settings and MCP parse, `auth.json` (provider ids only), skill/template/agent load, `store.db` presence, and git HEAD. Findings include a one-line remediation. It does not create a session, write the clipboard, or print secrets.

`elph doctor --json` (`schemaVersion` 1) is the snapshot to attach to a bug report. Also attach `APP_DATA/logs/elph.jsonl` and any `crash-*.jsonl`. Never attach `auth.json`. Connectivity probes stay on `elph mcp doctor`. There is no `doctor fix` — Elph does not rewrite shell or tmux config.

## Not in settings.json

- **MCP cache** — `mcp.json` keys `cacheTtlSecs` (default 60) and `cacheMaxEntries` (default 2048). Per-server `cacheTtlMs` still wins.
- **Trust** — `trust.json` keys `directories` and `defaultProjectTrust` (`ask` / `always` / `never`). Only gates **project WASM extensions**. `ask` has no prompt yet and behaves like `never`.
