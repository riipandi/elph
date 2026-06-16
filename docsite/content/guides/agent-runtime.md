---
title: Agent Runtime
summary: "How a user message becomes a provider completion."
weight: 100
---

How a user message becomes a provider completion, tool execution, and TUI update.

## Entry points

| Trigger                 | Handler                                                                                                             |
|-------------------------|---------------------------------------------------------------------------------------------------------------------|
| Normal chat input       | `runtime.Session.StartTurn` → `agent.RunTurn`                                                                       |
| Prompt template `/name` | Expand template → same as chat                                                                                      |
| `!cmd` / `!!cmd`        | `runtime.RunShell` / `RunShellContext` — optional follow-up agent turn                                              |
| No provider configured  | TUI blocks chat submit (model picker prompt); `agent.runPlaceholderTurn` only if a turn runs with `Provider == nil` |

`Session.StartTurn` (`internal/runtime/session.go`) injects:

- `SystemPrompt` from `prompt.Build` (see [Configuration § Project context](/configuration/configuration/#project-context-and-system-prompt))
- `Provider`, `Model` from session / settings
- `Messages` from `Session.History` when non-empty
- `ToolsEnabled = true` and `ExecuteTool` → `runtime.ExecuteTool` when provider is set

`prompt.Build` reads `preferedResponseLanguage` from settings (default `inherit`), discovers
`AGENTS.md` and `SKILL.md` entries, and injects current date, work dir, and `session.agentMode`
into the prompt. Response language follows the user’s message when set to `inherit`; a fixed value
or an explicit user request overrides it.

## Turn modes

`agent.RunTurn` (`pkg/core/agent/turn.go`):

```
if shell-context prompt → placeholder response
if no provider        → placeholder phases
if ToolsEnabled + ExecuteTool → runProviderLoop (native tools)
else                  → single Provider.Complete (no tools)
```

## Native tool loop

`runProviderLoop` (`pkg/core/agent/loop.go`):

- Max **25** iterations (configurable via `maxToolIterations` setting, `0` = default 25)
- Tools: `FilterProviderTools(opts.Tools)` or `tool.ProviderDefinitions()`
- Streams `EventResponseDelta`, `EventThinkingDelta`, `EventActivity`
- On `result.ToolCalls`: `EventToolCallStart` → `InteractTool` (if needed) → `ExecuteTool` or
  `ExecuteToolStream` → `EventToolCallOutputDelta` (shell tools) → `EventToolCallDone`
- Tool follow-ups after step 0 disable thinking for faster replies (e.g. after deny)
- Appends assistant + tool messages to `Messages`
- Ends with `TurnDoneWithHistoryEvent` (history for next turn)
Provider adapters:

- OpenAI: `tool_calls` in `pkg/ai/provider/openai.go`, `openai_tools.go`
- Anthropic: `tool_use` / `tool_result` in `anthropic.go`, `anthropic_tools.go`

## API tool filter

Only tools passing `IsProviderExposed` are sent to the provider:

- Today: **Read**, **Write**, **Edit**, **Grep**, **Glob**, **ReadMediaFile**, **WebSearch**, **AskUser**, **Bash**,
  **TodoList**, **Skill**, **CreateGoal**, **GetGoal**, **UpdateGoal**, **SetGoalBudget**
- Details: [Tools § Provider API exposure](/guides/tools/#provider-api-exposure)

## Runtime execution

`ExecuteTool` (`internal/runtime/execute.go`):

| Tool          | Implementation                                                            |
|---------------|---------------------------------------------------------------------------|
| Read          | Read file under workspace (256 KB cap, line_offset, n_lines)              |
| Write         | Create parent dirs and write/append file contents                         |
| Edit          | Exact string replace; `replace_all` for multi-match                       |
| Grep          | `rg` subprocess (`content`, `files_with_matches`, `count`, context_lines) |
| Glob          | `doublestar.FilepathGlob` (`**` semantics, files only)                    |
| ReadMediaFile | Decode/resize image → PNG metadata + base64 (32 KB cap)                   |
| WebSearch     | Multi-engine search (`pkg/tools/websearch`); 128 KB cap                   |
| FetchURL      | HTTP fetch with HTML extraction (`pkg/tools/fetchurl`)                    |
| CodeSearch    | GitHub/GitLab code search (`pkg/tools/codesearch`)                        |
| TodoList      | Session task list (`pkg/tools/todolist`); persists snapshot               |
| Skill         | Load and return skill body from registered `SKILL.md`                     |
| Bash          | `bash -c` via `RunShellContext`; streams stdout/stderr                    |
| CreateGoal    | Create a session goal with objective + optional criterion                 |
| GetGoal       | Return current goal snapshot (status, turns, tokens, budgets)             |
| UpdateGoal    | Update goal lifecycle status                                              |
| SetGoalBudget | Set token/turn/time budget for the current goal                           |


`ExecuteToolStream` (`session.toolExecuteStream`) passes chunks to `EventToolCallOutputDelta` for
live TUI updates. Bash validates syntax with `mvdan.cc/sh` before spawn and times out after 120s by
default.

Errors:

- `ErrToolUnknown` — not in `pkg/tools` catalog
- `ErrToolUnavailable` — known but `!IsExecutable`
- `ErrToolNotImplemented` — should not occur for current executables

## Text markup fallback

When the model writes XML-style tool tags in streamed text instead of native calls:

- `agent.StripToolCalls` parses and strips markup (`pkg/core/agent/toolcall*.go`)
- `StripExtractedPayloads` removes duplicate query text from the visible bubble
- Renderer applies stripping in `agent.go`, `agent_toolcall.go`, `stream.go`

System prompt discourages inventing `<toolcall>` tags (`internal/prompt/builder.go`).

Native tool calling is the primary path when a provider is configured.

## Agent events → TUI

`internal/renderer/agent_bridge.go`:

| Event                      | TUI effect                                                                                                                                                                     |
|----------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `EventActivity`            | Activity line + stopwatch                                                                                                                                                      |
| `EventThinkingDelta`       | Append to thinking block                                                                                                                                                       |
| `EventResponseDelta`       | Append to AI message; plain reflow while streaming; Glamour v2 after complete when markup detected; async `markdownRenderCmd` + copy hint footer (`markdown.go`, `ai_copy.go`) |
| `EventToolCallStart`       | Native tool detail box (running, `$ cmd` for Bash); TodoList skips box                                                                                                         |
| `EventToolCallOutputDelta` | Append streamed shell output to detail box                                                                                                                                     |
| `EventToolCallDone`        | Finalize detail status/body; TodoList updates Tasks panel / completion                                                                                                         |
| `EventTurnDone`            | Finalize turn, apply history, token/cost footer                                                                                                                                |

Native tool UI: `agent_native.go`, `tool_interact.go` (huh approval / AskUser). Text-markup tool
UI: `agent_toolcall.go`.

`toolInteractBridge` (`internal/renderer/tool_interact.go`) blocks the agent loop until the user
responds. Approval choices, session allow, and per-turn deny cache are documented in
[Tools § User approval](/guides/tools/#user-approval-huh).

## Agent modes

Modes: `build`, `plan`, `ask`, `brave` (`internal/constants`).

- Persisted in `settings.json` → `session.agentMode`
- Switched with **Ctrl+A** or footer click
- Shown in input border color and footer

**Runtime note:** Modes do not change system prompt or tool filter today. **Brave** skips huh
approval for requires-approval tools (`SkipToolApproval` in `buildTurnOptions`). **Build**, **plan**,
and **ask** behave the same at runtime for now.

## Thinking levels

Levels: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`.

- **Shift+Tab** cycles level in TUI
- Mapped per model via `thinkingLevelMap` in provider JSON
- Sent to provider as budget tokens (Anthropic) or `reasoning_effort` / compat formats (OpenAI-compatible)

## Session and logging

### Session ID

TypeID with prefix `sess` (`runtime.NewSession`). Shown in footer as `[sess_…]`.

### In-memory history

`Session.History []provider.ChatMessage` stores provider-native conversation including tool calls and results. Updated after each native-tool turn via `ApplyHistory`.

These limits keep idle and long-session RSS stable (~30 MB at rest after startup optimizations). See [Architecture § Performance and memory](/guides/architecture/#performance-and-memory) for git, catalog, and models.dev behavior.

History is compacted after every turn via `agent.CompactMessages`:

| Limit                         | Value   |
|-------------------------------|---------|
| Max messages                  | 32      |
| Max total size                | ~512 KB |
| Max tool result (API/history) | 32 KB   |
| Max tool result (TUI detail)  | 40 KB   |
| Max assistant message         | 64 KB   |
| Max AI bubble text (TUI)      | 48 KB   |

### Context-limit auto-compaction

When the provider returns a context-too-large error and `autoCompactContext` is `true` (default),
`agent.CompactMessagesForContext` aggressively reduces history and retries:

- Up to 3 retries with escalating aggressiveness (2×, 4×, 8× default limits)
- Floor: 4 messages / 16 KB minimum, 4 KB tool-result truncation floor
- No exponential backoff — compaction completes and retries immediately
- Percentage target controlled by `autoCompactLimit` setting (default 80%)

### Manual compaction (`/compact`)

The `/compact` slash command (alias `/c`) compacts history to a user-specified percentage
of the standard budget. With no argument, defaults to `autoCompactLimit`.
Result: "Reduced: N → M messages (X → Y)" shown in a detail block.


### User vision images

When the TUI attaches clipboard images and the model supports vision, `Session.StartTurn` passes
`TurnOptions.UserImages` (`[]provider.ImageAttachment`) into `agent.RunTurn`. `prepareTurnMessages`
appends a user message with `Images` set (text may be empty for image-only turns). Provider adapters
in `pkg/ai/providers/openai` and `pkg/ai/providers/anthropic` map these blocks to API image content.
Pasted files live under `<workDir>/.agents/elph/attachments/` (gitignored). Non-vision models receive
attachment paths in the text prompt instead. UI: [TUI § Image attachments](/guides/tui/#image-attachments).

### Session metadata

Per-session files live under `<workDir>/.agents/elph/metadata/<sess_id>/`:

| File                | Role                                                       |
|---------------------|------------------------------------------------------------|
| `todos.jsonl`       | Latest TodoList snapshot (single line; deleted when empty) |
| `log_events.json`   | User/system/AI/shell events                                |
| `log_requests.json` | Provider and tool trace                                    |

`internal/projectdir` helpers: `MetadataDir`, `SessionMetadataDir`, `SessionTodosPath`,
`EnsureSessionMetadataDir`.

### Session log

Path: `<workDir>/.agents/elph/metadata/<sess_id>/log_events.json`

Each line is JSON written via `log/slog` (`time`, `level`, `msg`, `kind`). `/diagnostic:open-log` formats records for display.

Kinds written in production (`runtime.AppendLog`):

| Kind            | Content                          |
|-----------------|----------------------------------|
| `user`          | User messages                    |
| `ai`            | Assistant responses              |
| `system`        | System/command output            |
| `shell`         | Shell command output             |
| `shell_context` | Shell output queued for agent    |
| `thinking`      | Reasoning blocks                 |
| `prompt`        | Expanded template prompts        |
| `tool_request`  | Parsed text-markup tool requests |

### Requests log

Path: `<workDir>/.agents/elph/metadata/<sess_id>/log_requests.json` — provider and tool trace written during agent turns. Both logs use `log/slog` JSONL records with a `kind` attribute for filtering.

### Goal session state

- In-memory store: `Session.goalManager` (`*goal.Manager`) is initialized on session creation.
- Passed to tool execution via `goal.WithManager(ctx, s.goalManager)` in `StartTurn`.
- Goal turn tracking: `RecordGoalTurn` callback in `TurnOptions` records each tool round progress
  (turns and tokens) when a goal is active.
- Implementation: `pkg/tools/goal` (types + manager), `internal/runtime/exec/goal.go` (execute).

### TodoList session state

- In-memory store: `Session.todoStore` (heap pointer; survives `Model` copies).
- New sessions load todos only from their own `metadata/<sess_id>/todos.jsonl` (a new TypeID starts empty).
- `SaveTodosSnapshot` overwrites the file on each change; clearing todos deletes the file.
- TUI **Tasks** panel (`internal/renderer/todo_panel.go`) shows pending/in-progress items above the input.
  When every item becomes `done`, the panel hides and a system notice lists completed tasks in the chat area.

## Related docs

- [Tools](/guides/tools/) — catalog and API exposure
- [Progress](/release-notes/progress/) — development history
- [Architecture](/guides/architecture/) — package map
- [Configuration](/configuration/configuration/) — settings and paths
