# Headless mode (`elph run`)

Non-interactive agent runs for scripts and CI. Inspired by Pi (`-p` / print mode) and Grok (`--prompt-file`, `--output-format`).

## Quick start

```sh
elph run "summarize this repo"
elph run --prompt-file=./task.md --model=openai/gpt-5.6-luna
elph run --mode=plan --effort=high "design auth for the API"
elph run --output-format=json "list top 3 TODOs" 2>/dev/null
```

## Flags

| Flag | Description |
| --- | --- |
| `PROMPT…` | Positional prompt (joined with spaces) |
| `--prompt-file <path>` | Load prompt from UTF-8 file |
| `-m, --model <provider/model_id>` | Override model |
| `--mode <build\|plan\|ask\|brave>` | Agent tool policy (**default: `brave`**) |
| `-b, --brave` | Alias for `--mode=brave` |
| `--system-prompt <text\|@path>` | Full system prompt override |
| `--no-session` | Delete session after the run (not resumable) |
| `--cwd <path>` | Working directory / project root |
| `--max-turns <N>` | Abort after N tool starts |
| `--output-format <…>` | `plain` (default), `json`, `stream-json`, `stream-message-json` |
| `--effort` / `--reasoning-effort` | `off\|low\|medium\|high\|xhigh\|max` |
| `--session-id <id>` | Open or create that session id |
| `-r, --resume <id>` | Resume existing session (error if missing) |
| `-c, --continue` | Resume latest project session |
| `-n, --name <name>` | Session display name |

Aliases for output format: `text`→`plain`, `streaming-json`→`stream-json`, `streaming-messages-json`→`stream-message-json`.

## Session trailer

Unless `--no-session`, stderr always prints:

```text
elph: session_id=<uuid> name=<optional>
elph: resume: elph run --session-id=<uuid> "…"
```

Stdout stays clean for machine formats (`json` / stream).

## Progress indicator

On a TTY, `elph run` shows a **stderr spinner** (same `CliSpinner` as codegraph / datastore):

| Phase | Example message |
| --- | --- |
| Bootstrap | `Starting session…` / `Resuming session…` |
| Waiting | `Waiting for openai/gpt-5.6-luna · mode brave…` |
| Thinking | `Thinking · openai/gpt-5.6-luna…` |
| Tools | `Tool \`read_file\` · path/to/file…` |
| Generating | `Generating · openai/gpt-5.6-luna…` |
| Subagents | `Subagent research · running · …` |

Stdout stays clean for `--output-format` payloads. Spinner is cleared before the final answer / JSON. Ctrl+C during the run yields a clean interrupt (exit 130) like other CLI progress phases.

When stderr is not a TTY (CI pipes), the spinner falls back to a one-line status print and stays quiet.

## Defaults

- **Agent mode:** `brave` (auto-approve tools) for headless only; TUI still defaults to `build`.
- **Output:** `plain` assistant text to stdout.
