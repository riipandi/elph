# TUI & Shell

The TUI system consists of two layers:

1. **`elph-tui`** — Reusable terminal UI widget library built on [tuie](https://crates.io/crates/tuie)
2. **`elph` shell** — The interactive coding agent application that uses `elph-tui`

Migrated from `superlighttui` to `tuie` in commit `b06c134`.

## elph-tui (Widget Library)

**Path**: `/crates/elph-tui/`

### Shell

**File**: `/crates/elph-tui/src/shell/`

| Type              | Purpose                                                                            |
| ----------------- | ---------------------------------------------------------------------------------- |
| `AgentShell`      | The main TUI shell runtime — event loop, render cycle                              |
| `ShellHost` trait | Host must implement: `poll`, `chrome_data`, `transcript_lines`, `on_prompt_action` |
| `ShellChromeData` | UI chrome state (activity, warnings, session info)                                 |

### Widgets

**File**: `/crates/elph-tui/src/widgets/`

| Widget               | Purpose                            |
| -------------------- | ---------------------------------- |
| `PromptPane`         | User input area with autocomplete  |
| `TranscriptPane`     | Chat transcript display            |
| `SidebarPlaceholder` | Sidebar panel                      |
| `StreamingText`      | Real-time streaming text display   |
| `CommandPalette`     | Slash command autocomplete palette |
| `chrome_tuie`        | Activity widget, footer widget     |

### Prompt

**File**: `/crates/elph-tui/src/prompt/`

| Type                    | Purpose                                          |
| ----------------------- | ------------------------------------------------ |
| `PromptState`           | User input state management                      |
| `ChatStreamState`       | Streaming response state                         |
| `PromptQueue`           | Queue of pending prompts                         |
| `ThinkingLevel`         | Thinking budget levels (none, low, medium, high) |
| `TranscriptStyle`       | Visual style for transcript entries              |
| `elph_builtin_commands` | Built-in slash command definitions               |

### Agent State

**File**: `/crates/elph-tui/src/agent/`

| Type                    | Purpose                                      |
| ----------------------- | -------------------------------------------- |
| `CollapseState`         | Collapse/expand state for transcript entries |
| `ModelSelectorState`    | Model selection UI state                     |
| `SessionSelectorState`  | Session selection UI state                   |
| `ToolApprovalState`     | Tool execution approval dialog state         |
| `PlanConfirmationState` | Plan mode confirmation dialog state          |
| `TreeNavigatorState`    | Session tree navigation state                |
| `OAuthSelectorState`    | OAuth provider selection UI state            |

### Diff Engine

**File**: `/crates/elph-tui/src/diff/`

A differential rendering engine with rich components:

| Component                      | Purpose                                         |
| ------------------------------ | ----------------------------------------------- |
| `DiffTui`                      | Main diff container                             |
| `DiffView`                     | Side-by-side diff view                          |
| `Editor`                       | Inline text editor                              |
| `Markdown`                     | Markdown renderer                               |
| `Text`                         | Text block rendering                            |
| `SelectList`                   | Selection list component                        |
| `SettingsList`                 | Settings configuration component                |
| `AutocompletePopup`            | Autocomplete popup                              |
| `Image`                        | Terminal image rendering (Sixel, Kitty, iTerm2) |
| `Loader` / `CancellableLoader` | Loading indicators                              |
| `OverlayHandle`                | Overlay management                              |

### Keymap

**File**: `/crates/elph-tui/src/keymap/`

| Type                 | Purpose                                             |
| -------------------- | --------------------------------------------------- |
| `GlobalChordHandler` | Global keyboard chord handling                      |
| `ShellAction`        | Actions dispatched from the shell                   |
| `ShellActionSink`    | Action sink for the shell                           |
| `PromptSubmitMode`   | How prompts are submitted (enter, ctrl+enter, etc.) |

### Transcript

**File**: `/crates/elph-tui/src/transcript/`

| Type                  | Purpose                                 |
| --------------------- | --------------------------------------- |
| `StreamingBuffer`     | Buffer for streaming transcript content |
| `TranscriptEntry`     | A single entry in the transcript        |
| `TranscriptRole`      | Role (user, assistant, tool, system)    |
| `ToolExecutionState`  | Tool execution tracking                 |
| `ToolExecutionStatus` | Tool execution status enum              |

### Theme

**File**: `/crates/elph-tui/src/theme/`

| Type        | Purpose                  |
| ----------- | ------------------------ |
| `Theme`     | Application theme        |
| `ThemeMode` | Light/dark mode          |
| `Color`     | Color definitions        |
| `Palette`   | Color palette management |

## elph Shell (Application)

**Path**: `/elph/src/shell/`

The interactive TUI application built on `elph-tui`.

### ElphApp

**File**: `/elph/src/shell/mod.rs`

The main application state struct:

```rust
pub struct ElphApp {
    pub prompt: PromptState,
    pub chat: ChatStreamState,
    pub theme: Theme,
    pub should_exit: bool,
    pub session_id: String,
    pub turn: u32,
    pub project_dir: String,
    pub thinking: ThinkingLevel,
    pub agent_running: bool,
    pub activity: ActivityState,
    pub slash_commands: Vec<SlashCommand>,
    pub git_branch: Option<String>,
    pub collapse: CollapseState,
    // ...
}
```

### Shell sub-modules

| Module                 | File                                   | Purpose                                        |
| ---------------------- | -------------------------------------- | ---------------------------------------------- |
| `events.rs`            | `/elph/src/shell/events.rs`            | UI event polling, global key handling          |
| `render.rs`            | `/elph/src/shell/render.rs`            | Frame rendering, `run_tui`, SIGINT watcher     |
| `overlays.rs`          | `/elph/src/shell/overlays.rs`          | Model/session/tree selector overlays           |
| `shell_host.rs`        | `/elph/src/shell/shell_host.rs`        | `ElphShellHost` — bridges TUI to agent runtime |
| `slash.rs`             | `/elph/src/shell/slash.rs`             | Slash command dispatch                         |
| `turn.rs`              | `/elph/src/shell/turn.rs`              | Turn lifecycle management                      |
| `transcript_render.rs` | `/elph/src/shell/transcript_render.rs` | Transcript line rendering                      |

### ShellHost implementation

`ElphShellHost` (`/elph/src/shell/shell_host.rs`) implements the `ShellHost` trait, bridging TUI events to the agent runtime. It:

1. Polls for UI events (model selection, tool approval, keyboard input)
2. Provides chrome data (session ID, git branch, activity state)
3. Dispatches prompt actions to the agent
4. Handles tool approval dialogs

## Slash Commands

**File**: `/elph/src/agent/slash_commands.rs` (implementation)
**Design doc**: `/docs/slash-commands.md`

Dispatch order:

1. **Built-in commands** (defined in `elph-builtin_commands`)
2. **Extension commands** (WASM plugins)
3. **Prompt templates** (`~/.elph/prompts/*.md` and `<project>/.elph/prompts/*.md`)

### Built-in commands

| Command                     | Aliases       | Description                                 |
| --------------------------- | ------------- | ------------------------------------------- |
| `/help`                     | —             | List all commands                           |
| `/model`                    | —             | Open model selector                         |
| `/goal`                     | `/goals`      | Manage session goals                        |
| `/exit`                     | `/quit`, `/q` | Quit                                        |
| `/commit`                   | —             | Generate commit message from staged changes |
| `/compact`                  | `/c`          | Compact history                             |
| `/reload`                   | —             | Reload extensions + resources               |
| `/diagnostic:list-tools`    | —             | List available tools                        |
| `/diagnostic:system-prompt` | —             | Show assembled system prompt                |
| `/diagnostic:open-log`      | —             | Open session log                            |

## TOON Prompt Encoding

**File**: `/crates/elph-agent/src/runtime/prompt_encoding/`

Optional structured-data encoding for tool results using the [TOON format](https://github.com/toon-format/toon). Enabled via `ELPH_PROMPT_ENCODING` env var or harness config.

| Mode   | Behavior                                                             |
| ------ | -------------------------------------------------------------------- |
| `off`  | Default — tool results pass through unchanged                        |
| `toon` | Encode eligible JSON at or above size threshold (default 1024 chars) |
| `auto` | Encode only uniform tabular JSON arrays                              |

### Implementation

| File           | Purpose                                                 |
| -------------- | ------------------------------------------------------- |
| `config.rs`    | `PromptEncodingConfig`, modes, targets, size thresholds |
| `encode.rs`    | `encode_value` — TOON encoding                          |
| `decode.rs`    | `decode_toon_fence` — Decode TOON fenced blocks         |
| `apply.rs`     | `apply_to_tool_result` — Apply encoding to tool results |
| `extract.rs`   | `extract_json_value` — Extract JSON from delimiters     |
| `fence.rs`     | `parse_toon_fence` — Parse TOON fence markers           |
| `heuristic.rs` | Heuristic detection of uniform tabular arrays           |

### Savings ratio

The encoding typically achieves 30–70% token savings on tabular data. The heuristic check avoids encoding non-tabular or small payloads.

## Key source files

| Concern           | Path                                              |
| ----------------- | ------------------------------------------------- |
| TUI widgets       | `/crates/elph-tui/src/`                           |
| Shell application | `/elph/src/shell/`                                |
| Shell host        | `/elph/src/shell/shell_host.rs`                   |
| Transcript render | `/elph/src/shell/transcript_render.rs`            |
| Slash dispatch    | `/elph/src/shell/slash.rs`                        |
| Overlays          | `/elph/src/shell/overlays.rs`                     |
| TUI runtime       | `/elph/src/shell/render.rs`                       |
| Agent interaction | `/elph/src/agent/`                                |
| Prompt encoding   | `/crates/elph-agent/src/runtime/prompt_encoding/` |

## Change guidance

- **New widget**: Add to `/crates/elph-tui/src/widgets/` and re-export from `lib.rs`
- **New slash command**: Add to `elph_builtin_commands()` in `/crates/elph-tui/src/prompt/mod.rs`, implement in `/elph/src/agent/slash_commands.rs`
- **Theme changes**: Modify `/crates/elph-tui/src/theme/`
- **Keymap changes**: Modify `/crates/elph-tui/src/keymap/`
- **Shell behavior**: Modify `/elph/src/shell/`
- **Prompt encoding**: Test in `/crates/elph-agent/tests/prompt_encoding.rs`
- **TUI tests**: `/crates/elph-tui/tests/tuie_shell.rs`, `tests/agent_demo.rs`
