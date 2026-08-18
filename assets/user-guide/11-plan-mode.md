# Plan Mode

Elph supports collaboration modes (Plan vs Build). In Plan mode the agent focuses on
design and asks for confirmation before implementing; Build mode enables the full tool
surface.

## Entering Plan

- **Shift+Tab** cycles Build → Plan → Ask → Brave. The first Shift+Tab to Plan **arms**
  Plan (badge only). The next user prompt activates read-only tools and Plan guidance.
- `elph run --mode=plan "…"` and ACP `session/set_mode=plan` activate immediately.
- Returning to Plan in the same session adds a short reentry reminder.

## Reviewing a proposed plan

When the agent wraps a plan in `<proposed_plan>…</proposed_plan>`, Elph saves it under
`<project>/.elph/plans/plan-*.md`. The full plan stays in the transcript. The
confirmation card shows only the subject and saved path.

| Shortcut | Action |
| -------- | ------ |
| `↑` `↓` | Move the highlighted action |
| Enter | Confirm the highlighted action |
| `a` | Implement in this session |
| `f` | Implement in a new session |
| `s` | Request changes (focus the prompt; Enter sends notes) |
| `q` | Leave Plan mode without implementing |
| `y` | Copy the plan to the clipboard |
| Tab | Confirmation ↔ revision prompt |
| Esc | Stay in Plan and close the card |

`/view-plan` (aliases `/show-plan`, `/plan-view`) reopens the latest saved plan.

Implementing a plan exits Plan mode and restores the Build tool surface.
