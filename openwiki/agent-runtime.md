---
type: Reference
title: Agent Runtime
description: App-agnostic AI agent runtime — AgentHarness, sessions, turn loop, compaction, goals, subagents, skills, prompt template engine, and built-in tools.
tags: [agent, runtime, elph-agent, harness, sessions, prompt, templates]
---

# Agent Runtime

The agent runtime module (`crates/elph-agent`) is the core of Elph's AI interaction engine. It provides an app-agnostic runtime with session persistence, tool execution, context compaction, goals, and subagent orchestration.

## Architecture

```
User Input → CLI/TUI
                ↓
         AgentHarness ─── Session (persistence)
                │
          Agent Loop ─── Stream completion → Tool call → Result → repeat
                │
        ┌───────┴────────┬───────────┐
        ↓                ↓           ↓
     Tools           Skills      MCP Servers
  (read_file, shell_exec, (SKILL.md   (external tools
   edit_file, grep,  files)      via rmcp)
   web_search, ...)
```

## AgentHarness

**File**: `/crates/elph-agent/src/agent/harness/mod.rs`

`AgentHarness<S>` is the central stateful runner. It wraps the low-level agent loop with:

- **Session persistence** via pluggable `SessionStorage` (filesystem `SessionDirStorage`, `TursoSessionStorage`, or `InMemorySessionStorage`)
- **Hook system** — `HookRegistry` with typed events: `BeforeAgentStart`, `BeforeProviderRequest`, `AfterProviderResponse`, `ToolCall`, `ToolResult`, `SessionCompact`, `SessionTree`, etc.
- **Compaction** — Automatic context window management
- **Plan mode** — Model must propose a plan before using mutating tools
- **Branch summarization** — Summarize branches during tree operations
- **Subagent support** — Spawn and control subagents
- **Goals** — Persisted session objectives

### Turn lifecycle

```
User message
  → assemble system prompt + resources + history
  → stream completion (with tool schemas)
  → [tool call?]
      → approve / ask user
      → execute tool
      → optionally TOON-encode result
      → append result
      → repeat until model stops calling tools
  → persist history + emit turn_done
```

Source: `/crates/elph-agent/src/agent/harness/run_loop/`

### Key harness events

Hooks are registered through `Harness::on_<event>()` methods. Event types defined in `/crates/elph-agent/src/agent/harness/hooks.rs`:

| Event                   | Trigger                   | Use case                        |
| ----------------------- | ------------------------- | ------------------------------- |
| `BeforeAgentStart`      | Before first completion   | Inject context, modify state    |
| `BeforeProviderRequest` | Before each provider call | Modify payload, add tools       |
| `AfterProviderResponse` | After each response       | Transform response, logging     |
| `ToolCall`              | Before tool execution     | Approval gate, logging          |
| `ToolResult`            | After tool result         | Transform result, TOON encoding |
| `SessionCompact`        | After compaction          | UI update, logging              |
| `SessionTree`           | After tree operation      | UI update                       |
| `SavePoint`             | Checkpoint reached        | Custom persistence              |

## Sessions

**File**: `/crates/elph-agent/src/session/`

Sessions are tree-structured: you can fork, branch, and resume. Each session has an ID (Kalid — 16 char, time-sortable, no prefix), metadata, and conversation history.

### Backends

| Backend                  | Storage                          | When to use               |
| ------------------------ | -------------------------------- | ------------------------- |
| `SessionDirStorage`      | Filesystem (`~/.elph/sessions/`) | Default local development |
| `TursoSessionStorage`    | Turso/libSQL database            | Production / shared state |
| `InMemorySessionStorage` | In-memory HashMap                | Testing                   |

### Session tree operations

- **Fork** — Create a new session from a point in history
- **Branch** — Summerize a branch for context
- **Resume** — Load a session by ID and continue
- **Export/Import** — Archive and restore sessions

Key files:

- `/crates/elph-agent/src/session/backends/session_dir/` — Filesystem backend
- `/crates/elph-agent/src/session/backends/turso/` — Turso backend
- `/crates/elph-agent/src/session/tree.rs` — `Session` struct, branch summarization
- `/crates/elph-agent/src/session/repo.rs` — Session repository (CRUD)

## Agent Loop

**File**: `/crates/elph-agent/src/runtime/`

The low-level turn runner that handles the stream → tool call → result → repeat cycle. It operates on `AgentContext` (messages + state) and `AgentLoopConfig` (tools, mode, limits).

- Default **25 tool iterations** per turn (`maxToolIterations`)
- Returns an `AgentEventStream` for real-time event consumption
- Supports `continue` mode for multi-turn interactions

Key files:

- `/crates/elph-agent/src/runtime/run_loop/` — The core loop implementation
- `/crates/elph-agent/src/runtime/stream.rs` — Event stream types
- `/crates/elph-agent/src/runtime/exec/` — Tool call dispatch, execution, and failure handling

## Prompt System

**File**: `/crates/elph-agent/src/prompt/`

The prompt module provides a MiniJinja-based template engine for assembling system prompts, replacing ad-hoc string formatting.

### Template engine

`PromptTemplateEngine` (`template.rs`) wraps a MiniJinja `Environment` with custom delimiters (`${{ }}` for variables, `${% %}` for blocks) to avoid collisions with Markdown and code. Templates are embedded at compile time via `include_str!()`.

### SystemPromptBuilder

`SystemPromptBuilder` (`system_builder.rs`) uses a builder pattern to assemble prompts from layered sources:

| Layer       | Source                                       | Description                                         |
| ----------- | -------------------------------------------- | --------------------------------------------------- |
| **Base**    | `templates/base.md`                          | Persona, working directory, date, OS, shell, skills |
| **Domain**  | Domain-specific (e.g. `coding_base.md`)      | Action safety, tool calling, output formatting      |
| **Mode**    | Mode-specific appendix (`mode_ask.md`, etc.) | Per-mode guidance and tool restrictions             |
| **Project** | AGENTS.md                                    | Pi-style XML `<project_context>` wrapper            |

Assembly modes: `Extend` (base + domain + mode + project context) or `Full` (domain-only with dedup checks).

### Context variables

`SystemPromptTemplateContext` (`context.rs`) provides serializable template variables:

- `persona`, `working_directory`, `current_date`, `os_name`, `shell_path`
- `agent_mode` — slugs: `build`, `plan`, `ask`, `brave`
- `active_tool_names` — per-turn `Vec<String>` for `<available_tools>` listing
- `tools` — `ToolNamesContext` (per-tool name resolution) and `ToolByKindContext` (category aliases for read, edit, shell_exec)
- `skills_section`, `mode_section`, `agents_md`

### Feature gate

The template engine is gated behind the `prompt-templates` Cargo feature (default-on via the `full` feature meta-flag), requiring the `minijinja` dependency with `custom_syntax`.

Key files:

- `/crates/elph-agent/src/prompt/template.rs` — `PromptTemplateEngine`
- `/crates/elph-agent/src/prompt/system_builder.rs` — `SystemPromptBuilder`, `PromptAssemblyMode`
- `/crates/elph-agent/src/prompt/context.rs` — Context types and builder
- `/crates/elph-agent/src/prompt/defaults.rs` — Default persona and resolve helpers
- `/crates/elph-agent/templates/base.md` — Embedded base template
- `/crates/elph-agent/tests/prompt_template.rs` — Template engine tests (requires `prompt-templates`)

### Binary crate integration

`elph/src/agent/prompt/builder.rs` provides `build_coding_system_prompt()` which layers:

1. `base.md` — generic persona + env + skills
2. `coding_base.md` — Grok-style sections (`<action_safety>`, `<tool_calling>`, `<output_efficiency>`, `<formatting>`)
3. Mode section from `modes.rs` — `<mode_context>` with per-mode guidance + mode appendix template
4. Project context — AGENTS.md in `<project_context>` wrapper

Mode-specific appendix templates live in `elph/templates/agent/`:

| Template        | Mode  | Behavior                                      |
| --------------- | ----- | --------------------------------------------- |
| `mode_build.md` | Build | Full tool access, approval may be required    |
| `mode_brave.md` | Brave | Full autonomy, no approval prompts            |
| `mode_plan.md`  | Plan  | Read-only, requires `<proposed_plan>` wrapper |
| `mode_ask.md`   | Ask   | Read-only Q&A, no mutating tools              |

### /system-prompt slash command

The `system_prompt_slash.rs` module provides a diagnostic command (`/system-prompt`, `/prompt`) that assembles and displays the current compiled system prompt in a TUI dialog. The prompt is rebuilt live from session state each time it is opened.

## Compaction

**File**: `/crates/elph-agent/src/compaction/`

Automatic context window management to stay within model context limits.

### Sub-modules

| Module                    | Purpose                                                     |
| ------------------------- | ----------------------------------------------------------- |
| `compact.rs`              | Main compaction orchestrator                                |
| `summarization.rs`        | Generate summaries of conversation segments                 |
| `estimation.rs`           | Token counting, context usage estimation, cut-point finding |
| `preparation.rs`          | Prepare entries for compaction                              |
| `branch_summarization.rs` | Summarize branch context for tree operations                |
| `types.rs`                | `CompactionDetails`, `CompactionResult`                     |
| `utils.rs`                | File operation extraction, conversation serialization       |

### Auto-compact

When enabled in settings (`auto_compact_context: true`), the harness automatically compacts when context usage exceeds a threshold.

## Goals

**File**: `/crates/elph-agent/src/goals/`

Persisted session objectives with auto-steering. The agent maintains a goal stack and can be steered toward completion.

| Component             | Purpose                                                  |
| --------------------- | -------------------------------------------------------- |
| `GoalStore`           | Persisted goal storage                                   |
| `GoalRuntime`         | Goal lifecycle management (start, pause, resume, cancel) |
| `create_goal_tools`   | Agent tools for goal management                          |
| `GoalAccountingState` | Token accounting per goal                                |

Goals are managed via the `/goal` slash command with subcommands: `status`, `pause`, `resume`, `cancel`, `replace`, `next`, and direct creation.

## Subagents

**File**: `/crates/elph-agent/src/agent/subagent/`

Codex-style multi-agent orchestration. The main agent can spawn subagents for parallel tasks, then merge results.

| Component             | Purpose                                   |
| --------------------- | ----------------------------------------- |
| `AgentControl`        | Control mechanism: spawn, signal, collect |
| `SubagentHarness`     | Harness for running subagents             |
| `AgentRegistry`       | Registry of active subagents              |
| `SubagentSpawnConfig` | Configuration for spawning subagents      |
| `AgentGraphStore`     | Graph of subagent relationships           |

## Skills

**File**: `/crates/elph-agent/src/skills/`

Skills provide reusable instructions for specific tasks. They follow the [agentskills.io](https://agentskills.io) specification.

### SKILL.md format

```markdown
---
name: my-skill
description: What this skill does
license: MIT
compatibility: Requires git
allowed-tools: read grep shell_exec
argument-hint: <file-path>
---
```

Frontmatter fields include `name`, `description`, `license`, `compatibility`, `allowed-tools`, `disable-model-invocation`, `metadata`, and `argument-hint`. The `argument-hint` field describes expected arguments (e.g. `"<file-path>"`) and supports `<placeholder>` syntax for required args. Skills with required arguments show a validation notice when invoked without args. Validation rules: name must match parent directory, be lowercase+digits+hyphens, ≤64 chars; description ≤1024 chars.

Skills are discovered from `.agents/skills/` directories in the project, home, and `~/.elph/skills/` directories. They are loaded into the system prompt as metadata; the agent reads the full body when relevant.

Key files:

- `/crates/elph-agent/src/skills/load/mod.rs` — Skill discovery and parsing
- `/crates/elph-agent/src/skills/load/parse.rs` — Frontmatter parsing and validation
- `/crates/elph-agent/src/skills/format.rs` — Skill formatting for system prompt
- `/crates/elph-agent/src/skills/args.rs` — Argument hint parsing, validation, and notice formatting
- `/elph/src/agent/skills_load.rs` — Workspace-level skill loader with conflict detection and `/skill:<name>` slash parsing

## Built-in Tools

**File**: `/crates/elph-agent/src/tools/`

| Helper                        | Tools                                                                                          |
| ----------------------------- | ---------------------------------------------------------------------------------------------- |
| `create_edit_tools`           | `edit_file`, `write_file`, `shell_exec`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| `create_search_tools`         | `read_file`, `grep`, `find_path`, `list_dir`                                                   |
| `create_all_tools`            | All enabled filesystem tools (11 tools)                                                        |
| `create_web_tools`            | `web_search`, `web_fetch`                                                                      |
| `create_all_tools_with_web`   | Filesystem + web tools                                                                         |
| `create_collaboration_tools`  | Collaboration tools (spawn, send_message, followup_task, wait_agent, list_agents)              |
| `create_list_available_tools` | Meta-tool listing all available tools with descriptions and parameters                         |

All filesystem tools resolve paths through `ExecutionEnv` and run on blocking thread pools. Web tools do not use `ExecutionEnv`.

Key source files (each tool in its own module):

- `/crates/elph-agent/src/tools/read_file.rs`
- `/crates/elph-agent/src/tools/shell_exec.rs`
- `/crates/elph-agent/src/tools/edit_file.rs`
- `/crates/elph-agent/src/tools/write_file.rs`
- `/crates/elph-agent/src/tools/create_dir.rs`
- `/crates/elph-agent/src/tools/copy_path.rs`
- `/crates/elph-agent/src/tools/delete_path.rs`
- `/crates/elph-agent/src/tools/move_path.rs`
- `/crates/elph-agent/src/tools/grep.rs`
- `/crates/elph-agent/src/tools/find_path.rs`
- `/crates/elph-agent/src/tools/list_dir.rs`
- `/crates/elph-agent/src/tools/web/` — `web_search.rs` and `web_fetch.rs`
- `/crates/elph-agent/src/tools/collaboration.rs` — Collaboration tools (replaces `multi_agent.rs`)
- `/crates/elph-agent/src/tools/list_available_tools.rs` — Meta-tool for tool discovery
- `/crates/elph-agent/src/tools/fff_picker.rs` — File picker integration

## Modes

**File**: `/crates/elph-agent/src/collaboration/`

| Mode        | Description                                           |
| ----------- | ----------------------------------------------------- |
| **Default** | Normal agent interaction with full tool access        |
| **Plan**    | Agent must propose a plan before using mutating tools |

`CollaborationMode` enum drives tool filtering and system prompt modifications.

### Tools catalog reconciliation

**File**: `/elph/src/agent/tools_catalog.rs`

The `tools_catalog` module provides runtime tool permission management:

- `refresh_tools_catalog(harness, active_names)` — Rebuilds the `list_available_tools` meta-tool to reflect only the currently active tool set
- `reconcile_harness_tools(harness, mode, mcp_registry)` — Orchestrates full tool-permission setup per mode: calls `AgentModePolicy::active_tool_names_for_mode()` to determine which tools should be active, enters plan mode if required, and refreshes the catalog

This enables the agent to dynamically adapt its tool set based on collaboration mode and MCP server availability.

## Key source files

| Concern                   | Path                                                                                       |
| ------------------------- | ------------------------------------------------------------------------------------------ |
| Agent harness             | `/crates/elph-agent/src/agent/harness/mod.rs`                                              |
| Agent harness run loop    | `/crates/elph-agent/src/agent/harness/run_loop/`                                           |
| Agent harness hook system | `/crates/elph-agent/src/agent/harness/hooks.rs`                                            |
| Agent harness types       | `/crates/elph-agent/src/agent/harness/types/`                                              |
| Session tree              | `/crates/elph-agent/src/session/tree.rs`                                                   |
| Session backends          | `/crates/elph-agent/src/session/backends/`                                                 |
| Compaction                | `/crates/elph-agent/src/compaction/`                                                       |
| Goals                     | `/crates/elph-agent/src/goals/`                                                            |
| Subagents                 | `/crates/elph-agent/src/agent/subagent/`                                                   |
| Skills                    | `/crates/elph-agent/src/skills/`                                                           |
| Built-in tools            | `/crates/elph-agent/src/tools/`                                                            |
| Collaboration modes       | `/crates/elph-agent/src/collaboration/`                                                    |
| Agent loop / runtime      | `/crates/elph-agent/src/runtime/`                                                          |
| Types                     | `/crates/elph-agent/src/types/`                                                            |
| Execution env             | `/crates/elph-agent/src/runtime/local_env/`                                                |
| Messages                  | `/crates/elph-agent/src/messages/`                                                         |
| Event stream              | `/crates/elph-agent/src/runtime/event_stream.rs`                                           |
| Plugin/WASM host          | `/crates/elph-agent/src/plugins/`                                                          |
| Prompt template engine    | `/crates/elph-agent/src/prompt/` (template.rs, system_builder.rs, context.rs, defaults.rs) |
| Embedded templates        | `/crates/elph-agent/templates/base.md`                                                     |
| MCP client                | `/crates/elph-agent/src/tools/mcp/`                                                        |

## Change guidance

- **Agent loop changes**: Test in `crates/elph-agent/tests/agent_loop.rs` and `tests/harness.rs`
- **Session changes**: Test in `crates/elph-agent/tests/session.rs` and `tests/repo.rs`
- **Compaction changes**: Test in `crates/elph-agent/tests/compaction.rs`
- **Goals changes**: Test in `crates/elph-agent/tests/goals.rs`
- **Subagent changes**: Test in `crates/elph-agent/tests/subagent.rs`
- **Tool changes**: Test in `crates/elph-agent/tests/tools_fff.rs` and `tests/web_tools.rs`
- **Skills changes**: Test in `crates/elph-agent/tests/skills.rs`
- **Prompt template changes**: Modify templates in `/crates/elph-agent/templates/` and `/elph/templates/agent/`; test in `crates/elph-agent/tests/prompt_template.rs` (requires `prompt-templates` feature)
- **System prompt builder**: Modify `/crates/elph-agent/src/prompt/system_builder.rs` for assembly logic or `/elph/src/agent/prompt/builder.rs` for coding-domain prompts
- **Configuration**: Check `HarnessOptions`, `CompactionSettings`, `SessionStorage` generics
  nStorage` generics
