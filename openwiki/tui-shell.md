# TUI & Shell

The TUI system currently lives directly in the `elph` binary crate while the tuie-based shell is being rebuilt iteratively.

The `elph-tui` library crate is **temporarily disabled** (the crate contains only a notice). All former TUI widgets and shell infrastructure were either moved into the `elph` binary or removed. Once the public API stabilises, the reusable widget library will be extracted back into `elph-tui` and published to crates.io.

**TUI source**: `/elph/src/tui.rs` — Minimal tuie-based demo app.
**Shell source**: `/elph/src/shell/mod.rs` — Only `TuiOptions` (launch configuration).

## elph-tui (Widget Library — Temporarily Disabled)

**Path**: `/crates/elph-tui/`

The crate is currently empty except for `lib.rs` which contains a notice that the TUI lives directly in the `elph` binary while iterating on the tuie-based shell. All former modules (`agent/`, `chrome/`, `diff/`, `keymap/`, `prompt/`, `runtime/`, `shell/`, `terminal/`, `theme/`, `transcript/`, `widgets/`, `utils/`) were removed.

**TUI source**: `/elph/src/tui.rs` — Minimal tuie-based demo app (`ElphTui` struct).
**Shell source**: `/elph/src/shell/mod.rs` — Only `TuiOptions` (resume session ID).

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

## Key source files

| Concern           | Path                                      |
| ----------------- | ----------------------------------------- |
| TUI (current)     | `/elph/src/tui.rs`                        |
| Shell options     | `/elph/src/shell/`                        |
| Agent interaction | `/elph/src/agent/`                        |
| Diagnostics tool  | `/elph/src/agent/diagnostics.rs`          |
| Prompt encoding   | `/crates/elph-agent/src/prompt/encoding/` |
| Slash commands    | `/elph/src/agent/slash_commands.rs`       |
| Agent runtime     | `/elph/src/agent/runtime.rs`              |

## Change guidance

- **New slash command**: Implement in `/elph/src/agent/slash_commands.rs`
- **TUI development**: Modify `/elph/src/tui.rs`
- **Prompt encoding**: Test in `/crates/elph-agent/tests/prompt_encoding.rs`
- **TUI tests**: No separate tests (elph-tui tests removed; TUI is being rebuilt)
