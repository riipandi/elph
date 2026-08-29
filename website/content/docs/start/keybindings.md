# Keybindings

Common TUI bindings (exact keys may vary by terminal; see in-app footer / help):

| Binding                            | Action                                    |
| ---------------------------------- | ----------------------------------------- |
| **Enter**                          | Send prompt                               |
| **Shift+Enter** / multi-line paste | Multiline input (terminal-dependent)      |
| **Ctrl+C**                         | Abort active turn                         |
| **Ctrl+D** / `/exit` / `:q`        | Quit (`:q!` force-quits mid-turn)         |
| **Ctrl+L**                         | Clear / focus helpers (context-dependent) |
| **Shift+Tab**                      | Cycle thinking level                      |
| **Ctrl+V** / **Cmd+V**             | Paste image or clipboard text             |
| **Esc**                            | Close dialog / picker                     |
| `/` at prompt start                | Open slash command palette                |

Model selector, provider connect, and rename dialogs have additional keys shown in their footers.

When the clipboard contains an image, Elph stages it asynchronously and inserts an atomic
`[Image #N]` marker. When it contains long text and `ui.atomicPaste` is enabled, Elph inserts an
atomic `[Paste#N: N lines]` marker instead. Move the caret onto a marker to open its preview;
press **Enter** or **Ctrl+O** to expand a text paste marker in place.
