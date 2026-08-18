# Improve Plan Mode: Grok-style review on Elph’s existing flow

## Goal

Adopt two Grok Build ideas, both fitted onto Elph’s existing architecture:

1. **Review surface** — scrollable preview, line comments, request-changes, copy, approve-with-comments (TUI).
2. **A slim plan tracker** — explicit Inactive / Pending / Active lifecycle next to `CollaborationMode`, not a replacement for it.

Do **not** port Grok’s full `PlanModeTracker` (ExitPending, mid-turn activation, reminder full/sparse, `plan_mode.json`, session-dir `plan.md`, `exit_plan_mode` tool).

The user-facing outcome: toggling Plan is a real phase; when the agent proposes a plan, the TUI opens a review surface instead of an 8-line truncated picker.

## Current Elph vs Grok (what we keep / skip)

```
Elph today                              Grok Build
──────────────────────────────────      ──────────────────────────────────
CollaborationMode::Plan / Default       PlanModeTracker: Inactive/Pending/
  (boolean, immediate)                    Active/ExitPending + mid-turn
<proposed_plan> in assistant text       Agent writes session plan.md
save_plan_to_disk → .elph/plans/        plan.md is the only writable file
Inline dialog, first 8 lines            Fullscreen line-viewer preview
Select list: implement / fresh /        Action bar: a/s/c/y/q
  stay / revise
Revise = dismiss + empty prompt         s → prompt; comments + freeform
No copy, no line comments               y copy; c comment; a w/ comments
No /view-plan                           /view-plan reopens preview
```

**Keep (Elph architecture):**

- `CollaborationMode` remains the **tool-filter + session-tree** source of truth
- `<proposed_plan>` extraction and `PlanConfirmationRequired`
- Save to `<project>/.elph/plans/plan-*.md` with YAML frontmatter
- `PlanConfirmationChoice::{Implement, ImplementFresh, StayInPlan}`
- Implement-this-session vs implement-fresh
- Mode cycle / `elph run --mode=plan` / `request_mode_change`
- Existing busy-block on Shift+Tab (`allow_mode_change_while_busy`) — this already covers Grok’s ExitPending

**Adopt (slim tracker only):**

| Grok piece                              | Elph adaptation                                                                           |
| --------------------------------------- | ----------------------------------------------------------------------------------------- |
| `Inactive` / `Pending` / `Active`       | Same three states, process-local on the harness                                           |
| First prompt activates Pending → Active | Shift+Tab to Plan = Pending (badge only); first user turn flips `CollaborationMode::Plan` |
| `was_previously_active` / reentry       | Short reentry appendix if they return to Plan in the same session                         |
| `awaiting_plan_approval`                | **Do not duplicate** — `pending_plan.is_some()` is the flag                               |

**Skip (complexity we do not want):**

- `ExitPending` — TUI already refuses mode change while a turn is in flight
- Mid-turn `activate_mid_turn` / `PendingActivation` withdraw-on-toggle
- Full/sparse `reminder_count` and MiniJinja reminder templates
- `plan_mode.json` snapshot — resume from existing `collaboration_mode_change` tree events
- `enter_plan_mode` / `exit_plan_mode` tools as the proposal mechanism
- Session-dir `plan.md` as the only writable path
- Ratatui `LineViewer` / casual post-approval commenting
- Image chips in review notes

## Target UX

When `PlanConfirmationRequired` fires, replace the compact select-list with an **inline plan review** (same chrome family as tool-approval / ask-user — `InlineDialogShell` above the prompt, not a new fullscreen app).

```
╭─ Review plan · .elph/plans/plan-20260818_1430.md ──────────────╮
│  12  ## Step 1                                                 │
│  13▸ Use a new auth middleware                            [c]  │
│      ↳ prefer the existing helper in crate X                   │
│  14  Add tests                                                 │
│                                                                │
│  [a] implement   [f] fresh   [s] revise                        │
│  [c] comment     [y] copy    [q] quit plan                     │
╰─ ↑↓/jk line · Tab prompt · Enter comment · Esc preview ────────╯
```

### Focus states (from Grok, mapped to Elph)

| Focus                 | What keys do                                                                             |
| --------------------- | ---------------------------------------------------------------------------------------- |
| **Preview** (default) | Scroll/select source lines; `a/f/s/c/y/q`                                                |
| **Commenting**        | Inline field (`DialogUserInputContent`) for the selected line; Enter saves, Esc discards |
| **Prompt**            | Main prompt is live for freeform revision notes; Tab returns to preview                  |

### Shortcuts (Preview)

| Key               | Action                      | Notes                                                                                 |
| ----------------- | --------------------------- | ------------------------------------------------------------------------------------- |
| `a` / `1`         | Implement this session      | If comments/notes exist → **approve with comments**                                   |
| `f` / `2`         | Implement in new session    | Same comment attach as `a`                                                            |
| `s`               | Request changes             | Empty notes+comments → focus Prompt + toast. Otherwise send revision and stay in Plan |
| `c` / Enter       | Comment on selected line    | On an existing comment line → edit it                                                 |
| `y`               | Copy full plan              | `elph_tui::copy_to_clipboard` + ephemeral banner                                      |
| `q`               | Quit plan                   | Dismiss review, **exit Plan → Build**, do **not** implement                           |
| Esc               | Commenting/Prompt → Preview | Preview Esc = stay in Plan, close review (today’s Esc)                                |
| Tab               | Preview ↔ Prompt            | Matches Grok; documented in footer                                                    |
| `↑` `↓`           | Move selected source line   | Viewport follows selection                                                            |
| `d` / Backspace   | Delete comment under cursor | Preview only                                                                          |

**Key remaps vs today:** `s` is no longer “Stay in Plan” (Stay becomes Esc / just leaving the review open). `r`/`4` drop as revise aliases in favor of `s`.

### Comment payload sent to the agent

Reuse Grok’s quoting format, adapted to Elph’s inline plan (not `@plan.md:`):

```
Plan revision requested.

Proposed plan line 13:
> Use a new auth middleware

Comment:
prefer the existing helper in crate X

Additional feedback:
add a verification section
```

Approve-with-comments prefixes with `The user approved the plan with the following review comments:` and then runs the existing implement path.

### `/view-plan`

Add `/view-plan` (aliases `show-plan`, `plan-view`):

- If a review is already pending → refocus Preview
- Else open the latest saved plan for this session (or `active_plan_file`) in the existing `scroll_text_dialog` (read-only, `[copy]` header already exists)
- Empty state: “No plan written yet.”

## Implementation (Elph-native)

### 0. Slim `PlanModeTracker` (harness, not a second mode system)

Add `crates/elph-agent/src/collaboration/tracker.rs`. Pure state machine, unit-tested in-file — same isolation idea as Grok, ~150 lines instead of ~1200.

```rust
enum PlanModeState { Inactive, Pending, Active }

struct PlanModeTracker {
    state: PlanModeState,
    was_previously_active: bool,
}
```

Methods only:

| Method                                          | Effect                                          |
| ----------------------------------------------- | ----------------------------------------------- |
| `enter_pending()`                               | `Inactive → Pending`. From `Active` is a no-op. |
| `activate()`                                    | `Pending → Active`, set `was_previously_active` |
| `deactivate()`                                  | `Active`/`Pending → Inactive`                   |
| `is_active()` / `is_pending()` / `is_reentry()` | Queries                                         |

**How it sits next to `CollaborationMode` (stable architecture):**

- `CollaborationMode` still owns tool filtering and the session-tree event.
- The tracker owns _when_ that flip happens.
- `pending_plan` still owns approval; the tracker does not grow an `awaiting_approval` field.

Wiring:

1. `AgentHarness` holds `Mutex<PlanModeTracker>` beside `collaboration_mode`.
2. TUI Shift+Tab → Plan: `enter_pending()`. TUI badge shows Plan. **Do not** call `set_collaboration_mode(Plan)` yet. Tools stay Build until the first prompt.
3. First user prompt while `Pending`: `activate()` then existing `enter_plan_mode()` / `reconcile_harness_tools(Plan)`.
4. `--mode=plan` / ACP `session/set_mode=plan` / `request_mode_change` approved: skip Pending, `activate()` + `CollaborationMode::Plan` immediately (user already committed).
5. Implement / ImplementFresh / Quit plan: `deactivate()` + existing `exit_plan_mode()`.
6. Revise / Stay: tracker stays `Active`.
7. Shift+Tab away from Plan while `Pending`: `deactivate()` only (model never saw Plan).
8. Shift+Tab away while `Active` and idle: `deactivate()` + `exit_plan_mode()`. While busy: keep today’s toast block — no `ExitPending`.

**Reentry prompt (one extra string, not a template engine):**

If `is_reentry()` at activate time, append a short appendix after `plan_mode_system_prompt()`:

> Returning to Plan mode. A previous plan may exist under `.elph/plans/`. End the turn with `<proposed_plan>` or `ask_user_question`.

No MiniJinja, no tool-name placeholders, no full/sparse alternation.

**Resume:** no new file. `collaboration_mode_change` on the tree is still authoritative.

- Restored `Plan` → tracker `{ Active, was_previously_active: true }`
- Restored `Default` → `Inactive`
- `Pending` is never persisted (same collapse Grok does for transient states)

`enter_plan_mode()` / `exit_plan_mode()` stay as thin wrappers; they also drive the tracker so CLI/ACP/TUI share one path.

**TUI hook:** today’s Shift+Tab calls `session.set_agent_mode(Plan)` which immediately `reconcile_harness_tools` → `enter_plan_mode()`. Split that: `set_agent_mode` / `try_set_mode_sync` set the TUI/session badge; only `commit_plan_mode()` (first prompt, or `--mode=plan` / ACP) calls `enter_plan_mode()`. Ask/Brave/Build still flip collaboration mode immediately as today.

### 1. New review module

Create `crates/coding-agent/src/tui/plan_review.rs` and move plan-dialog types out of the kitchen-sink `tool_approval.rs`.

```rust
enum PlanReviewFocus { Preview, Commenting, Prompt }

struct PlanComment {
    id: u64,
    line_range: Range<usize>, // 1-based, end exclusive — same as Grok
    text: String,
}

struct PendingPlanConfirmation {
    plan_text: String,
    plan_file: Option<String>,
    session: Option<Arc<CodingAgentSession>>,
    focus: PlanReviewFocus,
    selected_line: usize,       // 1-based source line
    comments: Vec<PlanComment>,
    next_comment_id: u64,
    commenting_range: Option<Range<usize>>,
    comment_draft: String,
    editing_comment_id: Option<u64>,
}
```

Pure helpers (unit-tested, no iocraft):

- `format_plan_feedback(comments, plan_text, freeform)` — quote snippets; handle out-of-range
- `plan_review_footer_hint(focus, comment_count)`
- `pick_plan_review_action(modifiers, code, focus) -> Option<PlanReviewAction>`
- `visible_line_window(selected, total, viewport)` — keep selection on screen
- `strip_plan_tags` (move from `tool_approval.rs`)

`PlanChoice` becomes:

```rust
enum PlanChoice { Implement, ImplementFresh, StayInPlan, RevisePlan, QuitPlan }
```

`RevisePlan` still does not map to `PlanConfirmationChoice` — TUI clears pending and **submits a user turn** with the formatted feedback (today it only clears pending and dumps the user on an empty prompt).

`QuitPlan` is TUI-side: `clear_pending_plan()` + `exit_plan_mode()` / `AgentMode::Build`. No new harness enum required.

### 2. Render: grow the existing inline dialog

`status_dialog.rs` `render_plan_confirmation_dialog`:

- Drop the 8-line `take(8)` preview + 4-row `SelectList`
- Numbered **source lines** of `plan_text` (not reflowed markdown — line comments must match the file)
- Selected line: `▸` + accent; commented lines show a `[c]` marker and a dim `↳` row under the range
- Growing `ScrollView` / clipped viewport using the same height budget pattern as tool-approval (`tool_approval_max_body_rows`) so 80×24 still leaves prompt + footer
- Compact action hint row (text + shortcuts, not a second select list)
- When `focus == Commenting`: `DialogUserInputContent` at the bottom of the dialog (same widget as ask-user custom answers)

`StatusDialogKind::PlanConfirmation` must carry enough to paint: `plan_text`, `plan_file`, `focus`, `selected_line`, `comments`, `comment_draft`. Pass these from `PendingPlanConfirmation` in `build_plan_confirmation_dialog_kind`.

Do **not** render the plan as markdown blocks for this overlay. Source-line preview is required for `c`. `/view-plan` on a saved file can stay in `scroll_text_dialog` (plain/copy).

### 3. Keys + submit wiring

`shell/keys.rs` plan-confirmation arm (~1792–1922) grows into a small state machine:

1. If `focus == Commenting` → only comment field keys (Enter save, Esc cancel, Tab → Preview and discard draft)
2. If `focus == Prompt` → let the main prompt handle typing; intercept Enter to `send_revision` (toast if empty and no comments); Esc/Tab → Preview
3. If `focus == Preview` → table above; `y` copies and **does not** dismiss; `a`/`f` run existing resolve + frontmatter `in_progress`; comments appended via implement prompt

Revision send (replace today’s “clear and walk away”):

1. `session.clear_pending_plan()`
2. Build feedback string
3. Submit it as a normal user turn (same path as prompt Enter — reuse the existing session prompt spawn used after implement)
4. Stay `AgentMode::Plan`; transcript row: “Revision requested”
5. Close the review

Approve-with-comments: extend `implement_prompt` in `elph-agent/src/prompt/builtin/plan.rs`:

```rust
pub fn implement_prompt(
    plan_text: &str,
    plan_file: Option<&str>,
    review_notes: Option<&str>,
) -> String
```

When `review_notes` is `Some`, append the formatted comments after the existing “implement this plan / read the plan file” body. Thread through `resolve_plan_with_file` / harness `resolve_plan_confirmation`. This is a small signature change (no compat shim — project policy).

Copy: `copy_to_clipboard(&pending.plan_text)` + existing `clipboard_notice_banner` / ephemeral banner. Copy the body only (no frontmatter, no line numbers).

### 4. `/view-plan` + slash registry

- `builtin_slash_commands()`: `view-plan` with aliases in `builtin_dispatch`
- `SlashDispatch::ViewPlan`
- `slash_handler`: if `pending_plan_confirmation` is set, no-op/refocus; else read latest `plan-*.md` for this session from `paths.plans_dir()` (helper on `plan_files.rs`: `latest_plan_path(paths, session_id)`) and `open_scroll_text_dialog`

Palette label: `/view-plan` (no args). Args-phase not needed.

### 5. ACP (parity, no line-comment UI)

`platform/acp/plan.rs` (+ v1 twin): add options `revise` and `quit` next to implement/fresh/stay. ACP clients have no line viewer — `revise` maps to `clear_pending_plan` (same as today’s TUI Revise). `quit` exits plan mode. Description remains the plan text.

### 6. Tests (narrow, in-file)

- Tracker: Inactive → Pending → Active; Pending cancel is clean; reentry after deactivate; activate-from-idle skip of Pending; double `enter_pending` is no-op
- `format_plan_feedback` quotes single/range lines; out-of-range → `[selected lines unavailable]`; freeform-only; comments+freeform “Additional feedback”
- Preview key table: `a/f/s/c/y/q`, remapped `s` ≠ Stay, Esc stay, `y` is not an approve
- Add/edit/delete comment; empty comment draft does not insert
- `implement_prompt` includes review notes when present, omitted when `None`
- `/view-plan` dispatch + `latest_plan_path` empty/missing
- Existing plan-mode harness tests still pass (`crates/elph-agent/tests/plan_mode.rs`); add activate-on-first-prompt coverage there

### 7. Docs (required — behavior change)

Update to match **implemented** behavior:

- `assets/user-guide/11-plan-mode.md` — Pending vs Active (badge before first prompt), review shortcuts, `/view-plan`, comment/revise/copy/quit
- `website/content/docs/start/plan-mode.md` — same, keep CLI `--mode=plan` (skips Pending)
- `crates/elph-agent/docs/agent-harness.md` — tracker + `implement_prompt` review-notes argument if those docs mention plan APIs

## Layout / a11y constraints (from `tui-design`)

- Keyboard-complete: every action has a key; mouse is optional
- Footer lists the chords; never color-only state (`[c]` marker + “comment” text)
- Ignore `KeyEventKind::Release`
- Sticky/preview must inset the scroller (`min_height: 0`, overflow hidden) so header/action bar do not scroll away
- 80×24: preview shrinks first; action bar + prompt stay visible
- `y` copy must not collide with tool-approval `y` (only one status dialog is open)

## Out of scope

- Grok `ExitPending`, mid-turn activation buffer, full/sparse reminders, `plan_mode.json`
- Letting the agent write `.elph/plans/*` itself (still host-saved on propose)
- Visual range-select (`v` / Shift+↑↓) — v1 comments a single selected line (range type is already `Range` so a later pass can widen)
- Casual comments after the review is closed
- Markdown-rendered preview inside the review overlay

## Verification

- `make check-elph` / `make lint-elph` (and `make test-elph` for `plan_review` + `plan_files` + `plan.rs` units)
- Mentally walk 80×24 and a 200-line plan: selection stays in viewport, action bar remains
- No TUI browser; this is terminal-only
