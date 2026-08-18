# Settings

User preferences live in JSON. The host (`elph`) maps them into `elph-agent` / `elph-ai` at session create.

## Files

| File | Path | Role |
| --- | --- | --- |
| Settings | `CONFIG_DIR/settings.json` + `<cwd>/.elph/settings.json` | UI, models, memory, notifications, compaction, session, workers, resources, logging |
| MCP | `CONFIG_DIR/mcp.json` + `<cwd>/.elph/mcp.json` | Servers, policy, **cache TTL / max entries** |
| Trust | `CONFIG_DIR/trust.json` | Trusted directories + `defaultProjectTrust` (WASM extensions only) |
| Auth | `CONFIG_DIR/auth.json` | Credentials |

Merge for settings and MCP: defaults ← home ← **project always** (nested objects deep-merge; arrays replace). Runtime `Settings::save` writes **home** only.

Schema: `schemas/elph-schema.json`, `schemas/mcp-schema.json`.

## Shape (one nesting level)

```json
{
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
  "resources": { "skills": [], "disabledSkills": [], "enableSkillCommands": true },
  "logging": { "level": "info", "file": true, "rotation": "daily", "trace": true }
}
```

`notifications.onStartupReady` in a **project** `.elph/settings.json` overrides home. Restart or `/reload` after edits.

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

## Not in settings.json

- **MCP cache** — `mcp.json` keys `cacheTtlSecs` (default 60) and `cacheMaxEntries` (default 2048). Per-server `cacheTtlMs` still wins.
- **Trust** — `trust.json` keys `directories` and `defaultProjectTrust` (`ask` / `always` / `never`). Only gates **project WASM extensions**. `ask` has no prompt yet and behaves like `never`.
