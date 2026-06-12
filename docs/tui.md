# TUI Layout Docs

```
╭─────────────────────────────────────────────────────────────────╮
│                                                                 │
│   ⣿⣿⡟⣿⡟⣿⣿    Welcome to Elph v0.79.1 (update available)         │
│   ⣿⣿⣿⣿⣿⣿⣿    Send /changelog to show version history.           │
│                                                                 │
│  Directory:  ~/some/path/to/project_dir                         │
│  Model:      Claude Sonnet 4.6 [anthropic] (000 available)      │   <- BANNER (line-clamp if not enough width)
│  Stats:      00 ext, 00 commands, 00 skills, 00 tools           │
│  MCP Server: 0/0 connected (000 tools)                          │
│                                                                 │
│  Tip: Use --no-session for ephemeral mode — no session file is  │   <- TIPS can be wrapped
│  saved, useful for one-off queries.                             │
│                                                                 │
╰─────────────────────────────────────────────────────────────────╯

 -----------------------------------------------------------------
 > This is an example input message from user                         <- MAIN_AREA / RESPONSE_STREAM
 -----------------------------------------------------------------
 This is an example response from AI agent

╭─────────────────────────────────────────────────────────────────╮
│ >                                                               │   <- INPUT_PROMPT (multiline with ctrl+j or shift+enter)
╰─────────────────────────────────────────────────────────────────╯
MODEL_NAME | PROVIDER | T: high | IMG           $0.00 | 0.0% (262k)   <- FOOTER / STATUSLINE (line-clamp if not enough width)
project_dir [sess_abcd12345]               turn: 0 | main [+00 -00]
```
