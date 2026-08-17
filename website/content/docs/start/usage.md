# Using Elph

Run Elph from a project directory:

```sh
elph
```

## In the TUI

- Type a message and press **Enter** to send.
- Tool calls stream into the transcript.
- Type `/` for slash commands, skills, and prompt templates.
- **Ctrl+C** interrupts the active turn. **Ctrl+D** or `/exit` quits.

Resume the last session for this project with `elph -c`. Resume a specific id with `elph -r <session-id>`.

## Headless

```sh
elph run "write a test"
elph run --mode=plan "design the auth boundary"
elph run --output=json "summarize this diff"
```

Formats: `plain`, `pretty`, `json`, `stream-json`, `stream-message-json`.

See [Slash commands](/docs/reference/commands) and [Keybindings](/docs/start/keybindings).
