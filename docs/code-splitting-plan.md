# Code Splitting Plan — `elph/src`

> **Scope:** `elph/src` (binary crate).  
> **Triggered by:** inspection of files ≥400 lines after the `tui/shell` split.  
> **Constraint:** keep public API stable via `mod.rs` re-exports; each increment verified with `make check` / `make lint` / `make test` + `cargo clippy --workspace --all-targets --features full -- -D warnings`.

---

## Current State (post-shell split)

| Module                 | Largest file                 | Lines |
| ---------------------- | ---------------------------- | ----- |
| `tui/shell/`           | `keys.rs`                    | 3,174 |
| `tui/`                 | `tool_params.rs`             | 1,457 |
| `tui/`                 | `agent_bridge.rs`            | 1,380 |
| `tui/transcript/`      | `types.rs`                   | 1,370 |
| `tui/`                 | `provider_connect_dialog.rs` | 1,264 |
| `tui/`                 | `user_question.rs`           | 1,263 |
| `tui/`                 | `status_dialog.rs`           | 1,033 |
| `tui/`                 | `model_selector.rs`          | 999   |
| `tui/transcript/card/` | `kinds.rs`                   | 966   |
| `platform/`            | `settings.rs`                | 949   |
| `tui/`                 | `startup.rs`                 | 914   |
| `tui/shell/`           | `tick.rs`                    | 911   |
| `agent/session/`       | `mod.rs`                     | 728   |

---

## Methodology

- Split by **seam**, not by arbitrary line count.
- Prefer directory → `mod.rs` + sibling files when the file has ≥2 cohesive domains.
- Prefer single file → `foo/core.rs` + `foo/tests.rs` when the file is one big impl/component.
- Never expose private internals publicly; use `pub(crate)` + `mod.rs` re-export only when needed by siblings.
- Commit each file extraction as one atomic change.

---

## Phase 1 — High Priority, Easy Seams

**Goal:** extract files with clean, independent domains. Low risk, high readability gain.

### 1.1 `tui/tool_params.rs` → `tui/tool_params/{mod,parse,format,path,view}.rs`

- **Why:** 4 independent domains (parsing, formatting, path abbreviation, view component). Tests (261 lines) can move to `view_tests.rs` or stay in `mod.rs`.
- **Proposed layout:**
    - `mod.rs` — re-exports + `ToolParamsView` component.
    - `parse.rs` — `ToolParam`, `parse_tool_params`, `params_from_json`.
    - `format.rs` — display/truncate/collapse helpers + approval summary helpers.
    - `path.rs` — `abbreviate_path`, `PathPrefix`, `split_display_path`.
    - `view.rs` — `ToolParamsViewProps`, `ToolParamsView` component.
- **Effort:** Small. Each module is pure functions + a few types.
- **Risk:** Low.

### 1.2 `platform/settings.rs` → `platform/settings/{mod,model,defaults,migration}.rs`

- **Why:** 8 settings structs + serde defaults + migration helpers + tests (322 lines).
- **Proposed layout:**
    - `mod.rs` — re-exports + `Settings::read`/`save`.
    - `model.rs` — `Settings`, `UiSettings`, `FilePickerSettings`, `SessionSettings`, `ModelsSettings`, `ProviderHttpSettings`, `MemorySettings`, `NotificationSettings`, `CompactionConfig`.
    - `defaults.rs` — all `default_*` helper functions.
    - `migration.rs` — `migrate_settings_value`, `lift_into_object`, `deep_merge`, `read_settings_value`, `parse_duration_ms`.
- **Effort:** Small–Medium. Mostly moving blocks; serde derives stay with structs.
- **Risk:** Low. Public structs re-exported from `mod.rs`.

### 1.3 `agent/session/mod.rs` → `agent/session/{mod,run,mode,tools,title}.rs`

- **Why:** `CodingAgentSession` impl is ~618 lines with 25+ methods, already grouped logically. `wiring.rs` exists as precedent.
- **Proposed layout:**
    - `mod.rs` — struct definition + constructor + tests.
    - `run.rs` — `submit_prompt`, `run_prompt_turn`, `queue_follow_up`, `queue_steer`, `promote_next_follow_up_to_steer`, `remove_queued`, `clear_prompt_queues`, `abort`, `compact`, `reload_resources`, `invoke_skill`, `prompt_from_template`, `navigate_tree_to`, `branch_entries`, `save_transcript_snapshot`.
    - `mode.rs` — `set_agent_mode`, `try_set_mode_sync`, `reconcile_tool_surface`, `attach_mcp_registry`, `apply_agent_mode`, `compiled_system_prompt`, `set_thinking_level`, cache helpers.
    - `tools.rs` — `mcp_registry`, `attach_mcp_registry` (if moved), tool reconciliation.
    - `title.rs` — `maybe_generate_session_title`, `generate_and_store_session_title`, `resolve_title_model`.
- **Effort:** Medium. Methods reference `self` extensively; each file needs `pub(crate)` on impl methods + `mod.rs` re-export if used cross-file.
- **Risk:** Low–Medium. Existing `wiring.rs` proves the pattern works.

### 1.4 `tui/provider_connect_dialog.rs` → `tui/provider_connect_dialog/{mod,model,filter,steps,disconnect}.rs`

- **Why:** 4 step renderers (`select_auth`, `select_provider`, `oauth_device_code`, `api_key`) + disconnect dialog + tests. Each step is a self-contained function.
- **Proposed layout:**
    - `mod.rs` — re-exports + `open/close` API.
    - `model.rs` — `ProviderOption`, `ProviderConfigStatus`, `PendingProviderConnectDialog`, `PendingProviderApiKeyDialog`, auth-method types.
    - `filter.rs` — `filtered_providers`, `provider_match_score`, `focus_provider_search`, `focus_provider_list`.
    - `steps.rs` — `render_select_auth_method_step`, `render_select_provider_step`, `render_oauth_device_code_step`, `render_api_key_step`.
    - `disconnect.rs` — `PendingProviderDisconnectDialog`, `open/close/render` disconnect.
- **Effort:** Small. Pure rendering functions; clean data flow.
- **Risk:** Low.

---

## Phase 2 — Medium Priority, Moderate Seams

**Goal:** files that are big but still separable with modest refactoring.

### 2.1 `tui/agent_bridge.rs` → `tui/agent_bridge/{mod,dispatcher,applier,queue}.rs`

- **Why:** 3 dispatcher impls + `TranscriptEventApplier` (~490 lines) + `PromptQueueView` + tests (477 lines).
- **Proposed layout:**
    - `mod.rs` — re-exports + `coalesce_agent_ui_events`.
    - `dispatcher.rs` — `TurnDispatcher`, `SlashDispatcher`.
    - `applier.rs` — `TranscriptEventApplier` + `last_message_index`, `trim_flush_trailing_ws`.
    - `queue.rs` — `PromptQueueView` / `PromptQueue`.
- **Effort:** Medium. Applier methods mutate `Vec<TranscriptMessage>` and use helper fns; helpers must be `pub(crate)`.
- **Risk:** Medium. Tests (477 lines) depend on multiple types; move carefully.

### 2.2 `tui/transcript/types.rs` → `tui/transcript/{types,message}.rs`

- **Why:** `TranscriptMessage` impl (~376 lines) + `ToolCardDetail` impl + enums + tests (530 lines).
- **Proposed layout:**
    - `types.rs` — structs, enums, constants.
    - `message.rs` — `impl TranscriptMessage` (gap calc, toggle, process log), `process_log_neighbor_gap`, `tool_entry_gap_after`, `toggle_collapsible_detail_at`, `toggle_latest_collapsible_detail`.
    - Move tests to `types_tests.rs` or keep inline.
- **Effort:** Small. Message logic is already self-contained.
- **Risk:** Low.

### 2.3 `tui/user_question.rs` → `tui/user_question/{mod,model,logic,nav,format}.rs`

- **Why:** `PendingUserQuestion` impl (~196) + many nav/format helpers + tests (310 lines).
- **Proposed layout:**
    - `mod.rs` — re-exports + public API.
    - `model.rs` — enums + `PendingUserQuestion`.
    - `logic.rs` — `apply_step_submit_outcome`, `try_resolve_submittable_answer`, `navigate_step_delta`, `reset_ui_for_step`, `apply_step_nav_outcome`, `restore_ui_from_collected`.
    - `nav.rs` — key-nav helpers (`question_tab_reverse`, `advance_question_selection`, `question_option_nav_delta`, `pick_step_tab_from_key`, etc.).
    - `format.rs` — display/validation helpers (`format_multi_select_answer`, `validate_text_answer`, `snapshot_current_answer`, `question_footer_hint`, etc.).
- **Effort:** Small–Medium. Many small pure functions; easy to move.
- **Risk:** Low.

### 2.4 `tui/status_dialog.rs` → `tui/status_dialog/{mod,kinds,render}.rs`

- **Why:** `StatusZone` component + 6 `build_*_dialog_kind` functions + per-kind renderers + tests.
- **Proposed layout:**
    - `mod.rs` — `StatusZone` component + re-exports.
    - `kinds.rs` — `PromptQueueAction`, `StatusDialogKind`, `build_*_dialog_kind` helpers.
    - `render.rs` — `render_tool_approval_dialog`, `render_plan_confirmation_dialog`, `render_mode_change_dialog`, `render_feedback_dialog`, `render_prompt_queue_dialog`, `render_ephemeral_banner`, `render_queue_action_chips`.
- **Effort:** Medium. Renderers reference `StatusZoneProps`; keep component in `mod.rs`.
- **Risk:** Low–Medium.

### 2.5 `tui/model_selector.rs` → `tui/model_selector/{mod,model,filter,view}.rs`

- **Why:** Catalog model + filter/fuzzy + `PendingModelSelector` impl + tests (425 lines).
- **Proposed layout:**
    - `mod.rs` — re-exports + `ModelSelectorShell` / view component.
    - `model.rs` — `ModelScopeMode`, `ModelProviderTab`, `ModelRow`, `ModelCatalogSnapshot`, `PendingModelSelector`.
    - `filter.rs` — `filter_models_fuzzy`, `model_match_score`, `model_query_tokens`, `model_row_match_score`.
    - `view.rs` — layout helpers (`model_selector_list_viewport_height`, `global_count_label`, `model_selector_footer_hint`).
- **Effort:** Small.
- **Risk:** Low.

### 2.6 `tui/transcript/card/kinds.rs` → `tui/transcript/card/{mod,kinds,renderers}.rs`

- **Why:** 7 card render functions + `ProcessHeaderToggle` component.
- **Proposed layout:**
    - `mod.rs` — re-exports + `ProcessHeaderToggle` component.
    - `kinds.rs` — `TranscriptCardKind`, `TranscriptStyle`, `tool_status_marker`, `status_line_process_state`.
    - `renderers.rs` — `user_prompt_card`, `skill_prompt_card`, `thinking_card`, `chat_response_card`, `error_card`, `meta_card`, `status_line_card`, `tool_call_card`, `thinking_response_pair_card`.
- **Effort:** Medium. Renderers are large but independent; they share only types from `kinds.rs`.
- **Risk:** Medium. `tool_call_card` (~211 lines) and `thinking_response_pair_card` (~116 lines) are big; keep them as fns, not sub-components.

### 2.7 `agent/slash_commands.rs` → `agent/slash_commands/{mod,registry,completions,dispatch}.rs`

- **Why:** Builtin registry + arg completions + dispatch fn + tests (263 lines).
- **Proposed layout:**
    - `mod.rs` — re-exports + `BuiltinSlashCommand`.
    - `registry.rs` — `builtin_slash_commands`, `slash_commands_for_palette`, arg-completion tables.
    - `completions.rs` — `slash_arg_completions`, per-command completion arrays.
    - `dispatch.rs` — `dispatch_slash_command`, `OverlayCommand`, `SlashDispatch`, `slash_unimplemented_message`, `format_help_message`.
- **Effort:** Small. Data-driven.
- **Risk:** Low.

### 2.8 `tui/startup.rs` → `tui/startup/{mod,bootstrap,transcript,mcp}.rs`

- **Why:** Bootstrap helpers + `reconstruct_transcript_from_llm_entries` (212 lines) + MCP bootstrap + tests (152 lines).
- **Proposed layout:**
    - `mod.rs` — re-exports + `spawn_bootstrap_worker`, `BootstrapUiEvent`.
    - `bootstrap.rs` — `TuiBootstrapConfig`, `BootstrapPhase`, agent/MCP bootstrap orchestrators.
    - `transcript.rs` — `load_transcript_snapshot_from_entries`, `reconstruct_transcript_from_llm_entries`, `AgentBootstrap`, `ToolResultInfo`.
    - `mcp.rs` — `McpBootstrapUpdate`, `bootstrap_mcp_for_session`, MCP helpers.
- **Effort:** Medium. `reconstruct_transcript_from_llm_entries` is self-contained.
- **Risk:** Low.

---

## Phase 3 — Deferred (Hard Seams)

**Goal:** acknowledge monoliths that need state-passing redesign before safe split.

### 3.1 `tui/shell/keys.rs` (3,174 lines)

- **Problem:** `handle_shell_key` is one match table over ~100 destructured bindings. Splitting requires either:
    - (a) extracting per-area action closures that still capture the bindings, or
    - (b) passing `&mut ShellCtx` and rewriting every binding access.
- **Interim improvement:** extract constants + small action helpers (`show_ephemeral_banner`, `arm_pending_quit`, etc.) into `helpers.rs`. Already done in prior split.
- **Full split:** requires a dedicated state-access redesign (e.g., `ShellCtx` sub-structs or accessor methods). Revisit after Phase 1–2 stabilize.

### 3.2 `tui/shell/view.rs` (1,709 lines)

- **Problem:** `build_shell_view` returns one giant element tree. Splitting means extracting sub-view builders that take the same 80+ hooks/bindings.
- **Interim improvement:** extract `render_*` functions for self-contained sub-trees (banner, prompt bar, transcript, dialogs).
- **Full split:** same redesign prerequisite as `keys.rs`.

### 3.3 `tui/shell/tick.rs` (911 lines)

- **Problem:** `shell_tick_loop` is a single async loop with interleaved concerns (ephemeral expiry, transcript publish, subagent output, bootstrap, prompt queue).
- **Interim improvement:** extract pure tick-timeout calculators and expiry handlers into `tick/helpers.rs` (keep loop monolith for now).
- **Full split:** requires per-tick-phase task spawning + channel-based coordination. Defer.

### 3.4 `tui/transcript/panel.rs` (569 lines)

- **Problem:** `TranscriptPanel` component is one big `#[component]` with hooks + markdown worker loop + render cache invalidation.
- **Interim improvement:** extract `TranscriptRenderCache` + markdown-worker loop into a separate hook/module.
- **Full split:** needs sub-component extraction (sticky header, scroll viewport, markdown overlay). Medium effort.

---

## Success Criteria & Gates

Each file split must pass:

```bash
cargo check -p elph
cargo clippy -p elph --all-targets -- -D warnings
make check   # cargo check --workspace
make lint    # clippy workspace
make test    # cargo nextest run --no-fail-fast
```

- Zero warnings.
- 1949 tests passed, 13 skipped (no regressions).
- Public API unchanged: `mod.rs` re-exports keep `crate::tui::foo::Bar` paths valid.
- Commit message format: `refactor(<path>): split <file> into <dir>/{mod,<files>}.rs`.

---

## Timeline Recommendation

| Phase | Files                                                                                                                        | Estimated Effort | Outcome                                                         |
| ----- | ---------------------------------------------------------------------------------------------------------------------------- | ---------------- | --------------------------------------------------------------- |
| **1** | `tool_params`, `settings`, `session/mod`, `provider_connect_dialog`                                                          | 4–6 increments   | 4 clean module directories; biggest agent/session god-impl gone |
| **2** | `agent_bridge`, `transcript/types`, `user_question`, `status_dialog`, `model_selector`, `kinds`, `slash_commands`, `startup` | 6–8 increments   | TUI and agent modules fully modular                             |
| **3** | `shell/{keys,view,tick}`, `transcript/panel`                                                                                 | deferred         | Requires state-access redesign; revisit after Phases 1–2        |

---

## Appendix — Full Inventory (≥400 lines)

| File                             | Lines | Category                      | Phase |
| -------------------------------- | ----- | ----------------------------- | ----- |
| `tui/shell/keys.rs`              | 3,174 | monolith dispatch             | 3     |
| `tui/shell/view.rs`              | 1,709 | monolith render               | 3     |
| `tui/tool_params.rs`             | 1,457 | 4 independent domains         | 1     |
| `tui/agent_bridge.rs`            | 1,380 | 3 dispatcher + applier        | 2     |
| `tui/transcript/types.rs`        | 1,370 | types + big impl + tests      | 2     |
| `tui/provider_connect_dialog.rs` | 1,264 | step renderers + types        | 1     |
| `tui/user_question.rs`           | 1,263 | model + logic + nav           | 2     |
| `tui/status_dialog.rs`           | 1,033 | kinds + builders + component  | 2     |
| `tui/model_selector.rs`          | 999   | model + filter + view         | 2     |
| `tui/transcript/card/kinds.rs`   | 966   | 7 card render fns             | 2     |
| `platform/settings.rs`           | 949   | 8 structs + serde + migration | 1     |
| `tui/startup.rs`                 | 914   | bootstrap + transcript + MCP  | 2     |
| `tui/shell/tick.rs`              | 911   | monolith tick loop            | 3     |
| `tui/shell/mod.rs`               | 820   | module root                   | done  |
| `agent/slash_commands.rs`        | 732   | registry + dispatch           | 2     |
| `agent/session/mod.rs`           | 728   | big impl + wiring.rs          | 1     |
| `tui/shell/helpers.rs`           | 631   | helpers (already split)       | done  |
| `tui/slash_handler.rs`           | 617   | handler + outcomes            | done  |
| `tui/transcript/panel.rs`        | 569   | monolith component            | 3     |
| `tui/tool_approval.rs`           | 557   | small structs + helpers       | done  |
| `tui/transcript/layout.rs`       | 488   | layout cache                  | done  |
| `tui/transcript/ephemeral.rs`    | 484   | banner builders               | done  |
| `cli/mcp.rs`                     | 460   | 3 CLI handlers                | low   |
| `tui/inline_dialog.rs`           | 452   | component                     | done  |
| `tui/user_question_bar.rs`       | 453   | component                     | done  |
| `agent/provider.rs`              | 469   | config + data table           | done  |
| `agent/session/wiring.rs`        | 423   | wiring submodule              | done  |
| `tui/slash_palette/keyboard.rs`  | 419   | key handlers                  | done  |
