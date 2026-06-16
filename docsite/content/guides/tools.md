---
title: Built-in Tools
summary: "Built-in tools catalog, exposure rules, execution, and approvals."
weight: 100
---

See also: [Agent Runtime](/guides/agent-runtime/) (execution flow), [Slash Commands](/guides/slash-commands/) (diagnostic helpers).

## Permissions

| Permission          | Description                                           |
|---------------------|-------------------------------------------------------|
| `auto-allow`        | Runs without approval by default; user can require it |
| `requires-approval` | Requires user approval before each run                |
| `always-approve`    | Always runs without approval; cannot be restricted    |

## File Tools

File tools handle reading, writing, and searching the local filesystem - the foundation for code analysis
and modification tasks.

| Tool          | Default Approval  | Description                                                                                                               |
|---------------|-------------------|---------------------------------------------------------------------------------------------------------------------------|
| Read          | Auto-allow        | Read a text file (supports line_offset and n_lines; negative offset = tail read). Fails on directories                    |
| Write         | Requires approval | Create, overwrite, or append to a file (supports mode: "overwrite" or "append"). Fails on directories                     |
| Edit          | Requires approval | Edit a file using string replacement (supports replace_all, no-op guard). Fails on directories                            |
| Grep          | Auto-allow        | `ripgrep` powered full-text search (supports context_lines)                                                               |
| Glob          | Auto-allow        | Find files and list directory contents by glob pattern. Use pattern "dir/**" to recursively list all files in a directory |
| ReadMediaFile | Auto-allow        | Read an image or video file                                                                                               |

## Shell Tools

| Tool | Default Approval  | Description                                                                          |
|------|-------------------|--------------------------------------------------------------------------------------|
| Bash | Requires approval | Execute a shell command (supports cwd and timeout overrides; default 120s, max 300s) |

## Web Tools

| Tool       | Default Approval | Description                                     |
|------------|------------------|-------------------------------------------------|
| FetchURL   | Auto-allow       | Fetch the content of a specified URL            |
| WebSearch  | Auto-allow       | Web search (supports limit and include_content) |
| CodeSearch | Auto-allow       | Search code on GitHub                           |

## Plan Mode

| Tool          | Default Approval | Description                        |
|---------------|------------------|------------------------------------|
| EnterPlanMode | Auto-allow       | Enter Plan mode                    |
| ExitPlanMode  | Auto-allow       | Exit Plan mode and submit the plan |

`ExitPlanMode` will requires user to confirm the plan.

## State Management

| Tool     | Default Approval | Description              |
|----------|------------------|--------------------------|
| TodoList | Auto-allow       | Manage a task to-do list |

TodoList maintains a visible **Tasks** panel in the TUI across multi-step operations. State is stored
in-memory on `Session.todoStore` and persisted to
`<workDir>/.agents/elph/metadata/<sess_id>/todos.jsonl` (latest snapshot only — the file is
overwritten on change and removed when the list is empty). A new session ID does not inherit another
session's todos.

The `todos` parameter accepts an array where each item has a `title` and status
(`pending` / `in_progress` / `done`). Omitting `todos` queries the current list; passing an empty
array clears it. When all items reach `done`, the Tasks panel hides and the TUI posts a system
notice in the chat area listing completed tasks.

Implementation: `pkg/tools/todolist` (apply/parse), `internal/runtime/todolist.go` (execute +
persist), `internal/renderer/todo_panel.go` (panel UI).

## Goal Tools

Goal tools let the agent create, inspect, and manage a session goal. The goal lifecycle:
`active` → `complete` / `blocked` / `paused` → `active` (resume). Tools are always exposed
but return clear error messages when no goal exists.

| Tool         | Default Approval | Description                                   |
|--------------|------------------|-----------------------------------------------|
| CreateGoal   | Auto-allow       | Create a new goal with a verifiable objective  |
| GetGoal      | Auto-allow       | Get the current goal status and usage          |
| UpdateGoal   | Auto-allow       | Update goal lifecycle status                   |
| SetGoalBudget | Auto-allow       | Set a token, turn, or time budget for the goal |

**CreateGoal** accepts `objective` (required), optional `completionCriterion`, and `replace`
(overwrite existing active/paused goal). Returns the created goal snapshot.

**GetGoal** takes no parameters and returns the current goal objective, status, turn/token
counters, and any budget limits.

**UpdateGoal** accepts `status`: `active` (resume paused), `complete`, `blocked`, or `paused`.
Only valid transitions are accepted; invalid ones return an error.

**SetGoalBudget** accepts `value` (positive number) and `unit` (`turns`, `tokens`, `milliseconds`, `seconds`, `minutes`, `hours`). Time budgets are validated between 1 second and 24 hours. Sets the budget limit for the current goal.

Implementation: `pkg/tools/goal` (types + manager), `internal/runtime/exec/goal.go` (execute),
`pkg/tools/schema` (provider schema). Goal state is scoped to the session and managed via
context.

## Collaboration Tools

Collaboration tools handle inter-Agent coordination, user interaction, and Skill invocation.

| Tool    | Default Approval | Description                                        |
|---------|------------------|----------------------------------------------------|
| AskUser | Auto-allow       | Ask the user a question to gather structured input |
| Skill   | Auto-allow       | Invoke a registered inline Skill                   |

`Skill` allows the Agent to actively invoke a registered inline-type Skill. Accepts `skill` (the Skill name)
and optional `args` (additional argument text). Only `type = "inline"` Skills can be called via this tool;
Skills with `disableModelInvocation: true` are rejected. Maximum nesting depth is 3 levels.

## Provider API exposure

Elph uses **native tool calling** (OpenAI `tools` / Anthropic `tools`) when a provider is
configured. The model receives only a filtered subset of built-in tools—not every tool listed
above.

Three layers decide what the model can see and what the runtime can run:

| Layer            | Purpose                                       | Source                         |
|------------------|-----------------------------------------------|--------------------------------|
| **Catalog**      | Shown in prompts and UI; full built-in list   | `pkg/tools/catalog`            |
| **Provider API** | JSON schemas sent to OpenAI / Anthropic       | `ProviderDefinitions()`        |
| **Runtime**      | Actually executed when the model calls a tool | `internal/runtime.ExecuteTool` |

A tool is sent to the provider API only when **all** of the following are true
(`IsProviderExposed`):

1. It is a known built-in (`Get`).
2. Its default approval is `auto-allow` or `requires-approval` (runtime gates the latter via huh).
3. The runtime can execute it (`IsExecutable`).
4. It has a provider JSON schema (`providerSchema`).

Today **Read**, **Write**, **Edit**, **Grep**, **Glob**, **ReadMediaFile**, **WebSearch**, **AskUser**,
**Bash**, **TodoList**, **Skill**, **CreateGoal**, **GetGoal**, **UpdateGoal**, and **SetGoalBudget** are exposed.
**AskUser** opens a huh question dialog. **Write**, **Edit**, and **Bash** show an approval dialog
unless agent mode is
**brave** or the user chose **allow for session** earlier in the TUI session. Auto-allow tools like
**FetchURL** and **CodeSearch** stay out until `IsExecutable` returns true for them.

### User approval (huh)

Requires-approval tools (**Write**, **Edit**, **Bash**) block in `runToolCall` until the renderer
answers `InteractTool` (`pkg/core/agent/interact.go`, `internal/renderer/tool_interact.go`). Completed
tool output appears in a collapsible detail box — shell/Bash stays expanded; other tools collapse when
the body is long (see [TUI § Detail blocks](/guides/tui/#input-modes)).

| Choice                | Shortcut | Effect                                                                              |
|-----------------------|----------|-------------------------------------------------------------------------------------|
| **Allow once**        | `y`, `1` | Run this call only; next Bash in the same or a later turn prompts again             |
| **Allow for session** | `a`, `2` | Skip approval for requires-approval tools until the TUI exits (`SessionAllowTools`) |
| **Deny**              | `n`, `3` | Return `User denied tool execution` to the model; do not run the command            |

- **Enter** on the default selection approves **once**.
- **Esc** on the approval form counts as **deny** (not cancel).
- After a deny, the same tool signature (e.g. `Bash` + identical `command`) is **auto-denied**
  for the rest of the **current agent turn** without showing the dialog again.
- **Brave** mode (`session.agentMode`) sets `SkipToolApproval` for the whole session UI — no huh
  prompt for requires-approval tools.

### Exposure vs approval vs execution

| Tool          | Default approval  | Provider API | Runtime (`IsExecutable`) |
|---------------|-------------------|--------------|--------------------------|
| Read          | Auto-allow        | Yes          | Yes                      |
| Grep          | Auto-allow        | Yes          | Yes                      |
| Glob          | Auto-allow        | Yes          | Yes                      |
| ReadMediaFile | Auto-allow        | Yes          | Yes                      |
| FetchURL      | Auto-allow        | No           | No                       |
| WebSearch     | Auto-allow        | Yes          | Yes                      |
| CodeSearch    | Auto-allow        | No           | No                       |
| EnterPlanMode | Auto-allow        | No           | No                       |
| ExitPlanMode  | Auto-allow        | No           | No                       |
| AskUser       | Auto-allow        | Yes          | Yes (huh question)       |
| TodoList      | Auto-allow        | Yes          | Yes                      |
| Skill         | Auto-allow        | Yes          | Yes                      |
| Write         | Requires approval | Yes          | Yes (huh confirm/brave)  |
| Edit          | Requires approval | Yes          | Yes (huh confirm/brave)  |
| Bash          | Requires approval | Yes          | Yes (huh confirm/brave)  |
| CreateGoal    | Auto-allow        | Yes          | Yes                      |
| GetGoal       | Auto-allow        | Yes          | Yes                      |
| UpdateGoal    | Auto-allow        | Yes          | Yes                      |
| SetGoalBudget | Auto-allow        | Yes          | Yes                      |

`requires-approval` tools are sent to the provider API when executable; **huh** gates each call unless
**brave** or **allow for session** applies. **AskUser** always uses huh before returning the answer
to the model.

**Bash** runs via `bash -c` in the workspace directory. Long-running commands (e.g. `ping` without
`-c`) are capped at **120s** (`defaultBashTimeout` in `internal/runtime/execute.go`). Output is
streamed to the TUI during execution (see [TUI § Native tool detail](/guides/tui/#native-tool-detail)).

**ReadMediaFile** reads image files under the workspace (`internal/runtime/media.go`,
`internal/mediaimage`). Supported formats decode via stdlib `image` plus `golang.org/x/image/webp`;
output is normalized PNG with metadata and base64 payload (32 KB tool-output cap). Video files return
`video files are not supported yet`. The catalog description mentions video for future work.

### User vision images (TUI paste)

Separate from **ReadMediaFile**, users can attach images to a turn when the active model supports
image input (`provider.SupportsImageInput` — footer shows **IMG**). **Ctrl+V** (or **Cmd+V** on
macOS) pastes a clipboard image into the input area; files are saved under
`<workDir>/.agents/elph/attachments/` and sent as `TurnOptions.UserImages` on submit
(`pkg/core/agent/messages.go`). OpenAI and Anthropic adapters map these to multimodal user messages.
Up to **4** images per message; each is downscaled (max dimension 1568) and re-encoded as PNG. When
the model does not support vision, pasted paths are appended to the text prompt instead so the agent
can call **ReadMediaFile**. See [TUI § Image attachments](/guides/tui/#image-attachments).

**WebSearch** queries the web via `pkg/tools/websearch` (ranking aligned with
[pi-extended/websearch](https://github.com/riipandi/pi-extended/tree/main/packages/websearch)). Engines:
**duckduckgo** (always available fallback), **jina** (optional `JINA_API_KEY`), **brave**
(`BRAVE_SEARCH_API_KEY`), **serpapi** (`SERPAPI_KEY`), **tavily** (`TAVILY_API_KEY`), **firecrawl**
(`FIRECRAWL_API_KEY`), **perplexity** (`PERPLEXITY_API_KEY`), **exa** (`EXA_API_KEY`). Omit `engine`
to auto-select the best configured backend; on failure, tries other configured engines and falls back
to DuckDuckGo last.

### Request flow

```mermaid
sequenceDiagram
    participant Session as internal/runtime/session
    participant Loop as pkg/core/agent/loop
    participant Tool as pkg/tools
    participant Provider as pkg/ai/provider
    participant Runtime as internal/runtime/execute

    Session->>Loop: StartTurn (ToolsEnabled, ExecuteTool)
    Loop->>Tool: FilterProviderTools / ProviderDefinitions
    Tool-->>Loop: Read, Write, Edit, Grep, Glob, ReadMediaFile, WebSearch, AskUser, Bash schemas
    Loop->>Provider: Complete(TurnRequest.Tools)
    Provider-->>Loop: tool_calls / tool_use
    Loop->>Loop: InteractTool (AskUser / approval)
    Loop->>Runtime: ExecuteTool / ExecuteToolStream
    Runtime->>Tool: IsExecutable(name)
    Runtime-->>Loop: output or error (streamed chunks via EventToolCallOutputDelta)
    Loop->>Provider: tool_result follow-up message
```

The agent loop runs up to 25 tool rounds (configurable via `maxToolIterations` setting, 0 = default 25).
Each round: completion with tools → execute calls → append tool results → complete again until
the model stops calling tools.

### Key functions

| Function                | Package            | Role                                                                                    |
|-------------------------|--------------------|-----------------------------------------------------------------------------------------|
| `ProviderDefinitions()` | `pkg/tools`        | Built-in schemas, then filtered                                                         |
| `FilterProviderTools()` | `pkg/tools`        | Filters any `[]provider.ToolDefinition`                                                 |
| `IsProviderExposed()`   | `pkg/tools`        | Single-tool API exposure check                                                          |
| `IsExecutable()`        | `pkg/tools`        | Whether runtime can run the tool                                                        |
| `ProviderSchema()`      | `pkg/tools/schema` | JSON Schema per built-in                                                                |
| `runProviderLoop()`     | `pkg/core/agent`   | Native tool loop                                                                        |
| `InteractTool()`        | `pkg/core/agent`   | AskUser + approval via huh (renderer)                                                   |
| `ExecuteTool()`         | `internal/runtime` | Read / Write / Edit / Grep / Glob / ReadMediaFile / WebSearch / TodoList / Skill / Bash |

Provider adapters map definitions to API formats:

- OpenAI-compatible: `pkg/ai/providers/openaicompat` (wraps `openai` + compat hooks)
- OpenRouter: `pkg/ai/providers/openrouter` (reasoning extra body on top of openaicompat)
- Anthropic: `pkg/ai/providers/anthropic` ([anthropic-sdk-go](https://github.com/anthropics/anthropic-sdk-go))

### Adding a new API-exposed tool

To expose a built-in to the model API end-to-end:

1. **Schema** — Add or extend `ProviderSchema()` in `pkg/tools/schema/schema.go`.
2. **Execution** — Implement the handler in `internal/runtime/execute.go` and add the name to
   `IsExecutable()` in `pkg/tools/exposure/exposure.go`.
3. **Approval** — If the tool should require user approval, keep
   `DefaultApproval: ApprovalRequiresApproval` in `pkg/tools/catalog/catalog.go`; it will not be API-exposed
   until approval is wired. Use `auto-allow` only for safe, read-only (or otherwise pre-approved)
   operations.
4. **Tests** — Update `pkg/tools/schema/schema_test.go`, `pkg/tools/exposure/exposure_test.go`, and runtime tests for the new executable.

No change to `ProviderDefinitions()` is required: filtering is driven by `IsProviderExposed`.

### Text markup fallback

When native tools are disabled or the model emits XML-style `<toolcall>` markup in text, a separate
parser path (`pkg/core/agent/toolcall*.go`) still handles legacy invocations. That path is
independent of provider API filtering; keeping the API list aligned with `IsExecutable` avoids the
model calling tools that cannot run.
