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

## `list_available_tools` filter + lazy activation

`create_list_available_tools` (`crates/elph-agent/src/tools/list_available_tools.rs`)
accepts an optional `name_prefix` argument. Passing it (e.g. `mcp_github__`)
returns only tools whose exposed name starts with that prefix; omitting it keeps
the old all-catalog behavior.

When `name_prefix` is provided, the tool result also sets `added_tool_names` to
the matched tool names. The harness run-loop consumes `added_tool_names` in its
`after_tool_call` hook (`crates/elph-agent/src/agent/harness/run_loop/loop_config.rs`):
names that exist in the registry but are not yet active are added to
`active_tool_names` (persisted durably, same path as `set_active_tools`) — this is
the **lazy tool activation** primitive. A model flow that wants a specific MCP
server's tools can request `list_available_tools(name_prefix: "mcp_github__")`,
and those tool schemas become active from the next turn.

MCP tools are still **registered** eagerly (`runtime.rs` / `create_agent_tools`)
so they remain executable and appear in the `list_available_tools` catalog, but
they are **default-inactive** on the model-visible wire:

- `AgentModePolicy::active_tool_names_for_mode` excludes all `mcp_*` names.
- Coding session bootstrap seeds `active_tool_names` without MCP (`runtime.rs`).
- `list_available_tools` catalogs the **full registry** (including inactive MCP).
- Prefix filter sets `added_tool_names`; the harness `after_tool_call` hook calls
  `activate_lazy_tools`, which also extends the collaboration-mode baseline.
- `reconcile_harness_tools` re-merges already-activated, mode-allowed MCP tools
  so hot-reload and mode switches do not wipe a mid-session lazy load.
- **Execution registry** (`AgentLoopConfig.execution_tools`) keeps the full tool
  map for dispatch. `prepare_tool_call` falls back to it when a name is missing
  from the active context — so a model that learned schemas from the catalog can
  still invoke the tool (no more `Tool … not found` solely because activation
  lagged or was skipped). First MCP call also auto-activates the name for later
  turns. Subagents receive the full parent registry with the parent's active set
  (MCP stays inactive until listed/called).

Prompt guidance (`coding_base.md`) tells the model to pass `name_prefix` (e.g.
`mcp_deepwiki__`) to activate. Full-harness coverage in
`crates/elph-agent/tests/harness.rs`:
`harness_lazy_activates_mcp_tools_via_list_available_tools`,
`harness_executes_inactive_mcp_tools_from_execution_registry`.

## On-demand skill discovery (`list_skills`)

`create_list_skills_tool` (`crates/elph-agent/src/tools/list_skills.rs`) exposes the
complete skill catalog to the model as a regular (non-lazy) tool. It accepts an
optional `relevance` argument (`all` | `project` | `global`) so the model can ask
for only project-scoped skills, global skills, or the full set — including skills
that were filtered out of `<available_skills>` by relevance gating.

The tool is wired in `BuiltinToolsBuilder::with_skills` when a skill set is
loaded, is kept available across mode reconciliations (`tools_catalog.rs`), and
is whitelisted in Ask/Plan mode policies (`tool_policy.rs`). Skills therefore
remain fully reachable by the model even when relevance filtering hides their
advertisement from the static prompt.

## Caller-side consistency (`format_skills_for_context`)

The single combined entry point `format_skills_for_context(skills, cwd)` (filter
+ XML render) is exported (`elph_agent`) and used by the coding-agent prompt
builder. Other hosts using `PromptAssemblyMode::Full` append `skills_section`
verbatim, so they should call `format_skills_for_context` (or pre-filter with
`filter_skills_for_context`) before setting `skills_section` — the builder itself
keeps rendering exactly what it receives.
