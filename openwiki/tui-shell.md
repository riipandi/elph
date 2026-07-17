---
type: Component Guide
title: TUI & Shell
description: Interactive terminal UI built with iocraft — shell, chrome, confetti overlay, slash commands, markdown rendering, focus switching, tool approval, and inline dialogs.
tags: [tui, shell, iocraft, elph-tui, confetti]
resource: /elph/src/tui/
---

# TUI & Shell

The TUI system currently lives directly in the `elph` binary crate while the iocraft-based shell is being rebuilt iteratively.

The `elph-tui` library crate provides iocraft component modules (20+ component modules + 10+ crate-level modules) and 26+ examples, with integration tests. Once the public API stabilises, the reusable widget library will be extracted back into `elph-tui` and published to crates.io.

**TUI source**: `/elph/src/tui/` — Modular iocraft-based shell: `shell.rs`, `focus.rs`, `tool_approval.rs`, `user_question.rs`, `activity.rs`, `agent_bridge.rs`, `labels.rs`, `theme.rs`, and the subdirectories `chrome/`, `confetti/`, `prompt/`, `transcript/`, `slash_palette/`.
**Shell**: `elph/src/tui/shell.rs` is the main interactive shell orchestrating all TUI components. Focus switching (`focus.rs`), tool approval modal (`tool_approval.rs`), user question prompts (`user_question.rs`), activity tracking (`activity.rs`), and the slash palette (`slash_palette/`) are all standalone modules.

## elph-tui (Widget Library — Component Stubs)

**Path**: `/crates/elph-tui/`

The crate provides `lib.rs` with 20+ component modules under `components/` (most are implemented, not stubs):

- `ascii_font`, `card`, `code`, `dialog_shell`, `diff`, `frame_buffer`, `input`, `line_numbers`
- `markdown`, `progress_indicator`, `qr_code`, `scroll_bar`, `scroll_box`, `select`, `slider`, `status_indicator`, `tab_select`, `text`, `textarea`, `theme`

`textarea` is a directory containing `component.rs`, `input/` (paste, submit, wire_edit sub-modules), `layout.rs`, and `state.rs`. `markdown` is now a full directory with 11+ sub-modules: `blocks.rs`, `colors.rs`, `highlight.rs`, `layout.rs`, `linkify.rs`, `model.rs`, `parse.rs`, `parser_config.rs`, `render.rs`, `syntax.rs`, `table.rs`, `theme.rs` — providing syntax-highlighted code blocks (via `syntect`), GFM table grid rendering, and auto-linked URLs. `dialog_shell` provides modal-like dialog panels for confirm, multi-choice, user input, todo list, and progress displays. Additional modules live at the crate root: `input_prefix`, `slash_palette/` (fuzzy, keyboard, layout, model, state), `text_editing/` (actions, input, line, submit, wire), `transcript_layout.rs`, `text_input_layout.rs`, `loader.rs`, `paste.rs`, `utils.rs`, `cli_progress.rs`, and `color.rs`.

In the `elph` binary TUI (`/elph/src/tui/transcript/message.rs`), the transcript now renders structured tool invocation cards via `ToolCardDetail` (name, args summary, output body), replacing inline text formatting. Theme constants `TOOL_ARGS_FG` and `TOOL_OUTPUT_FG` control card colors. The previous `format_tool_card_content` / `format_tool_card_result` public helpers have been removed in favor of the structured `tool_call()` constructor on `TranscriptMessage`.

### Examples (26+ total in `crates/elph-tui/examples/`)

| Example                   | Description                                            |
| ------------------------- | ------------------------------------------------------ |
| `weather`                 | Async data loading from remote APIs with iocraft       |
| `calculator`              | Calculator app with iocraft UI                         |
| `chat_layout`             | Chat layout with scrollable content, input, tool cards |
| `progress_bar`            | Animated progress bar demo                             |
| `basic_context`           | Context API usage example                              |
| `basic_counter`           | Simple counter with state management                   |
| `basic_form`              | Form with input validation                             |
| `basic_input`             | Text input handling demo                               |
| `basic_layout`            | Layout composition demo                                |
| `basic_output`            | Text output display                                    |
| `basic_overlap`           | Overlapping elements demo                              |
| `basic_scrolling`         | Scrollable content                                     |
| `basic_table`             | Table layout demo                                      |
| `codeing_agent`           | Full coding agent app with overlays and shell          |
| `demo_code`               | Code block rendering                                   |
| `demo_diff`               | Diff output rendering                                  |
| `demo_dialog_choices`     | Dialog shell with choice widgets                       |
| `demo_dialog_shell`       | Dialog shell component demo                            |
| `demo_input`              | Input widget demo                                      |
| `demo_markdown`           | Markdown rendering                                     |
| `demo_scroll`             | Scroll container demo                                  |
| `demo_select`             | Select widget demo                                     |
| `demo_special`            | Special elements demo                                  |
| `demo_text_card`          | Text card rendering                                    |
| `demo_theme`              | Theme system demo                                      |
| `demo_loading_indicator`  | Loading indicator demo                                 |
| `demo_progress_indicator` | Progress indicator demo                                |

Run examples with: `cargo run -p elph-tui --example <name>`

## Slash Commands

**File**: `/elph/src/agent/slash_commands.rs` (implementation)
**Design doc**: `/docs/slash-commands.md`

Dispatch order:

1. **Built-in commands** (defined in `elph-builtin_commands`)
2. **Extension commands** (WASM plugins)
3. **Prompt templates** (`~/.elph/prompts/*.md` and `<project>/.elph/prompts/*.md`)

### Built-in commands

| Command                     | Aliases                                | Description                                 |
| --------------------------- | -------------------------------------- | ------------------------------------------- |
| `/help`                     | —                                      | List all commands                           |
| `/model`                    | —                                      | Open model selector                         |
| `/goal`                     | `/goals`                               | Manage session goals                        |
| `/tools`                    | —                                      | Show active tools (json/list/table)         |
| `/exit`                     | `/quit`, `/q`                          | Quit                                        |
| `/commit`                   | —                                      | Generate commit message from staged changes |
| `/compact`                  | `/c`                                   | Compact history                             |
| `/reload`                   | —                                      | Reload extensions + resources               |
| `/system-prompt`            | `/prompt`, `/systemprompt`             | Show assembled system prompt in a dialog    |
| `/diagnostic:list-tools`    | —                                      | List available tools                        |
| `/diagnostic:system-prompt` | —                                      | (Legacy — use `/system-prompt`)             |
| `/diagnostic:open-log`      | —                                      | Open session log                            |
| `/confetti`                 | `/conffety`, `/confetty` (hidden)      | Easter egg — rain or firework confetti      |

## TOON Prompt Encoding

**File**: `/crates/elph-agent/src/prompt/encoding/`

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

## Rich Markdown Rendering

**Source**: `/crates/elph-tui/src/components/markdown/`

The markdown component was upgraded from a simple stub to a full rendering pipeline with syntax highlighting and auto-linked URLs.

Key capabilities:

- **Parsing** — Uses `pulldown-cmark` to parse markdown into a `MarkdownDocument` model with structured blocks
- **Syntax highlighting** — Code fences with language tags are highlighted via `syntect` with the bundled Tokyo Night theme (`crates/elph-tui/assets/tokyo-night.tmTheme`)
- **Auto-links** — Plain URLs in text are detected and rendered in the theme's link color
- **Streaming** — `streaming_tail_document` supports incremental rendering as the model streams tokens
- **Theming** — `MarkdownTheme` controls colors for headers, code, links, inline code, and emphasis; `markdown_document_with_theme(source, theme)` allows per-document theme overrides

### Sub-modules

| Module             | Purpose                                       |
| ------------------ | --------------------------------------------- |
| `blocks.rs`        | Block-level rendering (paragraph, code, list) |
| `colors.rs`        | Color scheme definitions                      |
| `highlight.rs`     | syntect syntax highlighting integration       |
| `layout.rs`        | Layout calculations, row counts               |
| `linkify.rs`       | Plain-text URL detection                      |
| `model.rs`         | `MarkdownDocument`, block/section types       |
| `parse.rs`         | Markdown → `MarkdownDocument` conversion      |
| `parser_config.rs` | Parser configuration and options              |
| `render.rs`        | iocraft rendering of blocks and documents     |
| `syntax.rs`        | Language detection and syntax set loading     |
| `theme.rs`         | Default and custom themes                     |

### Transcript integration

In the binary TUI (`/elph/src/tui/transcript/markdown/`), a dedicated module handles rendering assistant responses with a `RenderWorker` that processes markdown documents incrementally. Components: `buffer.rs` (output accumulation), `layout.rs` (sizing), `render.rs` (iocraft rendering), `worker.rs` (incremental processing).

## Slash Palette

**Source**: `/elph/src/tui/slash_palette/`

An autocomplete overlay that appears above the prompt editor when the user types `/`. It provides fuzzy-filtered command completion for slash commands, prompt templates, and skills.

| Module          | Purpose                                                              |
| --------------- | -------------------------------------------------------------------- |
| `mod.rs`        | Re-exports: `SlashCommandPalette`, `build_snapshot`                  |
| `component.rs`  | Floating palette shell component                                     |
| `fuzzy.rs`      | Fuzzy matching logic for filtering commands                          |
| `keyboard.rs`   | Key-to-action mapping (`CompleteDraft`, `MoveSelection`, `Dismiss`)  |
| `layout.rs`     | Anchor calculations                                                  |
| `model.rs`      | `SlashPaletteSnapshot` building from draft text and command registry |
| `state.rs`      | Selection sync when filter query changes                             |
| `row_layout.rs` | Row rendering with icon and description                              |
| `card/`         | Bordered palette panel (chrome, header, body, frame)                 |

The slash palette integrates with `skills_load.rs` (`/elph/src/agent/skills_load.rs`) to discover and display `/skill:<name>` invocations. Skill names are registered as `skill:<name>` entries in the palette with truncated descriptions (72 char max).

## Focus Switching

**Source**: `/elph/src/tui/focus.rs`

The TUI supports keyboard-driven focus switching between the prompt editor and transcript.

- `ShellFocus` enum: `Prompt` or `Transcript` (default: `Prompt`)
- **Esc** — Moves focus from Transcript back to Prompt
- **Transcript navigation** — When transcript has focus, arrow keys, PageUp/Down, Home/End scroll the transcript content. Shift+arrows scroll by larger increments.
- **Auto-refocus** — Pressing a letter, space, or `/` while transcript has focus automatically refocuses the prompt and seeds the character (`prompt_focus_char`).

## Tool Approval

**Source**: `/elph/src/tui/tool_approval.rs`

A modal-like component that replaces the prompt editor when the agent requests approval for a tool call.

- `PendingToolApproval` stores tool name, args summary, and a `oneshot::Sender<ToolApprovalChoice>`
- **Key bindings**: `y`/`1`/`Enter` → Approve, `a`/`2` → AllowSession, `n`/`3`/`Esc` → Reject
- `ToolApprovalPrompt` renders a bordered panel showing tool name, truncated args preview (up to 4 lines with word-wrap), and key hints footer
- Activity labels update based on choice: "Running approved tool…", "Running tool (session allow)…", or "Tool denied"

## Stream Token Tracking

**Source**: `/elph/src/tui/activity.rs`, `/elph/src/tui/chrome/status_row.rs`

The status row displays real-time token consumption during model streaming.

- `TurnTokenTracker` (in `activity.rs`): Tracks `baseline_tokens` (at turn start) and `stream_tokens` (incremental delta, estimated at 1 token ≈ 4 characters)
- **Status row right segment**: Shows `+<delta> · <t/s>` throughput (e.g. `+240 · 12 t/s`)
- **Idle state**: Shows a rotating tip (10 tips) or an `idle_notice` like "Turn complete · 1.2s" for 5 seconds after a turn, then reverts to tips
- **Busy state**: Left side shows braille spinner + activity label + elapsed time (`⠋ Thinking · 1.2s`); right side shows token info + cancel hint (`+240 · 12 t/s | Ctrl+C cancel`)
- Turn count is displayed in chrome stats (`/elph/src/tui/chrome/stats.rs`), refreshed on each user input

## Quit Confirmation During Active Turn

**Source**: `/elph/src/tui/shell.rs`, `/elph/src/tui/activity.rs`

When the user issues a quit command (`/exit`, `:q`, `Ctrl+D`) while a turn is in progress, the shell does not exit immediately — it confirms first.

- **First quit attempt while busy**: Posts a transcript notice in warm orange (`QUIT_BUSY_NOTICE_FG: #fab373`) with extra vertical padding. Arms pending quit state.
- **Confirmation**: `y`/`Y` → confirm quit (cancels turn, rejects pending approvals/questions, aborts session); `n`/`N`/`Esc` → dismiss
- **Force quit**: `/exit!` or repeat quit command while pending confirm → immediate exit
- **Ctrl+C while busy**: Cancels the current turn (aborts, clears queue, rejects approvals)
- **Ctrl+C while idle**: Clears the prompt draft
- Status row right segment appends `" | y quit · n stay"` when quit confirmation is pending

## Model Selector

**Source**: `/elph/src/tui/model_selector.rs`, `/elph/src/tui/model_selector_bar.rs`, `/elph/src/tui/model_selector_shell.rs`, `/elph/src/tui/model_option_list.rs`

A multi-tab model picker that replaces the simple model list. It renders as an inline dialog above the status row.

- **Three scope tabs**: All (every known model), Scoped (settings-restricted subset), and Provider (per-provider tab with paging — 4 providers per page)
- **Fuzzy filtering**: Weighted scoring (name > id > description) with real-time filtering as the user types
- **Compact layout**: Two-column rows (name + hint), `‹ N` / `N ›` paging indicators for provider tabs
- **Persistence**: Selected model is saved to `SessionPrefs`
- **Data source**: Uses `elph_ai::get_builtin_models/providers` for the catalog

## @-mention File Picker

**Source**: `/elph/src/tui/file_picker/`

An inline fuzzy file picker triggered by typing `@` in the prompt editor. Searches the workspace via `fff-search` (fast filesystem search).

| Module               | Purpose                                                   |
| -------------------- | --------------------------------------------------------- |
| `model.rs`           | `FilePickerOption`, `ActiveMention`, `FilePickerSnapshot` |
| `component.rs`       | `FilePickerPalette` iocraft component                     |
| `fuzzy_highlight.rs` | ANSI-highlighted fuzzy match rendering                    |
| `keyboard.rs`        | Key-to-action mapping                                     |
| `apply.rs`           | File picker apply logic                                   |
| `state.rs`           | Selection state                                           |
| `highlight.rs`       | File path display formatting                              |

- **Trigger**: When `@` is typed, an `ActiveMention` token is tracked at the cursor
- **Search**: `build_snapshot` fires `fff-search` with `SEARCH_LIMIT = 200`, results shown in a floating palette anchored above the editor
- **Navigation**: Keyboard arrows, `FAST_SCROLL_STEP = 5`, enter to select
- **Insertion**: Selecting an entry inserts the file path and closes the picker
- **Max visible**: `MAX_VISIBLE_ROWS = 8` in the floating palette

## Inline Dialogs

**Source**: `/elph/src/tui/inline_dialog.rs`, `/elph/src/tui/status_dialog.rs`, `/elph/src/tui/tool_params.rs`

Full-width inline dialog pattern that sits within the TUI shell chrome (not floating overlays). Shared by the model picker, tool-approval prompts, and user-question steps.

- `InlineDialogShell` — Renders a sectioned dialog with tab states (`Current`/`Answered`/`Upcoming`)
- `StatusDialog` — Tool-approval dialog below the status row. Shows a compact params summary (`ToolApprovalLayoutPlan` with `args_viewport` and `list_height`) followed by selectable actions
- `ToolParams` — Structured tool-call parameter preview with priority-key highlighting (top keys: `command`, `path`, `file`, `query`). Constants: `MAX_PARAM_VALUE_CHARS = 240`, `APPROVAL_MAX_PARAM_ROWS = 3`, `APPROVAL_VALUE_MAX_CHARS = 72`
- All dialogs share `inline_body_width()` (same as the shell editor) for consistent sizing

## Deferred MCP Loading / Startup UI

**Source**: `/elph/src/tui/startup.rs`, `/elph/src/agent/mcp_bootstrap.rs`

Agent session creation is split into stages so the TUI appears immediately while MCP server discovery runs asynchronously.

- `TuiBootstrapConfig` — Paths, settings, resume ID, preloaded resources
- `BootstrapPhase` — `Pending → Running → AgentReady → McpLoading → Done → Failed`
- **Transient startup rows**: Each MCP server gets a keyed transcript row (`startup_key: "startup:mcp:{name}"`) showing live progress (loading spinner → checkmark `✓` or error `✕`)
- **Config warnings**: Warnings from malformed MCP configs are surfaced as startup rows
- `discover_mcp_registry_with_progress()` — Calls `McpToolRegistry::load_with_options()` best-effort, emits `McpServerLoadProgress` events per server
- `wire_mcp_into_session()` — Binds the loaded registry to a running `CodingAgentSession`
- Formatting constants: `STARTUP_SEP`, `STARTUP_ELLIPSIS`, `STARTUP_MCP_INDENT`, `STARTUP_WARN_INDENT`

## Ephemeral Notices (Self-Expiring Transcript Messages)

**Source**: `/elph/src/tui/transcript/ephemeral.rs`, `/elph/src/tui/transcript/types.rs`

A keyed upsert mechanism for transient notices in the transcript. Instead of stacking repeated messages, a single row with a stable key is updated in place and auto-expires after a TTL.

- `upsert_ephemeral_notice()` — Finds existing message by `startup_key` and replaces content/style, or pushes a new one
- `remove_ephemeral_notice()` — Removes by key
- `show_agent_mode_notice()` — Upserts `transient:agent_mode` with `AGENT_MODE_NOTICE_TTL = 3s`
- `TranscriptMessage::is_ephemeral_notice()` — True when `startup_key` starts with `"transient:"`
- Layout system adds `EPHEMERAL_NOTICE_EXTRA_PAD_TOP = 1` extra scroll padding for ephemeral rows
- Periodic cleanup: TUI render loop calls `remove_expired_ephemeral_notices()` each frame

## Transcript Timestamps

**Source**: `/elph/src/tui/transcript/card/timestamp_layout.rs`

Each user-submitted transcript message (input card) shows a dimmed right-rail label with the wall-clock processing duration and submission timestamp.

- `user_input_right_rail(submitted_at, duration_secs)` → builds `"1.2s 14:32"`
- `layout_user_input_lines()` — Wraps content, measures `display_width(rail)`, subtracts it from the first-line budget
- Uses `unicode-width` for display-width calculations
- `TranscriptMessage` fields: `duration_secs: Option<f64>`, `submitted_at: Option<DateTime<Utc>>`

## GFM Table Rendering

**Source**: `/crates/elph-tui/src/components/markdown/table.rs`

Renders GitHub-Flavored Markdown tables as a box-drawing grid in the markdown output.

- `TableLayout` — Column widths (`MIN_COL_WIDTH = 4`), row heights, grid computation
- `TableLine` — `Rule` (separator rows) or `Row` (cell segments with header flag)
- `CELL_PAD_X = 1` — Single space padding inside each cell
- `grid_vertical_bar_count()` — Total vertical rules for layout measurement
- `cell_display_width()` — Uses `unicode-width` for correct CJK/emoji column widths

## Confetti Overlay (Easter Egg)

**Source**: `/elph/src/tui/confetti/`

An animated particle overlay that produces full-screen confetti effects. Hidden from the slash palette and `/help`, triggered only by manually typing `/confetti`.

| Module          | Purpose                                                          |
| --------------- | ---------------------------------------------------------------- |
| `mod.rs`        | Re-exports `ConfettiOverlay`, `ConfettiRuntime`, entry helpers   |
| `physics.rs`    | Tuning constants: `SIMULATION_FPS = 60.0`, `GRAVITY = 95.0`     |
| `simulation.rs` | Particle engine core — `System`, `Particle`, `Point`, `Vector`   |
| `rain.rs`       | Rain mode: 140 particles from random top positions, 10 colors    |
| `fireworks.rs`  | Firework mode: 3 rockets per salvo, 72-pellet circular burst     |
| `overlay.rs`    | iocraft `ConfettiOverlay` component, full-screen absolutely-pos   |
| `array.rs`      | `sample()` utility for random selection                          |

- **Modes**: `/confetti` (default rain), `/confetti firework` / `/confetti fireworks`
- **Behavior**: Auto-closes after 2–5 seconds once particles settle; restores the prompt draft and focus on close.
- **Rendering**: Updates at ~60 FPS. Uses `crossterm::terminal::size()` polling for reliable full-screen dimensions during resize.
- **Keyboard**: While confetti is open, all other keyboard handlers are skipped (early return).

## System Prompt Dialog

**Source**: `/elph/src/tui/system_prompt_dialog.rs`

Opened by `/system-prompt` (aliases: `/prompt`, `/systemprompt`). Renders the compiled system prompt in a scrollable iocraft dialog overlay.

- `SystemPromptDialogOverlay` — Scrollable dialog using `ScrollBox` + `DialogChrome` + `DialogShellOverlay` from elph-tui.
- `open_system_prompt_dialog()` — Stashes any in-progress prompt draft, opens the dialog.
- `close_system_prompt_dialog()` — Clears the editor (no draft restore) and returns focus.
- Dynamic sizing: `MIN_DIALOG_WIDTH = 72`, `MAX_DIALOG_WIDTH = 120`, respects screen margins.
- The compiled prompt is rebuilt live via `session.compiled_system_prompt()` which calls `build_coding_system_prompt()` with current resources, tools, mode, and AGENTS.md content.

## Key source files

| Concern                            | Path                                                                                                                                 |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| TUI (current)                      | `/elph/src/tui/` (shell.rs, focus.rs, tool_approval.rs, activity.rs, agent_bridge.rs, chrome/, confetti/, prompt/, transcript/, slash_palette/) |
| Shell implementation               | `/elph/src/tui/shell.rs`                                                                                                             |
| Focus switching                    | `/elph/src/tui/focus.rs`                                                                                                             |
| Model selector                     | `/elph/src/tui/model_selector.rs`, `model_selector_bar.rs`, `model_selector_shell.rs`, `model_option_list.rs`                        |
| @-mention file picker              | `/elph/src/tui/file_picker/`                                                                                                         |
| Inline dialogs                     | `/elph/src/tui/inline_dialog.rs`, `/elph/src/tui/status_dialog.rs`, `/elph/src/tui/tool_params.rs`                                   |
| Tool approval modal                | `/elph/src/tui/tool_approval.rs`                                                                                                     |
| User question prompts              | `/elph/src/tui/user_question.rs`                                                                                                     |
| Activity + token tracking          | `/elph/src/tui/activity.rs`                                                                                                          |
| Slash palette                      | `/elph/src/tui/slash_palette/`                                                                                                       |
| Isomorphic text editor             | `/elph/src/tui/prompt/editor.rs`                                                                                                     |
| Transcript cards + markdown        | `/elph/src/tui/transcript/`                                                                                                          |
| Startup UI / MCP bootstrap         | `/elph/src/tui/startup.rs`, `/elph/src/agent/mcp_bootstrap.rs`                                                                       |
| Confetti overlay                   | `/elph/src/tui/confetti/`                                                                                                            |
| System prompt dialog               | `/elph/src/tui/system_prompt_dialog.rs`                                                                                              |
| Chrome (header, stats, status_row) | `/elph/src/tui/chrome/`                                                                                                              |
| Agent interaction                  | `/elph/src/agent/`                                                                                                                   |
| Diagnostics tool                   | `/elph/src/agent/diagnostics.rs`                                                                                                     |
| Skills load + slash parse          | `/elph/src/agent/skills_load.rs`                                                                                                     |
| Tools catalog reconciliation       | `/elph/src/agent/tools_catalog.rs`                                                                                                   |
| ui-components                      | `/crates/elph-tui/src/components/`                                                                                                   |
| TUI examples                       | `/crates/elph-tui/examples/`                                                                                                         |
| Prompt encoding                    | `/crates/elph-agent/src/prompt/encoding/`                                                                                            |
| Slash commands                     | `/elph/src/agent/slash_commands.rs`                                                                                                  |
| Agent runtime                      | `/elph/src/agent/runtime.rs`                                                                                                         |

## Change guidance

- **New slash command**: Implement in `/elph/src/agent/slash_commands.rs`
- **Slash palette update**: Modify `/elph/src/tui/slash_palette/model.rs` (snapshot building) and `/elph/src/tui/slash_handler.rs`
- **Skills slash parsing**: `/elph/src/agent/skills_load.rs` (`parse_skill_slash`, `skill_slash_name`)
- **TUI development**: Modify files under `/elph/src/tui/` (shell, focus, tool_approval, user_question, activity, agent_bridge, chrome, prompt, transcript, slash_palette)
- **Focus behavior**: `/elph/src/tui/focus.rs` — `ShellFocus` enum, `prompt_focus_char`, `transcript_nav_key`
- **Tool approval**: `/elph/src/tui/tool_approval.rs` — key bindings, `PendingToolApproval`
- **Token tracking**: `/elph/src/tui/activity.rs` — `TurnTokenTracker`, `format_busy_token_info`
- **Confetti overlay**: Add new particle effect in `/elph/src/tui/confetti/{rain,fireworks}.rs` and register in `simulation.rs`; register hidden slash command in `/elph/src/agent/slash_commands.rs` with `hidden: true`
- **System prompt dialog**: Modify `/elph/src/tui/system_prompt_dialog.rs` for layout/behavior changes; slash dispatch in `/elph/src/tui/slash_handler.rs`
- **Chrome / status row**: `/elph/src/tui/chrome/status_row.rs` — `busy_token_info`, `idle_notice` props
- **New TUI example**: Add to `/crates/elph-tui/examples/` and register in the crate's `Cargo.toml`
- **New component stub**: Add module to `/crates/elph-tui/src/components/` and register in `mod.rs`
- **Prompt encoding**: Test in `/crates/elph-agent/tests/prompt_encoding.rs`
- **TUI tests**: `/crates/elph-tui/tests/` — 14 test files including `transcript_layout`, `textarea`, `text_editing`, `text_input_layout`, `scroll`, `color`, `components_helpers`, `components_mock`, `coverage_gaps`, `coverage_helpers`
