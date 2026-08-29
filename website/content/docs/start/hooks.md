# Lifecycle hooks

Elph lifecycle hooks are native commands configured with JSON. They can observe
or influence agent lifecycle events, but they cannot add tools, providers, UI,
or slash commands.

## Configuration

Elph loads hook definitions from:

1. `CONFIG_DIR/hooks.json`
2. `<project>/.elph/hooks.json`, only after the project passes the trust gate

The home file is loaded first and the project file is appended. Hook IDs must
be unique across both files. A malformed file is ignored as a unit.

The canonical schema is
[`schemas/hooks-schema.json`](https://github.com/riipandi/elph/blob/main/schemas/hooks-schema.json).

```json
{
  "$schema": "https://elph.space/hooks-schema.json",
  "hooks": [
    {
      "id": "audit-tool-calls",
      "event": "preToolUse",
      "matcher": {
        "toolNames": ["write_file", "apply_patch"]
      },
      "command": "hooks/audit-tool-calls",
      "timeoutMs": 5000,
      "enabled": true
    }
  ]
}
```

`command` is executed directly; Elph does not invoke a shell implicitly.
Relative executable paths resolve against the directory containing the
defining `hooks.json`. The child process working directory is the active
project directory. Use an explicit shell executable and literal `args` when
shell syntax is required.

## Events

Supported events are:

- `sessionStart` — once when a session is initialized.
- `userPromptSubmit` — before an agent turn; observation-only.
- `beforeAgent` — before a provider request; may return `systemPrompt`.
- `preToolUse` — before native approval and execution; may block a tool call.
- `postToolUse` — after a successful tool call.
- `postToolUseFailure` — after a failed tool call.
- `preCompact` — before compaction; may cancel or provide instructions.
- `postCompact` — after compaction; observation-only.
- `stop` — when a turn settles; observation-only.

`sessionEnd` is reserved in the schema but is not emitted by the current ACP
session owner.

Tool events may use `matcher.toolNames` with exact, `prefix*`, or `*suffix`
patterns. Handlers run serially in configuration order.

## Command protocol and safety

- JSON is sent on stdin; an optional JSON outcome is read from stdout.
- Empty stdout means no change. Stderr is diagnostic text and is not parsed.
- Spawn errors, non-zero exits, timeouts, invalid JSON, and oversized output
  fail open for the current operation.
- Input is limited to 128 KiB; stdout and stderr are limited to 64 KiB.
- The default timeout is 10 seconds and the maximum is 60 seconds.
- Returned context is limited to 32 KiB.
- Commands run with the user's operating-system permissions and are not a
  security boundary.

Use native approval, sandbox, agent mode, MCP policy, and tool-schema
validation for security enforcement.

## Reload and integration boundaries

Use `/reload` after editing either hook file. The command reloads hooks and
other workspace resources without restarting the TUI. `elph doctor` reports
hook configuration diagnostics and the project trust state.

Use MCP for dynamic tools, skills and prompt templates for reusable workflows,
and `CONFIG_DIR/providers/*.json` for custom provider catalogs supported by an
existing Elph adapter.
