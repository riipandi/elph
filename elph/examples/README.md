# Elph Examples

This directory contains example programs to test and demonstrate Elph functionality.

## test_notifications.rs

Test program for Elph's notification system using OSC escape sequences. This example demonstrates:

- Basic notification sending via terminal escape sequences
- Rate limiting and deduplication
- Different notification types (turn complete, tool permission, user question, error, startup ready)
- Settings-based filtering (master switch, per-type toggles, duration thresholds)
- Notification queue behavior

### Running the Example

```sh
cargo run -p elph --example test_notifications
```

### Expected Behavior

The program will:

1. Send 6 terminal notifications via OSC escape sequences
2. Demonstrate rate limiting (immediate duplicates are blocked)
3. Show different notification types with appropriate messages
4. Test settings-based filtering (fast turns, disabled types, master switch)

### Notifications You Should Receive

1. **Turn complete** - Agent finished responding (10s)
2. **Tool permission** - Agent wants to execute: read_file
3. **User question** - Agent has a question
4. **Error** - Failed to connect to database
5. **Startup ready** - Agent and MCP servers are initialized
6. **Turn complete** - After rate limit expiration

### Console Output

The program prints detailed output showing:

- Which notifications are being sent
- Rate limiting behavior
- Settings-based filtering results
- Test completion status

### Platform Support

This example uses terminal escape sequences (OSC 99, OSC 9, OSC 777) which work on:

**Supported Terminals:**
- **Kitty** (OSC 99) - Full notification support with title and body
- **iTerm2** (OSC 9) - Basic notifications
- **WezTerm** (OSC 777) - Basic notifications
- **Ghostty** (OSC 777) - Basic notifications
- **Windows Terminal** (OSC 9) - Basic notifications
- **VTE-based terminals** (OSC 777) - GNOME Terminal, etc.

**Advantages of OSC escape sequences:**
- Works via SSH, tmux, screen
- No external dependencies required
- Graceful degradation on unsupported terminals
- More reliable for CLI/TUI applications than desktop notifications

**Terminal Compatibility:**
Terminals that don't support these sequences will simply ignore them, making this a safe fallback approach. The notification system is designed to work gracefully in all environments.
