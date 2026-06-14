# Built-in Tools

See also: [agent-runtime.md](./agent-runtime.md) (execution flow), [slash-commands.md](./slash-commands.md) (diagnostic helpers).

## Permissions

| Permission          | Description                                           |
|---------------------|-------------------------------------------------------|
| `auto-allow`        | Runs without approval by default; user can require it |
| `requires-approval` | Requires user approval before each run                |
| `always-approve`    | Always runs without approval; cannot be restricted    |

## File Tools

File tools handle reading, writing, and searching the local filesystem - the foundation for code analysis
and modification tasks.

| Tool          | Default Approval  | Description                        |
|---------------|-------------------|------------------------------------|
| Read          | Auto-allow        | Read a text file's contents        |
| Write         | Requires approval | Create or overwrite a file         |
| Edit          | Requires approval | Precise string replacement         |
| Grep          | Auto-allow        | `ripgrep` powered full-text search |
| Glob          | Auto-allow        | Find files by glob pattern         |
| ReadMediaFile | Auto-allow        | Read an image or video file        |

## Shell Tools

| Tool | Default Approval  | Description             |
|------|-------------------|-------------------------|
| Bash | Requires approval | Execute a shell command |

## Web Tools

| Tool       | Default Approval | Description                          |
|------------|------------------|--------------------------------------|
| FetchURL   | Auto-allow       | Fetch the content of a specified URL |
| WebSearch  | Auto-allow       | Web search with multiple engines     |
| CodeSearch | Auto-allow       | Search code on GitHub                |

## Plan Mode

| Tool          | Default Approval | Description                        |
|---------------|------------------|------------------------------------|
| EnterPlanMode | Auto-allow       | Enter Plan mode                    |
| ExitPlanMode  | Auto-allow       | Exit Plan mode and submit the plan |

`ExitPlanMode` will requires user to confirm the plan.

## Collaboration Tools

Collaboration tools handle inter-Agent coordination, user interaction, and Skill invocation.

| Tool    | Default Approval | Description                                        |
|---------|------------------|----------------------------------------------------|
| AskUser | Auto-allow       | Ask the user a question to gather structured input |

## Provider API exposure

Elph uses **native tool calling** (OpenAI `tools` / Anthropic `tools`) when a provider is
configured. The model receives only a filtered subset of built-in tools—not every tool listed
above.

Three layers decide what the model can see and what the runtime can run:

| Layer            | Purpose                                       | Source                         |
|------------------|-----------------------------------------------|--------------------------------|
| **Catalog**      | Shown in prompts and UI; full built-in list   | `pkg/tool` `builtin`           |
| **Provider API** | JSON schemas sent to OpenAI / Anthropic       | `ProviderDefinitions()`        |
| **Runtime**      | Actually executed when the model calls a tool | `internal/runtime.ExecuteTool` |

A tool is sent to the provider API only when **all** of the following are true
(`IsProviderExposed`):

1. It is a known built-in (`Get`).
2. Its default approval is `auto-allow` or `requires-approval` (runtime gates the latter via huh).
3. The runtime can execute it (`IsExecutable`).
4. It has a provider JSON schema (`providerSchema`).

Today **Read**, **Write**, **Edit**, **Grep**, **Glob**, **ReadMediaFile**, **AskUser**, and **Bash** are exposed.
**AskUser** opens a huh question dialog. **Write**, **Edit**, and **Bash** show an approval dialog
unless agent mode is
**brave** or the user chose **allow for session** earlier in the TUI session. Auto-allow tools like
WebSearch stay out until `IsExecutable` returns true for them.

### User approval (huh)

Requires-approval tools (**Write**, **Edit**, **Bash**) block in `runToolCall` until the renderer
answers `InteractTool` (`pkg/core/agent/interact.go`, `internal/renderer/tool_interact.go`). Completed
tool output appears in a collapsible detail box — shell/Bash stays expanded; other tools collapse when
the body is long (see [tui.md § Detail blocks](./tui.md#input-modes)).

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
| WebSearch     | Auto-allow        | No           | No                       |
| CodeSearch    | Auto-allow        | No           | No                       |
| EnterPlanMode | Auto-allow        | No           | No                       |
| ExitPlanMode  | Auto-allow        | No           | No                       |
| AskUser       | Auto-allow        | Yes          | Yes (huh question)       |
| Write         | Requires approval | Yes          | Yes (huh confirm/brave)  |
| Edit          | Requires approval | Yes          | Yes (huh confirm/brave)  |
| Bash          | Requires approval | Yes          | Yes (huh confirm/brave)  |

`requires-approval` tools are sent to the provider API when executable; **huh** gates each call unless
**brave** or **allow for session** applies. **AskUser** always uses huh before returning the answer
to the model.

**Bash** runs via `bash -c` in the workspace directory. Long-running commands (e.g. `ping` without
`-c`) are capped at **120s** (`defaultBashTimeout` in `internal/runtime/execute.go`). Output is
streamed to the TUI during execution (see [tui.md § Native tool detail](./tui.md#native-tool-detail)).

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
can call **ReadMediaFile**. See [tui.md § Image attachments](./tui.md#image-attachments).

### Request flow

```mermaid
sequenceDiagram
    participant Session as internal/runtime/session
    participant Loop as pkg/core/agent/loop
    participant Tool as pkg/tool
    participant Provider as pkg/ai/provider
    participant Runtime as internal/runtime/execute

    Session->>Loop: StartTurn (ToolsEnabled, ExecuteTool)
    Loop->>Tool: FilterProviderTools / ProviderDefinitions
    Tool-->>Loop: Read, Write, Edit, Grep, Glob, ReadMediaFile, AskUser, Bash schemas
    Loop->>Provider: Complete(TurnRequest.Tools)
    Provider-->>Loop: tool_calls / tool_use
    Loop->>Loop: InteractTool (AskUser / approval)
    Loop->>Runtime: ExecuteTool / ExecuteToolStream
    Runtime->>Tool: IsExecutable(name)
    Runtime-->>Loop: output or error (streamed chunks via EventToolCallOutputDelta)
    Loop->>Provider: tool_result follow-up message
```

The agent loop runs up to eight tool rounds (`maxToolIterations`). Each round: completion with
tools → execute calls → append tool results → complete again until the model stops calling
tools.

### Key functions

| Function                | Package            | Role                                                     |
|-------------------------|--------------------|----------------------------------------------------------|
| `ProviderDefinitions()` | `pkg/tool`         | Built-in schemas, then filtered                          |
| `FilterProviderTools()` | `pkg/tool`         | Filters any `[]provider.ToolDefinition`                  |
| `IsProviderExposed()`   | `pkg/tool`         | Single-tool API exposure check                           |
| `IsExecutable()`        | `pkg/tool`         | Whether runtime can run the tool                         |
| `providerSchema()`      | `pkg/tool`         | JSON Schema per built-in (private)                       |
| `runProviderLoop()`     | `pkg/core/agent`   | Native tool loop                                         |
| `InteractTool()`        | `pkg/core/agent`   | AskUser + approval via huh (renderer)                    |
| `ExecuteTool()`         | `internal/runtime` | Read / Write / Edit / Grep / Glob / ReadMediaFile / Bash |

Provider adapters map definitions to API formats:

- OpenAI-compatible: `pkg/ai/providers/openaicompat` (wraps `openai` + compat hooks)
- OpenRouter: `pkg/ai/providers/openrouter` (reasoning extra body on top of openaicompat)
- Anthropic: `pkg/ai/providers/anthropic` ([anthropic-sdk-go](https://github.com/anthropics/anthropic-sdk-go))

### Adding a new API-exposed tool

To expose a built-in to the model API end-to-end:

1. **Schema** — Add or extend `providerSchema()` in `pkg/tool/schema.go`.
2. **Execution** — Implement the handler in `internal/runtime/execute.go` and add the name to
   `IsExecutable()` in `pkg/tool/availability.go`.
3. **Approval** — If the tool should require user approval, keep
   `DefaultApproval: ApprovalRequiresApproval` in `pkg/tool/builtin.go`; it will not be API-exposed
   until approval is wired. Use `auto-allow` only for safe, read-only (or otherwise pre-approved)
   operations.
4. **Tests** — Update `pkg/tool/schema_test.go` and runtime tests for the new executable.

No change to `ProviderDefinitions()` is required: filtering is driven by `IsProviderExposed`.

### Text markup fallback

When native tools are disabled or the model emits XML-style `<toolcall>` markup in text, a separate
parser path (`pkg/core/agent/toolcall*.go`) still handles legacy invocations. That path is
independent of provider API filtering; keeping the API list aligned with `IsExecutable` avoids the
model calling tools that cannot run.
