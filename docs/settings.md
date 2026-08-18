# Settings

User preferences live in JSON. The host (`elph` / `coding-agent`) maps them into `elph-agent` / `elph-ai` options at session create. Those crates never read `settings.json`.

## Files

| Layer | Path | Role |
| --- | --- | --- |
| Defaults | (in code) | Serde field defaults |
| Home | `CONFIG_DIR/settings.json` (`~/.config/elph/` unless `ELPH_HOME`) | Global prefs; default write target |
| Project | `<cwd>/.elph/settings.json` | Per-repo overlay **when the project is trusted** |

Merge: defaults ← home ← project. Nested objects deep-merge. **Arrays replace** (a project `models.enabled` does not concatenate with home).

`trust.defaultProjectTrust` is **home-only**. A project file cannot change it.

Runtime `Settings::save` writes the home file only.

Schema for editors: `schemas/elph-schema.json`.

## Project trust

`trust.json` (`CONFIG_DIR/trust.json`) stores saved `/trust` decisions (folder + ancestors).

`trust.defaultProjectTrust`:

- `always` — load project settings, skills, prompts, and extensions even without a saved decision.
- `ask` or `never` — skip the project layer unless `/trust` (or an ancestor) already recorded yes. There is no interactive trust prompt in this version; `ask` behaves like `never` until a TUI prompt exists.

When the project layer is skipped, project `.elph/settings.json`, `.elph/skills`, `.elph/prompts`, `.elph/extensions`, and `.agents/skills` / `.agents/prompts` are not loaded.

## Groups

### Models (`models`)

- `defaultModel` / `defaultThinkingLevel` — seeds for **new** sessions only. Live model and thinking stay per-session.
- `scopedModels` — exact `provider/model_id` list for Ctrl+P / Scoped tab. Not stripped by `enabled`.
- `enabled` — glob filter for the catalog (`provider/model_id` or bare id). Empty = no filter. `*`, `!exclude`, `+exact`, `-exact`.
- `showConfiguredOnly` — picker All/Provider tabs only show providers with `auth.json` credentials.
- `thinkingBudgets` — optional `{ minimal, low, medium, high }` token budgets.
- `sessionTitleModel`, `compactionModel`, `treeBranchSummaries` — `inherit` or `provider/model_id`.
- `embed` — local embedding model for memory.

### Resources (`resources`)

- `skills` / `prompts` / `extensions` — extra paths (globs and `!`/`-` excludes).
- `disabledSkills` / `disabledExtensions` — name filters after discovery.
- `enableSkillCommands` — when false, skills stay in `list_skills` / the prompt catalog but are not registered as `/name` commands (`/skill:name` still works).

Built-in search order (last wins): bundled skills → `~/.agents/skills` → `CONFIG_DIR/skills` → (if trusted) project `.agents/skills` and `.elph/skills`.

WASM extension enable/disable is this group, not a sidecar `extensions.json`.

### Tools (`tools`)

- `default`: `null` / omitted — all builtins. `[]` — no builtins. Non-empty — allowlist of builtin names.
- `list_available_tools` and `list_skills` always stay.
- MCP and WASM tools are not filtered here. Ask/Plan mode still hides mutating tools.

### Other host knobs

- `ui.quietStartup` — hide bootstrap chatter unless `ELPH_QUIET` is already set (env wins).
- `compaction.thresholdPct`, `keepRecentTokens`, `reserveTokens`, `physicalPrune`. Auto-compact has no kill-switch.
- `shell.path`, `shell.commandPrefix` — `shell_exec` binary and command prefix.
- `network.httpProxy` — set as `HTTP_PROXY` / `HTTPS_PROXY` when those env vars are unset.
- `session.retention.*`, `workers.*`, `memory.*`, `notifications.*`, `mcp.*`, `ui.*` (theme, thinking display, density, …) as before.

## `/settings`

Prints home/project paths and group names. Interactive editor is not in this version; edit JSON and `/reload` or restart.
