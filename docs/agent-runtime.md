# Agent Runtime

How a user message becomes a provider completion, tool execution, and TUI update.

## Entry points

| Trigger                 | Handler                                                                |
|-------------------------|------------------------------------------------------------------------|
| Normal chat input       | `runtime.Session.StartTurn` → `agent.RunTurn`                          |
| Prompt template `/name` | Expand template → same as chat                                         |
| `!cmd` / `!!cmd`        | `runtime.RunShell` / `RunShellContext` — optional follow-up agent turn |
| No provider configured  | Placeholder simulation (`agent.runPlaceholderTurn`)                    |

`Session.StartTurn` (`internal/runtime/session.go`) injects:

- `SystemPrompt` from `prompt.Build` (see [configuration.md § Project context](./configuration.md#project-context-and-system-prompt))
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

- Max **8** iterations (`maxToolIterations`)
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

- Today: **Read**, **Write**, **Edit**, **Grep**, **Glob**, **AskUser**, **Bash**
- Details: [tools.md § Provider API exposure](./tools.md#provider-api-exposure)

## Runtime execution

`ExecuteTool` (`internal/runtime/execute.go`):

| Tool  | Implementation                                             |
|-------|------------------------------------------------------------|
| Read  | Read file under workspace (256 KB cap)                     |
| Write | Create parent dirs and write file contents                 |
| Edit  | Exact string replace; `replace_all` for multi-match        |
| Grep  | `rg` subprocess (`content`, `files_with_matches`, `count`) |
| Glob  | `doublestar.FilepathGlob` (`**` semantics, files only)     |
| Bash  | `bash -c` via `RunShellContext`; streams stdout/stderr     |

`ExecuteToolStream` (`session.toolExecuteStream`) passes chunks to `EventToolCallOutputDelta` for
live TUI updates. Bash validates syntax with `mvdan.cc/sh` before spawn and times out after 120s by
default.

Errors:

- `ErrToolUnknown` — not in `pkg/tool` catalog
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

| Event                      | TUI effect                                                               |
|----------------------------|--------------------------------------------------------------------------|
| `EventActivity`            | Activity line + stopwatch                                                |
| `EventThinkingDelta`       | Append to thinking block                                                 |
| `EventResponseDelta`       | Append to AI message (markdown async)                                    |
| `EventToolCallStart`       | Native tool detail box (running, `$ cmd` for Bash)                       |
| `EventToolCallOutputDelta` | Append streamed shell output to detail box                               |
| `EventToolCallDone`        | Finalize detail status/body; expand/collapse via `tool_detail_expand.go` |
| `EventTurnDone`            | Finalize turn, apply history, token/cost footer                          |

Native tool UI: `agent_native.go`, `tool_interact.go` (huh approval / AskUser). Text-markup tool
UI: `agent_toolcall.go`.

`toolInteractBridge` (`internal/renderer/tool_interact.go`) blocks the agent loop until the user
responds. Approval choices, session allow, and per-turn deny cache are documented in
[tools.md § User approval](./tools.md#user-approval-huh).

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

These limits keep idle and long-session RSS stable (~30 MB at rest after startup optimizations). See [architecture.md § Performance and memory](./architecture.md#performance-and-memory) for git, catalog, and models.dev behavior.

History is compacted automatically (`agent.CompactMessages`):

| Limit                         | Value   |
|-------------------------------|---------|
| Max messages                  | 32      |
| Max total size                | ~512 KB |
| Max tool result (API/history) | 32 KB   |
| Max tool result (TUI detail)  | 40 KB   |
| Max assistant message         | 64 KB   |
| Max AI bubble text (TUI)      | 48 KB   |

Provider catalogs kept in session are trimmed: inactive models drop compat/thinking/headers metadata. Prompt templates load on first `/` use. System prompt lists only API-exposed built-in tools.

Tool execution also caps raw output: Read 256 KB, Grep/Glob 128 KB, Glob 500 paths.

### Session log

Path: `<workDir>/.agents/elph/logs/<sess_id>/events.jsonl`

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

Path: `<workDir>/.agents/elph/logs/<sess_id>/requests.jsonl` — provider and tool trace written during agent turns. Both logs use `log/slog` JSONL records with a `kind` attribute for filtering.

## Related docs

- [tools.md](./tools.md) — catalog and API exposure
- [progress.md](./progress.md) — development history
- [architecture.md](./architecture.md) — package map
- [configuration.md](./configuration.md) — settings and paths
