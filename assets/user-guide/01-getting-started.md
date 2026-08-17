# Getting Started

Elph is a terminal-based AI coding companion. It runs as an interactive TUI, can execute
headless prompts (`elph run`), and speaks ACP v2 for editor integrations (`elph acp`).

## Installation

**Pre-built binaries** (Linux and macOS):

```sh
curl -fsSL https://elph.space/elph/install.sh | bash
```

**From source** (Rust ≥ 1.97):

```sh
cargo install --path elph
# or
cargo install --locked elph
```

Verify:

```sh
elph --version
```

## First launch

```sh
elph
```

On first run Elph scaffolds:

- Config: `~/.config/elph/` (`ELPH_HOME`)
- Data: `~/.local/share/elph/` (`ELPH_DATA_DIR`)
- Project: `<cwd>/.elph/`

Provider catalogs are unpacked into `CONFIG_DIR/providers/`. Built-in skills and this
user guide land under `CONFIG_DIR/bundled/`.

## Basic interaction

- Type a message and press **Enter** to send.
- Tool calls stream into the transcript as they run.
- Type `/` for slash commands, skills, and prompt templates.
- **Ctrl+C** interrupts the active turn; **Ctrl+D** / `/exit` quits.

## Next steps

- [Authentication](02-authentication.md) — API keys and credentials
- [Configuration](05-configuration.md) — layout and settings
- [Skills](07-skills.md) — reusable task packages
