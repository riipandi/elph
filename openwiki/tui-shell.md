---
type: Guide
title: TUI Shell
description: The iocraft-based terminal UI for Elph — layout zones, interaction modes, theme system, prompt chrome, and session bootstrap flow.
tags: [tui, shell, iocraft, ui, theming]
resource: /elph/src/tui/
---

# TUI Shell

The Elph TUI is built on a patched local vendor of [iocraft](https://crates.io/crates/iocraft) (v0.8.4) for terminal UI rendering. It integrates with the agent runtime via the `AgentBridge` event channel.

**Source:** `/elph/src/tui/`, `/crates/elph-tui/`

## Layout zones

```
┌──────────────────────────────────────────────────────────────┐
│ BANNER / HEADER                                              │
│ Welcome, directory, model, stats, MCP status, tips           │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│                   MAIN CHAT AREA                             │
│               (Conversation transcript)                      │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│  > Input Prompt (multiline, with prefix detection)           │
├──────────────────────────────────────────────────────────────┤
│ FOOTER / STATUS LINE                                         │
│ Model | Provider | Thinking | Cost | Tokens | Turn | Git     │
└──────────────────────────────────────────────────────────────┘
```

**Source:** `/elph/src/tui/shell.rs` — `MainShell` component

### Banner

Shows on session startup:

- Braille logo + version
- Current working directory
- Active model and provider
- Extension/skill/tool counts
- MCP server status
- Random tip

**Source:** `/elph/src/tui/chrome/`

### Transcript

The main chat area renders:

- User messages with purple pipe (`│`)
- AI responses with grey pipe (`│`)
- Tool calls and results as collapsible cards
- Subagent activity indicators
- Ephemeral banners for status updates
- Confetti overlay on goal completion

**Source:** `/elph/src/tui/transcript/`

### Prompt

The input area features:

| Prefix | Meaning               | Action                                |
| ------ | --------------------- | ------------------------------------- |
| `>`    | Normal chat           | Send to agent                         |
| `/`    | Slash command         | Dispatch to slash handler             |
| `!`    | Shell with context    | Execute locally, feed output to agent |
| `!!`   | Shell without context | Execute locally, output only          |

- **Multiline editing**: Ctrl+J or Shift+Enter for newline; Enter to submit
- **Fuzzy slash palette**: Ctrl+Space or `/` to trigger
- **History navigation**: Up/Down arrows

**Source:** `/elph/src/tui/prompt/`, `/crates/elph-tui/src/input_prefix.rs`

### Footer

Shows:

- Agent mode (Build/Plan/Ask/Brave) with mode-specific color
- Model name + provider
- Thinking level with color
- Token usage and costs
- Turn counter
- Git branch info (staged/unstaged changes)
- Image support indicator

**Source:** `/elph/src/tui/chrome/status_row.rs`

## Interaction modes

| Mode                | Toggle         | Description                                              |
| ------------------- | -------------- | -------------------------------------------------------- |
| Agent mode          | `ctrl+m`       | Cycle: Build → Plan → Ask → Brave                        |
| Thinking level      | `ctrl+t`       | Cycle: Off → Minimal → Low → Medium → High → Xhigh → Max |
| Model picker        | `ctrl+p`       | Open model selection dialog                              |
| Theme               | `ctrl+shift+t` | Cycle: Auto → Dark → Light                               |
| Scoped models       | `ctrl+shift+m` | Open scoped models configuration                         |
| System prompt       | `ctrl+o`       | Show system prompt editor                                |
| Session preferences | `ctrl+,`       | Open session settings                                    |

**Source:** `/elph/src/tui/focus.rs`, `/elph/src/tui/model_selector.rs`

## Theme system

The theme system (`crates/elph-tui/src/theme_config.rs`) supports:

- **Modes**: Auto (follow system), Dark, Light
- **Terminal detection**: Reads terminal appearance settings
- **Palette tokens**: Named colors for each UI element
- **Syntax highlighting**: Powered by `syntect` with built-in themes

Color palette principles (Ghostty dark palette):

| Token         | Dark      | Usage                    |
| ------------- | --------- | ------------------------ |
| `blueCol`     | `#3B82F6` | Banner border            |
| `yellowCol`   | `#EAB308` | Tip label                |
| `special`     | `#73F59F` | Braille logo             |
| `dimText`     | `#5C5C5C` | Labels, secondary info   |
| `brightText`  | `#D1D5DB` | Values, metadata content |
| `userPipeCol` | `#A78BFA` | User message pipe        |
| `aiPipeCol`   | `#9CA3AF` | AI response pipe         |

**Source:** `/crates/elph-tui/src/theme_config.rs`, `/elph/src/tui/theme.rs`

## Session bootstrap flow

`startup.rs` implements the TUI bootstrap:

1. Resolve paths (`Paths::resolve()`)
2. Ensure settings directory exists
3. Load settings (home + project layered merge)
4. Initialize extension host
5. Load prompt templates and skills
6. Resolve provider and model from settings
7. Create/call `create_coding_session_with_events()` (from `agent/runtime.rs`)
8. Display startup banner with info
9. Enter main shell event loop

**Source:** `/elph/src/tui/startup.rs`

## Event bridge

The `AgentBridge` (`/elph/src/tui/agent_bridge.rs`) converts agent runtime events into TUI component updates:

- `ThinkingDelta` → status row thinking indicator
- `ResponseDelta` → streaming text in transcript
- `ToolStart` / `ToolOutput` / `ToolDone` → tool cards
- `TurnDone` → flush transcript, update stats
- `SubagentUpdate` → subagent display
- `Error` → API error display

## Reusable components (`elph-tui`)

The `elph-tui` crate provides reusable widgets used by the main TUI and external consumers:

| Component          | File                               | Description                                     |
| ------------------ | ---------------------------------- | ----------------------------------------------- |
| Markdown           | `components/markdown/`             | Live markdown renderer with syntax highlighting |
| Textarea           | `components/textarea/`             | Multi-line text editor                          |
| Dialog shell       | `components/dialog_shell/`         | Modal dialog framework                          |
| Progress indicator | `components/progress_indicator.rs` | Progress bar                                    |
| Status indicator   | `components/status_indicator.rs`   | Status dots/indicators                          |
| Select             | `components/select.rs`             | Selection list                                  |
| Transcript layout  | `transcript_layout.rs`             | Chat-like vertical layout                       |
| Text input layout  | `text_input_layout.rs`             | Input area layout                               |
| Slash palette      | `slash_palette/`                   | Fuzzy completion palette                        |
| Color              | `color.rs`                         | Color parsing and conversion                    |
| Theme config       | `theme_config.rs`                  | Theme system definition                         |
| Loader             | `loader.rs`                        | Loading animations                              |
| CLI progress       | `cli_progress.rs`                  | Terminal progress spinners                      |

## Changing the TUI

When modifying TUI components, relevant test locations:

- Component tests — `crates/elph-tui/tests/`
- Key areas: transcript layout, textarea, color parsing, theme config
- Integration test in `/elph/tests/bootstrap.rs` for full TUI startup

Run: `cargo nextest run -p elph-tui`
