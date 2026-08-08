# System Prompt Context Efficiency

Reduces the per-turn system prompt token footprint for the coding agent without
degrading structure, tool discipline, or memory/skill recall quality.

## Skill relevance gating (`metadata.scope`)

`format_skills_for_system_prompt` (`crates/elph-agent/src/agent/harness/system_prompt.rs`)
now gate skill visibility per-session via the `filter_skills_for_context` helper,
keyed on the soft-typed frontmatter key `metadata.scope`:

| `metadata.scope`          | Behavior                                                       |
|---------------------------|----------------------------------------------------------------|
| *(unset)*                 | Always visible (backward compatible)                           |
| `global`                  | Always visible                                                 |
| `project`                 | Visible only while `cwd` is inside the skill's project root    |
| anything else             | Treated as unset (never silently hides a skill)                |

A project-scoped skill must live at `<project>/.agents/skills/<name>/SKILL.md`
(or `<project>/.elph/skills/...`). The project root is reconstructed from the
`file_path` as the fourth directory up from `SKILL.md`, i.e. one above the
`.agents`/`.elph` skills directory. On-disk symlinks are resolvable because the
matched paths come from the loaded skill records themselves.

Example (`docs` repo is a Rust project, so Go skills are only global):

```yaml
---
name: rust-lean-refactor
description: Reorganize Rust code to be lean, clean, and non-bloated.
metadata:
    scope: project
---
```

Designed so that all existing skills (no `scope`) behave exactly as before.
Full set of skills remains reachable via the slash palette / dispatch; the
filter only removes *model-visible advertisements* in `<available_skills>`.

Measured (tokenx estimator, same module used by compaction) on the dev machine's
18-skill set + 33 native/memory tools, cwd `/tmp/elph-project`:

| Mode  | Before (full) | After (full) | Before `<available_skills>` | After `<available_skills>` |
|-------|---------------|--------------|-----------------------------|----------------------------|
| Build | 4083          | 3704         | 1279                        | 913                        |
| Plan  | 3876          | 3497         | 1279                        | 913                        |
| Brave | 4048          | 3669         | 1279                        | 913                        |

See `docs/archive/prompt-baseline-2026-08-08.md` for the full measurement.

## `list_available_tools` filter

`create_list_available_tools` (`crates/elph-agent/src/tools/list_available_tools.rs`)
accepts an optional `name_prefix` argument. Passing it (e.g. `mcp_github__`)
returns only tools whose exposed name starts with that prefix; omitting it keeps
the old all-catalog behavior. This is the primitive a future lazy MCP
registration or `/tools` slash command can build on.

## MCP tool exposure note

Today every connected MCP server's tools are emitted into the model's active
tool set for every turn (`crates/coding-agent/src/agent/runtime.rs`, and
`crates/elph-agent/src/agent/harness/setters.rs` `set_tools`). A dedicated
follow-up plan is required to make MCP tool schemas lazy (see
`docs/planned/system-prompt-revamp.md`, Phase 4).
