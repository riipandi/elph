---
name: tui-design
description: >-
    Guide terminal UI development with iocraft: component mental model, flex layout, scroll regions,
    overlap decoration, state/hooks, input handling, accessibility, and common layout pitfalls.
    Use when building or refactoring a TUI, fixing scroll/focus/layout bugs, choosing iocraft patterns,
    improving keyboard/contrast/screen-reader ergonomics, or the user mentions iocraft, terminal UI,
    TUI components, accessibility, a11y, or runs /tui-design.
---

# TUI Design (iocraft)

## Objective

Help implement and refine terminal UIs using **iocraft** with a clear mental model and repeatable patterns — not tied to any single product layout or design doc.

Read existing TUI code in the repo first; apply the principles below to match local conventions.

## Mental model

### 1. Terminal as a single viewport

The UI is one tree of elements sized to the terminal (`use_terminal_size`). Think **web flexbox in a fixed viewport**: columns stack top-to-bottom; one region usually **grows** to fill leftover height; fixed chrome (header, input, footer) does not grow.

### 2. Component = function + props + element tree

Each visual unit is a `#[component]` function taking `&Props` and `Hooks`. Props are plain structs (`#[derive(Default, Props)]`). Parent passes data down; interactive state lives in hooks, not in props.

Decompose by **responsibility**, not by screen coordinates:

- Shell (root layout, global events, exit)
- Scrollable content panel
- Fixed chrome (header, status, input area)
- Small leaf widgets (labels, inputs, buttons)

Prefer several small components over one monolithic `element!` block.

### 3. State vs presentation

| Concern                                          | Where it lives                              |
| ------------------------------------------------ | ------------------------------------------- |
| Layout dimensions                                | `use_terminal_size`, derived widths/heights |
| User-driven values (mode, selection, draft text) | `use_state` / focused widget state          |
| Side effects (timers, async)                     | `use_future`                                |
| Global app control (quit)                        | `use_context_mut::<SystemContext>`          |
| Pure rendering                                   | Props + read-only hook state                |

Initialize state with a closure: `use_state(|| initial)` — not `use_state(initial)` for non-Copy values.

### 4. Events are edge-triggered

Handle `use_terminal_events` on **press**, not release:

```rust
if kind == KeyEventKind::Release {
    return;
}
```

Match on `(modifiers, code)`. Decide whether shortcuts are global (shell) or local (focused `TextInput`) before wiring keys.

### 5. Layering: flow vs overlap

**Flow layout** — children affect parent size and sibling position (default).

**Overlap** — child with `position: Position::Absolute` is taken out of flow. Use for badges, border labels, overlays. Parent needs `position: Position::Relative`. Set `background_color: Color::Reset` on the overlay so it paints over the border cleanly. Negative margin (`margin_bottom: -1`) can sit a label on the border line without adding row height.

## Layout best practices

### Full-height column shell

```rust
View(
    width: screen_width,
    height: screen_height,
    flex_direction: FlexDirection::Column,
    justify_content: JustifyContent::FlexStart,
    align_items: AlignItems::Center,
) { /* fixed chrome + growing panel */ }
```

Use `FlexStart` on the **root**. `FlexEnd` on the root combined with inner `auto_scroll` can push fixed chrome off-screen.

### Growing scroll region (flex + clip)

The classic pattern for a chat/log area between fixed bars:

```rust
View(
    flex_grow: 1f32,
    flex_shrink: 1f32,
    min_height: 0,
    overflow: Overflow::Hidden,
) {
    View(width: 100pct, height: 100pct, overflow: Overflow::Hidden) {
        ScrollView(
            scrollbar: true,
            auto_scroll: true,
            keyboard_scroll: true,
        ) { /* content */ }
    }
}
```

`min_height: 0` is required for flex children to shrink below content size (same idea as CSS `min-height: 0`).

**Bottom-anchored content:** inner column may use `justify_content: JustifyContent::End` so new items sit at the bottom; pair with `auto_scroll: true` to follow the latest line.

### Common layout pitfalls

| Symptom                     | Likely cause                                                       | Fix                                                                 |
| --------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------- |
| Scrollbar missing           | `flex_grow` + `height: 100pct` on wrong node, or no bounded height | Use grow + `min_height: 0` + overflow hidden wrapper                |
| Header/footer scroll away   | Root `justify_content: FlexEnd` with auto-scroll                   | Root stays `FlexStart`; only inner content uses `FlexEnd` if needed |
| Extra blank row under input | Overlap label in normal flow                                       | `position: Absolute` on label container                             |
| Props compile errors        | Missing `Default` or wrong prop syntax                             | `#[derive(Default, Props)]`, named props in `element!`              |
| State won't init            | Passed value instead of initializer                                | `use_state(                                                         |     | value)` |

### Width discipline

Pass `screen_width` (or computed `width - padding`) into children that wrap text or draw borders so lines do not reflow unexpectedly. Split footers/status rows with `width / 2` or `justify_content: SpaceBetween`.

## iocraft component patterns

### Props

```rust
#[derive(Default, Props)]
struct PanelProps {
    width: u16,
    label: String,
}

#[derive(Clone, Copy, Default, Props)]
struct BadgeProps {
    width: u16,
    mode: Mode,
}
```

Use `Clone` on props when passing owned strings into child components. Use `Copy` for small enums passed by value.

### Hooks checklist

- `use_terminal_size()` — responsive layout
- `use_state(|| …)` — UI state
- `use_terminal_events(closure)` — keyboard/mouse
- `use_context_mut::<SystemContext>()` — `system.exit()`
- `use_future(async { … })` — intervals, background work (keep loops cancel-friendly)

### Entry point

```rust
element!(App(/* props */))
    .render_loop()
    .fullscreen()
    .enable_mouse_capture()  // when click/scroll matters
    .ignore_ctrl_c()         // handle Ctrl+C in app logic if needed
    .await?;
```

Use `.print()` for static snapshots in examples without a runtime loop.

## Input & focus

- One primary `TextInput` usually has `has_focus: true` unless building a multi-pane UI.
- `multiline: true` for prompt-style editors; document newline vs submit keys in status/help text.
- Global shortcuts (quit, cycle mode) belong in the shell's `use_terminal_events`; avoid duplicating handlers in every child.
- Tab may be consumed by the focused input — choose bindings that do not fight focus when simulating mode switches in demos.

## Accessibility

Terminal UIs are **keyboard-first** by nature, but accessibility still requires deliberate design: discoverable bindings, readable contrast, non-color-only state, and layouts that do not trap or hide content. Treat a11y as a first-class constraint alongside layout — not a post-hoc polish pass.

### Keyboard-first & discoverability

- **Every interactive action must have a keyboard path.** Mouse wheel / click are optional enhancements (`enable_mouse_capture`), never the only way to scroll, select, or submit.
- **Document shortcuts in visible chrome** — status row, footer, or `/help`. Prefer text like `Shift+↑/↓ scroll` over expecting users to guess.
- **Use consistent modifier vocabulary** across the app: e.g. `Shift` for transcript scroll, `Ctrl` for app control, plain `Enter` vs `Shift+Enter` for submit vs newline in chat editors.
- **Avoid binding conflicts** between global shell handlers and focused widgets. When a region disables `keyboard_scroll` on `ScrollView`, provide an explicit alternative (e.g. `Shift+Arrow` on the panel).
- **Handle press, ignore release** — prevents double-firing for users with sticky keys or assistive tech that synthesizes both events.

```rust
if kind == KeyEventKind::Release {
    return;
}
```

- **Expose quit / exit** on a well-known chord (`Ctrl+D`, `:q`) and mention it in help text.

### Focus & input ergonomics

- **One clear focus target** per screen state. Border style changes (`BorderStyle::Round` when focused) help sighted users; pair with a text cue if the focused region is not obvious.
- **Do not steal focus** on every re-render. Store draft text and editor state in `use_state` / `use_ref`, not ephemeral props recreated each frame.
- **Multiline editors:** distinguish submit vs newline in both behavior and docs (`submit_on_enter: true` → plain Enter submits, Shift+Enter / Ctrl+J insert newline).
- **Bracketed paste & CSI u** (when the app enables keyboard enhancement) improve paste fidelity and modified keys for screen readers and non-US layouts — preserve these hooks in the platform layer; do not strip enhancement flags without cause.

### Visual readability

- **Contrast:** body text, borders, scrollbar thumb/track, and error/success states must remain legible on default dark terminals. Prefer `Color::Grey` / `Color::DarkGrey` minimum for secondary chrome; avoid mid-grey on grey for primary content.
- **Never encode state by color alone.** Pair accent colors with text labels (e.g. agent mode badge shows `build` / `plan` / `ask`, not only a colored border).
- **Weight and punctuation** can reinforce meaning when color is muted: `Weight::Bold` on labels, explicit prefixes (`Error:`, `Tool:`) in transcript bubbles.
- **Visible caret** in editors — render a cursor block or distinct cell in focused `Textarea` / `TextInput`; do not rely on blink alone (many terminals disable blink).
- **Unicode symbols** (scrollbar `│`, borders) aid sighted users but degrade in some fonts/locales — keep critical information in ASCII labels nearby.

### Structure, motion & screen readers

Terminals lack a full accessibility tree like the web. Mitigate with **plain-text structure**:

- **Stable reading order:** header → transcript → status → input → footer. Do not reorder zones visually with absolute positioning unless the DOM-equivalent order still makes sense when read top-to-bottom.
- **Meaningful text content** over decorative-only rows. Tool call lines should say what happened (`read_file: path`), not just an icon glyph.
- **Avoid gratuitous flicker** — spinning indicators are fine for in-progress work; do not flash large regions. Respect `use_future` tick rates that are readable (1s clocks, not 50ms strobe).
- **Screen readers / braille displays** often linearize the screen. Assume users hear one line at a time: put the most important status on dedicated rows, not only in overlapping corner badges.
- **Announce destructive actions** in text (`:q!` vs `:q`) — confirmation copy is accessibility as much as safety.

### Scroll regions & sticky chrome

Overlapping sticky headers are a common **accessibility trap**: content below gets covered and becomes unreachable.

- **Reserve real viewport space** for sticky UI — inset the `ScrollView` below the sticky header (`top: sticky_rows`, `bottom: 0`), do not float a full-height overlay on top of a full-height scroller.
- **Cap sticky height** so a long pinned prompt cannot consume the entire panel; keep a minimum scrollable band (e.g. 3+ lines).
- **Clip sticky overflow** (`overflow: Hidden`, explicit `height`) so wrapped user prompts do not spill over assistant content.
- **Pair `auto_scroll: true`** with a manual scroll path so users who scrolled up can return to the latest message without restarting.
- **Sticky only when the user scrolled up** — disable sticky while `is_auto_scroll_pinned()`; a bottom-pinned offset looks like a large manual scroll and wrongly pins the latest long user bubble after submit. Clamp sticky height against the **full panel** viewport, not the inset scroll viewport, or long headers flicker (inset shrinks → clamp returns 0 → inset expands → repeat).
- **Line-clamp sticky prompts** — long pasted user messages use `layout_sticky_header` / `clamp_wrapped_transcript_lines` (default 4 body lines, ellipsis on the last line, dim `⋯ full prompt in transcript` hint). Never size the inset from the full wrapped row count of a paste.

```rust
// Sticky header in flow at top; ScrollView lives in the remaining band.
View(position: Relative, height: 100pct) {
    View(position: Absolute, top: sticky_rows, bottom: 0) {
        ScrollView(auto_scroll: true, scrollbar: true) { /* transcript */ }
    }
    View(position: Absolute, top: 0, height: sticky_rows, overflow: Hidden) {
        /* sticky user prompt */
    }
}
```

### Terminal size & environment

- **Responsive layout:** derive widths/heights from `use_terminal_size`; test mentally at 80×24 and very wide terminals. `min_height: 0` on flex children prevents clipped input at small heights.
- **Wrap width discipline** — pass consistent `screen_width - padding` into bubbles and editors so reflow does not shuffle content under users mid-read.
- **256-color / truecolor** assumptions fail on monochrome or high-contrast host settings — semantic roles should still read correctly without color (labels, weight, position).

### Accessibility checklist (before merge)

| Check                     | Pass criteria                                       |
| ------------------------- | --------------------------------------------------- |
| Keyboard complete         | All flows work with mouse disabled                  |
| Shortcuts documented      | Footer/status/help lists non-obvious chords         |
| Focus visible             | Focused editor/list has border or caret             |
| Color + text              | Modes and errors use words, not hue alone           |
| Contrast OK               | Primary text readable on default background         |
| Sticky safe               | Sticky headers inset scroll; min scroll rows remain |
| No release double-actions | Handlers ignore `KeyEventKind::Release`             |
| Small terminal            | 80×24: input + transcript + footer all reachable    |
| Linearizable              | Top-to-bottom read order still makes sense          |

## Visual design (general)

- Prefer a small set of semantic roles: primary text, secondary/dim, border, accent, error/success.
- Borders: structural chrome often neutral; accents on **text or small badges**, not every border, unless the UX calls for it.
- Scrollbars: enable explicitly; thumb/track colors should contrast with background.
- Padding: keep horizontal padding consistent between header, content, and footer so columns align visually.

Do not invent product-specific copy, palettes, or zone layouts unless the user or existing code defines them.

## Workflow

When implementing or fixing a TUI:

1. **Locate** existing shell/components in the repo; follow naming and file placement already in use.
2. **Sketch** the zone split (fixed vs growing) before writing `element!` trees.
3. **Extract** a new `#[component]` when a block exceeds ~40 lines or is reused.
4. **Wire state** at the lowest ancestor that needs it; pass derived values as props.
5. **Test layout** at small and large terminal sizes mentally: does the grow region still clip?
6. **Run the accessibility checklist** — keyboard-only path, focus visibility, sticky/scroll traps, color+text labels.
7. **Verify** compile the affected crate/example:
    ```bash
    cargo check -p <crate>
    cargo check -p <crate> --example <name>   # if applicable
    ```
8. **Change only what the task needs** — no drive-by refactors across unrelated widgets.

## Decision guide

| Need                                  | Pattern                                           |
| ------------------------------------- | ------------------------------------------------- |
| List/chat that fills remaining height | Flex grow + `ScrollView`                          |
| Pin to latest message                 | `auto_scroll: true` + bottom-aligned inner column |
| Sticky user prompt                    | Inset `ScrollView` below header + clipped overlay |
| Label on border corner                | Absolute child + `background_color: Reset`        |
| Periodic UI refresh                   | `use_future` + `use_state` for clock/tick         |
| Quit app                              | `should_exit` state + `system.exit()`             |
| Modal / overlay (future)              | Absolute full-size `View` above siblings          |
| Keyboard-only scroll                  | `Shift+Arrow` or `PageUp`/`PageDown` on panel     |
| Accessible mode/status                | Text label + color (not color alone)              |

## Anti-patterns

- Monolithic shell with all markup and handlers in one component
- Hard-coded terminal dimensions instead of `use_terminal_size`
- Scrollable root wrapping fixed header (fixed zones should be **siblings outside** the scroll view)
- Coloring every border by transient state when only a badge needs emphasis
- Ignoring `KeyEventKind::Release` and firing actions twice
- Copy-pasting layout trees instead of shared child components
- **Mouse-only** scroll or click targets with no keyboard equivalent
- **Color-only** state (red border with no error text; mode hue with no label)
- **Full-height sticky overlay** on a full-height `ScrollView` — hides and blocks content below
- **Low-contrast** grey-on-grey for primary transcript or input text
- **Hidden shortcuts** — power features with no footer/help mention
- **Focus loss on re-render** — controlled `TextInput` round-trips that reset cursor; prefer a single buffer + direct render for multiline editors
