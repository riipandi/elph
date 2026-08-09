# Plan: Headless `elph run` (parity with pi / grok)

## Goal

Make `elph run` a full headless agent driver: one prompt (or prompt file) in, structured/streamed output out, optional durable session for resume. Align flag names with **Grok** headless / **Pi** print mode, while using Elph’s existing agent modes and session store.

## Current state

| Piece                 | Status                                                                                                                                                                                                 |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `elph run` subcommand | Exists (`cli/run.rs`)                                                                                                                                                                                  |
| Core path             | `run_non_interactive` → `create_coding_session(headless: true)` → `submit_prompt`                                                                                                                      |
| Implemented flags     | `PROMPT…`, `-m/--model`, `--output-format` (ignored except `text`), `-c/--continue`, `-r/--resume`, `--fork` (stub), `-f/--file` (stub), `-b/--brave`, resilience knobs                                |
| Gaps                  | No `--prompt-file`, `--cwd`, `--mode`, `--system-prompt`, `--no-session`, `--max-turns`, real multi-format output, `--effort`, `--session-id` (create/open by id), `--name`, session summary after run |
| Docs                  | `docs/planned/headless-mode.md` empty — fill as source of truth after impl                                                                                                                             |

## References (flag mapping)

| Requested        | Pi                       | Grok                                                   | Elph plan                                                                                                                                                                |
| ---------------- | ------------------------ | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Prompt arg       | positional / `-p`        | `[PROMPT]` / `-p`                                      | Keep positional `PROMPT…` (joined with space)                                                                                                                            |
| Prompt file      | `@file` style            | `--prompt-file`                                        | **`--prompt-file <path>`** (wins if set; exclusive with empty positional)                                                                                                |
| Model            | `--model` / `--provider` | `-m/--model`                                           | **`-m/--model provider/model_id`** (existing)                                                                                                                            |
| Agent mode       | `--ask` / `--plan` ext   | permission-mode                                        | **`--mode=build\|plan\|ask\|brave`** (Elph agent modes; default `build`)                                                                                                 |
| Brave            | n/a                      | always-approve-ish                                     | Keep **`-b/--brave`** as alias for `--mode=brave`                                                                                                                        |
| System prompt    | `--system-prompt`        | `--system-prompt` / override                           | **`--system-prompt <text\|@path>`** full override of coding system prompt                                                                                                |
| No session       | `--no-session`           | (ephemeral via no save)                                | **`--no-session`** ephemeral (no durable tree / not listable)                                                                                                            |
| Session id       | `--session-id`           | `-s/--session-id` (new UUID only)                      | **`--session-id <id>`** open existing **or** create with that id if missing; keep `-r/--resume` as explicit resume (error if missing)                                    |
| Name             | `-n/--name`              | title on rename                                        | **`--name <name>`** set display name after create/resume                                                                                                                 |
| CWD              | n/a (cwd)                | `--cwd`                                                | **`--cwd <path>`** `chdir` + use as project/cwd for paths + tools                                                                                                        |
| Max turns        | n/a                      | `--max-turns`                                          | **`--max-turns <N>`** stop after N harness agent turns (tool loops count as turns if harness does)                                                                       |
| Output           | `--mode text\|json\|rpc` | `plain\|json\|streaming-json\|streaming-messages-json` | **`--output-format=plain\|json\|stream-json\|stream-message-json`** (aliases: `text→plain`, `streaming-json→stream-json`, `streaming-messages-json→stream-message-json`) |
| Effort           | `--thinking`             | `--reasoning-effort` / `--effort`                      | **`--effort=off\|low\|medium\|high\|xhigh\|max`** (+ optional `minimal`) → `ThinkingLevel`                                                                               |
| Continue         | `-c`                     | `-c`                                                   | Keep **`-c/--continue`**                                                                                                                                                 |
| Session info out | n/a                      | implied by resume                                      | Always emit **session trailer** (unless `--no-session`): id, name, path hints for resume                                                                                 |

**Out of scope (v1):** `--fork` (keep warn stub), multi-file `@attach` parity, RPC mode like Pi, JSON-schema structured output, permission-mode matrix beyond agent modes.

---

## CLI surface (`RunArgs`)

```text
elph run [OPTIONS] [PROMPT]...

  --prompt-file <PATH>
  -m, --model <provider/model_id>
  --mode <build|plan|ask|brave>     default: build
  -b, --brave                       alias for --mode=brave
  --system-prompt <TEXT>            literal or @path / file path if starts with @ or is readable file
  --no-session
  --cwd <PATH>
  --max-turns <N>
  --output-format <plain|json|stream-json|stream-message-json>
  --effort <off|low|medium|high|xhigh|max>
  --session-id <ID>
  -r, --resume <SESSION_ID>         must exist
  -c, --continue
  --name <NAME>
  # keep resilience flags
```

### Prompt resolution order

1. If `--prompt-file` set → read UTF-8 file (error if missing).
2. Else join positional `PROMPT…` with spaces.
3. Empty after trim → usage error (same as today).

### Mutual exclusions / validation

- `--continue` + `--session-id` / `--resume`: prefer explicit id; error if both continue and resume id disagree.
- `--no-session` + `--continue`/`--resume`/`--session-id`: error (nothing to resume).
- `--mode` and `--brave`: if both set and conflict, error; else brave wins only when mode omitted.
- `--model`: parse `provider/model_id` via existing `resolve_provider_and_model` path (already used by `model_override`).

---

## Runtime design

### 1. Expand `RunModeOptions` + `run_non_interactive`

File: `crates/coding-agent/src/agent/run_mode.rs`

```rust
pub struct RunModeOptions<'a> {
    // existing...
    pub mode: AgentMode,              // not just brave bool
    pub system_prompt_override: Option<&'a str>,
    pub no_session: bool,
    pub max_turns: Option<u32>,
    pub output_format: OutputFormat,
    pub effort: Option<ThinkingLevel>,
    pub session_id: Option<&'a str>,  // create-or-open
    pub name: Option<&'a str>,
}
```

Flow:

```text
resolve Paths (after optional chdir from --cwd)
load Settings
resolve session launch (continue | resume | session-id | new | ephemeral)
create_coding_session(...)
apply effort (set_thinking_level)
apply name (harness.set_session_name)
apply system_prompt override (see below)
subscribe output sink for format
submit_prompt / drive until idle
enforce max_turns (hook or harness option)
print session summary (unless no-session or format already embeds it)
exit 0/1
```

### 2. `CreateSessionOptions` / session factory

File: `runtime.rs` + `session_manager.rs` as needed

| Option                         | Wiring                                                                                                                                                                                                                                                         |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent_mode`                   | Already exists — pass `--mode` (not only brave)                                                                                                                                                                                                                |
| `model_override`               | Existing `--model`                                                                                                                                                                                                                                             |
| `headless`                     | true                                                                                                                                                                                                                                                           |
| `system_prompt_override`       | **New**: when set, `SystemPrompt` becomes fixed string (skip or wrap `build_coding_system_prompt`); still allow tools/resources unless product later wants bare                                                                                                |
| `session_id` create-if-missing | Extend `SessionManager::create` / Turso create options to accept explicit id when new                                                                                                                                                                          |
| `no_session`                   | Prefer **in-memory / temp session** not registered for list/continue, **or** create then delete on drop — pick **ephemeral flag on create** so GC/list ignore; implement minimal: `SessionManager::create_ephemeral` using harness without durable resume path |

**Recommendation for `--no-session`:** create normal session under a temp project key or mark `ephemeral=1` in metadata and **delete on successful exit** (and best-effort on error). Avoid inventing a full second storage backend in v1.

### 3. Output formats

| Format                | Behavior                                                                                                                                                                    |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `plain` (default)     | Stream assistant **text** to stdout as it completes (or live if event stream available); tools silent on stdout (stderr optional status). Trailer on stderr: `session_id=…` |
| `json`                | Single JSON object after run: `{ session, result, messages?, usage?, error? }`                                                                                              |
| `stream-json`         | NDJSON lines of Elph/ACP-ish session updates (assistant deltas, tool start/end, turn done) — map from existing `AgentUiEvent` / harness stream                              |
| `stream-message-json` | NDJSON closer to Anthropic Messages wire shape (message_start / content_block_delta / message_stop) for tooling that already speaks Grok’s `streaming-messages-json`        |

Implementation sketch:

- Create session **with** UI event channel (`create_coding_session_with_events`).
- Spawn task that writes formatted lines while `submit_prompt` runs.
- For `plain`, keep simple path if events lag: print final assistant text (current behavior) **plus** stream deltas when events available.

### 4. Max turns

- Prefer harness-native limit if present; else count **completed agent turns** (each model call in the tool loop) via event hook and **abort** with clear error when `N` exceeded.
- Document: user message = 1 outer prompt; tool loops consume additional turns.

### 5. Session information (resumable)

Always after success (and on failure when session exists), emit:

**stderr (plain):**

```text
elph: session_id=<uuid> name=<optional>
elph: resume: elph run --session-id=<uuid> "…"
# or: elph -r <uuid>
```

**json / stream final:** include `session: { id, name, cwd, model, mode }`.

Do **not** print secrets.

### 6. CWD

```rust
if let Some(cwd) = args.cwd {
  std::env::set_current_dir(&cwd)?;
}
let cwd = env::current_dir()?;
// Paths::resolve() should run *after* chdir so project_dir / store bind to target tree
```

Order: **chdir first**, then `Paths::resolve()`, then session.

### 7. Effort / thinking

Map CLI → `crate::types::ThinkingLevel` → existing `session.set_thinking_level` after create (same as TUI).

### 8. Model override

Reuse `CreateSessionOptions.model_override` with full `provider/model` string; parsing already in agent provider helpers.

---

## Files to touch

| File                            | Change                                                                                           |
| ------------------------------- | ------------------------------------------------------------------------------------------------ |
| `cli/run.rs`                    | Expand `RunArgs`, prompt-file, cwd, validation, map to `RunModeOptions`                          |
| `agent/run_mode.rs`             | Full headless driver, output formats, session trailer                                            |
| `agent/runtime.rs`              | system prompt override, session_id create-or-open, ephemeral, mode                               |
| `agent/session_manager.rs`      | create with fixed id; ephemeral cleanup helper                                                   |
| `agent/session/mod.rs`          | optional: override system prompt cache when forced                                               |
| `cli/session_launch.rs`         | integrate `--session-id` vs continue/resume                                                      |
| `docs/planned/headless-mode.md` | become implemented doc (or move to `docs/` / user-guide)                                         |
| `assets/user-guide/`            | short `run` / headless section if user-facing CLI docs live there                                |
| Tests                           | unit: arg parse, format enum, prompt resolve; integration: plain run with faux model if feasible |

---

## Testing plan

1. **Unit:** `OutputFormat` parse + aliases; prompt file vs positional; mode/brave conflict; effort parse.
2. **Unit:** `modify`/session launch matrix (continue vs session-id vs no-session errors).
3. **Integration (narrow):** `run_non_interactive` with faux provider → plain output contains assistant text + session id on stderr; `json` is valid object with `session.id`.
4. **Manual:** `elph run --prompt-file=… --model=… --mode=ask --effort=low --output-format=stream-json "…"` against real auth.

---

## Implementation order

1. **CLI args + validation** (no behavior change for defaults except `output-format` default `plain`).
2. **CWD + mode + effort + name + model** (thin wiring into existing session APIs).
3. **System prompt override**.
4. **Session id create-or-open + session trailer**.
5. **`--no-session` ephemeral cleanup**.
6. **Output formats** (`plain` polish → `json` → streams).
7. **`max-turns`**.
8. **Docs + tests**.

---

## Risks / decisions locked

1. **`--mode` = agent mode** (build/plan/ask/brave), **not** Pi’s output `--mode`. Output uses `--output-format` only.
2. **Default output** renames `text` → `plain` (alias `text` kept).
3. **`--session-id`** create-or-open (more useful for scripts than Grok’s new-only UUID rule); **`--resume`** remains fail-if-missing.
4. **System prompt override** replaces compiled coding prompt for that run (not append). Append can be a later `--append-system-prompt` if needed.
5. Stream formats are **best-effort maps** of existing events, not full Grok ACP clone.

---

## Acceptance criteria

- [ ] `elph run "do something"` works as today (plain text + session trailer).
- [ ] `elph run --prompt-file=./p.md` loads file.
- [ ] `--model provider/id` selects model.
- [ ] `--mode plan|ask|brave|build` changes tool policy.
- [ ] `--system-prompt` overrides prompt body used by harness.
- [ ] `--no-session` does not leave a resumable session in list.
- [ ] `--cwd` runs tools relative to that directory / project store.
- [ ] `--max-turns N` stops with non-zero when exceeded.
- [ ] `--output-format json|stream-json|stream-message-json` produce parseable stdout.
- [ ] `--effort` changes thinking level for the session.
- [ ] `--session-id` / `--name` enable named/resumable headless runs; trailer prints resume command.
- [ ] `elph run --help` documents all flags (clap).
- [ ] Docs updated (`headless-mode` + auth/CLI user guide as needed).
