# TUI Layout Docs

## Layout

```
╭─────────────────────────────────────────────────────────────────╮
│                                                                 │
│   ⣿⣿⡟⣿⡟⣿⣿    Welcome to Elph v0.79.1 (update available)         │
│   ⣿⣿⣿⣿⣿⣿⣿    Send /changelog to show version history.           │
│                                                                 │
│  Directory:  ~/some/path/to/project_dir                         │
│  Model:      Claude Sonnet 4.6 [anthropic] (000 available)      │   <- BANNER (line-clamp if not enough width)
│  Stats:      00 ext, 00 commands, 00 skills, 00 tools           │
│  MCP:        0/0 connected (000 tools)                          │
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
MODEL_NAME | PROVIDER | T: high | IMG           $0.00 | 0.0% (262k)   <- FOOTER / STATUSLINE (line-clamp if not enough width)
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
- **Configurable**: `showPromptPrefix` (default: `true`). When `false`, prefix is hidden.

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

---

## Footer / Statusline

### Structure (no border)

```
MODEL_NAME | PROVIDER | T: level | IMG           $0.00 | X% (262k)
project_dir [session_id] mode             turn: 0 | branch [+N -N]
```

### Line 1

| Segment                       | Color                    | Notes                    |
|-------------------------------|--------------------------|--------------------------|
| MODEL_NAME                    | `ThinkingColor(level)`   | Adapts to thinking level |
| `| PROVIDER | T: level | IMG` | `dimText`                | Static metadata          |
| `$0.00`                       | `ContextUsageColor(pct)` | Cost                     |
| `X% (262k)`                   | `ContextUsageColor(pct)` | Context usage percentage |

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

---

## Keybindings

| Key           | Action                  |
|---------------|-------------------------|
| `Ctrl+C`      | Cancel / Quit           |
| `Ctrl+X`      | Cancel / Quit           |
| `Ctrl+D`      | Exit application        |
| `Alt+M`        | Switch agent mode       |
| `Enter`       | Send message            |
| `Ctrl+J`      | Insert newline in input |
| `Shift+Enter` | Insert newline in input |
| `Shift+Tab`   | Cycle thinking level    |
| `:q` / `:q!`  | Quit (vim-style)        |

## Stream Messages

| Type   | Prefix | Color         |
|--------|--------|---------------|
| User   | `\|`   | `userPipeCol` |
| AI     | `\|`   | `aiPipeCol`   |
| System | `> `   | `highlight`   |
