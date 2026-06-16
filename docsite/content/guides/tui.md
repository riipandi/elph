---
title: TUI Layout Docs
summary: "Terminal UI layout, keybindings, and component reference."
weight: 100
---

## Layout

```
╭─────────────────────────────────────────────────────────────────╮
│                                                                 │
│   ⣿⣿⡟⣿⡟⣿⣿    Welcome to Elph v0.79.1 (update available)         │
│   ⣿⣿⣿⣿⣿⣿⣿    Send /changelog to show version history.           │
│                                                                 │
│  Directory:  ~/some/path/to/project_dir                         │
│  Model:      Claude Sonnet 4.6 [anthropic] (000 available)      │   <- BANNER (line-clamp if not enough width)
│  Stats:      00 exts, 00 commands, 00 skills, 00 tools          │   <- placeholder zeros (not wired)
│  MCP Server: 0/0 connected (0 tools)                            │   <- placeholder (MCP not implemented)
│                                                                 │
│  Tip: Use --no-session for ephemeral mode — no session file is  │   <- TIPS can be wrapped
│  saved, useful for one-off queries.                             │
│                                                                 │
╰─────────────────────────────────────────────────────────────────╯

 | This is an example input message from user                         <- MAIN_AREA / RESPONSE_STREAM
 | This is an example response from AI agent

╭─────────────────────────────────────────────────────────────────╮
│ >                                                               │   <- INPUT_PROMPT (multiline with ctrl+j or shift+enter)
╰─────────────────────────────────────────────────────────────────╯
MODEL_NAME | PROVIDER | T: high | IMG      $0.00 | 0k | 0.0% (262k)   <- FOOTER / STATUSLINE (line-clamp if not enough width)
project_dir [sess_abcd12345] agent_mode          turn: 0 | main [-]
```

---

## Color Palette

| Token         | Dark Mode | Light Mode | Usage                                 |
|---------------|-----------|------------|---------------------------------------|
| `blueCol`     | `#3B82F6` | `#3B82F6`  | Banner border                         |
| `yellowCol`   | `#EAB308` | `#EAB308`  | Tip label, context warning            |
| `highlight`   | `#7C56DC` | `#874BFD`  | System message prefix `> `            |
| `special`     | `#73F59F` | `#43BF6D`  | Braille logo                          |
| `dimText`     | `#5C5C5C` | `#9B9B9B`  | Labels, secondary info                |
| `brightText`  | `#D1D5DB` | `#6B7280`  | Values, metadata content              |
| `userPipeCol` | `#A78BFA` | `#7C56DC`  | User message pipe `|`                 |
| `aiPipeCol`   | `#9CA3AF` | `#6B7280`  | AI response pipe `|`                  |
| `whiteCol`    | `#FFFFFF` | `#FFFFFF`  | Project dir, turn info, prompt prefix |

---

## Banner

### Structure

```
╭─────────────────────────── border: blueCol ────────────────────╮
│  padding(1, 2)                                                 │
│  [logo]  header (bold, white)                                  │
│          subtitle (dimText, line-clamped)                      │
│                                                                │
│  Directory:  path          dimText + brightText                │
│  Model:      name [prov]  dimText + brightText                 │
│  Stats:      00 ext, ...  dimText + brightText                 │
│  MCP Server: 0/0 ...      dimText + brightText                 │
│                                                                │
│  Tip: yellow label, dimText body, italic, word-wrapped         │
╰────────────────────────────────────────────────────────────────╯
```

### Coloring Rules

| Element          | Style                                                       |
|------------------|-------------------------------------------------------------|
| Border           | `blueCol` (`#3B82F6`)                                       |
| Logo             | `special` (green adaptive)                                  |
| Header           | Bold, default foreground (white)                            |
| Subtitle         | `dimText`, `MaxWidth(metaW)` — line-clamped (truncated)     |
| Metadata labels  | `dimText` — "Directory:", "Model:", "Stats:", "MCP Server:" |
| Metadata values  | `brightText` — actual content after label                   |
| Tip label `Tip:` | `yellowCol` (`#EAB308`), italic                             |
| Tip body         | `dimText`, italic, `Width(tipW)` — word-wrapped             |

### Behaviour

- **Subtitle**: `MaxWidth` truncates to one line if too long (line-clamp).
- **Tip**: `Width` wraps text to multiple lines if too long (word-wrap).
- **Metadata**: `MaxWidth` truncates individual lines.
- **Layout**: Logo + header/subtitle sit in a `JoinHorizontal` at the top.
  Metadata lines sit below, left-aligned to the banner edge (no logo offset).

---

## Input Prompt

### Structure

```
╭─────────────── border: modeBorderColor(mode) ───────────────────╮
│ > placeholder text                                        <- PROMPT
╰─────────────────────────────────────────────────────────────────╯
```

### Behaviour

- **Multiline**: `Ctrl+J` or `Shift+Enter` inserts newline.
- **Submit**: `Enter` sends message and clears input.
- **Prompt prefix**: Rendered as a separate element before the textarea (not using textarea's Prompt).
- **Trigger stripped on submit**: `/cmd` → message is `cmd`, `!!rpt` → message is `rpt`.
- **Configurable**: `showPromptPrefix` (default: `false` in `internal/renderer/model.go`). When `false`, prefix is hidden.

### Prompt Prefix (dynamic)

The prompt character changes based on input content. Always **white**, **bold**.
Leading spaces are trimmed before detection. Prefix resets to `>` when input is empty.

| Input starts with | Prompt | Meaning                       |
|-------------------|--------|-------------------------------|
| (default)         | `>`    | Normal chat input             |
| `/`               | `/`    | Slash command                 |
| `!`               | `$`    | Shell command with context    |
| `!!`              | `#`    | Shell command without context |

Check order: `!!` → `!` → `/` → default (`>`).

### Slash Commands

Inputs starting with `/` invoke slash commands. Built-in commands (for example `/help`, `/model`,
`/exit`) are always available. Custom prompt templates are loaded from `~/.elph/prompts/*.md` and
`<workDir>/.agents/elph/prompts/*.md` — each file becomes a slash command named after the filename.

Detail blocks (prompt templates, diagnostics, shell output, native tool results) and thinking blocks
are shown separately from user input. They are dimmed and collapsible.

| Block kind                                       | Default expand                   | Notes                                                             |
|--------------------------------------------------|----------------------------------|-------------------------------------------------------------------|
| Thinking                                         | Collapsed                        | Respects `autoExpandThinking` in settings                         |
| Prompt template detail                           | Collapsed                        | One-line preview when collapsed                                   |
| `/diagnostic:system-prompt`                      | Collapsed                        | Full prompt in detail body                                        |
| `/diagnostic:list-tools`, `/diagnostic:open-log` | Expanded                         | Tool list and log tail                                            |
| Shell (`!` / `!!`) and Bash tool                 | Expanded                         | Streamed output stays visible                                     |
| Other native tool results                        | Short: expanded; long: collapsed | ≥2 lines or >120 bytes starts collapsed (`tool_detail_expand.go`) |

Click a header or hint row to expand or collapse that block. Detail titles are plain text (no
background); the hint row is clickable for detail blocks. `Ctrl+O` toggles the most recent
collapsible block in the session (unless the input has a collapsed paste token or the paste editor
is open — then **Ctrl+O** handles paste preview/edit first). Detail box colors reflect status:
neutral, running, success, warning, or error.

### Slash command palette

Typing `/` opens a fuzzy command list above the input. `Tab` or `→` completes the highlighted
entry; `↑`/`↓` change selection. `Enter` runs commands that need no arguments; for commands with
`Args` or an `argument-hint`, `Enter` completes the name (or runs with the highlighted arg when the
arg palette is active). See [Slash Commands](/guides/slash-commands/#command-palette-keys).

See [Prompt Templates](/guides/prompt-templates/) for format, argument placeholders, and examples.

### Image attachments

When the selected model supports vision, **Ctrl+V** / **Cmd+V** pastes a clipboard image into the
pending turn (up to **4** images). Files are saved under `<workDir>/.agents/elph/attachments/` as
`paste_<session_suffix>_*.png` and listed below the input while pending.

| UI element       | Behavior                                                                    |
|------------------|-----------------------------------------------------------------------------|
| Input suffix     | `[images: paste_….png, …]` on the prompt line                               |
| Hint row         | Count + shortcuts; bullet list of relative paths                            |
| Footer **IMG**   | Shown when `provider.SupportsImageInput(model.Input)` is true               |
| Non-vision model | Paste blocked with a system message; switch model via **Ctrl+L** / `/model` |

On submit, images are sent as `TurnOptions.UserImages` to the provider API. For non-vision models,
paths are appended to the text prompt so the agent can use **ReadMediaFile** instead.

**Remove attachments** (only when the input textarea is empty):

| Key                                | Action            |
|------------------------------------|-------------------|
| `Backspace` / `Delete`             | Remove last image |
| `Ctrl+Backspace` / `Ctrl+Delete`   | Remove last image |
| `Shift+Backspace` / `Shift+Delete` | Clear all images  |
| `Cmd+Backspace` / `Cmd+Delete`     | Clear all images  |

Terminals that emit raw CSI for **Cmd+Delete** (e.g. Ghostty `\x1b[3;9~`) are handled before word-delete logic (`attachment_keys.go`). **Ctrl+C** twice also clears pending attachments.

Implementation: `internal/renderer/attachments.go`, `paste_keys.go`, `internal/clipboardmedia`.

### Long text paste

When **Ctrl+V** / **Cmd+V** pastes plain text (not an image), long payloads collapse so the input
stays readable. Thresholds: **≥ 4 lines** or **≥ 400 runes**. The textarea stores an internal token
(`[[paste:id]]`); the UI shows **`[Pasted: N lines]`**. On submit, tokens expand to the full text
sent to the agent.

| UI element   | Behavior                                                                 |
|--------------|--------------------------------------------------------------------------|
| Hint row     | `Pasted block · N lines · ctrl+o to preview/edit` when a token is active |
| **Ctrl+O**   | Opens a full-screen paste editor overlay (when input has a paste token)  |
| Editor keys  | **Ctrl+J** / **Shift+Enter** — newline; **Ctrl+O** or **Esc** — save     |
| After paste  | Cursor moves to the end of the paste token                               |
| After editor | Main input cursor restores to the pre-edit line/column                   |

Set `"useRawPaste": true` in `~/.elph/settings.json` to insert pasted text verbatim (no collapse).
Default is `false`. See [Configuration § settings.json](/configuration/configuration/#settingsjson).

Implementation: `internal/renderer/paste.go`, `paste_editor.go`, `attachments.go`.

---

## Footer / Statusline

### Structure (no border)

```
MODEL_NAME | PROVIDER | T: level | IMG           $0.00 | 0.0% | 262k
project_dir [session_id] mode               turn: 0 | branch [+N -N]
```

The token display format is configurable via `footerTokenDisplay` setting (see [Configuration](/configuration/configuration/#settings-reference)). Context limit is always displayed.

When no actual token usage data is available (e.g., at startup before the first API call), token counts are estimated from the system prompt.


| Format       | Example                      | Description                                        |
|--------------|------------------------------|----------------------------------------------------|
| `both`       | `$0.00 | 131k | 0.0% | 262k` | Default — shows used tokens, percentage, and limit |
| `percentage` | `$0.00 | 0.0% | 262k`        | Shows percentage and context window only           |
| `count`      | `$0.00 | 131k | 262k`        | Shows used tokens and context window only          |

### Line 1

| Segment                       | Color                    | Notes                                                                              |
|-------------------------------|--------------------------|------------------------------------------------------------------------------------|
| MODEL_NAME                    | `ThinkingColor(level)`   | Adapts to thinking level                                                           |
| `| PROVIDER | T: level | IMG` | `dimText`                | **IMG** when the active model supports image input (`provider.SupportsImageInput`) |
| `$0.00`                       | `ContextUsageColor(pct)` | Cost                                                                               |
| `X%` or `X% | 262k` or `262k` | `ContextUsageColor(pct)` | Token usage (configurable via `footerTokenDisplay`)                                |

### Line 2

| Segment            | Color                             | Notes                       |
|--------------------|-----------------------------------|-----------------------------|
| `project_dir`      | `whiteCol`                        | Bold directory name         |
| `[session_id]`     | `dimText`                         | Session identifier          |
| `mode`             | `modeBorderColor(mode)`, **bold** | Agent mode, lowercase       |
| `turn: 0 | branch` | `whiteCol`                        | Turn count and branch name  |
| `[+N -N]` or `[-]` | Git change color                  | See git status colors below |

---

## Color Functions

### Thinking Level → Color

| Level   | Color  | Hex       |
|---------|--------|-----------|
| off     | gray   | `#6B7280` |
| minimal | gray   | `#6B7280` |
| low     | green  | `#22C55E` |
| medium  | yellow | `#EAB308` |
| high    | orange | `#F97316` |
| xhigh   | red    | `#EF4444` |

### Context Usage → Color

| Range | Color  | Hex       |
|-------|--------|-----------|
| ≤ 50% | white  | `#FFFFFF` |
| ≤ 79% | yellow | `#EAB308` |
| ≤ 89% | orange | `#F97316` |
| ≥ 90% | red    | `#EF4444` |

### Agent Mode → Color (border + footer)

| Mode  | Color        | Hex       |
|-------|--------------|-----------|
| build | neutral gray | `#6B7280` |
| plan  | cyan         | `#06B6D4` |
| ask   | blue         | `#3B82F6` |
| brave | red          | `#EF4444` |

### Git Status → Color

| Condition      | Display   | Color  | Hex       |
|----------------|-----------|--------|-----------|
| no changes     | `[-]`     | gray   | `#6B7280` |
| additions only | `[+3 -0]` | green  | `#22C55E` |
| deletions only | `[+0 -2]` | red    | `#EF4444` |
| mixed          | `[+3 -2]` | yellow | `#EAB308` |

### Git refresh behavior

To avoid loading the full repository via go-git while idle:

| When                            | What updates                                                    |
|---------------------------------|-----------------------------------------------------------------|
| TUI startup (async)             | Branch name only (`git.ReadBranch` — reads `.git/HEAD`)         |
| Every 2 minutes (idle tick)     | Branch name only; `+N -N` stats are **not** refreshed           |
| Footer click on branch/git area | Full stats (`git.Read` — go-git, line diffs capped at 32 paths) |
| After shell command completes   | Full stats (async)                                              |

Until a full refresh runs, `[+N -N]` may show stale values while the branch name stays current.

---

## Models.dev update dialog

When model metadata may be outdated (`syncInterval` elapsed since `version.json` → `lastSyncProviders`), the TUI checks models.dev **once at startup**. If updates are available, a **[huh](https://github.com/charmbracelet/huh) confirm** replaces the input area:

- **Label:** Model update
- **Description:** Model metadata updates are available.
- **Actions:** `Update` (full sync) or `Skip` (record sync time, no download)

Implementation: `internal/renderer/models_sync.go`. Settings: [Configuration § Models.dev sync in the TUI](/configuration/configuration/#modelsdev-sync-in-the-tui).

---

## Keybindings

Source of truth: `internal/constants/keymap.go`.

| Key                 | Action                                                                                           |
|---------------------|--------------------------------------------------------------------------------------------------|
| `Ctrl+C`            | First press: quit notice; second press: quit (or clear input + attachments if typing)            |
| `Ctrl+X`            | Cancel / Quit                                                                                    |
| `Ctrl+D`            | Exit application                                                                                 |
| `Ctrl+A`            | Switch agent mode                                                                                |
| `Shift+Tab`         | Cycle thinking level                                                                             |
| `Enter`             | Send message; in slash palette, run or complete selected command                                 |
| `Ctrl+J`            | Insert newline in input                                                                          |
| `Shift+Enter`       | Insert newline in input                                                                          |
| `Ctrl+L`            | Open model selector                                                                              |
| `Ctrl+Y`            | Copy last AI response (raw markdown source)                                                      |
| Click copy hint     | Copy that assistant message (raw source) — see [AI response formatting](#ai-response-formatting) |
| `Ctrl+V`            | Paste image from clipboard (**Cmd+V** on macOS); falls back to text paste                        |
| `Ctrl+O`            | Preview/edit pasted block (input); else expand/collapse newest collapsible block                 |
| `Ctrl+Shift+T`      | Cycle theme (auto/dark/light)                                                                    |
| Click header/footer | Expand/collapse that specific block                                                              |
| `:q` / `:q!`        | Quit (vim-style)                                                                                 |

Agent modes (`build`, `plan`, `ask`, `brave`) are also clickable in the footer. Modes are persisted in `~/.elph/settings.json` but do not change runtime tool or prompt behavior yet — see [Agent Runtime](/guides/agent-runtime/).

## Message timestamps

User and assistant blocks can show a compact local timestamp (`internal/renderer/message_time.go`):

- Today: `15:04:05`
- Other days: `Jan 2 15:04:05`

## Activity stopwatch

During agent activity (connecting, thinking, tool work), a stopwatch shows elapsed time (`internal/renderer/activity_stopwatch.go`). Updates every 100ms while active.

## Model selector

`Ctrl+L` or `/model` opens a fuzzy overlay (`internal/renderer/model_selector.go`). Filter providers with arrow keys; select with Enter. Left/Right (with an empty filter) cycle provider groups.

### No model selected

Elph does not pick a default model at startup. The footer shows **No model selected** until you choose one. Sending a message without an active provider opens the model picker (your draft text is preserved).

### Missing API key

If the provider JSON has no resolved `apiKey`, confirming a model still **saves** `session.providerId` / `session.modelId` to `~/.elph/settings.json` and updates the footer. Chat remains blocked until credentials work; the status message points at `~/.elph/providers/<id>.json` or the referenced environment variable.

### Draft preservation

Opening the picker while the input has text stashes the draft, clears the filter field, and restores the draft when the picker closes (Esc, **Ctrl+L**, or after confirming a model).

## @-mentions

Type `@` in input to fuzzy-search workspace files and directories (`internal/mention`). Skips `.git`, `node_modules`, and similar directories.

## Tasks panel (TodoList)

When the agent maintains todos via **TodoList**, a **Tasks** panel appears above the input
(`internal/renderer/todo_panel.go`). It lists pending and in-progress items with status markers
(`○` pending, spinner/`◐` in progress, `✓` done). The panel width matches the input chrome border.

- Hidden when there are no active (non-done) tasks.
- When the last item is marked `done`, the panel closes and a system notice in the chat area lists
  completed tasks (`All tasks completed.` plus `✓` lines).
- TodoList tool calls do not open a collapsible detail box (unlike Bash/Read).

Persisted per session at `<workDir>/.agents/elph/metadata/<sess_id>/todos.jsonl` — see
[Agent Runtime § TodoList session state](/guides/agent-runtime/#todolist-session-state).

## Shell input

| Prefix  | Meaning                                          |
|---------|--------------------------------------------------|
| `!cmd`  | Run shell; output can be queued as agent context |
| `!!cmd` | Run shell without agent context                  |

Output appears in a collapsible detail box labeled `$ <command>` with status colors (running /
success / error / cancelled). Stream chunks honor terminal carriage returns (e.g. ping statistics
overwriting one line) while preserving newlines between separate lines.

## Tool approval and AskUser

When the agent calls **Write**, **Edit**, **Bash**, or **AskUser**, a huh form appears in the input
chrome (replacing the normal prompt until you answer).

### Write, Edit, and Bash approval

Write and Edit use the same huh select as Bash (`Allow once` / `Allow for session` / `Deny`). The
description shows tool arguments (`path`, `contents`, `old_string`, `new_string`, or `command`).
Long Bash commands and argument text are word-wrapped and capped to a few lines so the dialog fits
the terminal (`internal/renderer/tool_interact.go`).

| Input     | Action                                  |
|-----------|-----------------------------------------|
| `y` / `1` | Allow once                              |
| `a` / `2` | Allow for session                       |
| `n` / `3` | Deny                                    |
| `Enter`   | Confirm selection (default: allow once) |
| `Esc`     | Deny                                    |

Denying returns `User denied tool execution` to the model. The same tool signature is not prompted
again during the current agent turn. See [Tools § User approval](/guides/tools/#user-approval-huh).

### AskUser

Questions with `options` support number keys `1`–`9` and arrow keys. Free-text questions use a
single-line input.

Agent events (tool start, stream deltas, response tokens) continue to update the main area while the
dialog is open.

## Native tool detail

Native tool calls (`EventToolCallStart` / `EventToolCallOutputDelta` / `EventToolCallDone`) render in
`internal/renderer/agent_native.go`:

| Tool  | Title         | Body                                                                         |
|-------|---------------|------------------------------------------------------------------------------|
| Bash  | `$ <command>` | Raw stdout/stderr streamed live; final body adds `(exit N)` on non-zero exit |
| Other | Tool name     | Formatted via `runtime.FormatToolDetailBodyFromResult`                       |

While **running**, the detail box shows a spinner until the first output byte arrives, then streams
text live. **Bash** and `!`/`!!` shell boxes stay expanded during streaming; other tools may start
collapsed when the final body is long. Chunk boundaries preserve `\n` so line-oriented tools (e.g.
`ping`) stay readable. Shell `!` output uses the same `ApplyStreamChunk` helper
(`internal/runtime/shell.go`).

## Stream Messages

| Type     | Prefix | Color                                                                  |
|----------|--------|------------------------------------------------------------------------|
| User     | `\|`   | `userPipeCol`                                                          |
| AI       | `\|`   | `aiPipeCol`                                                            |
| System   | `> `   | `highlight`                                                            |
| Detail   | —      | Soft status-colored box — neutral, running, success, warning, error    |
| Thinking | —      | Neutral dim gray box; `autoExpandThinking` in settings (default false) |

### AI response formatting

Assistant messages use one of three render paths (`internal/renderer/markdown.go`, `view.go`):

| Phase     | Path         | Notes                                                         |
|-----------|--------------|---------------------------------------------------------------|
| Streaming | Plain reflow | No Glamour; markdown syntax stays visible until the turn ends |
| Complete, plain    | Plain reflow      | Lightweight markdown stripped (`**`, `` ` ``, links) before reflow   |
| Complete, markup   | Glamour v2        | Headings, lists, tables, blockquotes, code, etc.; async when detected |

**Glamour path**

- Renderer: `charm.land/glamour/v2` with Elph styles (`glamour_styles.go`) — H1 matches other
  headings (no dark-theme badge); image alt text only (no `Image:` prefix).
- Options: `WithPreservedNewLines()` (blockquotes keep one line per `>` row),
  `WithTableWrap(false)` (wide cells truncate instead of breaking row alignment),
  `WithEmoji()` (shortcodes such as `:smile:`).
- Preprocess (`preprocessMarkdownForGlamour`): normalize nested blockquotes; convert footnotes and
  `<details>` blocks; strip abbreviation definition lines; images before links (OSC 8 hyperlinks on
  link text only, URL hidden).
- Output is painted per line via `renderAIPreformattedBlock` so table bars and quote `│` columns
  are not reflowed by lipgloss.
- Async: `scheduleMarkdownRender` sets `markdownPending` and refreshes via `markdownRenderCmd`; the
  per-turn Glamour instance cache (`aiMarkdownCache`) is reset in `agent.go` after each turn.

**Plain prose path**

`formatAIProse` reflows to the message column without hyphenation. Paragraph heuristics
(`splitAIProseParagraphs`, `shouldAIProseParagraphBreak`, `joinAIProseLines`):

| Signal                                         | Treatment                                                   |
|------------------------------------------------|-------------------------------------------------------------|
| Blank line (`\n\n`)                            | Always a new paragraph                                      |
| Sentence end (`.!?`) + capital/digit next line | New paragraph if the previous line is not near column width |
| Lowercase continuation, hyphen wrap (`word-`)  | Join into the same paragraph                                |
| Line nearly full width + newline               | Treated as soft wrap (join)                                 |

`renderAIBlock` inserts one blank row between paragraph blocks. AI prose separator lines (`---`,
`========`) are stripped on the plain path only — Glamour renders `---` / `***` / `___` as
horizontal rules.

**Copy hint**

Finished (non-streaming) assistant messages show a dim italic footer: `click or ctrl+y to copy`
(`ai_copy.go`). Copy writes the **raw message source** (markdown as stored), not the rendered
ANSI output. `Ctrl+Y` copies the last AI reply; clicking the hint copies that specific message.
Implementation: `collapsible_hit.go` (`zoneAICopyFooter`), `mouse.go`.

**Markdown support notes (Glamour path)**

| Feature               | TUI behavior                                             |
|-----------------------|----------------------------------------------------------|
| Headings, lists, code | Full Glamour styling                                     |
| Tables, blockquotes   | Preserved line structure; nested quotes normalized       |
| Links                 | Link text + OSC 8 click (terminal-dependent)             |
| Images                | Alt text only                                            |
| Footnotes             | `[^n]` → `(n)`; definitions appended as blockquote notes |
| `<details>`           | Converted to blockquote with bold summary                |
| Abbreviations         | Definition lines stripped (no hover in terminal)         |
| Math (`$…$`, `$$…$$`) | Shown literally (no LaTeX renderer)                      |
| Raw HTML              | Tags stripped after `<details>` preprocess               |

Regression fixture: `internal/renderer/markdown_sample_test.go` (comprehensive user markdown).

**Performance**

- Message render cache avoids re-lipgloss on unchanged blocks (`message_render.go`).
- Stream prefix cache reuses rendered history while the active reply grows (`stream.go`).
