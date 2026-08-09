# Headless mode (`elph run`)

Non-interactive agent runs for scripts and CI. Inspired by Pi (`-p` / print mode) and Grok (`--prompt-file`, `--output-format`).

## Quick start

```sh
elph run "summarize this repo"
elph run --prompt-file=./task.md --model=openai/gpt-5.6-luna
elph run --mode=plan --effort=high "design auth for the API"
elph run --output-format=json "list top 3 TODOs" 2>/dev/null

# Skills (same discovery as TUI)
elph run "/skill:code-review focus on auth"
elph run "/skill:tui-design"          # also: /tui-design if that skill name exists

# Prompt templates (name = file stem under prompts/)
elph run "/my-template arg1 arg2"
```

## Skills & prompt templates

Headless uses the same resource load as the TUI (project + home skills / prompts).

| Input | Action |
| --- | --- |
| Plain text | Normal agent prompt |
| `/skill:name [args]` | Invoke skill (legacy prefix; preferred explicit form) |
| `/skill-name [args]` | Invoke skill by raw name (if no built-in/template conflict) |
| `/template-name [args]` | Expand prompt template and run |

Other slash commands (`/compact`, `/help`, …) are **not** supported in headless and return a clear error.

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

## Turn footer (after the response)

Unless `--no-session`, stderr prints a **dimmed** turn block with blank lines above/below so it does not blend into the model answer (stdout):

```text
  session      <uuid>
  name         my-run
  turn         skill:code-review
  model        openai/gpt-5.6-luna
  context      12K / 200K (6.1%)
  resume       elph run --session-id=<uuid> "…"
```

- `context` = estimated tokens used / model window (same family of estimate as the TUI chrome).
- Colors only when stderr is a TTY and `NO_COLOR` is unset.
- JSON / stream formats also embed the same fields under `session` on stdout.

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
