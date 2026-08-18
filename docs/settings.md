# Settings

User preferences live in JSON. The host (`elph`) maps them into `elph-agent` / `elph-ai` at session create.

## Files

| File | Path | Role |
| --- | --- | --- |
| Settings | `CONFIG_DIR/settings.json` + `<cwd>/.elph/settings.json` | UI, models, memory, notifications, compaction, session, workers, resources |
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
  "resources": { "skills": [], "disabledSkills": [], "enableSkillCommands": true }
}
```

`notifications.onStartupReady` in a **project** `.elph/settings.json` overrides home. Restart or `/reload` after edits.

## Not in settings.json

- **MCP cache** — `mcp.json` keys `cacheTtlSecs` (default 60) and `cacheMaxEntries` (default 2048). Per-server `cacheTtlMs` still wins.
- **Trust** — `trust.json` keys `directories` and `defaultProjectTrust` (`ask` / `always` / `never`). Only gates **project WASM extensions**. `ask` has no prompt yet and behaves like `never`.
