# Usage Accounting & Token Displays

How token counts and cost figures in the TUI are computed, what each number means,
and why the numbers in different surfaces are intentionally different.

## Three surfaces, three different questions

### 1. Turn-complete stats card (transcript)

Rendered under the last assistant reply after each real agent/chat turn
(`ui.turnStats`, default on). Source: `session_turns` row for that turn.

```
turn: 1m50s · 3K in · 2K out · 1K cached · $0.0123 · anthropic/claude-sonnet-4
```

| Field                       | Meaning                                                                                                                                                                                                                                               |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `1m50s`                     | Wall-clock of the turn                                                                                                                                                                                                                                |
| `3K in`                     | **Sum of `input` across every API call in the turn** — tool-call iterations (`StopReason::ToolUse`) and the final reply are separate API calls, each with its own usage. Tool-call tokens are part of provider `usage`, so they are already included. |
| `2K out`                    | Sum of `output` across those calls                                                                                                                                                                                                                    |
| `1K cached`                 | Sum of `cache_read` (falls back to `cache_write` if no read)                                                                                                                                                                                          |
| `$0.0123`                   | Sum of `cost.total` across the turn's calls                                                                                                                                                                                                           |
| `anthropic/claude-sonnet-4` | `provider_id/model_id` recorded on the turn                                                                                                                                                                                                           |

Because each API call re-sends the whole context as input, the summed `in` can
legitimately exceed the model's context window for tool-heavy turns. It measures
**provider-reported volume sent**, not a unique context size.

Only real agent/chat turns get a card. System operations that spin the UI without
an AI response (`/compact` answering "History is already up to date", …) do not
produce a `session_turns` row and are suppressed.

### 2. Header chrome (`ChromeStats`)

Source: session **tree entries** (`session_entries`), aggregated in
`crates/coding-agent/src/tui/chrome/stats.rs`:

- `$cost` — `aggregate_usage_from_entries` (`crates/coding-agent/src/platform/exit_message.rs`):
  sums **real provider usage** (`usage.cost.total` per assistant message) over the whole
  active session.
- `N tokens` — `estimate_context_tokens(context.messages)` →
  `estimate_tokens_with_system_prompt`: estimates tokens from the current branch
  (chars/4 + system prompt), reusing the last provider `total_tokens` when available.
- `NN%` — `tokens_used / context_window`, i.e. **how full the next request's context would be**.

These are **session-cumulative** (all turns) and the token/percent figure is an
**estimate of unique context**, not a sum of bytes sent to the provider.

### 3. Goal budgets / rollups (`sessions` columns)

`finish_turn` (status `completed`) rolls the turn's `TurnUsage` delta into
`sessions.total_*` + `turn_count` inside the same SQLite transaction. Used by goal
accounting and session-level reporting.

## Why the surfaces differ (intentionally)

| Question                                         | Surface               | Number type                                   |
| ------------------------------------------------ | --------------------- | --------------------------------------------- |
| "What did this one turn cost / send?"            | Stats card            | **Actual** provider usage, summed per turn    |
| "What has the whole session spent?"              | Header `$`            | **Actual** provider usage, summed per session |
| "How much context is used for the next request?" | Header `tokens` + `%` | **Estimate** of unique context                |

So the header `tokens`/`%` will not equal the sum of per-turn cards: the former is
a unique-context estimate biased to the last call's `total_tokens`, the latter sums
input volume across all calls in the turn. This is by design — each answers a
different question.

## Data flow

```
provider stream
  └─ AssistantMessage.usage (per API call)        ← includes tool-call tokens
       ├─ run_agent_loop
       │    └─ accumulate TurnUsage across ALL assistant messages in the turn
       │         └─ turn_execution.rs → session_turns.finish_turn (transactional
       │              + sessions.total_* rollup when status = completed)
       │              └─ emit_run_completed reads latest_turn → RunCompleted{usage}
       │                   └─ stats card (turnStats)
       └─ session_entries (per MessageEnd)
            └─ aggregate_usage_from_entries → header $ (session)
            └─ estimate_context_tokens        → header tokens / % (next-request context)
```

Notes:

- The relational `session_turns` table is **best-effort**: failures degrade to a
  zeroed row, never to data corruption across turns. Rollups only run on
  `completed`, so cancelled/failed turns never double-count the session totals.
- Only the _last_ assistant message's usage used to be written per turn; this
  under-reported tool-heavy turns. The turn now accumulates **all** assistant
  messages (tool-call iterations included) via `impl AddAssign for TurnUsage`
  (`crates/elph-agent/src/turns/types.rs`).
- `context_pct` and `tokens_used` deliberately reuse provider `total_tokens` (which
  already includes the system prompt) to avoid double-counting — see
  [`system-prompt-efficiency.md`](./system-prompt-efficiency.md) and
  `crates/elph-agent/src/compaction/estimation.rs`.
