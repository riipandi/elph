# Elph Examples

This directory contains example programs to test and demonstrate Elph functionality.

## test_notifications.rs

Test program for Elph's notification system. This example demonstrates:

- Basic notification sending
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

1. Send 6 desktop notifications to your OS notification center
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

This example works on:

- **macOS**: Notification Center
- **Linux**: D-Bus (XDG Desktop Notifications)
- **Windows**: Toast notifications

On headless or CI environments, notifications will fail silently and log a warning.
