# Plan: `/aside` side question (Elph ↔ Grok Build `/btw`)

## Goal

Ship **`/aside <question>`** in Elph — the Grok Build **`/btw`** product feature, renamed for Elph:

- Ask a **side question** while the main agent turn keeps running
- **Bypass** prompt queue / `turn_gate` / steer
- One-shot model answer over a **snapshot** of session context
- **Do not** append Q/A into the main session transcript tree (no pollution of the next main turn)
- Show answer in a **dismissible UI panel**; Esc dismisses (optional: collapse into a sticky meta card)

Reference (Grok Build):

- Slash: [`btw.rs`](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/slash/commands/btw.rs) → `Action::SendBtw`
- UI: `views/btw_overlay.rs` (Loading / Done / Error), `scrollback/blocks/btw.rs` (collapsed after Esc)
- Dispatch: `dispatch/notes.rs` `dispatch_send_btw` → `Effect::SendBtw` → ACP `x.ai/btw`
- Shell: `extensions/feedback.rs` `handle_btw` → `SessionCommand::SideQuestion`
- Core: `acp_session_impl/recap.rs` `handle_side_question` (+ `side_call.rs` cache skeleton)

**Not** the Grok skill prompt wrapper (`.agents` / skill-only). This is a **real parallel completion**, not a skill script.

---

## Product contract (match Grok semantics)

| Behavior        | Spec                                                                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Command         | `/aside <question>` only (no `/btw` alias unless you want one later)                                                                     |
| Args required   | Empty args → usage status: `Usage: /aside <question>`                                                                                    |
| Busy agent      | **Works mid-turn** — never waits on `turn_gate`, never `queue_steer`                                                                     |
| Tools           | **No tool execution** — text-only answer                                                                                                 |
| Context         | Full harness snapshot: system prompt + messages (+ optional streaming assistant truncated); **strip unpaired tool_calls** if mid-turn    |
| Session storage | **Do not** `append_message` / persist branch entries for aside Q or A                                                                    |
| Main turn       | Uninterrupted                                                                                                                            |
| UI              | Panel above prompt: spinner → answer (markdown) / error; **Esc** dismisses                                                               |
| After dismiss   | Optional sticky transcript meta card `/aside …` (collapsed), not a user/assistant chat pair                                              |
| Concurrent      | One active aside panel; a second `/aside` replaces/cancels prior in-flight request (request-id correlation like Grok minimal_request_id) |

### Side-question system reminder / `intercom` (from Grok, adapted)

Append a user message (or system reminder / `intercom`) that states:

- Separate lightweight instance; main agent is **not** interrupted
- Answer in **one** response; no tools; no “let me check…”
- Answer only from conversation context already known
- Then the user’s question

---

## Elph architecture (integrated, no ACP)

Elph has no pager/shell split. Map Grok’s pipeline onto coding-agent:

```text
/aside <q>
  → SlashDispatch::Aside { question }
  → slash_handler: clear prompt (optional), open AsidePanel Loading
  → tokio::spawn CodingAgentSession::run_aside(question, request_id)
       snapshot harness.state()  // system_prompt, model, messages
       build elph_ai::Context (no tools)
       models.complete(...)     // or stream → deltas to panel
  → AgentUiEvent::AsideProgress { id, phase } / AsideDone { id, answer } / AsideError
  → shell: update panel; Esc → dismiss (+ optional TranscriptNotice card)
```

Key APIs already present:

- `AgentHarness::state()` → `AgentState { system_prompt, model, messages, … }`
- `ModelSelection` / session holds `Arc<elph_ai::Models>`
- `Models::complete` / `stream` with empty tools
- Message convert path used by agent run (`convert_to_llm` / equivalent) — reuse, **clone** messages only

**Do not** go through `CodingAgentSession` turn entrypoints that take `turn_gate` (`prompt`, compact, etc.).

---

## Implementation modules

### 1. Slash surface

| File                         | Change                                                                                                                                                      |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent/slash_commands.rs`    | `builtin_with_args("aside", "Ask a side question without interrupting")`; `SlashDispatch::Aside { question: String }`; `builtin_dispatch`                   |
| `agent/slash_misc.rs` / help | Document `/aside` in help text                                                                                                                              |
| `tui/slash_handler.rs`       | Handle `Aside`: validate non-empty; spawn `run_aside`; return outcome that does **not** echo as user card (`BackgroundTaskQuiet` or dedicated `StartAside`) |
| Docs                         | `docs/archive/slash-commands.md` table row; short note in planned/help if needed                                                                            |

### 2. Core: `run_aside`

New file e.g. `agent/aside.rs` (or `session/aside.rs`):

```rust
pub async fn run_aside(session: &CodingAgentSession, question: &str, request_id: Uuid) -> Result<String, String>
```

Steps:

1. `let snap = harness.state().await`
2. Clone `messages`; if last assistant has pending tool calls without results, **pop** trailing incomplete tool run (Grok `pop_trailing_tool_run`)
3. Build LLM messages from snap + side instruction + question
4. Resolve `Models` + `Model` from session selection
5. `complete` / short stream **with tools: none** (Elph can omit tools entirely — simpler than Grok’s “same tools for cache, prompt forbids”)
6. Extract assistant text; empty → error
7. Emit UI events with `request_id` (drop late responses if panel already dismissed / superseded)

Optional later: persist `aside_history.jsonl` under session dir (Grok `btw_history.jsonl`) — **v1 skip** unless free.

### 3. UI: Aside panel

Grok parity target (iocraft, not ratatui):

| Piece         | Location                                                                                                     |
| ------------- | ------------------------------------------------------------------------------------------------------------ |
| State enum    | `tui/aside_panel.rs`: `Loading { question }`, `Done { question, text, scroll }`, `Error { question, error }` |
| Shell state   | `pending_aside: Option<AsidePanelState>` + `aside_request_id` + focus flag                                   |
| Layout        | Reserve height above prompt (like status/queue), `DONE_MAX_BODY_LINES` ~12, Esc hint in chrome               |
| Events        | Esc dismisses; ↑↓ scroll when Done + focused; click Esc hit optional                                         |
| Render answer | Prefer plain wrapped text first; markdown via existing transcript markdown path or `rendown` if cheap        |

**v1 acceptable simplification** (if full chrome is large):

1. Status line “Answering aside…” + `ScrollTextDialog` when done
2. Esc closes dialog

Still must: mid-turn, no session pollution, no tools. Prefer full panel if effort allows — matches product expectation.

### 4. AgentUiEvent

Add:

```rust
AsideStarted { request_id, question },
AsideDelta { request_id, text },      // optional streaming
AsideFinished { request_id, answer },
AsideFailed { request_id, error },
```

Shell ignores events whose `request_id` ≠ current open panel.

### 5. Focus / input ownership

Mirror Grok rules lightly:

- While Loading: prompt stays usable (main agent continues)
- While Done: optional focus on panel for scroll; Esc dismisses
- Do not open `/resume` picker etc. over aside without dismiss (optional hard gate)

### 6. Tests

| Layer      | Cases                                                                               |
| ---------- | ----------------------------------------------------------------------------------- |
| Unit       | `dispatch_slash_command("/aside", …)` → Aside; empty → usage                        |
| Unit       | `run_aside` with faux/mock Models returns answer; **messages** after call unchanged |
| Unit       | Mid-turn snapshot strips unpaired tool call                                         |
| UI / event | Superseded request_id dropped; Esc clears panel                                     |

---

## Out of scope (v1)

- `/btw` alias
- ACP `x.ai/btw` wire protocol (Elph in-process only; ACP can come later if needed)
- Hosted web search during aside (Grok allows hosted tools; Elph: **no tools**)
- Prompt-cache prefix sharing gymnastics (nice-to-have if same model + stream options free)
- Skill-based “ASIDE:” formatting (prompt skill is unrelated)

---

## Implementation order

1. Slash register + dispatch + usage
2. `run_aside` complete path + unit tests (no UI)
3. `AgentUiEvent` + shell panel / dialog wiring
4. Esc dismiss + optional transcript meta card
5. Docs + help + smoke mid-turn

---

## Acceptance criteria

- [ ] `/aside what is X?` works while agent is streaming / tool-calling
- [ ] Main turn continues; steer/queue unchanged by aside
- [ ] Aside Q/A **not** in next main-turn model messages
- [ ] No tool execution from aside
- [ ] Empty `/aside` shows usage
- [ ] Answer visible; Esc dismisses without canceling main agent
- [ ] Second `/aside` supersedes first in-flight (no double panel race)
- [ ] `cargo test -p elph` (relevant) / `cargo check -p elph` pass

---

## Risks

| Risk                           | Mitigation                                                         |
| ------------------------------ | ------------------------------------------------------------------ |
| Concurrent API load mid-turn   | Short timeout / abort on dismiss; document rate limits             |
| Huge context clone             | Snapshot as-is (same as main); optional max messages later         |
| Streaming assistant incomplete | Pop unpaired tools; include partial assistant text if useful       |
| UI complexity                  | Ship dialog path first only if panel blocks; default plan is panel |
