# Lifecycle hooks

Elph lifecycle hooks are native commands configured with JSON. They are an
automation and policy mechanism around the agent loop; they are not plugins and
cannot add tools, providers, UI, or slash commands.

## Configuration

Elph reads these files in order:

1. `CONFIG_DIR/hooks.json`
2. `<project>/.elph/hooks.json`, only for a trusted project

The two `hooks` arrays are concatenated. Hook `id` values must be unique in the
combined configuration. A malformed file is skipped as a unit. The canonical
schema is [`schemas/hooks-schema.json`](../schemas/hooks-schema.json).
A minimal project configuration is available in [`.elph/hooks.json`](../.elph/hooks.json).

```json
{
    "$schema": "https://elph.space/hooks-schema.json",
    "hooks": [
        {
            "id": "audit-tool-calls",
            "event": "sessionStart",
            "command": "hooks/audit-tool-calls.sh",
            "args": [],
            "timeoutMs": 5000,
            "enabled": true
        }
    ]
}
```

`command` is executed directly; Elph does not implicitly invoke a shell.
Relative executable paths are resolved against the directory containing the
defining configuration file. The process working directory is the active
project directory. Use an explicit shell executable in `command` and `args`
when shell syntax is required.

The project configuration may reference an executable inside
`PROJECT_DIR/.elph/hooks/`, as the sample above does. Elph does not scan or
auto-load every file in that directory: each hook must be declared in
`hooks.json`. The sample script records the `sessionStart` JSON payload in
`.elph/hook-audit.log` and intentionally writes no stdout.

### Tool names for matchers

Matchers use the names registered in Elph's tool catalog. Availability depends
on compiled features, agent mode, and MCP activation. Common current names are:

| Group | Tool names |
| --- | --- |
| Read/search | `read_file`, `grep`, `find_path`, `list_dir` |
| File mutation | `edit_file`, `write_file`, `create_dir`, `copy_path`, `delete_path`, `move_path` |
| Shell | `shell_exec`, `shell_use` |
| Web | `web_search`, `web_fetch`, `web_extract` |
| Discovery | `list_available_tools`, `list_skills` |
| Agent | `ask_user_question`, `request_mode_change` |
| Goals/todos | `create_goal`, `get_goal`, `update_goal`, `set_goal_budget`, `todo_write`, `todo_read` |
| Memory | `memory_start_task`, `memory_end_task`, `memory_report`, `memory_contradict`, `memory_status`, `memory_search`, `memory_recent` |
| Session/collaboration | `get_session_summary`, `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `list_agents` |
| MCP | `mcp_{server}__{tool}` when registered and activated |

`apply_patch` is not an Elph model tool; use `edit_file` in a matcher for
regular file edits.

## Lifecycle

```mermaid
flowchart TD
    A["AgentHarness lifecycle"] --> B["HookRegistry"]
    B --> C["Native Rust handlers"]
    B --> D["Configured command handlers"]
    D --> E["JSON stdin/stdout"]
    E --> B
    B --> F["Deterministic reduced outcome"]
    F --> A
    G["MCP"] --> H["Dynamic tools"]
    I["Skills and prompt templates"] --> J["Reusable workflows"]
    K["Provider JSON"] --> L["Catalog overlays and supported adapters"]
```

The currently supported command events are:

- `sessionStart`: after a new or resumed session is ready.
- `userPromptSubmit`: before an agent turn begins; this event is
  observation-only.
- `beforeAgent`: before a provider request; may append `systemPrompt` or
  `additionalContext`, and may append temporary `messages` to the turn.
- `context`: after the turn context is assembled and before it is sent to the
  provider; may replace the complete `messages` array for that request.
- `preToolUse`: before approval and execution; may return `decision`, `block`,
  `reason`, or a replacement `toolInput`.
- `postToolUse`: after a successful tool call; may replace `content`, `details`,
  or `isError`, and may request `terminate`.
- `postToolUseFailure`: after a failed tool call; supports the same result
  patch fields as `postToolUse`.
- `preCompact`: before compaction; may return `cancel` or
  `customInstructions`.
- `postCompact`: after compaction; observation-only.
- `stop`: when a turn settles; observation-only.

`sessionEnd` is reserved in the schema for the orderly shutdown lifecycle but
is not emitted by the current session owners.

The event payload is a small JSON object on standard input. It contains no
credentials, provider headers, or arbitrary process environment. The `context`
event intentionally includes the provider-bound message array; this is the
request context, not a new persisted transcript. For example, a `preToolUse`
payload contains `toolName`, `toolCallId`, and `toolInput`.

```mermaid
sequenceDiagram
    participant A as AgentHarness
    participant R as HookRegistry
    participant H as Hook command
    participant T as Tool runtime

    A->>R: preToolUse
    R->>H: JSON payload on stdin
    H-->>R: JSON decision on stdout
    R-->>A: Reduced decision
    A->>T: Native approval and execution
    T-->>A: Result
    A->>R: postToolUse or postToolUseFailure
```

Handlers run serially in configuration order. Tool-input transformations are
passed to the next matching handler, and tool-call decisions from native
handlers are merged rather than discarded. A hook cannot loosen a native
approval decision. A `deny`/`block` result prevents the current tool call. A
replacement `toolInput` is validated against the tool's schema again before
approval or execution. Replaced tool results and context messages are validated
before they enter the model context.

## Command protocol and limits

- Exit code `0` with empty stdout means no change.
- Non-empty stdout must be one JSON outcome for the event.
- Stderr is diagnostic text and is never parsed.
- `beforeAgent.systemPrompt` and `beforeAgent.additionalContext` are appended
  to the compiled system prompt with a blank-line separator. `messages` are
  appended to the current turn only and are not persisted as a new session
  entry.
- `context.messages` replaces the full provider-bound message array. This is
  useful for redaction, filtering, or deterministic context shaping; it does
  not change the durable session transcript.
- `preToolUse.toolInput` is a JSON object passed through the remaining hook
  chain, then schema-validated again. The final arguments are used for native
  approval and execution.
- `postToolUse` and `postToolUseFailure` accept `content` blocks with
  `{ "type": "text", "text": "..." }` or
  `{ "type": "image", "data": "...", "mime_type": "..." }`, plus `details`,
  `isError`, and `terminate`.
- Spawn errors, non-zero exits, timeouts, invalid JSON, and oversized output are
  logged and fail open for the current operation.
- Input is limited to 128 KiB; stdout and stderr are limited to 64 KiB.
- Returned context is limited to 32 KiB.
- The default timeout is 10 seconds and the maximum is 60 seconds.
- A hook process is terminated when its timeout expires; hook failures do not
  abort the surrounding agent operation.

Hooks are not a security boundary. Native tool approval, sandbox, agent mode,
MCP policy, authentication, and schema validation remain authoritative.
Commands run with the user's operating-system permissions. Do not use a hook
as the only control protecting sensitive files.

## Limitations

- Hook commands run serially in configuration order and are awaited by the
  lifecycle operation.
- `userPromptSubmit`, `sessionStart`, `postCompact`, and `stop` are
  observation-only in the current implementation.
- `beforeAgent` can add prompt material and temporary messages, but cannot
  remove or replace the compiled system prompt. `context` can replace
  provider-bound messages, but cannot rewrite the durable transcript.
- Hooks cannot add tools through `postToolUse.addedToolNames`; dynamic tool
  registration remains an MCP responsibility.
- Hook failures fail open: a failed hook does not block a tool or abort a turn.
- Hooks do not run in the background and do not expose streaming token events.

## Capability matrix

| Capability | Status | Contract |
| --- | --- | --- |
| Observe lifecycle | Supported | `sessionStart`, `userPromptSubmit`, `postCompact`, `stop` |
| Inject system prompt/context | Supported | `beforeAgent.systemPrompt` and `additionalContext` |
| Inject temporary messages | Supported | `beforeAgent.messages` |
| Transform provider context | Supported | `context.messages` replacement |
| Block tool calls | Supported | `preToolUse.block` or `decision: "deny"` |
| Rewrite tool input | Supported | `preToolUse.toolInput`, schema-validated again |
| Rewrite tool results | Supported | Content, details, error state, and termination |
| Control compaction | Supported | `preCompact.cancel` and `customInstructions` |
| Add tools or tool servers | Not a hook capability | Use MCP |
| Add providers or model catalogs | Not a hook capability | Use provider JSON and an existing adapter |
| Add slash commands or UI | Not a hook capability | Use native Elph features, skills, or prompt templates |
| Load WASM or Pi extensions | Not supported | Removed from the architecture |

## Trust and reload

Home hooks are user-owned configuration. Project hooks are ignored until the
project passes Elph's executable-resource trust gate. On interactive TUI startup,
an untrusted project with `defaultProjectTrust: ask` displays a trust prompt
before the datastore or TUI opens. Choosing No, declining the prompt, or
starting without a terminal exits without launching the project. The doctor
output reports skipped or malformed project hooks.

Use `/trust` to mark the current project trusted. Use `/untrust` to write an
explicit `false` decision, but only when the project is currently trusted.
An explicit project decision overrides inherited trust from a parent directory.
The command palette shows exactly one of these commands based on the current
trust state, and refreshes that choice after either command succeeds.

`/reload` re-reads hook configuration and replaces the active command handlers
only after the new configuration has been parsed. It also reloads provider
catalogs, MCP resources, skills, and prompt templates through their existing
resource paths.

## Extensibility boundaries

- **MCP** adds dynamic tools and remote tool servers.
- **Skills and prompt templates** add reusable instructions and user-invoked
  workflows.
- **Provider JSON** in `CONFIG_DIR/providers/*.json` adds catalog overlays and
  disk-only providers when their API matches an existing Elph adapter.
- **Hooks** observe or influence lifecycle operations only.
- Built-in tools, UI, and slash commands are native Elph code. Hooks cannot
  register any of them.
