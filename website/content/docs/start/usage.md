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

## Clipboard paste

Use **Ctrl+V** (or **Cmd+V** on macOS) in the prompt editor:

- Image pastes are read asynchronously and inserted as atomic `[Image #N]` markers. The loading
  state is shown as an ephemeral notification, and the preview dialog opens above the textarea only
  when the caret touches the marker.
- Images are staged as PNG files in `APP_DATA/attachments/` and are submitted only to models that
  support vision/image input. JPEG/JPG, Bitmap/DIB, and other decodable raster clipboard formats
  are normalized to PNG. SVG is not treated as a vector attachment.
- With the default `ui.atomicPaste: true`, long text pastes become atomic
  `[Paste#N: N lines]` markers and are staged in `APP_DATA/temp/`. Cursor movement, backspace,
  delete, and selection do not split the marker. Press **Enter** or **Ctrl+O** to expand it; set
  `ui.atomicPaste` to `false` for normal inline text pastes.

Temporary image and text-paste files are cleaned up when their markers are expanded or removed,
when a prompt is submitted, or when the pending prompt is discarded.

## Headless

```sh
elph run "write a test"
elph run --mode=plan "design the auth boundary"
elph run --output=json "summarize this diff"
```

Formats: `plain`, `pretty`, `json`, `stream-json`, `stream-message-json`.

See [Slash commands](/docs/reference/commands) and [Keybindings](/docs/start/keybindings).
