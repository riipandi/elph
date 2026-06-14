# Development Progress — Agent Tools & Provider Integration

This document records work done to improve Elph's agent tool handling, provider
integration, and TUI feedback. It is a living log for contributors tracking what
shipped, where the code lives, and what remains.

**Last updated:** June 2026

> **Documentation index:** [docs/README.md](./README.md) — architecture, configuration, CLI, slash commands, agent runtime, and a gap audit vs the codebase.

---

## Context

Early sessions surfaced several related problems:

1. Models sometimes wrote **XML-style tool markup** in plain assistant text
   (`<toolcall>`, `<function=WebSearch>`, `<parameter>`) instead of using the
   provider's native tool-calling API.
2. **Raw markup and tool queries** leaked into the AI message bubble (e.g. a
   WebSearch query appearing as the assistant reply).
3. The model could be offered **tools the runtime could not execute**, leading
   to confusing `tool unavailable` outcomes.
4. The TUI needed clearer **timing and status** feedback during agent activity
   and tool runs.

The work below addresses these in layers: TUI polish, text-markup mitigation,
native provider tool calling, API-side tool filtering, and clearer error
presentation.

---

## 1. TUI timing and message metadata

### User message timestamps

User and assistant message blocks can show a compact local timestamp.

| Item                 | Location                                              |
|----------------------|-------------------------------------------------------|
| Timestamp formatting | `internal/renderer/message_time.go`                   |
| Render integration   | `internal/renderer/collapsible_message.go`, `view.go` |
| Tests                | `internal/renderer/message_time_test.go`              |

Format: `15:04:05` for today; `Jan 2 15:04:05` for other days.

### Activity stopwatch

A stopwatch tracks elapsed time during agent activity phases (thinking,
connecting, tool work).

| Item                         | Location                                              |
|------------------------------|-------------------------------------------------------|
| Stopwatch model & formatting | `internal/renderer/activity_stopwatch.go`             |
| Agent state                  | `internal/renderer/state.go` (`AgentState.Stopwatch`) |
| Tests                        | `internal/renderer/activity_stopwatch_test.go`        |

Provider bootstrap/update flows also use a stopwatch in
`cmd/coding-agent/provider_progress.go`.

---

## 2. Tool failure and unavailable-tool UX

### Runtime error types

| Error                   | Meaning                                        |
|-------------------------|------------------------------------------------|
| `ErrToolUnknown`        | Name not in the built-in catalog               |
| `ErrToolUnavailable`    | Known tool, but not executable in this session |
| `ErrToolNotImplemented` | Executable path exists but handler missing     |

Defined in `internal/runtime/tool.go` and `internal/runtime/execute.go`.

### Detail box status

Collapsible tool detail blocks use status-driven colors and preview labels:

| Status                    | When                            |
|---------------------------|---------------------------------|
| `DetailStatusRunning`     | Tool in progress                |
| `DetailStatusSuccess`     | Completed without error         |
| `DetailStatusError`       | Unknown tool or execution error |
| `DetailStatusUnavailable` | Known but not executable        |
| `DetailStatusWarning`     | Cancelled                       |

Implemented in `internal/constants/detail_status.go` and
`internal/renderer/detail_status.go`.

### User-facing messages

`ResolveToolRequest` in `internal/runtime/tool.go` classifies text-markup tool
requests and returns structured copy for the detail box, including hints for
diagnostic helpers (e.g. use `/diagnostic:list-tools` instead of invoking
`ListTools` as an agent tool).

`FormatToolDetailBody` surfaces `Tool failed` with the error and any partial
output for native tool results.

Tests: `internal/renderer/tool_detail_test.go`.

---

## 3. Text markup tool-call mitigation (fallback path)

When the model emits tool-like XML in assistant text, a parser strips markup,
extracts invocations, and prevents duplicate payloads from appearing in the
visible reply.

### Parser pipeline

`StripToolCalls` in `pkg/core/agent/toolcall.go` runs a multi-stage pipeline:

1. **Smart strip first** — malformed segments without well-formed blocks
   (`pkg/core/agent/toolcall_smart.go`)
2. **Well-formed blocks** — `<toolcall>`, `<function=name>`, `<parameter=name>`
3. **Loose / orphan markup** — unclosed tags, orphan closers, name fragments
   (e.g. `=WebSearch>`)
4. **Unnamed parameters** — `<parameter>value</parameter>`
5. **Payload deduplication** — `StripExtractedPayloads` removes text that
   duplicates parsed parameter values (e.g. raw search queries)

### Renderer integration

The renderer applies stripping during streaming and on turn completion:

| File                                         | Role                           |
|----------------------------------------------|--------------------------------|
| `internal/renderer/agent.go`                 | Turn-end sanitization          |
| `internal/renderer/agent_toolcall.go`        | Stream-time tool-call handling |
| `internal/renderer/markdown.go`, `stream.go` | Markdown/stream sanitization   |

### Tests

| Package             | Files                                          |
|---------------------|------------------------------------------------|
| `pkg/core/agent`    | `toolcall_test.go`, `toolcall_payload_test.go` |
| `internal/renderer` | `agent_toolcall_test.go`                       |

### System prompt guardrail

`internal/prompt/builder.go` instructs the model to use **provider-native
tools** and not invent XML-like tool tags in assistant text.

---

## 4. Native provider tool calling

The long-term fix: use each provider's native tool-calling API so the model
receives JSON schemas and returns structured `tool_calls` / `tool_use` blocks.

### Provider layer

| Component                                   | Location                                             |
|---------------------------------------------|------------------------------------------------------|
| `ToolDefinition`, `ToolCall`, `ChatMessage` | `pkg/ai/provider/message.go`                         |
| `StopReason` (`end_turn`, `tool_use`, …)    | `pkg/ai/provider/message.go`                         |
| OpenAI tools & messages                     | `pkg/ai/provider/openai_tools.go`, `openai.go`       |
| Anthropic tools & messages                  | `pkg/ai/provider/anthropic_tools.go`, `anthropic.go` |
| Shared helpers                              | `pkg/ai/provider/tools.go`                           |
| Turn result fields                          | `pkg/ai/provider/result.go`                          |

### Agent loop

| Component                                | Location                                                                                          |
|------------------------------------------|---------------------------------------------------------------------------------------------------|
| Multi-round tool loop (max 8 iterations) | `pkg/core/agent/loop.go`                                                                          |
| Tool execution hook                      | `pkg/core/agent/toolrun.go`                                                                       |
| Turn routing (native vs placeholder)     | `pkg/core/agent/turn.go`                                                                          |
| Events                                   | `pkg/core/agent/event.go` — `EventToolCallStart`, `EventToolCallDone`, `TurnDoneWithHistoryEvent` |
| Options                                  | `pkg/core/agent/options.go` — `ToolsEnabled`, `ExecuteTool`, `Messages`                           |
| Tests                                    | `pkg/core/agent/loop_test.go`                                                                     |

Flow per iteration:

1. `Provider.Complete` with `TurnRequest.Tools` and conversation `Messages`
2. If `result.ToolCalls` non-empty → emit start/done events, run `ExecuteTool`
3. Append assistant message (with tool calls) and tool-result messages
4. Repeat until no tool calls or iteration limit

### Session wiring

`internal/runtime/session.go`:

- Enables `ToolsEnabled` when a provider is configured
- Injects `ExecuteTool` → `internal/runtime/execute.go`
- Persists `History` (`[]provider.ChatMessage`) across turns via
  `ApplyHistory` / `TurnDoneWithHistoryEvent`

### Runtime execution (phase 1)

`internal/runtime/execute.go` implements:

| Tool     | Behavior                  |
|----------|---------------------------|
| **Read** | Read file under workspace |
| **Grep** | ripgrep search            |
| **Glob** | Glob file discovery       |

`pkg/tool/availability.go` — `IsExecutable` returns true only for Read, Grep,
Glob.

Tests: `internal/runtime/tool_test.go`.

### TUI — native tool rendering

| File                                | Role                                     |
|-------------------------------------|------------------------------------------|
| `internal/renderer/agent_native.go` | Collapsible detail boxes per native call |
| `internal/renderer/agent_bridge.go` | Maps agent events to TUI updates         |

Native tool messages are tracked by `NativeToolMsgIDs` in `AgentState`
(`internal/renderer/state.go`).

---

## 5. Provider API tool filtering

**Problem:** `ProviderDefinitions()` initially exposed every `auto-allow` tool
with a schema (WebSearch, FetchURL, …) even though only Read/Grep/Glob could
run. Models called unavailable tools and got errors.

**Solution:** Expose tools to the API only when all of the following hold
(`IsProviderExposed`):

1. Known built-in
2. `DefaultApproval == auto-allow`
3. `IsExecutable(name)`
4. Has a provider JSON schema

| Function              | Location                   | Role                             |
|-----------------------|----------------------------|----------------------------------|
| `IsProviderExposed`   | `pkg/tool/availability.go` | Single-tool gate                 |
| `FilterProviderTools` | `pkg/tool/schema.go`       | Filter any tool list             |
| `ProviderDefinitions` | `pkg/tool/schema.go`       | Built-in schemas → filtered      |
| Loop integration      | `pkg/core/agent/loop.go`   | Always filters before `Complete` |

**Currently API-exposed:** Read, Grep, Glob only.

Detailed reference: [docs/tools.md § Provider API exposure](./tools.md#provider-api-exposure).

Tests: `pkg/tool/schema_test.go`, `pkg/tool/availability_test.go`.

---

## 6. Architecture overview

```mermaid
flowchart TB
    subgraph TUI["TUI (internal/renderer)"]
        TS[Timestamps & stopwatch]
        TB[agent_bridge events]
        TN[agent_native tool details]
        TX[Text markup strip / agent_toolcall]
    end

    subgraph Agent["Agent (pkg/core/agent)"]
        RT[runTurn / runProviderLoop]
        ST[StripToolCalls fallback]
        EV[Events]
    end

    subgraph ToolPkg["Tool catalog (pkg/tool)"]
        BD[builtin definitions]
        PE[IsProviderExposed filter]
        PD[ProviderDefinitions]
    end

    subgraph Provider["Provider (pkg/ai/provider)"]
        OAI[OpenAI tools + messages]
        ANT[Anthropic tools + messages]
    end

    subgraph Runtime["Runtime (internal/runtime)"]
        SE[session.StartTurn]
        EX[ExecuteTool Read/Grep/Glob]
    end

    SE --> RT
    RT --> PE
    PE --> PD
    PD --> OAI
    PD --> ANT
    RT --> EX
    EV --> TB
    TB --> TN
    RT --> ST
    ST --> TX
    TS --> TUI
```

---

## 7. File index (new or materially changed)

### `pkg/core/agent`

- `loop.go`, `loop_test.go` — native tool loop
- `toolcall.go`, `toolcall_smart.go`, `toolcall_payload.go` — text markup parser
- `toolrun.go` — tool result formatting
- `event.go`, `options.go`, `turn.go` — events and turn routing

### `pkg/tool`

- `schema.go`, `schema_test.go` — provider schemas and API filter
- `availability.go`, `availability_test.go` — `IsExecutable`, `IsProviderExposed`

### `pkg/ai/provider`

- `message.go`, `tools.go`, `result.go` — shared types
- `openai_tools.go`, `anthropic_tools.go` — provider adapters
- `openai.go`, `anthropic.go` — request building with tools

### `internal/runtime`

- `session.go` — history, tools enabled, execute hook
- `execute.go`, `tool.go`, `tool_test.go` — execution and errors

### `internal/renderer`

- `agent_native.go`, `agent_bridge.go`, `agent_toolcall.go` — agent UI
- `activity_stopwatch.go`, `message_time.go` — timing
- `detail_status.go`, `collapsible_status.go` — status presentation

### `internal/prompt`

- `builder.go` — native-tool instructions in system prompt

### `docs`

- `tools.md` — Provider API exposure section
- `progress.md` — this document

---

## 8. Verification

At the time of this log, the following passed:

```sh
go test ./...
go build -o elph ./cmd/coding-agent
```

---

## 9. Roadmap (not yet done)

| Item                                          | Notes                                                                          |
|-----------------------------------------------|--------------------------------------------------------------------------------|
| **WebSearch, FetchURL, CodeSearch execution** | Schemas exist; need runtime handlers + `IsExecutable`                          |
| **Write, Edit, Bash**                         | Require approval UI before API exposure                                        |
| **ReadMediaFile, plan mode, AskUser**         | Need runtime handlers and exposure rules                                       |
| **MCP tools in provider schemas**             | `internal/tools/lookup.go` stub; wire to `ProviderDefinitions`                 |
| **Disable XML parser when native-only**       | Reduce dual-path complexity once providers are stable                          |
| **Slash commands**                            | `/diff`, `/settings`, `/changelog` still `notImplemented`; `/commit` not added |

When adding an API-exposed tool, follow the checklist in
[docs/tools.md § Adding a new API-exposed tool](./tools.md#adding-a-new-api-exposed-tool).

---

## 10. Session timeline (summary)

| Phase | Focus               | Outcome                                                                |
|-------|---------------------|------------------------------------------------------------------------|
| 1     | TUI feedback        | Message timestamps; activity stopwatch                                 |
| 2     | Error presentation  | Distinct unavailable/unknown/failed states in detail boxes             |
| 3     | Markup leakage      | Multi-stage parser + `StripExtractedPayloads`; renderer sanitization   |
| 4     | Native tool calling | OpenAI/Anthropic tools, agent loop, session history, TUI events        |
| 5     | API tool filter     | `IsProviderExposed` — only Read, Grep, Glob sent to providers          |
| 6     | Documentation       | `docs/tools.md` exposure section; this progress log                    |
| 7     | Doc audit           | Full doc set in `docs/README.md`; fixed `tui.md`, tips, stale messages |

---

## 11. Documentation audit (June 2026)

See [docs/README.md § Documentation gaps](./README.md#documentation-gaps-audit-summary) for the living gap list.

Added: `architecture.md`, `configuration.md`, `cli.md`, `slash-commands.md`, `agent-runtime.md`, `docs/README.md` index.

Corrected: `tui.md` keybindings and defaults; root `README.md` requirements; `notExecutableToolMessage`; banner tips.
