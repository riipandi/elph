//! Terminal key handler (extracted from the MainShell `use_terminal_events` closure).

use super::*;

/// Handles terminal key events (extracted from the MainShell `use_terminal_events` closure).
pub(crate) fn handle_shell_key(ctx: ShellCtx, event: TerminalEvent) {
    let ShellCtx {
        mut active_plan_file,
        mut activity_label,
        mut activity_started_at,
        mut agent_mode,
        agent_session,
        mut agent_turn_active,
        allow_mode_change_while_busy,
        mut approval_selected,
        mut busy,
        mut busy_started_at,
        mut chrome_refresh_pending,
        mut chrome_stats,
        mut chrome_ui_revision,
        mut confetti_runtime,
        cwd,
        mut draft,
        mut ephemeral_banner,
        mut ephemeral_banner_generation,
        ephemeral_expire,
        extension_host,
        mut file_picker_active,
        file_picker_index,
        mut file_picker_key_handled,
        mut file_picker_show_hidden,
        file_picker_suppressed,
        mut force_editor_clear,
        mut force_palette_sync,
        mut idle_status_notice,
        mut input_prefix_kind,
        mut last_activity_label,
        mut last_arrow_up_at,
        mut live_cursor,
        mut live_draft,
        mention_index,
        messages,
        messages_arc,
        messages_revision,
        model_filter,
        model_input_focus,
        model_provider_index,
        model_selected_index,
        paths,
        mut pending_confetti,
        mut pending_feedback,
        mut pending_memory_flush,
        mut pending_mode_change,
        pending_model_selector,
        mut pending_retry_prompt,
        mut pending_plan_confirmation,
        mut pending_mcp_auth,
        mut pending_provider_api_key,
        mut pending_provider_connect,
        mut pending_provider_disconnect,
        pending_quit_confirm,
        mut pending_rename,
        mut pending_item_selector,
        mut item_selector_selected,
        mut pending_scoped_models,
        mut pending_system_prompt,
        mut pending_aside,
        mut pending_worker_chat,
        mut worker_chat_selected,
        pending_tool_approval,
        mut pending_transcript_notice_expires,
        pending_user_question,
        mut pre_echoed_user_prompts,
        mut prompt_editor_mirror,
        mut prompt_history,
        mut prompt_history_index,
        mut prompt_history_open,
        mut prompt_queue,
        prompt_templates,
        mut provider_connect_api_key,
        mut provider_connect_filter,
        mut provider_connect_input_focus,
        mut provider_connect_selected,
        mut provider_disconnect_selected,
        mut question_answer,
        mut question_confirm_focus,
        question_input_focus,
        question_multi_checked,
        mut question_selected,
        question_validation_error,
        mut queue_manager_action,
        mut queue_manager_open,
        mut queue_manager_selected,
        mut queue_ui_revision,
        mut rename_value,
        mut scoped_filter,
        mut scoped_selected_index,
        screen_height,
        mut select_mode,
        mut session_elapsed_secs,
        mut session_scoped_items,
        mut shell_focus,
        mut shift_held,
        mut shift_last_pressed,
        mut should_exit,
        skills,
        slash_commands,
        mut slash_palette_active,
        mut slash_palette_index,
        mut slash_palette_query,
        mut suppress_enter_newline,
        system_prompt_scroll,
        mut system_prompt_scroll_tick,
        mut thinking_level,
        mut turn_cancel_requested,
        mut turn_token_tracker,
        user_shell_abort,
        todos: _,
        mut resume_session_requested,
        screen_width,
        ..
    } = ctx;
    let paths = paths.read().clone();
    let agent_session = agent_session.clone();
    let extension_host_for_keys = extension_host.clone();
    let cwd_for_keys = cwd.clone();
    let mut messages = messages;
    let mut messages_revision = messages_revision;
    // Copy for terminal-events closure so pre-echo paths can sync to the shared arc.
    let mut messages_arc = messages_arc;
    let TerminalEvent::Key(KeyEvent {
        code, kind, modifiers, ..
    }) = event
    else {
        // Non-Key events (e.g. Paste) must propagate to child hooks. Do not consume.
        return;
    };
    if kind == KeyEventKind::Release {
        return;
    }

    // Track whether Shift is held so the transcript can hide the scrollbar
    // during native text selection (like a temporary Ctrl+S toggle).
    // Shift sets the flag and resets a 10-second timer. Only Shift press
    // extends the timer — non-Shift keys do nothing. This allows:
    // 1. Hold Shift → select text with mouse (timer starts at 10s)
    // 2. Release Shift → 10s grace to press Ctrl+C/Cmd+V (modifier chords
    //    arrive without visible modifiers on macOS terminals)
    // 3. Hold Shift again → timer resets to 10s
    // 4. After 10s of no Shift → scrollbar reappears automatically
    if modifiers.contains(KeyModifiers::SHIFT) {
        shift_held.set(true);
        shift_last_pressed.set(Some(Instant::now()));
    }

    // Arrow Up burst detection for mouse-wheel vs deliberate keypress.
    // Must measure gap *before* updating the timestamp — otherwise
    // `elapsed()` is always ~0 and prompt history can never open.
    let arrow_up_gap_ok = if code == KeyCode::Up {
        let since_last = last_arrow_up_at.get().elapsed();
        last_arrow_up_at.set(Instant::now());
        crate::tui::prompt_history::is_deliberate_arrow_up(since_last)
    } else {
        true
    };

    // Textarea handles `@` picker keys before this hook; do not fall through to agent-mode Tab.
    if file_picker_key_handled.get() {
        file_picker_key_handled.set(false);
        return;
    }

    // Prompt history palette (Arrow Up on empty focused editor; Tab/Enter apply).
    {
        let history_open = prompt_history_open.get();
        let history_snap = build_prompt_history_snapshot(
            history_open,
            &prompt_history.read(),
            48, // height only affects viewport cap; real height applied at render
        );
        if history_open {
            // Dismiss prompt history when text-select mode is active (Ctrl+S or Shift-held).
            if select_mode.get() || shift_held.get() {
                prompt_history_open.set(false);
                prompt_history_index.set(0);
            } else if let Some(action) =
                resolve_prompt_history_key_action(&history_snap, prompt_history_index.get(), code, modifiers)
            {
                match action {
                    PromptHistoryKeyAction::MoveSelection(index) => {
                        prompt_history_index.set(index);
                    }
                    PromptHistoryKeyAction::ApplyToPrompt { text } => {
                        draft.set(text.clone());
                        live_draft.set(text.clone());
                        live_cursor.set(text.len());
                        force_palette_sync.set(true);
                        prompt_history_open.set(false);
                        prompt_history_index.set(0);
                        shell_focus.set(ShellFocus::Prompt);
                    }
                    PromptHistoryKeyAction::Dismiss => {
                        prompt_history_open.set(false);
                        prompt_history_index.set(0);
                    }
                }
                return;
            }
            // Consume other plain keys while open (type to dismiss + seed).
            if modifiers.is_empty() && matches!(code, KeyCode::Char(_)) {
                prompt_history_open.set(false);
                prompt_history_index.set(0);
                // Fall through so the character reaches the editor.
            } else if modifiers.is_empty()
                && matches!(code, KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::Enter | KeyCode::Esc)
            {
                return;
            }
        } else if shell_focus.get() == ShellFocus::Prompt
            && !select_mode.get()
            && !shift_held.get()
            && kind == KeyEventKind::Press
            && is_prompt_history_open_key(code, modifiers)
            && arrow_up_gap_ok
        {
            let draft_body = {
                let live = live_draft.read().clone();
                let stored = draft.read().clone();
                if live.len() >= stored.len() { live } else { stored }
            };
            let slash_open = palette_visible(&compose_palette_draft(input_prefix_kind.get(), &draft_body));
            let picker_open = input_prefix_kind.get() == InputPrefixKind::Default
                && file_picker_open(&draft_body, live_cursor.get().min(draft_body.len()));
            if can_open_history(true, &draft_body, slash_open, picker_open, prompt_history.read().len()) {
                prompt_history_open.set(true);
                let history_len = prompt_history.read().len();
                prompt_history_index.set(history_len.saturating_sub(1));
                return;
            }
        } else if shell_focus.get() == ShellFocus::Prompt
            && (select_mode.get() || shift_held.get())
            && modifiers.is_empty()
            && matches!(code, KeyCode::Up | KeyCode::Down)
        {
            // Select text mode: redirect focus to the transcript so Arrow Up/Down
            // scrolls the transcript panel instead of the prompt editor.
            shell_focus.set(ShellFocus::Transcript);
            return;
        }
    }

    // Ctrl+S (or Ctrl+Shift+S) — toggle mouse capture for native text selection.
    // Persistent until toggled again. Skipped when scoped-models editor needs Ctrl+S to save
    // (that handler runs later while the overlay is open).
    let scoped_models_open_early = pending_scoped_models.read().is_some();
    if !scoped_models_open_early && is_text_select_toggle_key(modifiers, code) {
        let next = !select_mode.get();
        select_mode.set(next);
        let expire_tx = ephemeral_expire.read().tx.clone();
        show_ephemeral_banner(
            &mut ephemeral_banner,
            &mut ephemeral_banner_generation,
            &expire_tx,
            if next {
                select_mode_on_banner()
            } else {
                select_mode_off_banner()
            },
        );
        return;
    }

    let mut pending_tool_approval = pending_tool_approval;
    let mut pending_user_question = pending_user_question;
    let mut pending_model_selector = pending_model_selector;
    let mut model_provider_index = model_provider_index;
    let mut model_selected_index = model_selected_index;
    let mut model_filter = model_filter;
    let mut model_input_focus = model_input_focus;
    let mut question_multi_checked = question_multi_checked;
    let mut question_input_focus = question_input_focus;
    let mut question_validation_error = question_validation_error;
    let mut pending_quit_confirm = pending_quit_confirm;
    // Ctrl+Enter interject — handle early so status dialogs / queue manager do not swallow it.
    if is_ctrl_enter_interject(modifiers, code)
        && pending_tool_approval.read().is_none()
        && pending_user_question.read().is_none()
        && pending_model_selector.read().is_none()
        && pending_scoped_models.read().is_none()
        && pending_system_prompt.read().is_none()
        && pending_rename.read().is_none()
        && pending_confetti.read().is_none()
    {
        // Close queue manager if open; interject still runs.
        if queue_manager_open.get() {
            queue_manager_open.set(false);
            queue_manager_selected.set(0);
            queue_manager_action.set(PromptQueueAction::SendNow);
            shell_focus.set(ShellFocus::Prompt);
        }
        let editor_body = {
            let live = live_draft.read().clone();
            let stored = draft.read().clone();
            let (mirror, _) = prompt_editor_mirror.read().clone();
            [live, stored, mirror]
                .into_iter()
                .max_by_key(|s| s.len())
                .unwrap_or_default()
        };
        let body = editor_body.trim().to_string();
        // Ctrl+Enter from the textarea always interjects editor text directly —
        // never enqueue as follow-up and never prefer the prompt-queue list.
        // (Queue items are sent via the queue [Send] chip or when the editor is empty.)
        if !body.is_empty() {
            if let Some(session) = agent_session.as_ref() {
                let mut submitted = TranscriptMessage::text(body.clone(), TranscriptStyle::User);
                submitted.submitted_at = Some(chrono::Utc::now());
                // Sync to shared arc so the arc-to-state sync never loses this pre-echoed prompt.
                messages_arc.write().write().unwrap().push(submitted.clone());
                push_transcript_message(&mut messages, &mut messages_revision, &mut prompt_history, submitted);
                pre_echoed_user_prompts.set(pre_echoed_user_prompts.get().saturating_add(1));
                if agent_turn_active.get() {
                    // Suppress the text from the queue list: `spawn_steer` adds it to the
                    // harness queue, which will send back a `QueueUpdate`. Without this,
                    // the prompt reappears in the queue UI as if it was never sent.
                    prompt_queue.write().suppress_sent(body.clone());
                    queue_ui_revision.set(queue_ui_revision.get().wrapping_add(1));
                    TurnDispatcher::spawn_steer(Arc::clone(session), body);
                } else {
                    // Idle: start a normal turn (steer while idle falls back the same way).
                    agent_turn_active.set(true);
                    chrome_refresh_pending.set(true);
                    idle_status_notice.set(None);
                    turn_cancel_requested.set(false);
                    mark_busy(
                        &mut BusyActivation {
                            busy: &mut busy,
                            busy_started_at: &mut busy_started_at,
                            activity_started_at: &mut activity_started_at,
                            activity_label: &mut activity_label,
                            last_activity_label: &mut last_activity_label,
                        },
                        true,
                        None,
                    );
                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                    TurnDispatcher::spawn_turn(Arc::clone(session), body, false);
                }
            }
            draft.set(String::new());
            live_draft.set(String::new());
            force_editor_clear.set(true);
            suppress_enter_newline.set(true);
            return;
        }
        // Empty editor: optional — interject the front queue item (one at a time).
        // Always remove from harness queue (not only local UI) so QueueUpdate cannot resurrect it.
        if !prompt_queue.read().is_empty() {
            let popped = {
                let mut q = prompt_queue.write();
                q.pop_front_local()
            };
            if let Some(item) = popped {
                prompt_queue.write().suppress_sent(item.text.clone());
                queue_ui_revision.set(queue_ui_revision.get().wrapping_add(1));
                let mut submitted = TranscriptMessage::text(item.text.clone(), TranscriptStyle::User);
                submitted.submitted_at = Some(chrono::Utc::now());
                // Sync to shared arc so the arc-to-state sync never loses this pre-echoed prompt.
                messages_arc.write().write().unwrap().push(submitted.clone());
                push_transcript_message(&mut messages, &mut messages_revision, &mut prompt_history, submitted);
                pre_echoed_user_prompts.set(pre_echoed_user_prompts.get().saturating_add(1));
                if let Some(session) = agent_session.as_ref() {
                    // Always remove from harness + steer (idle path used to only spawn_turn,
                    // leaving the item in the harness queue so it reappeared in the list).
                    agent_turn_active.set(true);
                    chrome_refresh_pending.set(true);
                    idle_status_notice.set(None);
                    turn_cancel_requested.set(false);
                    mark_busy(
                        &mut BusyActivation {
                            busy: &mut busy,
                            busy_started_at: &mut busy_started_at,
                            activity_started_at: &mut activity_started_at,
                            activity_label: &mut activity_label,
                            last_activity_label: &mut last_activity_label,
                        },
                        true,
                        None,
                    );
                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                    TurnDispatcher::spawn_interject_queued(Arc::clone(session), item.kind, item.kind_index, item.text);
                }
            }
        }
        return;
    }

    // Ctrl+R — retry the last transient provider/stream error (stream cutoff, 5xx, …)
    // without re-typing. The error card shows the hint; the recovery prompt is stashed by
    // the tick loop on `AgentUiEvent::RetryablePrompt`. Only fires while idle with no modal
    // open, and only on the exact Ctrl+R chord (no extra Shift/Alt/Meta).
    let retryable_prompt = pending_retry_prompt.read().clone();
    if let Some(retry_text) = retryable_prompt
        && modifiers.contains(KeyModifiers::CONTROL)
        && !modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::META)
        && matches!(code, KeyCode::Char('r') | KeyCode::Char('R'))
        && !agent_turn_active.get()
        && !busy.get()
        && pending_tool_approval.read().is_none()
        && pending_user_question.read().is_none()
        && pending_model_selector.read().is_none()
        && pending_scoped_models.read().is_none()
        && pending_system_prompt.read().is_none()
        && pending_rename.read().is_none()
        && pending_plan_confirmation.read().is_none()
        && pending_mode_change.read().is_none()
        && !pending_quit_confirm.get()
        && !queue_manager_open.get()
    {
        pending_retry_prompt.set(None);
        if let Some(session) = agent_session.as_ref() {
            // Recovery prompt — render a slim status label, not a user bubble (and not
            // Arrow-Up history). The pre-echoed counter consumes the matching
            // UserPromptCommitted from the agent loop so it does not render twice.
            let mut notice = TranscriptMessage::text("Continuing tasks…", TranscriptStyle::Meta);
            notice.sticky_meta = true;
            messages_arc.write().write().unwrap().push(notice.clone());
            push_transcript_message(&mut messages, &mut messages_revision, &mut prompt_history, notice);
            pre_echoed_user_prompts.set(pre_echoed_user_prompts.get().saturating_add(1));
            agent_turn_active.set(true);
            chrome_refresh_pending.set(true);
            idle_status_notice.set(None);
            turn_cancel_requested.set(false);
            mark_busy(
                &mut BusyActivation {
                    busy: &mut busy,
                    busy_started_at: &mut busy_started_at,
                    activity_started_at: &mut activity_started_at,
                    activity_label: &mut activity_label,
                    last_activity_label: &mut last_activity_label,
                },
                true,
                None,
            );
            begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
            TurnDispatcher::spawn_turn(Arc::clone(session), retry_text, false);
        }
        return;
    }

    if pending_quit_confirm.get() && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                confirm_pending_quit(
                    PendingQuitAction {
                        pending_quit_confirm: &mut pending_quit_confirm,
                        should_exit: &mut should_exit,
                        busy: &busy,
                        turn_cancel_requested: &mut turn_cancel_requested,
                        prompt_queue: &mut prompt_queue,
                        pending_tool_approval: &mut pending_tool_approval,
                        pending_user_question: &mut pending_user_question,
                        agent_session: &agent_session,
                    },
                    &mut ephemeral_banner,
                    &mut ephemeral_banner_generation,
                );
                return;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                dismiss_pending_quit(
                    &mut pending_quit_confirm,
                    &mut idle_status_notice,
                    &mut ephemeral_banner,
                    &mut ephemeral_banner_generation,
                );
                return;
            }
            _ => {}
        }
    }

    let system_prompt_open = pending_system_prompt.read().is_some();
    let rename_open = pending_rename.read().is_some();
    let confetti_open = pending_confetti.read().is_some();
    let aside_open = pending_aside.read().is_some();
    let worker_chat_open = pending_worker_chat.read().is_some();

    // ── Worker chat overlay (Alt+M / `/intercom`) ─────────────────────
    let worker_chat_state = pending_worker_chat.read().clone();
    let worker_chat_open = worker_chat_open || worker_chat_state.is_some();
    let worker_chat_active = worker_chat_state.as_ref().and_then(|s| s.active.clone());
    let worker_chat_peers_len = worker_chat_state.as_ref().map(|s| s.peers.len()).unwrap_or(0);
    if worker_chat_open && kind == KeyEventKind::Press {
        if modifiers.is_empty() && code == KeyCode::Esc {
            // Esc: back to picker first, then close.
            let mut state = pending_worker_chat.write();
            if let Some(s) = state.as_mut() {
                if s.active.is_some() {
                    crate::tui::worker_chat::back_to_worker_picker(s);
                    worker_chat_selected.set(s.selected);
                } else {
                    drop(state);
                    crate::tui::worker_chat::close_worker_chat(
                        &mut pending_worker_chat,
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                        true,
                    );
                }
            }
            return;
        }
        if worker_chat_active.is_none() {
            // Picker navigation (+ select thread).
            if modifiers.is_empty() && code == KeyCode::Up {
                if worker_chat_peers_len > 0 {
                    let next = worker_chat_selected.get().saturating_sub(1);
                    worker_chat_selected.set(next);
                    if let Some(s) = pending_worker_chat.write().as_mut() {
                        s.selected = next;
                    }
                }
                return;
            }
            if modifiers.is_empty() && code == KeyCode::Down {
                if worker_chat_peers_len > 0 {
                    let next = (worker_chat_selected.get() + 1).min(worker_chat_peers_len - 1);
                    worker_chat_selected.set(next);
                    if let Some(s) = pending_worker_chat.write().as_mut() {
                        s.selected = next;
                    }
                }
                return;
            }
            if modifiers.is_empty() && code == KeyCode::Enter {
                let idx = worker_chat_selected.get();
                if let Some(s) = pending_worker_chat.write().as_mut() {
                    let _ = crate::tui::worker_chat::select_worker_thread(s, idx);
                    worker_chat_selected.set(s.selected);
                }
                return;
            }
        } else {
            // Thread view: compose input + Enter send (via tokio spawn so the async
            // mailbox write never blocks the key handler).
            if modifiers.is_empty()
                && let KeyCode::Char(c) = code
                && !c.is_control()
            {
                if let Some(s) = pending_worker_chat.write().as_mut() {
                    crate::tui::worker_chat::worker_compose_push(s, c);
                }
                return;
            }
            if modifiers.is_empty() && code == KeyCode::Backspace {
                if let Some(s) = pending_worker_chat.write().as_mut() {
                    let _ = crate::tui::worker_chat::worker_compose_backspace(s);
                }
                return;
            }
            if modifiers.is_empty() && code == KeyCode::Enter {
                let body = pending_worker_chat
                    .read()
                    .as_ref()
                    .map(|s| s.compose.clone())
                    .unwrap_or_default();
                let body = body.trim().to_string();
                if !body.is_empty() {
                    let peer = pending_worker_chat
                        .read()
                        .as_ref()
                        .and_then(|s| s.active.clone())
                        .map(|(id, name)| elph_agent::LiveWorker {
                            worker_id: id,
                            session_id: String::new(),
                            name: name.clone(),
                            purpose: String::new(),
                            model: None,
                            status: elph_agent::WorkerStatus::Online,
                            context_pct: None,
                            is_self: false,
                        });
                    let parent = pending_worker_chat
                        .read()
                        .as_ref()
                        .and_then(|s| s.thread_parent.clone());
                    if let (Some(peer), Some(session)) = (peer, agent_session.as_ref()) {
                        let session = Arc::clone(session);
                        let worker_id = peer.worker_id.clone();
                        let body_for_task = body.clone();
                        let parent_for_task = parent.clone();
                        tokio::spawn(async move {
                            if let Err(err) = session
                                .tui_send_worker_message(&peer, &body_for_task, parent_for_task.as_deref())
                                .await
                            {
                                log::warn!("worker chat send failed: {err:#}");
                            }
                        });
                        if let Some(s) = pending_worker_chat.write().as_mut() {
                            crate::tui::worker_chat::worker_compose_clear(s);
                        }
                        push_transcript_message_synced(
                            &mut messages,
                            messages_arc,
                            &mut messages_revision,
                            &mut prompt_history,
                            TranscriptMessage::text(format!("→ {worker_id}: {body}"), TranscriptStyle::Meta),
                        );
                    }
                }
                return;
            }
        }
        // In-modal keys: swallow everything except shell global shortcuts.
        if !shell_global_shortcut(modifiers, code) {
            return;
        }
    }

    // Alt+M — open worker chat (only when no other modal is open).
    let worker_modal_blocked = pending_tool_approval.read().is_some()
        || pending_user_question.read().is_some()
        || pending_mode_change.read().is_some()
        || pending_plan_confirmation.read().is_some()
        || pending_memory_flush.read().is_some()
        || *pending_feedback.read()
        || pending_item_selector.read().is_some()
        || aside_open;
    if modifiers.contains(KeyModifiers::ALT)
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::META)
        && matches!(code, KeyCode::Char('m') | KeyCode::Char('M'))
        && !worker_modal_blocked
    {
        let Some(session) = agent_session.clone() else {
            // No live agent session: open an empty picker.
            let mut state = crate::tui::worker_chat::WorkerChatState::new();
            state.rebuild_peers(Vec::new());
            let stashed = {
                let current = live_draft.read().clone();
                if current.trim().is_empty() { None } else { Some(current) }
            };
            if stashed.is_some() {
                draft.set(String::new());
                live_draft.set(String::new());
            }
            if let Some(text) = stashed {
                state.compose = text;
            }
            pending_worker_chat.set(Some(state));
            shell_focus.set(ShellFocus::StatusDialog);
            return;
        };
        tokio::spawn(async move {
            let peers = session.tui_worker_peers().await.unwrap_or_default();
            let history = session
                .tui_worker_inbox(crate::tui::worker_chat::WORKER_CHAT_INBOX_LIMIT)
                .await
                .unwrap_or_default();
            // Hand to the shell: refresh or create the pending chat state.
            // We mutate shell state directly here (we are on the keys path).
            if let Some(pending) = pending_worker_chat.write().as_mut() {
                // Already open? just refresh history.
                pending.messages = history;
                pending.rebuild_peers(peers);
                pending.revision = pending.revision.wrapping_add(1);
                return;
            }
            let mut state = crate::tui::worker_chat::WorkerChatState::new();
            if history.len() > crate::tui::worker_chat::WORKER_CHAT_INBOX_LIMIT as usize {
                let start = history.len() - crate::tui::worker_chat::WORKER_CHAT_INBOX_LIMIT as usize;
                state.messages = history[start..].to_vec();
            } else {
                state.messages = history;
            }
            state.rebuild_peers(peers);
            pending_worker_chat.set(Some(state));
            shell_focus.set(ShellFocus::StatusDialog);
        });
        return;
    }

    // Escape closes confetti/fireworks overlay.
    if confetti_open && modifiers.is_empty() && code == KeyCode::Esc {
        close_confetti(
            &mut pending_confetti,
            &mut confetti_runtime,
            &mut draft,
            &mut live_draft,
            &mut shell_focus,
        );
        return;
    }

    // `/aside` panel: Esc dismisses; ↑↓ scroll Done answers (when scrollable).
    if aside_open && kind == KeyEventKind::Press && modifiers.is_empty() {
        use crate::tui::aside_panel::dismiss_aside_panel;
        use crate::tui::inline_dialog::inline_body_width;
        let content_w = inline_body_width(screen_width) as usize;
        if code == KeyCode::Esc {
            if let Some(state) = pending_aside.write().take() {
                let (_id, notice) = dismiss_aside_panel(state);
                if let Some(notice) = notice {
                    push_transcript_message_synced(
                        &mut messages,
                        messages_arc,
                        &mut messages_revision,
                        &mut prompt_history,
                        crate::tui::transcript::TranscriptMessage::text(
                            notice,
                            crate::tui::transcript::TranscriptStyle::Meta,
                        ),
                    );
                }
            }
            return;
        }
        if matches!(code, KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown)
            && let Some(state) = pending_aside.write().as_mut()
        {
            let max_off = state.max_scroll_offset(content_w);
            if max_off > 0 {
                let page = crate::tui::aside_panel::ASIDE_MAX_BODY_LINES.saturating_sub(1).max(1);
                match code {
                    KeyCode::Up => state.scroll_up(1),
                    KeyCode::Down => state.scroll_down(1, max_off),
                    KeyCode::PageUp => state.scroll_up(page),
                    KeyCode::PageDown => state.scroll_down(page, max_off),
                    _ => {}
                }
                return;
            }
        }
    }

    let model_selector_open = pending_model_selector.read().is_some();
    let scoped_models_open = pending_scoped_models.read().is_some();
    let item_selector_open = pending_item_selector.read().is_some();
    let provider_connect_open = pending_provider_connect.read().is_some();
    let mcp_auth_open = pending_mcp_auth.read().is_some();
    let provider_disconnect_open = pending_provider_disconnect.read().is_some();
    let provider_api_key_open = pending_provider_api_key.read().is_some();
    let queue_manager_is_open = queue_manager_open.get();
    let status_dialog_open = pending_tool_approval.read().is_some()
        || pending_mode_change.read().is_some()
        || pending_plan_confirmation.read().is_some()
        || pending_memory_flush.read().is_some()
        || *pending_feedback.read()
        || pending_user_question.read().is_some()
        || model_selector_open
        || scoped_models_open
        || item_selector_open
        || system_prompt_open
        || rename_open
        || confetti_open
        || provider_connect_open
        || mcp_auth_open
        || provider_disconnect_open
        || provider_api_key_open
        || queue_manager_is_open;

    if status_dialog_open {
        if confetti_open {
            return;
        }

        // Ctrl+Q queue manager: ↑↓ item · ←→ action · Enter activate · Esc close.
        if queue_manager_is_open
            && pending_tool_approval.read().is_none()
            && pending_user_question.read().is_none()
            && !model_selector_open
            && !scoped_models_open
            && !system_prompt_open
        {
            let len = prompt_queue.read().len();
            if len == 0 {
                queue_manager_open.set(false);
                queue_manager_action.set(PromptQueueAction::SendNow);
                shell_focus.set(ShellFocus::Prompt);
                return;
            }
            if modifiers.is_empty() && code == KeyCode::Esc {
                queue_manager_open.set(false);
                queue_manager_selected.set(0);
                queue_manager_action.set(PromptQueueAction::SendNow);
                shell_focus.set(ShellFocus::Prompt);
                return;
            }
            if modifiers.is_empty() && matches!(code, KeyCode::Up | KeyCode::Char('k')) {
                let idx = queue_manager_selected.get();
                queue_manager_selected.set(idx.saturating_sub(1));
                return;
            }
            if modifiers.is_empty() && matches!(code, KeyCode::Down | KeyCode::Char('j')) {
                let idx = queue_manager_selected.get();
                queue_manager_selected.set((idx + 1).min(len.saturating_sub(1)));
                return;
            }
            if modifiers.is_empty() && matches!(code, KeyCode::Left | KeyCode::Char('h')) {
                queue_manager_action.set(queue_manager_action.get().prev());
                return;
            }
            if modifiers.is_empty() && matches!(code, KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab) {
                queue_manager_action.set(queue_manager_action.get().next());
                return;
            }
            let action_hotkey = if modifiers.is_empty() {
                match code {
                    KeyCode::Char('s') | KeyCode::Char('S') => Some(PromptQueueAction::SendNow),
                    KeyCode::Char('e') | KeyCode::Char('E') => Some(PromptQueueAction::Edit),
                    KeyCode::Char('c')
                    | KeyCode::Char('C')
                    | KeyCode::Backspace
                    | KeyCode::Delete
                    | KeyCode::Char('x')
                    | KeyCode::Char('X') => Some(PromptQueueAction::Cancel),
                    KeyCode::Enter => Some(queue_manager_action.get()),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(action) = action_hotkey {
                queue_manager_action.set(action);
                let idx = queue_manager_selected.get();
                let turn_active = agent_turn_active.get();
                let session = agent_session.clone();
                let mark_busy_for_idle = apply_prompt_queue_action(
                    action,
                    idx,
                    &mut PromptQueueActionCtx {
                        prompt_queue: &mut prompt_queue,
                        queue_ui_revision: &mut queue_ui_revision,
                        agent_session: &session,
                        agent_turn_active: turn_active,
                        messages: &mut messages,
                        messages_revision: &mut messages_revision,
                        prompt_history: &mut prompt_history,
                        pre_echoed_user_prompts: &mut pre_echoed_user_prompts,
                        draft: &mut draft,
                        live_draft: &mut live_draft,
                        live_cursor: &mut live_cursor,
                        prompt_editor_mirror: &mut prompt_editor_mirror,
                        force_palette_sync: &mut force_palette_sync,
                        shell_focus: &mut shell_focus,
                        queue_manager_open: &mut queue_manager_open,
                        queue_manager_selected: &mut queue_manager_selected,
                        queue_manager_action: &mut queue_manager_action,
                        messages_arc: &mut messages_arc,
                    },
                );
                // Send while idle: interject spawn runs the turn; mark shell busy UI only.
                if mark_busy_for_idle.is_some() {
                    agent_turn_active.set(true);
                    chrome_refresh_pending.set(true);
                    idle_status_notice.set(None);
                    turn_cancel_requested.set(false);
                    mark_busy(
                        &mut BusyActivation {
                            busy: &mut busy,
                            busy_started_at: &mut busy_started_at,
                            activity_started_at: &mut activity_started_at,
                            activity_label: &mut activity_label,
                            last_activity_label: &mut last_activity_label,
                        },
                        false,
                        None,
                    );
                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                }
                return;
            }
            // Digit jump 1–9.
            if modifiers.is_empty()
                && let KeyCode::Char(ch) = code
                && ch.is_ascii_digit()
                && ch != '0'
            {
                let n = (ch as u8 - b'0') as usize;
                if n >= 1 && n <= len {
                    queue_manager_selected.set(n - 1);
                }
                return;
            }
            return;
        }

        if system_prompt_open {
            let mut pending_system_prompt = pending_system_prompt;
            let mut draft = draft;
            let mut live_draft = live_draft;
            let mut shell_focus = shell_focus;
            let mut system_prompt_scroll = system_prompt_scroll;

            if modifiers.is_empty() && code == KeyCode::Esc {
                close_system_prompt_dialog(
                    &mut pending_system_prompt,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    &mut force_editor_clear,
                );
                return;
            }

            if modifiers.is_empty() {
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        scroll_view_up(&mut system_prompt_scroll.write(), 1);
                        system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                        return;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        scroll_view_down(&mut system_prompt_scroll.write(), 1);
                        system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                        return;
                    }
                    KeyCode::PageUp => {
                        scroll_view_up(&mut system_prompt_scroll.write(), 10);
                        system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                        return;
                    }
                    KeyCode::PageDown => {
                        scroll_view_down(&mut system_prompt_scroll.write(), 10);
                        system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                        return;
                    }
                    _ => {}
                }
            }

            if !shell_global_shortcut(modifiers, code) {
                return;
            }
        }

        if rename_open {
            let mut pending_rename = pending_rename;
            let mut rename_value = rename_value;
            let mut draft = draft;
            let mut live_draft = live_draft;
            let mut shell_focus = shell_focus;
            if modifiers.is_empty() && code == KeyCode::Esc {
                close_rename_dialog(
                    &mut pending_rename,
                    &mut rename_value,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    true,
                );
                force_editor_clear.set(true);
                return;
            }
            // Text input owns typing / Enter / Esc via DialogUserInputContent.
            if !shell_global_shortcut(modifiers, code) {
                return;
            }
        }

        if scoped_models_open
            && pending_user_question.read().is_none()
            && !system_prompt_open
            && !confetti_open
            && !model_selector_open
        {
            let mut pending_scoped_models = pending_scoped_models;
            let mut scoped_selected_index = scoped_selected_index;
            let scoped_filter = scoped_filter;
            let mut draft = draft;
            let mut live_draft = live_draft;
            let mut shell_focus = shell_focus;
            let mut session_scoped_items = session_scoped_items;
            let paths_snapshot = paths.clone();

            if modifiers.is_empty() && code == KeyCode::Esc {
                cancel_scoped_models(
                    &mut pending_scoped_models,
                    &mut session_scoped_items.write(),
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                );
                return;
            }

            // Scoped editor owns Ctrl+S for save (do not require !SHIFT — either chord saves).
            if modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META)
                && matches!(code, KeyCode::Char('s') | KeyCode::Char('S'))
            {
                if let Some(pending) = pending_scoped_models.write().as_mut() {
                    save_scoped_models(pending, &paths_snapshot, &mut session_scoped_items.write());
                    push_transcript_message_synced(
                        &mut messages,
                        messages_arc,
                        &mut messages_revision,
                        &mut prompt_history,
                        TranscriptMessage::text(
                            format!("Scoped models saved ({} enabled).", pending.enabled_count()),
                            TranscriptStyle::Meta,
                        ),
                    );
                }
                return;
            }

            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('a') | KeyCode::Char('A')) {
                if let Some(pending) = pending_scoped_models.write().as_mut() {
                    sync_scoped_filter(pending, &scoped_filter.read());
                    pending.enable_all_visible_or_all();
                    apply_scoped_session(pending, &mut session_scoped_items.write());
                    scoped_selected_index.set(pending.selected_index);
                }
                return;
            }

            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('x') | KeyCode::Char('X')) {
                if let Some(pending) = pending_scoped_models.write().as_mut() {
                    sync_scoped_filter(pending, &scoped_filter.read());
                    pending.clear_all_visible_or_all();
                    apply_scoped_session(pending, &mut session_scoped_items.write());
                    scoped_selected_index.set(pending.selected_index);
                }
                return;
            }

            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('p') | KeyCode::Char('P')) {
                if let Some(pending) = pending_scoped_models.write().as_mut() {
                    sync_scoped_filter(pending, &scoped_filter.read());
                    pending.toggle_selected_provider();
                    apply_scoped_session(pending, &mut session_scoped_items.write());
                    scoped_selected_index.set(pending.selected_index);
                }
                return;
            }

            if let Some(delta) = scoped_models_reorder_delta(modifiers, code) {
                if let Some(pending) = pending_scoped_models.write().as_mut() {
                    sync_scoped_filter(pending, &scoped_filter.read());
                    if pending.reorder_selected(delta) {
                        apply_scoped_session(pending, &mut session_scoped_items.write());
                        scoped_selected_index.set(pending.selected_index);
                    }
                }
                return;
            }

            if let Some(delta) = scoped_models_list_nav_delta(modifiers, code) {
                if let Some(pending) = pending_scoped_models.write().as_mut() {
                    sync_scoped_filter(pending, &scoped_filter.read());
                    pending.move_selection(delta);
                    scoped_selected_index.set(pending.selected_index);
                }
                return;
            }

            if modifiers.is_empty() && code == KeyCode::Enter {
                if let Some(pending) = pending_scoped_models.write().as_mut() {
                    sync_scoped_filter(pending, &scoped_filter.read());
                    pending.toggle_selected();
                    apply_scoped_session(pending, &mut session_scoped_items.write());
                    scoped_selected_index.set(pending.selected_index);
                }
                return;
            }

            if !shell_global_shortcut(modifiers, code) {
                return;
            }
        }

        // ── Item selector (/resume, /tree) ─────────────────────────────
        if item_selector_open
            && pending_user_question.read().is_none()
            && !system_prompt_open
            && !confetti_open
            && !model_selector_open
            && !scoped_models_open
            && !rename_open
        {
            if modifiers.is_empty() && code == KeyCode::Esc {
                close_item_selector(&mut pending_item_selector, &mut draft, &mut live_draft, &mut shell_focus, true);
                force_editor_clear.set(true);
                return;
            }

            // Pi TreeSelector filter modes (Tab / Ctrl+O cycle, Ctrl+D/T/U/L/A).
            if let Some(action) = tree_filter_key_action(modifiers, code)
                && let Some(pending) = pending_item_selector.write().as_mut()
                && pending.purpose == ItemSelectorPurpose::NavigateTree
            {
                apply_tree_filter_key(pending, action);
                item_selector_selected.set(pending.filtered_selected());
                return;
            }
            // Resume picker: ignore tree filter chords (fall through for Esc-global etc.).

            if let Some(delta) = item_selector_list_nav_delta(modifiers, code) {
                if let Some(pending) = pending_item_selector.write().as_mut() {
                    if delta == isize::MIN / 4 {
                        let indices = pending.filtered_indices();
                        if let Some(&first) = indices.first() {
                            pending.selected = first;
                        }
                    } else if delta == isize::MAX / 4 {
                        let indices = pending.filtered_indices();
                        if let Some(&last) = indices.last() {
                            pending.selected = last;
                        }
                    } else {
                        pending.move_delta(delta);
                    }
                    item_selector_selected.set(pending.filtered_selected());
                }
                return;
            }

            if modifiers.is_empty() && code == KeyCode::Backspace {
                if let Some(pending) = pending_item_selector.write().as_mut()
                    && pending.filter_backspace()
                {
                    item_selector_selected.set(pending.filtered_selected());
                }
                return;
            }

            // Printable filter characters (no modifiers).
            if modifiers.is_empty()
                && let KeyCode::Char(c) = code
                && !c.is_control()
            {
                if let Some(pending) = pending_item_selector.write().as_mut() {
                    pending.apply_filter_char(c);
                    item_selector_selected.set(pending.filtered_selected());
                }
                return;
            }

            let with_summary = item_selector_confirm_summary_on_ctrl_enter(modifiers, code);
            let plain_confirm = item_selector_confirm_on_enter(modifiers, code);
            if plain_confirm || with_summary {
                let snapshot = pending_item_selector.read().clone();
                let Some(pending) = snapshot else {
                    return;
                };
                let Some(value) = pending.selected_value().map(str::to_string) else {
                    return;
                };
                let purpose = pending.purpose;
                close_item_selector(&mut pending_item_selector, &mut draft, &mut live_draft, &mut shell_focus, false);
                force_editor_clear.set(true);
                match purpose {
                    ItemSelectorPurpose::ResumeSession => {
                        if let Some(session) = agent_session.as_ref() {
                            let session = Arc::clone(session);
                            tokio::spawn(async move {
                                session.shutdown_workers().await;
                            });
                        }
                        push_transcript_message_synced(
                            &mut messages,
                            messages_arc,
                            &mut messages_revision,
                            &mut prompt_history,
                            TranscriptMessage::text(format!("Resuming session {value}…"), TranscriptStyle::Meta),
                        );
                        resume_session_requested.set(Some(value));
                    }
                    ItemSelectorPurpose::NavigateTree => {
                        let Some(session) = agent_session.as_ref().map(Arc::clone) else {
                            push_transcript_message_synced(
                                &mut messages,
                                messages_arc,
                                &mut messages_revision,
                                &mut prompt_history,
                                TranscriptMessage::text(
                                    "Agent session required for /tree.".to_string(),
                                    TranscriptStyle::Meta,
                                ),
                            );
                            return;
                        };
                        let summarize = with_summary;
                        let entry_id = value.clone();
                        let sid = session.session_id().to_string();
                        let nav = elph_agent::try_block_on(async {
                            session.navigate_tree_to_with_options(&entry_id, summarize).await
                        });
                        match nav {
                            Ok(Ok(())) => {
                                push_transcript_message_synced(
                                    &mut messages,
                                    messages_arc,
                                    &mut messages_revision,
                                    &mut prompt_history,
                                    TranscriptMessage::text(
                                        format!(
                                            "Navigated to {entry_id}{}",
                                            if summarize { " (with summary)" } else { "" }
                                        ),
                                        TranscriptStyle::Meta,
                                    ),
                                );
                                // Reload transcript for the new leaf.
                                resume_session_requested.set(Some(sid));
                            }
                            Ok(Err(e)) => {
                                push_transcript_message_synced(
                                    &mut messages,
                                    messages_arc,
                                    &mut messages_revision,
                                    &mut prompt_history,
                                    TranscriptMessage::text(
                                        format!("/tree navigate failed: {e:#}"),
                                        TranscriptStyle::Meta,
                                    ),
                                );
                            }
                            Err(e) => {
                                push_transcript_message_synced(
                                    &mut messages,
                                    messages_arc,
                                    &mut messages_revision,
                                    &mut prompt_history,
                                    TranscriptMessage::text(
                                        format!("/tree navigate failed: {e}"),
                                        TranscriptStyle::Meta,
                                    ),
                                );
                            }
                        }
                    }
                }
                return;
            }

            if !shell_global_shortcut(modifiers, code) {
                return;
            }
        }

        if model_selector_open
            && pending_user_question.read().is_none()
            && !system_prompt_open
            && !confetti_open
            && !scoped_models_open
        {
            let mut pending_model_selector = pending_model_selector;
            let mut model_provider_index = model_provider_index;
            let mut model_selected_index = model_selected_index;
            let mut model_filter = model_filter;
            let mut model_input_focus = model_input_focus;
            let mut draft = draft;
            let mut live_draft = live_draft;
            let mut shell_focus = shell_focus;
            let mut chrome_stats = chrome_stats;
            let mut chrome_refresh_pending = chrome_refresh_pending;

            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('l') | KeyCode::Char('L')) {
                close_model_selector(&mut pending_model_selector, &mut draft, &mut live_draft, &mut shell_focus);
                return;
            }

            if modifiers.is_empty() && code == KeyCode::Esc {
                close_model_selector(&mut pending_model_selector, &mut draft, &mut live_draft, &mut shell_focus);
                return;
            }

            if modifiers.is_empty() && code == KeyCode::Tab {
                let next = match model_input_focus.get() {
                    ModelSelectorFocus::Search => ModelSelectorFocus::List,
                    ModelSelectorFocus::List => ModelSelectorFocus::Search,
                };
                model_input_focus.set(next);
                if let Some(pending) = pending_model_selector.write().as_mut() {
                    pending.input_focus = next;
                }
                return;
            }

            // `+` / `-` — add / remove highlighted model from scoped models.
            // Works from list *or* filter focus (filter field blocks these chars from typing).
            if let Some(action) = model_selector_scoped_action(modifiers, code) {
                let selection = pending_model_selector.read().as_ref().and_then(|pending| {
                    let mut pending = pending.clone();
                    sync_pending_filter(&mut pending, &model_filter.read());
                    pending.selected_model().map(|row| row.value)
                });
                if let Some(value) = selection {
                    let mut session_scoped = session_scoped_items.write();
                    if let Some(status) = apply_model_scoped_action(&paths, &mut session_scoped, &value, action) {
                        drop(session_scoped);
                        if let Some(pending) = pending_model_selector.write().as_mut() {
                            sync_pending_filter(pending, &model_filter.read());
                            pending.refresh_scoped_models(&session_scoped_items.read());
                            model_selected_index.set(pending.model_index);
                            model_provider_index.set(pending.provider_index);
                        }
                        publish_ephemeral_transcript_notice(
                            &mut messages,
                            &mut messages_revision,
                            &mut pending_transcript_notice_expires,
                            MODEL_SET_NOTICE_KEY,
                            status,
                        );
                    }
                }
                return;
            }

            if model_input_focus.get() == ModelSelectorFocus::List
                && let Some(seed) = model_selector_filter_seed(modifiers, code)
                && let Some(pending) = pending_model_selector.write().as_mut()
            {
                apply_model_selector_filter_seed(seed, &mut model_filter, &mut model_input_focus, pending);
                model_selected_index.set(pending.model_index);
                return;
            }

            // `[` / `]` — All | Scoped | Provider (any focus).
            if let Some(delta) = model_selector_scope_delta(modifiers, code) {
                if let Some(pending) = pending_model_selector.write().as_mut() {
                    sync_pending_filter(pending, &model_filter.read());
                    pending.apply_scope_nav(delta);
                    model_provider_index.set(pending.provider_index);
                    model_selected_index.set(pending.model_index);
                    if model_input_focus.get() == ModelSelectorFocus::List {
                        focus_model_selector_list(&mut model_input_focus, pending);
                    }
                }
                return;
            }

            // `$` — toggle sort order (Default → CostAsc → CostDesc → …).
            if modifiers.is_empty() && code == KeyCode::Char('$') {
                if let Some(pending) = pending_model_selector.write().as_mut() {
                    let next = match pending.sort_order {
                        crate::tui::model_selector::SortOrder::Default => {
                            crate::tui::model_selector::SortOrder::CostAsc
                        }
                        crate::tui::model_selector::SortOrder::CostAsc => {
                            crate::tui::model_selector::SortOrder::CostDesc
                        }
                        crate::tui::model_selector::SortOrder::CostDesc => {
                            crate::tui::model_selector::SortOrder::Default
                        }
                    };
                    pending.sort_order = next;
                    model_selected_index.set(pending.model_index);
                }
                return;
            }

            // ←/→ (and h/l on list) — cycle providers only on the Provider scope tab.
            // Arrows also work from the filter when it is empty so users need not Tab first.
            if let Some(delta) = model_selector_provider_delta(modifiers, code) {
                let list_focused = model_input_focus.get() == ModelSelectorFocus::List;
                let is_arrow = matches!(code, KeyCode::Left | KeyCode::Right);
                let filter_empty = model_filter.read().trim().is_empty();
                let wants_provider_nav = list_focused || (is_arrow && filter_empty);
                if wants_provider_nav {
                    if let Some(pending) = pending_model_selector.write().as_mut() {
                        // All / Scoped: consume the key but do not switch providers.
                        if pending.is_provider_scope_mode() {
                            if list_focused {
                                focus_model_selector_list(&mut model_input_focus, pending);
                            }
                            sync_pending_filter(pending, &model_filter.read());
                            pending.apply_provider_nav(delta);
                            model_provider_index.set(pending.provider_index);
                            model_selected_index.set(pending.model_index);
                        }
                    }
                    return;
                }
            }

            if model_input_focus.get() == ModelSelectorFocus::List {
                if modifiers.is_empty()
                    && code == KeyCode::Backspace
                    && let Some(pending) = pending_model_selector.write().as_mut()
                    && model_selector_list_backspace(model_input_focus.get(), &mut model_filter, pending)
                {
                    model_selected_index.set(pending.model_index);
                    return;
                }

                if let Some(delta) = model_selector_list_nav_delta(modifiers, code) {
                    if let Some(pending) = pending_model_selector.write().as_mut() {
                        focus_model_selector_list(&mut model_input_focus, pending);
                        sync_pending_filter(pending, &model_filter.read());
                        let len = pending.filtered_models().len();
                        if len > 0 {
                            let next = (pending.model_index as isize + delta).clamp(0, len as isize - 1) as usize;
                            pending.model_index = next;
                            model_selected_index.set(next);
                        }
                    }
                    return;
                }
            }

            if modifiers.is_empty()
                && code == KeyCode::Enter
                && model_selector_confirm_on_enter(model_input_focus.get())
            {
                let selection = pending_model_selector.read().as_ref().and_then(|pending| {
                    let mut pending = pending.clone();
                    sync_pending_filter(&mut pending, &model_filter.read());
                    pending.selected_model().map(|row| row.value)
                });
                if let Some(value) = selection {
                    let paths_snapshot = paths.clone();
                    let agent = agent_session.clone();
                    let mut stats = chrome_stats.read().clone();
                    match apply_model_selection_locally(&value, &paths_snapshot, &mut stats) {
                        Ok(label) => {
                            publish_chrome_stats(&mut chrome_stats, &mut chrome_ui_revision, stats);
                            chrome_refresh_pending.set(true);
                            // Keep footer / Ctrl+. levels aligned with the new model catalog.
                            let clamped = clamp_thinking_for_model_value(thinking_level.get(), &value);
                            if clamped != thinking_level.get() {
                                thinking_level.set(clamped);
                            }
                            publish_ephemeral_transcript_notice(
                                &mut messages,
                                &mut messages_revision,
                                &mut pending_transcript_notice_expires,
                                MODEL_SET_NOTICE_KEY,
                                model_set_notice_text(&label),
                            );
                            if let Some(session) = agent {
                                spawn_runtime_model_switch(session, value, thinking_level.get());
                            }
                        }
                        Err(err) => {
                            push_transcript_message_synced(
                                &mut messages,
                                messages_arc,
                                &mut messages_revision,
                                &mut prompt_history,
                                TranscriptMessage::text(format!("{err}"), TranscriptStyle::Meta),
                            );
                        }
                    }
                }
                close_model_selector(&mut pending_model_selector, &mut draft, &mut live_draft, &mut shell_focus);
                return;
            }

            if !shell_global_shortcut(modifiers, code) {
                return;
            }
        }

        if (model_selector_open || scoped_models_open || system_prompt_open || confetti_open)
            && pending_user_question.read().is_none()
        {
            return;
        }

        let step_tab_jump = {
            let pending_ref = pending_user_question.read();
            match pending_ref.as_ref() {
                Some(pending) if pending.step_count() > 1 => {
                    pick_step_tab_from_key(modifiers, code, pending.step_count()).map(|target| {
                        let snapshot = snapshot_current_answer(
                            pending,
                            &question_answer.read(),
                            question_selected.get(),
                            &question_multi_checked.read(),
                        );
                        (target, snapshot)
                    })
                }
                _ => None,
            }
        };
        if let Some((target, snapshot)) = step_tab_jump {
            let outcome = pending_user_question
                .write()
                .take()
                .map(|pending| pending.jump_to_step(target, snapshot));
            if let Some(StepNavOutcome::Jumped(pending)) = outcome {
                apply_step_nav_outcome(
                    StepNavOutcome::Jumped(pending),
                    &mut pending_user_question,
                    &mut question_selected,
                    &mut question_confirm_focus,
                    &mut question_answer,
                    &mut question_multi_checked,
                    &mut question_input_focus,
                    &mut activity_label,
                    &mut question_validation_error,
                );
            }
            return;
        }

        let step_nav_delta = {
            let pending_ref = pending_user_question.read();
            match pending_ref.as_ref() {
                Some(pending)
                    if pending.step_count() > 1 && !pending.is_confirm() && !question_input_focus.get().is_custom() =>
                {
                    question_step_nav_delta(modifiers, code).map(|delta| {
                        let snapshot = snapshot_current_answer(
                            pending,
                            &question_answer.read(),
                            question_selected.get(),
                            &question_multi_checked.read(),
                        );
                        (delta, snapshot)
                    })
                }
                _ => None,
            }
        };
        if let Some((delta, snapshot)) = step_nav_delta {
            let outcome = pending_user_question
                .write()
                .take()
                .and_then(|pending| navigate_step_delta(pending, delta, snapshot));
            if let Some(nav) = outcome {
                apply_step_nav_outcome(
                    nav,
                    &mut pending_user_question,
                    &mut question_selected,
                    &mut question_confirm_focus,
                    &mut question_answer,
                    &mut question_multi_checked,
                    &mut question_input_focus,
                    &mut activity_label,
                    &mut question_validation_error,
                );
            }
            return;
        }

        let step_back = {
            let pending_ref = pending_user_question.read();
            match pending_ref.as_ref() {
                Some(pending)
                    if pending.can_go_back()
                        && modifiers.is_empty()
                        && code == KeyCode::Backspace
                        && question_input_focus.get().is_choices() =>
                {
                    let snapshot = snapshot_current_answer(
                        pending,
                        &question_answer.read(),
                        question_selected.get(),
                        &question_multi_checked.read(),
                    );
                    Some(snapshot)
                }
                _ => None,
            }
        };
        if let Some(snapshot) = step_back {
            let outcome = pending_user_question
                .write()
                .take()
                .and_then(|pending| pending.go_back(snapshot));
            if let Some(StepNavOutcome::Jumped(pending)) = outcome {
                apply_step_nav_outcome(
                    StepNavOutcome::Jumped(pending),
                    &mut pending_user_question,
                    &mut question_selected,
                    &mut question_confirm_focus,
                    &mut question_answer,
                    &mut question_multi_checked,
                    &mut question_input_focus,
                    &mut activity_label,
                    &mut question_validation_error,
                );
            }
            return;
        }

        let optional_skip = {
            let pending_ref = pending_user_question.read();
            match pending_ref.as_ref() {
                Some(pending)
                    if !pending.is_required()
                        && !pending.is_confirm()
                        && modifiers.is_empty()
                        && code == KeyCode::Esc =>
                {
                    Some(())
                }
                _ => None,
            }
        };
        if optional_skip.is_some() {
            let outcome = pending_user_question
                .write()
                .take()
                .map(|pending| pending.respond(String::new()));
            if let Some(outcome) = outcome
                && let Some(summary) = apply_step_submit_outcome(
                    outcome,
                    &mut pending_user_question,
                    &mut question_selected,
                    &mut question_confirm_focus,
                    &mut question_answer,
                    &mut question_multi_checked,
                    &mut question_input_focus,
                    &mut shell_focus,
                    &mut activity_label,
                    &mut question_validation_error,
                )
            {
                push_transcript_message_synced(
                    &mut messages,
                    messages_arc,
                    &mut messages_revision,
                    &mut prompt_history,
                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                );
            }
            return;
        }

        let approval_choice = {
            let user_question_active = pending_user_question.read().is_some();
            if pending_tool_approval.read().is_some() && !user_question_active {
                if modifiers.is_empty() && code == KeyCode::Esc {
                    Some(ToolApprovalChoice::Reject)
                } else {
                    pick_tool_approval_index_from_key(modifiers, code)
                        .and_then(choice_at_index)
                        .or_else(|| {
                            (modifiers.is_empty() && code == KeyCode::Enter)
                                .then(|| choice_at_index(approval_selected.get()))
                                .flatten()
                        })
                }
            } else {
                None
            }
        };
        if let Some(choice) = approval_choice {
            if let Some(pending) = pending_tool_approval.write().take() {
                let key = pending.transcript_key();
                let verb = tool_display_verb(&pending.tool_name);
                let (style, detail) = match choice {
                    ToolApprovalChoice::Approve => (TranscriptStyle::StatusSuccess, format!("{verb} · allowed once")),
                    ToolApprovalChoice::AllowSession => {
                        (TranscriptStyle::StatusSuccess, format!("{verb} · allowed session"))
                    }
                    ToolApprovalChoice::AllowAllTools => {
                        (TranscriptStyle::StatusSuccess, "all tools · session".to_string())
                    }
                    ToolApprovalChoice::Reject => (TranscriptStyle::StatusFailed, format!("{verb} · denied")),
                };
                {
                    let mut msgs = messages.write();
                    if let Some(row) = msgs.iter_mut().find(|m| m.startup_key.as_deref() == Some(key.as_str())) {
                        row.content = "Tool approval".to_string();
                        row.status_detail = Some(detail);
                        row.style = style;
                    }
                }
                messages_revision.set(messages_revision.get().wrapping_add(1));
                pending.respond(choice);
            }
            shell_focus.set(ShellFocus::Prompt);
            activity_label.set(match choice {
                ToolApprovalChoice::Approve => "Running approved tool…".to_string(),
                ToolApprovalChoice::AllowSession => "Running tool (session allow)…".to_string(),
                ToolApprovalChoice::AllowAllTools => "Running tool (all tools allowed this session)…".to_string(),
                ToolApprovalChoice::Reject => "Tool denied".to_string(),
            });
            return;
        }

        // ── Mode Change approval (approve/deny) ──────────────────────
        let mode_change_choice = {
            let user_question_active = pending_user_question.read().is_some();
            if pending_mode_change.read().is_some() && !user_question_active {
                if modifiers.is_empty() && code == KeyCode::Esc {
                    Some(false)
                } else {
                    pick_mode_change_index_from_key(modifiers, code)
                        .or_else(|| {
                            (modifiers.is_empty() && code == KeyCode::Enter)
                                .then(|| match approval_selected.get() {
                                    0 => Some(0),
                                    _ => Some(1),
                                })
                                .flatten()
                        })
                        .map(|idx| idx == 0)
                }
            } else {
                None
            }
        };
        if let Some(approved) = mode_change_choice {
            if let Some(pending) = pending_mode_change.write().take() {
                let mode = crate::agent::agent_mode_from_setting(&pending.target_mode);
                let mode_label = pending.target_mode.to_ascii_uppercase();
                // Apply mode before responding (fixes race: policy must be updated
                // before the agent continues its turn).
                agent_mode.set(mode);
                // Update the transcript status row.
                {
                    let mut msgs = messages.write();
                    let key = "mode-change:pending";
                    let (style, detail) = if approved {
                        (TranscriptStyle::StatusSuccess, format!("Switched to {mode_label}"))
                    } else {
                        (TranscriptStyle::StatusFailed, format!("Stayed in {mode_label}"))
                    };
                    if let Some(row) = msgs.iter_mut().find(|m| m.startup_key.as_deref() == Some(key)) {
                        row.content = "Mode change".to_string();
                        row.status_detail = Some(detail);
                        row.style = style;
                    }
                }
                messages_revision.set(messages_revision.get().wrapping_add(1));
                if let Some(session) = agent_session.as_ref() {
                    // Eagerly invalidate cache and set mode_state so
                    // the agent's next turn and /system-prompt reflect
                    // the new mode before the background task completes.
                    session.invalidate_system_prompt_cache();
                    session.try_set_mode_sync(mode);
                    let session = session.clone();
                    let mode_for_session = mode;
                    let pending_for_response = pending;
                    tokio::spawn(async move {
                        if let Err(err) = session.set_agent_mode(mode_for_session).await {
                            log::warn!("mode change failed: {err}");
                        }
                        // Respond AFTER mode is applied so the policy is up-to-date.
                        pending_for_response.respond(approved);
                    });
                } else {
                    pending.respond(approved);
                }
                activity_label.set(match approved {
                    true => format!("Switched to {}", mode.label()),
                    false => format!("Stayed in {}", agent_mode.get().label()),
                });
            }
            shell_focus.set(ShellFocus::Prompt);
            return;
        }

        // ── Plan Confirmation ──────────────────────────────────────
        let plan_choice = {
            if pending_plan_confirmation.read().is_some() {
                if modifiers.is_empty() && code == KeyCode::Esc {
                    Some(PlanChoice::StayInPlan)
                } else if let Some(idx) = pick_plan_confirmation_index_from_key(modifiers, code) {
                    plan_choice_at_index(idx)
                } else if modifiers.is_empty() && code == KeyCode::Enter {
                    plan_choice_at_index(approval_selected.get())
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(choice) = plan_choice {
            if let Some(pending) = pending_plan_confirmation.write().take() {
                let key = plan_confirmation_transcript_key();

                // Revise: clear pending plan and let the user type revision feedback.
                if choice == PlanChoice::RevisePlan {
                    // Clear the harness's pending plan so the agent can propose a new one.
                    if let Some(session) = pending.session.as_ref() {
                        let session = session.clone();
                        tokio::spawn(async move {
                            if let Err(err) = session.clear_pending_plan().await {
                                log::error!("clear pending plan failed: {err}");
                            }
                        });
                    }
                    // Update transcript row to show cancelled.
                    {
                        let mut msgs = messages.write();
                        if let Some(row) = msgs.iter_mut().find(|m| m.startup_key.as_deref() == Some(key.as_str())) {
                            row.content = "Plan confirmation".to_string();
                            row.status_detail = Some("Revising plan…".to_string());
                            row.style = TranscriptStyle::StatusFailed;
                        }
                    }
                    messages_revision.set(messages_revision.get().wrapping_add(1));
                    activity_label.set("Revised plan requested".to_string());
                    shell_focus.set(ShellFocus::Prompt);
                    return;
                }

                let (style, detail) = match choice {
                    PlanChoice::Implement => (
                        TranscriptStyle::StatusSuccess,
                        "Switched to Build — implementing plan…".to_string(),
                    ),
                    PlanChoice::ImplementFresh => (
                        TranscriptStyle::StatusSuccess,
                        "Switched to Build — implementing plan (fresh context)…".to_string(),
                    ),
                    PlanChoice::StayInPlan => (TranscriptStyle::StatusFailed, "Stayed in Plan mode".to_string()),
                    PlanChoice::RevisePlan => unreachable!(), // handled above
                };
                // Update transcript status row.
                {
                    let mut msgs = messages.write();
                    if let Some(row) = msgs.iter_mut().find(|m| m.startup_key.as_deref() == Some(key.as_str())) {
                        row.content = "Plan confirmation".to_string();
                        row.status_detail = Some(detail);
                        row.style = style;
                    }
                }
                messages_revision.set(messages_revision.get().wrapping_add(1));
                // Sync TUI mode state BEFORE spawning the async resolve — the session's
                // internal mode change (Build) doesn't emit an event back to the TUI.
                // The plan confirmation dialog IS the user's approval; no second dialog needed.
                if matches!(choice, PlanChoice::Implement | PlanChoice::ImplementFresh) {
                    agent_mode.set(AgentMode::Build);
                    // Eagerly invalidate cache and set mode_state so
                    // /system-prompt and the next turn see the new mode
                    // before the background resolve task completes.
                    if let Some(session) = pending.session.as_ref() {
                        session.invalidate_system_prompt_cache();
                        session.try_set_mode_sync(AgentMode::Build);
                    }
                    // Show ephemeral banner about the mode switch.
                    let expire_tx = ephemeral_expire.read().tx.clone();
                    show_ephemeral_banner(
                        &mut ephemeral_banner,
                        &mut ephemeral_banner_generation,
                        &expire_tx,
                        EphemeralBanner {
                            key: "plan-implement",
                            text: "Switched to Build — implementing the approved plan.".to_string(),
                            kind: EphemeralBannerKind::Notice,
                            expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
                        },
                    );
                }

                // Auto-update plan frontmatter: Status → in_progress when user picks Implement.
                // Track the active plan path so RunCompleted can transition to completed.
                if matches!(choice, PlanChoice::Implement | PlanChoice::ImplementFresh)
                    && let Some(ref plan_path) = pending.plan_file
                {
                    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
                    if let Err(err) =
                        crate::agent::plan_files::update_plan_frontmatter(plan_path, "in_progress", &now, None)
                    {
                        log::error!("Failed to update plan frontmatter: {err}");
                    }
                    active_plan_file.write().clone_from(&pending.plan_file);
                }

                // Resolve via session (triggers mode change + implement prompt).
                if let Some(session) = pending.session.as_ref() {
                    let session = session.clone();
                    let plan_file = pending.plan_file.clone();
                    let harness_choice = to_harness_choice(choice);
                    tokio::spawn(async move {
                        if let Some(choice) = harness_choice
                            && let Err(err) = session.resolve_plan_with_file(choice, plan_file).await
                        {
                            log::error!("plan confirmation failed: {err}");
                        }
                    });
                }
                activity_label.set(match choice {
                    PlanChoice::Implement => "Switched to Build — implementing plan…".to_string(),
                    PlanChoice::ImplementFresh => "Switched to Build — implementing plan (fresh)…".to_string(),
                    PlanChoice::StayInPlan => "Stayed in Plan mode".to_string(),
                    PlanChoice::RevisePlan => unreachable!(),
                });
            }
            shell_focus.set(ShellFocus::Prompt);
            return;
        }

        // ── Memory flush confirmation ──────────────────────────────
        let memory_flush_choice = {
            let user_question_active = pending_user_question.read().is_some();
            if pending_memory_flush.read().is_some() && !user_question_active {
                if modifiers.is_empty() && code == KeyCode::Esc {
                    Some(false)
                } else {
                    pick_memory_flush_index_from_key(modifiers, code)
                        .or_else(|| {
                            (modifiers.is_empty() && code == KeyCode::Enter)
                                .then(|| match approval_selected.get() {
                                    0 => Some(0),
                                    _ => Some(1),
                                })
                                .flatten()
                        })
                        .map(|idx| idx == 0)
                }
            } else {
                None
            }
        };
        if let Some(confirmed) = memory_flush_choice {
            let _ = pending_memory_flush.write().take();
            shell_focus.set(ShellFocus::Prompt);
            if confirmed {
                let paths = paths.clone();
                let ui_tx = agent_session.as_ref().map(|s| s.ui_event_sender());
                activity_label.set("Flushing memory store…".to_string());
                if let Some(tx) = ui_tx {
                    tokio::spawn(async move {
                        let output = match crate::memory::execute_flush(&paths).await {
                            Ok(text) => text,
                            Err(err) => format!("Memory error: {err}"),
                        };
                        let _ = tx.send(crate::agent::AgentUiEvent::MemoryResult(output));
                    });
                } else {
                    // No session UI channel — run inline and open result dialog.
                    match elph_agent::try_block_on(crate::memory::execute_flush(&paths)) {
                        Ok(Ok(text)) => {
                            let body_height = (text.lines().count() as u16).saturating_add(3).clamp(8, 40);
                            open_scroll_text_dialog(OpenScrollTextDialogArgs {
                                pending: &mut pending_system_prompt,
                                shell_focus: &mut shell_focus,
                                title: "Memory".to_string(),
                                text,
                                width_pct: 80,
                                body_height: Some(body_height),
                                show_copy: false,
                            });
                        }
                        Ok(Err(err)) => {
                            activity_label.set(format!("Memory error: {err}"));
                        }
                        Err(err) => {
                            activity_label.set(format!("Memory error: {err:#}"));
                        }
                    }
                }
            } else {
                activity_label.set("Flush cancelled".to_string());
            }
            return;
        }

        // ── Feedback dialog (Report a Bug / Join Community / Support) ──
        if *pending_feedback.read() {
            if modifiers.is_empty() && code == KeyCode::Esc {
                *pending_feedback.write() = false;
                shell_focus.set(ShellFocus::Prompt);
                return;
            }
            if modifiers.is_empty() && code == KeyCode::Enter {
                let index = approval_selected.get();
                if let Some(url) = feedback_url_at_index(index) {
                    let url = url.to_string();
                    std::thread::spawn(move || {
                        let _ = open_url(&url);
                    });
                }
                *pending_feedback.write() = false;
                shell_focus.set(ShellFocus::Prompt);
                return;
            }
            if let Some(index) = pick_feedback_index_from_key(modifiers, code) {
                approval_selected.set(index);
                if let Some(url) = feedback_url_at_index(index) {
                    let url = url.to_string();
                    std::thread::spawn(move || {
                        let _ = open_url(&url);
                    });
                }
                *pending_feedback.write() = false;
                shell_focus.set(ShellFocus::Prompt);
                return;
            }
        }

        // ── MCP OAuth dialog ───────────────────────────────────────
        if mcp_auth_open {
            use crate::tui::mcp_auth_dialog::{
                McpAuthStep, close_mcp_auth_dialog, count_filtered_mcp_servers, get_filtered_mcp_server_at,
                start_mcp_oauth_for_server,
            };

            if modifiers.is_empty() && code == KeyCode::Esc && kind == KeyEventKind::Press {
                close_mcp_auth_dialog(&mut pending_mcp_auth, &mut draft, &mut live_draft, &mut shell_focus);
                force_editor_clear.set(true);
                return;
            }

            let step = pending_mcp_auth.read().as_ref().map(|p| p.step);
            let filter = provider_connect_filter.read().clone();
            let selected = *provider_connect_selected.read();

            if step == Some(McpAuthStep::SelectServer) {
                if modifiers.is_empty() && kind == KeyEventKind::Press {
                    match code {
                        KeyCode::Up => {
                            let count = pending_mcp_auth
                                .read()
                                .as_ref()
                                .map(|p| count_filtered_mcp_servers(&p.servers, &filter))
                                .unwrap_or(0);
                            if count > 0 {
                                let next = selected.saturating_sub(1);
                                provider_connect_selected.set(next);
                                if let Some(p) = pending_mcp_auth.write().as_mut() {
                                    p.selected = next;
                                }
                            }
                            return;
                        }
                        KeyCode::Down => {
                            let count = pending_mcp_auth
                                .read()
                                .as_ref()
                                .map(|p| count_filtered_mcp_servers(&p.servers, &filter))
                                .unwrap_or(0);
                            if count > 0 {
                                let next = (selected + 1).min(count - 1);
                                provider_connect_selected.set(next);
                                if let Some(p) = pending_mcp_auth.write().as_mut() {
                                    p.selected = next;
                                }
                            }
                            return;
                        }
                        KeyCode::Enter => {
                            // Suppress accidental Enter from slash submit.
                            if pending_mcp_auth
                                .read()
                                .as_ref()
                                .is_some_and(|p| p.opened_at.elapsed().as_millis() < 200)
                            {
                                return;
                            }
                            let server_name = pending_mcp_auth.read().as_ref().and_then(|p| {
                                get_filtered_mcp_server_at(&p.servers, &filter, selected).map(|s| s.name.clone())
                            });
                            if let Some(name) = server_name
                                && let Err(err) = start_mcp_oauth_for_server(pending_mcp_auth, &paths, &name)
                                && let Some(p) = pending_mcp_auth.write().as_mut()
                            {
                                p.step = McpAuthStep::Failed;
                                p.status_message = err;
                            }
                            return;
                        }
                        KeyCode::Backspace => {
                            let mut f = filter;
                            f.pop();
                            provider_connect_filter.set(f.clone());
                            provider_connect_selected.set(0);
                            if let Some(p) = pending_mcp_auth.write().as_mut() {
                                p.filter = f;
                                p.selected = 0;
                            }
                            return;
                        }
                        KeyCode::Char(c) if !c.is_control() => {
                            let mut f = filter;
                            f.push(c);
                            provider_connect_filter.set(f.clone());
                            provider_connect_selected.set(0);
                            if let Some(p) = pending_mcp_auth.write().as_mut() {
                                p.filter = f;
                                p.selected = 0;
                            }
                            return;
                        }
                        _ => {}
                    }
                }
                return;
            }

            if step == Some(McpAuthStep::Failed)
                && modifiers.is_empty()
                && code == KeyCode::Enter
                && kind == KeyEventKind::Press
            {
                // Retry: back to list.
                if let Some(p) = pending_mcp_auth.write().as_mut() {
                    p.step = McpAuthStep::SelectServer;
                    p.status_message.clear();
                }
                return;
            }

            // WaitingBrowser: ignore keys except Esc (handled above).
            return;
        }

        // ── Provider connect dialog ────────────────────────────────
        let step = {
            let pending_ref = pending_provider_connect.read();
            pending_ref.as_ref().map(|p| p.step)
        };
        let is_select_auth_method = step == Some(ProviderConnectStep::SelectAuthMethod);
        let is_select_provider = step == Some(ProviderConnectStep::SelectProvider);
        let is_oauth_device_code = step == Some(ProviderConnectStep::OAuthDeviceCode);
        let is_oauth_select = step == Some(ProviderConnectStep::OAuthSelect);

        if provider_connect_open {
            // ── OAuth completed: close dialog, clear draft ─────
            let oauth_done = pending_provider_connect
                .read()
                .as_ref()
                .map(|p| p.done)
                .unwrap_or(false);
            if oauth_done {
                close_provider_connect_dialog(
                    &mut pending_provider_connect,
                    &mut provider_connect_selected,
                    &mut provider_connect_filter,
                    &mut provider_connect_api_key,
                    &mut provider_connect_input_focus,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    false,
                );
                force_editor_clear.set(true);
                return;
            }

            let _is_enter_api_key = step == Some(ProviderConnectStep::EnterApiKey);

            // ── Esc ──────────────────────────────────────────────
            if modifiers.is_empty() && code == KeyCode::Esc {
                // Esc always closes the dialog from any step
                close_provider_connect_dialog(
                    &mut pending_provider_connect,
                    &mut provider_connect_selected,
                    &mut provider_connect_filter,
                    &mut provider_connect_api_key,
                    &mut provider_connect_input_focus,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    false,
                );
                force_editor_clear.set(true);
                return;
            }

            // ── OAuth Select step: navigate with ↑↓ and confirm with Enter ──
            if is_oauth_select && modifiers.is_empty() && kind == KeyEventKind::Press {
                if code == KeyCode::Enter {
                    // Submit the selected option back to the OAuth flow
                    let pending_state = pending_provider_connect.read();
                    let (selected_id, selected_index) = pending_state
                        .as_ref()
                        .map(|p| (p.oauth_select_ids.get(p.oauth_select_index).cloned(), p.oauth_select_index))
                        .unwrap_or((None, 0));
                    drop(pending_state);

                    if let Some(selected_id) = selected_id {
                        log::info!("OAuth select: {} (index {})", selected_id, selected_index);
                        let mut store = OAUTH_PROMPT_STORE.lock().unwrap();
                        if let Some(prompt_id) = store.keys().next().cloned()
                            && let Some(tx) = store.remove(&prompt_id)
                        {
                            let _ = tx.send(selected_id);
                        }
                    }
                    return;
                }
                if let Some(delta) = provider_list_nav_delta(modifiers, code) {
                    let mut pending_ref = pending_provider_connect.write();
                    if let Some(pending) = pending_ref.as_mut() {
                        let count = pending.oauth_select_ids.len();
                        if count > 0 {
                            let next =
                                (pending.oauth_select_index as isize + delta).clamp(0, count as isize - 1) as usize;
                            pending.oauth_select_index = next;
                            provider_connect_selected.set(next);
                        }
                    }
                    return;
                }
            }

            // ── Enter in OAuth device code step: submit prompt response ──
            // Must be checked BEFORE the main Enter handler to avoid conflicts.
            if is_oauth_device_code && modifiers.is_empty() && code == KeyCode::Enter && kind == KeyEventKind::Press {
                let is_prompt = pending_provider_connect
                    .read()
                    .as_ref()
                    .map(|p| p.oauth_is_prompt)
                    .unwrap_or(false);
                let response = pending_provider_connect
                    .read()
                    .as_ref()
                    .map(|p| p.oauth_code.clone())
                    .unwrap_or_default();

                // Allow empty submit in prompt mode (e.g. blank means github.com)
                if !response.is_empty() || (is_prompt && !OAUTH_PROMPT_STORE.lock().unwrap().is_empty()) {
                    log::info!("OAuth prompt response submitted: {}", response);
                    let mut store = OAUTH_PROMPT_STORE.lock().unwrap();
                    if let Some(prompt_id) = store.keys().next().cloned()
                        && let Some(tx) = store.remove(&prompt_id)
                    {
                        let _ = tx.send(response);
                    }
                    if let Some(ref mut pending) = *pending_provider_connect.write() {
                        pending.oauth_code.clear();
                    }
                }
                return;
            }

            // ── Text input in OAuth device code step (prompt mode only) ──
            if is_oauth_device_code
                && let Some(ref mut pending) = *pending_provider_connect.write()
                && pending.oauth_is_prompt
            {
                if modifiers.is_empty() && code == KeyCode::Backspace && kind == KeyEventKind::Press {
                    pending.oauth_code.pop();
                    return;
                }
                if modifiers.is_empty()
                    && let KeyCode::Char(c) = code
                    && kind == KeyEventKind::Press
                {
                    pending.oauth_code.push(c);
                    return;
                }
            }

            // ── Enter (confirm) ──────────────────────────────────
            if modifiers.is_empty() && code == KeyCode::Enter {
                if kind != KeyEventKind::Press {
                    return;
                }
                if pending_provider_connect
                    .read()
                    .as_ref()
                    .map(|pending| pending.fresh_open)
                    .unwrap_or(false)
                {
                    if let Some(ref mut pending) = *pending_provider_connect.write() {
                        pending.fresh_open = false;
                    }
                    return;
                }
                if is_select_auth_method {
                    // Confirm authentication method selection
                    let auth_methods = crate::tui::provider_connect_dialog::get_auth_methods();
                    let selected_idx = *provider_connect_selected.read();
                    if auth_methods.get(selected_idx).is_some() {
                        // Transition to provider selection
                        if let Some(ref mut pending) = *pending_provider_connect.write() {
                            pending.step = ProviderConnectStep::SelectProvider;
                            pending.selected_auth_method = selected_idx;
                            pending.selected_provider = 0;
                            pending.filter.clear();
                            pending.api_key_input.clear();
                            pending.oauth_code.clear();
                            pending.oauth_url.clear();
                            pending.oauth_provider_name.clear();
                            pending.done = false;
                            pending.input_focus = ProviderConnectFocus::Search;
                            pending.fresh_open = false;
                            provider_connect_input_focus.set(ProviderConnectFocus::Search);
                            provider_connect_selected.set(0);
                            provider_connect_filter.set(String::new());
                            provider_connect_api_key.set(String::new());
                        }
                    }
                    return;
                }

                if is_select_provider {
                    // Only confirm when focus is on the list, not the search field
                    let focus = pending_provider_connect
                        .read()
                        .as_ref()
                        .map(|p| p.input_focus)
                        .unwrap_or(ProviderConnectFocus::List);
                    if !provider_confirm_on_enter(focus) {
                        // Enter on search: move focus to list
                        if let Some(ref mut pending) = *pending_provider_connect.write() {
                            focus_provider_list(&mut provider_connect_input_focus, pending);
                        }
                        return;
                    }
                    let selected_idx = *provider_connect_selected.read();
                    let current_filter = provider_connect_filter.read().clone();
                    let auth_method_idx = pending_provider_connect
                        .read()
                        .as_ref()
                        .map(|p| p.selected_auth_method)
                        .unwrap_or(0);
                    let auth_method = provider_auth_method_from_index(auth_method_idx);
                    let providers = get_provider_options_for_auth_method(auth_method);
                    if let Some(provider) = crate::tui::provider_connect_dialog::get_filtered_provider_at(
                        &providers,
                        &current_filter,
                        selected_idx,
                    ) {
                        let is_oauth_method = matches!(
                            provider_auth_method_from_index(auth_method_idx),
                            crate::tui::provider_connect_dialog::ProviderAuthMethod::Account
                        );

                        if provider.supports_oauth && provider_supports_oauth(&provider.id) && is_oauth_method {
                            // OAuth — trigger OAuth flow
                            let provider_id = provider.id.clone();
                            let provider_name = format_provider_name(&provider_id);
                            let provider_name_for_clone = provider_name.clone();
                            let auth_store_path = paths.auth_store_path();

                            log::info!("Starting OAuth flow for provider: {}", provider_id);

                            // Transition to OAuth device code step
                            if let Some(ref mut pending) = *pending_provider_connect.write() {
                                pending.step = ProviderConnectStep::OAuthDeviceCode;
                                pending.fresh_open = false;
                                pending.oauth_provider_name =
                                    crate::tui::provider_connect_dialog::format_provider_name(&provider_id);
                                pending.oauth_url = String::new();
                                pending.oauth_code = String::new();
                                pending.input_focus = ProviderConnectFocus::OAuthCodeInput;
                                provider_connect_input_focus.set(ProviderConnectFocus::OAuthCodeInput);
                            }

                            // Channel to push OAuth events back to the dialog state
                            let (oauth_event_tx, mut oauth_event_rx) =
                                tokio::sync::mpsc::unbounded_channel::<OAuthDialogEvent>();

                            // Store the sender so the spawned task can notify us
                            let provider_id_for_task = provider_id.clone();
                            let mut pending_ref = pending_provider_connect;
                            let auth_store_path_for_task = auth_store_path.clone();
                            // Inject into the live session models store after save (no restart).
                            let session_for_inject = agent_session.clone();

                            tokio::spawn(async move {
                                // Build AuthLoginCallbacks that sends events through the channel
                                let callbacks = Arc::new(OAuthLoginCallbacksImpl { tx: oauth_event_tx });

                                match elph_ai::oauth_provider_login(&provider_id_for_task, callbacks).await {
                                    Ok(credential) => {
                                        log::info!("OAuth login succeeded for provider: {}", provider_id_for_task);

                                        // Prefer refreshed credential from get_oauth_api_key when needed.
                                        let credential = match elph_ai::get_oauth_api_key(
                                            &provider_id_for_task,
                                            credential.clone(),
                                        )
                                        .await
                                        {
                                            Ok(api_key_result) => {
                                                log::info!(
                                                    "OAuth login complete for {} — token expires at {}",
                                                    provider_id_for_task,
                                                    api_key_result.new_credentials.expires,
                                                );
                                                api_key_result.new_credentials
                                            }
                                            Err(e) => {
                                                log::warn!(
                                                    "get_oauth_api_key for {provider_id_for_task} failed ({e}); using login credential"
                                                );
                                                credential
                                            }
                                        };

                                        // Persist OAuth JSON blob in auth.json.
                                        let save_ok = match serde_json::to_string(&credential) {
                                            Ok(json) => {
                                                match crate::tui::provider_credential_store::save_provider_credential(
                                                    &auth_store_path_for_task,
                                                    &provider_id_for_task,
                                                    &json,
                                                )
                                                .await
                                                {
                                                    Ok(()) => {
                                                        log::info!(
                                                            "saved OAuth credential for {provider_id_for_task} to auth.json"
                                                        );
                                                        true
                                                    }
                                                    Err(e) => {
                                                        log::error!(
                                                            "failed to save OAuth credential for {provider_id_for_task}: {e}"
                                                        );
                                                        false
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                log::error!("serialize OAuth credential: {e}");
                                                false
                                            }
                                        };

                                        // Always inject into the live Models store so the current
                                        // session can stream without restart.
                                        if let Some(session) = session_for_inject.as_ref() {
                                            session
                                                .inject_provider_credential(
                                                    &provider_id_for_task,
                                                    elph_ai::Credential::OAuth(credential.clone()),
                                                )
                                                .await;
                                        } else if save_ok {
                                            // No live session — disk save is enough for next boot.
                                        }

                                        if let Some(pending) = pending_ref.write().as_mut() {
                                            pending.done = true;
                                            pending.completed_provider_id = Some(provider_id_for_task.clone());
                                            pending.oauth_url = if save_ok {
                                                format!("Signed in to {provider_name_for_clone}")
                                            } else {
                                                format!(
                                                    "Signed in to {provider_name_for_clone} (live only; auth.json save failed)"
                                                )
                                            };
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("OAuth login failed for {}: {}", provider_id_for_task, e);
                                        if let Some(pending) = pending_ref.write().as_mut() {
                                            pending.oauth_url = format!("OAuth failed: {e}");
                                            pending.oauth_is_prompt = false;
                                        }
                                    }
                                }
                            });

                            // Process any events that arrived before the spawn
                            while let Ok(event) = oauth_event_rx.try_recv() {
                                if let Some(ref mut pending) = *pending_provider_connect.write() {
                                    match event {
                                        OAuthDialogEvent::DeviceCode { url, code } => {
                                            pending.oauth_url = url;
                                            pending.oauth_code = code;
                                            pending.oauth_is_prompt = false;
                                            pending.step = ProviderConnectStep::OAuthDeviceCode;
                                            pending.input_focus = ProviderConnectFocus::OAuthCodeInput;
                                            provider_connect_input_focus.set(ProviderConnectFocus::OAuthCodeInput);
                                        }
                                        OAuthDialogEvent::PromptText {
                                            id: _,
                                            message,
                                            placeholder: _,
                                        } => {
                                            pending.oauth_code = String::new();
                                            pending.oauth_prompt_message = message.clone();
                                            pending.oauth_is_prompt = true;
                                            pending.step = ProviderConnectStep::OAuthDeviceCode;
                                            pending.input_focus = ProviderConnectFocus::OAuthCodeInput;
                                            provider_connect_input_focus.set(ProviderConnectFocus::OAuthCodeInput);
                                        }
                                        OAuthDialogEvent::PromptManualCode {
                                            id: _,
                                            message,
                                            placeholder,
                                        } => {
                                            pending.oauth_code = placeholder.clone().unwrap_or_default();
                                            pending.oauth_provider_name = format!("{provider_name} - {message}");
                                            pending.oauth_is_prompt = true;
                                            pending.step = ProviderConnectStep::OAuthDeviceCode;
                                            pending.input_focus = ProviderConnectFocus::OAuthCodeInput;
                                            provider_connect_input_focus.set(ProviderConnectFocus::OAuthCodeInput);
                                        }
                                        OAuthDialogEvent::PromptSelect {
                                            id: _,
                                            message,
                                            options,
                                        } => {
                                            pending.oauth_url = message.clone();
                                            pending.oauth_code = String::new();
                                            pending.oauth_provider_name = format!("{provider_name} - {message}");
                                            // Store options as selectable labels
                                            pending.oauth_select_labels =
                                                options.iter().map(|o| o.label.clone()).collect();
                                            pending.oauth_select_ids = options.iter().map(|o| o.id.clone()).collect();
                                            pending.oauth_select_index = 0;
                                            pending.step = ProviderConnectStep::OAuthSelect;
                                            pending.input_focus = ProviderConnectFocus::OAuthSelectList;
                                            provider_connect_input_focus.set(ProviderConnectFocus::OAuthSelectList);
                                        }
                                    }
                                }
                            }

                            // Read incoming OAuth events in a background task
                            // to keep the dialog updated
                            let mut pending_ref = pending_provider_connect;
                            let provider_name_for_task = provider_name.clone();
                            tokio::spawn(async move {
                                while let Some(event) = oauth_event_rx.recv().await {
                                    if let Some(pending) = pending_ref.write().as_mut() {
                                        match event {
                                            OAuthDialogEvent::DeviceCode { url, code } => {
                                                pending.oauth_url = url;
                                                pending.oauth_code = code;
                                                pending.oauth_is_prompt = false;
                                                pending.step = ProviderConnectStep::OAuthDeviceCode;
                                                pending.input_focus = ProviderConnectFocus::OAuthCodeInput;
                                            }
                                            OAuthDialogEvent::PromptText {
                                                id: _,
                                                message,
                                                placeholder: _,
                                            } => {
                                                pending.oauth_code = String::new();
                                                pending.oauth_prompt_message = message.clone();
                                                pending.oauth_is_prompt = true;
                                                pending.step = ProviderConnectStep::OAuthDeviceCode;
                                                pending.input_focus = ProviderConnectFocus::OAuthCodeInput;
                                            }
                                            OAuthDialogEvent::PromptManualCode {
                                                id: _,
                                                message,
                                                placeholder,
                                            } => {
                                                pending.oauth_code = placeholder.clone().unwrap_or_default();
                                                pending.oauth_provider_name =
                                                    format!("{provider_name_for_task} - {message}");
                                                pending.oauth_is_prompt = true;
                                                pending.step = ProviderConnectStep::OAuthDeviceCode;
                                                pending.input_focus = ProviderConnectFocus::OAuthCodeInput;
                                            }
                                            OAuthDialogEvent::PromptSelect {
                                                id: _,
                                                message,
                                                options,
                                            } => {
                                                pending.oauth_url = message.clone();
                                                pending.oauth_code = String::new();
                                                pending.oauth_provider_name =
                                                    format!("{provider_name_for_task} - {message}");
                                                pending.oauth_select_labels =
                                                    options.iter().map(|o| o.label.clone()).collect();
                                                pending.oauth_select_ids =
                                                    options.iter().map(|o| o.id.clone()).collect();
                                                pending.oauth_select_index = 0;
                                                pending.step = ProviderConnectStep::OAuthSelect;
                                                pending.input_focus = ProviderConnectFocus::OAuthSelectList;
                                            }
                                        }
                                    }
                                }
                            });
                        } else {
                            // API Key: close provider selection, open dedicated API key dialog
                            let provider_id = provider.id.clone();
                            let provider_name = format_provider_name(&provider.id);
                            close_provider_connect_dialog(
                                &mut pending_provider_connect,
                                &mut provider_connect_selected,
                                &mut provider_connect_filter,
                                &mut provider_connect_api_key,
                                &mut provider_connect_input_focus,
                                &mut draft,
                                &mut live_draft,
                                &mut shell_focus,
                                false,
                            );
                            open_provider_api_key_dialog(OpenProviderApiKeyDialogArgs {
                                pending: &mut pending_provider_api_key,
                                api_key_input: &mut provider_connect_api_key,
                                draft: &mut draft,
                                live_draft: &mut live_draft,
                                shell_focus: &mut shell_focus,
                                provider_id,
                                provider_name,
                            });
                        }
                    }
                }
                return;
            }
        }

        // ── Provider disconnect dialog ─────────────────────────
        if provider_disconnect_open {
            let opened_at = pending_provider_disconnect
                .read()
                .as_ref()
                .map(|p| p.opened_at)
                .unwrap_or(Instant::now());
            // Guard: suppress Enter that leaks from the slash-submit keystroke
            let enter_ok = kind == KeyEventKind::Press && opened_at.elapsed() > Duration::from_millis(200);

            let has_any = pending_provider_disconnect
                .read()
                .as_ref()
                .map(|p| !p.provider_ids.is_empty())
                .unwrap_or(false);

            if modifiers.is_empty() && code == KeyCode::Esc {
                close_provider_disconnect_dialog(
                    &mut pending_provider_disconnect,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    false,
                );
                force_editor_clear.set(true);
                return;
            }

            if !has_any {
                return;
            }

            // List navigation (↑/↓)
            if let Some(delta) = provider_list_nav_delta(modifiers, code) {
                let pending_ref = &mut *pending_provider_disconnect.write();
                if let Some(pending) = pending_ref {
                    let count = pending.provider_ids.len();
                    if count > 0 {
                        pending.selected_index =
                            ((pending.selected_index as isize + delta).rem_euclid(count as isize)) as usize;
                        provider_disconnect_selected.set(pending.selected_index);
                    }
                }
                return;
            }

            // Enter: delete selected credential
            if modifiers.is_empty() && code == KeyCode::Enter && enter_ok {
                let (provider_id, index) = {
                    let pending = pending_provider_disconnect.read();
                    let pending = pending.as_ref();
                    let id = pending.and_then(|p| p.provider_ids.get(p.selected_index).cloned());
                    let ix = pending.map(|p| p.selected_index).unwrap_or(0);
                    (id, ix)
                };

                if let Some(pid) = provider_id {
                    let auth_store_path = paths.auth_store_path();
                    let pid_for_task = pid.clone();
                    tokio::spawn(async move {
                        match crate::tui::provider_credential_store::delete_provider_credential(
                            &auth_store_path,
                            &pid_for_task,
                        )
                        .await
                        {
                            Ok(true) => {
                                log::info!("Removed credentials for provider: {}", pid_for_task);
                            }
                            Ok(false) => {
                                log::info!("No credentials to remove for provider: {}", pid_for_task);
                            }
                            Err(e) => {
                                log::error!("Failed to remove credentials for {}: {}", pid_for_task, e);
                            }
                        }
                    });

                    // Push transcript notification immediately
                    let provider_name = format_provider_name(&pid);
                    push_transcript_message_synced(
                        &mut messages,
                        messages_arc,
                        &mut messages_revision,
                        &mut prompt_history,
                        TranscriptMessage::text(format!("Signed out from {provider_name}"), TranscriptStyle::Meta),
                    );

                    // Remove from list and adjust selection
                    if let Some(pending) = pending_provider_disconnect.write().as_mut() {
                        pending.provider_ids.remove(index);
                        if !pending.provider_ids.is_empty() {
                            pending.selected_index =
                                pending.selected_index.min(pending.provider_ids.len().saturating_sub(1));
                            provider_disconnect_selected.set(pending.selected_index);
                        } else {
                            pending.selected_index = 0;
                            pending.done = true;
                        }
                    }
                }
                return;
            }
        }

        // ── Ctrl+O: open OAuth URL in browser ──────────────
        if is_oauth_device_code
            && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META)
            && (modifiers == KeyModifiers::CONTROL && matches!(code, KeyCode::Char('o') | KeyCode::Char('O')))
        {
            let url = pending_provider_connect
                .read()
                .as_ref()
                .map(|p| p.oauth_url.clone())
                .unwrap_or_default();
            if !url.is_empty() {
                std::thread::spawn(move || {
                    let _ = open_url(&url);
                });
            }
            return;
        }

        // ── Auth method selection: ↑/↓ only ──────────────────
        if is_select_auth_method && let Some(delta) = provider_list_nav_delta(modifiers, code) {
            let count = crate::tui::provider_connect_dialog::get_auth_methods().len();
            let pending_ref = &mut *pending_provider_connect.write();
            if let Some(pending) = pending_ref
                && count > 0
            {
                let new_idx = ((pending.selected_auth_method as isize + delta).rem_euclid(count as isize)) as usize;
                pending.selected_auth_method = new_idx;
                provider_connect_selected.set(new_idx);
            }
            return;
        }

        // ── Provider selection: filter + list navigation ─────
        // Mirrors the model selector: Tab switches focus, ↑/↓ move the highlight,
        // and any printable key typed on the list seeds the filter field.
        if is_select_provider && kind == KeyEventKind::Press {
            let focus = pending_provider_connect
                .read()
                .as_ref()
                .map(|p| p.input_focus)
                .unwrap_or(ProviderConnectFocus::Search);

            if modifiers.is_empty() && code == KeyCode::Tab {
                if let Some(pending) = pending_provider_connect.write().as_mut() {
                    if focus == ProviderConnectFocus::List {
                        focus_provider_search(&mut provider_connect_input_focus, pending);
                    } else {
                        focus_provider_list(&mut provider_connect_input_focus, pending);
                    }
                }
                return;
            }

            // ↑/↓ move the highlight and hand focus to the list so Enter confirms.
            if let Some(delta) = provider_list_nav_delta(modifiers, code) {
                let auth_method_idx = pending_provider_connect
                    .read()
                    .as_ref()
                    .map(|p| p.selected_auth_method)
                    .unwrap_or(0);
                let providers = get_provider_options_for_auth_method(provider_auth_method_from_index(auth_method_idx));
                let count =
                    crate::tui::provider_connect_dialog::count_filtered(&providers, &provider_connect_filter.read());
                if let Some(pending) = pending_provider_connect.write().as_mut() {
                    focus_provider_list(&mut provider_connect_input_focus, pending);
                    if count > 0 {
                        let current = *provider_connect_selected.read();
                        let next = (current as isize + delta).clamp(0, count as isize - 1) as usize;
                        pending.selected_provider = next;
                        provider_connect_selected.set(next);
                    }
                }
                return;
            }

            if focus == ProviderConnectFocus::List {
                // Backspace trims the filter without leaving the list.
                if modifiers.is_empty() && code == KeyCode::Backspace {
                    if let Some(pending) = pending_provider_connect.write().as_mut() {
                        provider_list_backspace(&mut provider_connect_filter, pending);
                        provider_connect_selected.set(pending.selected_provider);
                    }
                    return;
                }

                // Printable keys jump back to the filter field (`/` without inserting).
                if let Some(seed) = provider_filter_seed(modifiers, code) {
                    if let Some(pending) = pending_provider_connect.write().as_mut() {
                        apply_provider_filter_seed(
                            seed,
                            &mut provider_connect_filter,
                            &mut provider_connect_input_focus,
                            pending,
                        );
                        provider_connect_selected.set(pending.selected_provider);
                    }
                    return;
                }
            }

            // Search focus: typing and backspace belong to the Input component (TextInput);
            // the filter State is synced back into the pending dialog on render.
        }

        // ── API key dialog (separate dialog) ─────────────────────
        if provider_api_key_open {
            // Esc
            if modifiers.is_empty() && code == KeyCode::Esc {
                close_provider_api_key_dialog(
                    &mut pending_provider_api_key,
                    &mut provider_connect_api_key,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    false,
                );
                force_editor_clear.set(true);
                return;
            }

            // Enter — save API key
            if modifiers.is_empty() && code == KeyCode::Enter {
                if kind != KeyEventKind::Press {
                    return;
                }
                let api_key = provider_connect_api_key.read().clone();
                let provider_id = pending_provider_api_key.read().as_ref().map(|p| p.provider_id.clone());
                close_provider_api_key_dialog(
                    &mut pending_provider_api_key,
                    &mut provider_connect_api_key,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    false,
                );
                force_editor_clear.set(true);
                if let Some(pid) = provider_id {
                    let auth_store_path = paths.auth_store_path();
                    let api_key_clone = api_key.clone();
                    let session_for_inject = agent_session.clone();
                    tokio::spawn(async move {
                        // Detect env: prefix — store as plaintext reference, not encrypted.
                        let save_result = if let Some(env_var) = api_key_clone.strip_prefix("env:") {
                            crate::tui::provider_credential_store::save_provider_env_ref(
                                &auth_store_path,
                                &pid,
                                env_var,
                            )
                            .await
                            .map(|_| {
                                log::info!("Saved env ref for provider: {pid}");
                                crate::agent::model_registry::credential_from_auth_value(&format!(
                                    "{}{env_var}",
                                    elph_agent::ENV_REF_PREFIX
                                ))
                            })
                        } else {
                            crate::tui::provider_credential_store::save_provider_credential(
                                &auth_store_path,
                                &pid,
                                &api_key_clone,
                            )
                            .await
                            .map(|_| {
                                log::info!("Saved encrypted API key for provider: {pid}");
                                crate::agent::model_registry::credential_from_auth_value(&api_key_clone)
                            })
                        };
                        match save_result {
                            Ok(Some(cred)) => {
                                if let Some(session) = session_for_inject.as_ref() {
                                    session.inject_provider_credential(&pid, cred).await;
                                }
                            }
                            Ok(None) => log::warn!("empty credential for provider {pid}"),
                            Err(e) => log::error!("Failed to save credential for provider {pid}: {e}"),
                        }
                    });
                }
                return;
            }

            // Character typing, backspace, paste: owned by DialogUserInputContent → TextInput
            // (shared `provider_connect_api_key` State). Do not also push/pop here —
            // that double-applied each keystroke (`ad` → `adad`).
            // Let all keystrokes through to the Input component. Only Ctrl+C/Ctrl+D
            // are intercepted by the shell global shortcut handler above.
            return;
        }

        let option_nav = {
            let pending_ref = pending_user_question.read();
            match (pending_ref.as_ref(), question_option_nav_delta(modifiers, code)) {
                (Some(pending), Some(delta)) if pending.options().is_some() && !pending.is_confirm() => {
                    let current = current_choice_index(pending, question_selected.get(), question_input_focus.get());
                    advance_question_selection(pending, current, delta)
                }
                _ => None,
            }
        };
        if let Some((next_index, focus)) = option_nav {
            question_selected.set(next_index);
            question_input_focus.set(focus);
            question_validation_error.set(None);
            return;
        }

        let activate_custom_input = {
            let pending_ref = pending_user_question.read();
            match pending_ref.as_ref() {
                Some(pending)
                    if pending.allow_custom()
                        && question_input_focus.get().is_choices()
                        && is_custom_choice_index(pending, question_selected.get())
                        && modifiers.is_empty()
                        && code == KeyCode::Enter =>
                {
                    Some(())
                }
                _ => None,
            }
        };
        if activate_custom_input.is_some() {
            if let Some(pending) = pending_user_question.read().as_ref()
                && let Some(options) = pending.options()
            {
                question_selected.set(options.len());
            }
            question_input_focus.set(QuestionInputFocus::Custom);
            question_validation_error.set(None);
            return;
        }

        let multi_select_answer = {
            let pending_ref = pending_user_question.read();
            match pending_ref.as_ref() {
                Some(pending)
                    if pending.is_multi_select()
                        && question_input_focus.get().is_choices()
                        && !is_custom_choice_index(pending, question_selected.get())
                        && modifiers.is_empty()
                        && code == KeyCode::Enter =>
                {
                    let text = question_answer.read().clone();
                    try_resolve_submittable_answer(
                        pending,
                        &text,
                        question_selected.get(),
                        &question_multi_checked.read(),
                    )
                    .ok()
                }
                _ => None,
            }
        };
        if let Some(answer) = multi_select_answer {
            let outcome = pending_user_question
                .write()
                .take()
                .map(|pending| pending.respond(answer));
            if let Some(outcome) = outcome
                && let Some(summary) = apply_step_submit_outcome(
                    outcome,
                    &mut pending_user_question,
                    &mut question_selected,
                    &mut question_confirm_focus,
                    &mut question_answer,
                    &mut question_multi_checked,
                    &mut question_input_focus,
                    &mut shell_focus,
                    &mut activity_label,
                    &mut question_validation_error,
                )
            {
                push_transcript_message_synced(
                    &mut messages,
                    messages_arc,
                    &mut messages_revision,
                    &mut prompt_history,
                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                );
            }
            return;
        }
        if let Some(pending) = pending_user_question.read().as_ref()
            && pending.is_multi_select()
            && question_input_focus.get().is_choices()
            && !is_custom_choice_index(pending, question_selected.get())
            && modifiers.is_empty()
            && code == KeyCode::Enter
            && let Err(err) = try_resolve_submittable_answer(
                pending,
                &question_answer.read(),
                question_selected.get(),
                &question_multi_checked.read(),
            )
        {
            question_validation_error.set(Some(err));
            return;
        }

        // ── Confirm step (Yes/No) ───────────────────────────────────
        let confirm_choice = {
            let should_submit = pending_user_question.read().as_ref().is_some_and(|p| {
                p.is_confirm()
                    && question_input_focus.get().is_choices()
                    && modifiers.is_empty()
                    && matches!(
                        code,
                        KeyCode::Char('y')
                            | KeyCode::Char('Y')
                            | KeyCode::Char('n')
                            | KeyCode::Char('N')
                            | KeyCode::Enter
                            | KeyCode::Esc
                    )
            });
            if should_submit {
                let yes = matches!(code, KeyCode::Char('y') | KeyCode::Char('Y'))
                    || (code == KeyCode::Enter && question_selected.get() == 0);
                pending_user_question.write().take().map(|p| p.respond_confirm(yes))
            } else {
                None
            }
        };
        if let Some(outcome) = confirm_choice {
            if let Some(summary) = apply_step_submit_outcome(
                outcome,
                &mut pending_user_question,
                &mut question_selected,
                &mut question_confirm_focus,
                &mut question_answer,
                &mut question_multi_checked,
                &mut question_input_focus,
                &mut shell_focus,
                &mut activity_label,
                &mut question_validation_error,
            ) {
                push_transcript_message_synced(
                    &mut messages,
                    messages_arc,
                    &mut messages_revision,
                    &mut prompt_history,
                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                );
            }
            return;
        }

        let picked_option = {
            let pending_ref = pending_user_question.read();
            match pending_ref.as_ref() {
                Some(pending)
                    if pending.is_single_select()
                        && question_input_focus.get().is_choices()
                        && !is_custom_choice_index(pending, question_selected.get())
                        && modifiers.is_empty()
                        && code == KeyCode::Enter =>
                {
                    let options = pending.options().unwrap_or(&[]);
                    select_value_at(options, question_selected.get())
                }
                _ => None,
            }
        };
        if let Some(value) = picked_option {
            let outcome = pending_user_question
                .write()
                .take()
                .map(|pending| pending.respond_option(value));
            if let Some(outcome) = outcome
                && let Some(summary) = apply_step_submit_outcome(
                    outcome,
                    &mut pending_user_question,
                    &mut question_selected,
                    &mut question_confirm_focus,
                    &mut question_answer,
                    &mut question_multi_checked,
                    &mut question_input_focus,
                    &mut shell_focus,
                    &mut activity_label,
                    &mut question_validation_error,
                )
            {
                push_transcript_message_synced(
                    &mut messages,
                    messages_arc,
                    &mut messages_revision,
                    &mut prompt_history,
                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                );
            }
            return;
        }

        if !shell_global_shortcut(modifiers, code) {
            return;
        }
    }

    let prefix_config = PromptPrefixConfig::default();
    let (mirror_draft, mirror_cursor) = prompt_editor_mirror.read().clone();
    let live_body = live_draft.read().clone();
    let stored_body = draft.read().clone();
    let use_mirror = mirror_draft.len() >= live_body.len() && mirror_draft.len() >= stored_body.len();
    let draft_body = if use_mirror {
        mirror_draft
    } else if live_body.len() >= stored_body.len() {
        live_body
    } else {
        stored_body
    };
    let editor_cursor = if use_mirror {
        mirror_cursor.min(draft_body.len())
    } else {
        live_cursor.get().min(draft_body.len())
    };
    let picker_open = input_prefix_kind.get() == InputPrefixKind::Default
        && !status_dialog_open
        && !file_picker_suppressed.get()
        && file_picker_open(&draft_body, editor_cursor);
    if picker_open
        && modifiers.is_empty()
        && matches!(
            code,
            KeyCode::Tab | KeyCode::Enter | KeyCode::Right | KeyCode::Up | KeyCode::Down | KeyCode::Esc
        )
    {
        return;
    }
    let draft_text = compose_palette_draft(input_prefix_kind.get(), &draft_body);
    let palette_snapshot = build_snapshot(&draft_text, &slash_commands.read(), screen_height);
    if !status_dialog_open
        && let Some(action) =
            resolve_snapshot_key_action(&draft_text, &palette_snapshot, slash_palette_index.get(), code, modifiers)
    {
        match action {
            SlashPaletteKeyAction::CompleteDraft {
                text: completed,
                suppress_enter_newline: suppress_enter,
            } => {
                let (kind, body) = absorb_inline_triggers(input_prefix_kind.get(), &completed, &prefix_config);
                input_prefix_kind.set(kind);
                draft.set(body.clone());
                live_draft.set(body.clone());
                live_cursor.set(body.len());
                suppress_enter_newline.set(suppress_enter);
                force_palette_sync.set(true);
                if !palette_visible(&compose_palette_draft(kind, &body)) {
                    slash_palette_active.set(false);
                }
                slash_palette_query.write().clear();
                slash_palette_index.set(0);
            }
            SlashPaletteKeyAction::MoveSelection(index) => {
                slash_palette_index.set(index);
            }
            SlashPaletteKeyAction::Dismiss => {
                draft.set(String::new());
                live_draft.set(String::new());
                live_cursor.set(0);
                input_prefix_kind.set(InputPrefixKind::Default);
                slash_palette_active.set(false);
                slash_palette_index.set(0);
                suppress_enter_newline.set(true);
            }
            SlashPaletteKeyAction::SubmitCommand { slash_input } => {
                input_prefix_kind.set(InputPrefixKind::Default);
                draft.set(String::new());
                live_draft.set(String::new());
                slash_palette_query.write().clear();
                slash_palette_index.set(0);
                suppress_enter_newline.set(true);
                force_palette_sync.set(true);

                // Transcript + prompt history keep the leading `/` (skills → `/skill:…`).
                let echo = {
                    let s = slash_input.trim();
                    if s.starts_with('/') {
                        s.to_string()
                    } else {
                        format!("/{s}")
                    }
                };

                let extension_registry = extension_host_for_keys.registry();
                let ext_registry = extension_registry.read();
                let templates = prompt_templates.read().clone();
                let loaded_skills = skills.read().clone();

                // `/tools` and `/system-prompt` are safe during a streaming turn
                // (detached tool snapshot + fallback, or cached system prompt), so the
                // "still responding" banner is intentionally not shown for them.

                let outcome = handle_slash_submit(SlashContext {
                    input: &slash_input,
                    extensions: Some(&ext_registry),
                    prompt_templates: Some(&templates),
                    skills: Some(&loaded_skills),
                    agent_session: agent_session.clone(),
                    extension_host: Some(&extension_host_for_keys),
                    paths: Some(&paths),
                    cwd: Some(&cwd_for_keys),
                });

                // The handler ALWAYS dispatches turn-spawning work; turn_gate queues it
                // behind the active turn. When busy, suppress the pre-echo — the busy arm
                // shows a "queued" notice instead, and no raw slash text reaches the model.
                let queue_follow_up = agent_turn_active.get();
                if slash_echoes_prompt_in_transcript(&outcome) && !queue_follow_up {
                    // Add prefix for skills to distinguish them in transcript.
                    let formatted_echo = match &outcome {
                        SlashOutcome::SpawnAgentTurnSkill { name: _ } => {
                            format!("[skill] {}", echo)
                        }
                        SlashOutcome::SpawnAgentTurnPromptTemplate { name: _ } => {
                            echo // No prefix for prompt templates
                        }
                        _ => echo,
                    };
                    let mut submitted =
                        TranscriptMessage::text(formatted_echo, TranscriptStyle::for_slash_turn_echo(&slash_input));
                    if submitted.style.is_user_input_card() {
                        submitted.submitted_at = Some(chrono::Utc::now());
                        // Sync to shared arc so the arc-to-state sync never loses this pre-echoed prompt.
                        messages_arc.write().write().unwrap().push(submitted.clone());
                        pre_echoed_user_prompts.set(pre_echoed_user_prompts.get().saturating_add(1));
                    }
                    push_transcript_message(&mut messages, &mut messages_revision, &mut prompt_history, submitted);
                }

                match outcome {
                    SlashOutcome::OpenModelSelector { filter } => {
                        let settings = Settings::load(&paths).ok();
                        let default_pm = settings.as_ref().and_then(|s| s.models.default_provider_and_model());
                        let live_pm = agent_session.as_ref().map(|s| (s.model_provider(), s.model_id()));
                        let (sel_provider, sel_model) = match live_pm {
                            Some((p, m)) => (Some(p), Some(m)),
                            None => match default_pm {
                                Some((p, m)) => (Some(p), Some(m)),
                                None => (None, None),
                            },
                        };
                        open_model_selector(OpenModelSelectorArgs {
                            pending: &mut pending_model_selector,
                            provider_index: &mut model_provider_index,
                            model_index: &mut model_selected_index,
                            filter: &mut model_filter,
                            input_focus: &mut model_input_focus,
                            draft: &mut draft,
                            live_draft: &mut live_draft,
                            shell_focus: &mut shell_focus,
                            initial_filter: filter,
                            paths: &paths,
                            provider_id: sel_provider.as_deref(),
                            model_id: sel_model.as_deref(),
                            session_scoped: Some(session_scoped_items.read().as_slice()),
                        });
                    }
                    SlashOutcome::OpenScopedModels => {
                        open_scoped_models(OpenScopedModelsArgs {
                            pending: &mut pending_scoped_models,
                            selected_index: &mut scoped_selected_index,
                            filter: &mut scoped_filter,
                            draft: &mut draft,
                            live_draft: &mut live_draft,
                            shell_focus: &mut shell_focus,
                            paths: &paths,
                            session_scoped: &session_scoped_items.read(),
                        });
                    }
                    SlashOutcome::OpenSystemPromptDialog { text } => {
                        open_system_prompt_dialog(OpenSystemPromptDialogArgs {
                            pending: &mut pending_system_prompt,
                            shell_focus: &mut shell_focus,
                            text,
                            width_pct: None,
                        });
                    }
                    SlashOutcome::OpenToolsDialog { text } => {
                        open_scroll_text_dialog(OpenScrollTextDialogArgs {
                            pending: &mut pending_system_prompt,
                            shell_focus: &mut shell_focus,
                            title: "Tools".to_string(),
                            text,
                            width_pct: TOOLS_DIALOG_WIDTH_PCT,
                            body_height: None,
                            show_copy: true,
                        });
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenSessionInfoDialog { text } => {
                        open_scroll_text_dialog(OpenScrollTextDialogArgs {
                            pending: &mut pending_system_prompt,
                            shell_focus: &mut shell_focus,
                            title: "Session".to_string(),
                            text: text.clone(),
                            width_pct: DEFAULT_SCROLL_TEXT_WIDTH_PCT,
                            body_height: None,
                            show_copy: true,
                        });
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenProviderListDialog { text } => {
                        let body_height = (text.lines().count() as u16).saturating_add(3).clamp(6, 30);
                        open_scroll_text_dialog(OpenScrollTextDialogArgs {
                            pending: &mut pending_system_prompt,
                            shell_focus: &mut shell_focus,
                            title: "Configured Providers".to_string(),
                            text,
                            width_pct: 55,
                            body_height: Some(body_height),
                            show_copy: false,
                        });
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenProviderUpdateDialog { text } => {
                        let body_height = (text.lines().count() as u16).saturating_add(3).clamp(6, 40);
                        open_scroll_text_dialog(OpenScrollTextDialogArgs {
                            pending: &mut pending_system_prompt,
                            shell_focus: &mut shell_focus,
                            title: "Provider Update".to_string(),
                            text,
                            width_pct: 60,
                            body_height: Some(body_height),
                            show_copy: false,
                        });
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenMemoryResultDialog { text } => {
                        let body_height = (text.lines().count() as u16).saturating_add(3).clamp(6, 30);
                        open_scroll_text_dialog(OpenScrollTextDialogArgs {
                            pending: &mut pending_system_prompt,
                            shell_focus: &mut shell_focus,
                            title: "Memory".to_string(),
                            text,
                            width_pct: 55,
                            body_height: Some(body_height),
                            show_copy: false,
                        });
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenItemSelector {
                        purpose,
                        title,
                        items,
                        preferred_value,
                        footer_hint,
                    } => {
                        open_item_selector(OpenItemSelectorArgs {
                            pending: &mut pending_item_selector,
                            draft: &mut draft,
                            live_draft: &mut live_draft,
                            shell_focus: &mut shell_focus,
                            selected_index: Some(&mut item_selector_selected),
                            purpose,
                            title,
                            items,
                            preferred_value,
                            footer_hint,
                        });
                        draft.set(String::new());
                        live_draft.set(String::new());
                        force_editor_clear.set(true);
                        suppress_enter_newline.set(true);
                    }
                    SlashOutcome::OpenRenameDialog { initial } => {
                        open_rename_dialog(OpenRenameDialogArgs {
                            pending: &mut pending_rename,
                            value: &mut rename_value,
                            draft: &mut draft,
                            live_draft: &mut live_draft,
                            shell_focus: &mut shell_focus,
                            initial,
                        });
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::PlayConfetti { mode } => {
                        open_confetti(OpenConfettiArgs {
                            pending: &mut pending_confetti,
                            state: &mut confetti_runtime,
                            draft: &mut draft,
                            live_draft: &mut live_draft,
                            shell_focus: &mut shell_focus,
                            mode,
                        });
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenMemoryFlushConfirm {
                        memory_count,
                        task_count,
                    } => {
                        *pending_memory_flush.write() = Some(PendingMemoryFlush {
                            memory_count,
                            task_count,
                        });
                        // Default selection to Cancel (index 1) for safety.
                        approval_selected.set(1);
                        shell_focus.set(ShellFocus::StatusDialog);
                        suppress_enter_newline.set(true);
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenFeedbackDialog => {
                        *pending_feedback.write() = true;
                        approval_selected.set(FEEDBACK_DEFAULT_INDEX);
                        shell_focus.set(ShellFocus::StatusDialog);
                        suppress_enter_newline.set(true);
                        force_editor_clear.set(true);
                    }
                    SlashOutcome::OpenProviderConnectDialog { provider_id } => {
                        open_provider_connect_dialog(OpenProviderConnectDialogArgs {
                            pending: &mut pending_provider_connect,
                            selected: &mut provider_connect_selected,
                            filter: &mut provider_connect_filter,
                            api_key_input: &mut provider_connect_api_key,
                            input_focus: &mut provider_connect_input_focus,
                            draft: &mut draft,
                            live_draft: &mut live_draft,
                            shell_focus: &mut shell_focus,
                            provider_id,
                        });
                        approval_selected.set(0);
                        suppress_enter_newline.set(true);
                        force_editor_clear.set(true);
                        return;
                    }
                    SlashOutcome::OpenMcpAuthDialog { server_name } => {
                        open_mcp_auth_dialog(OpenMcpAuthDialogArgs {
                            pending: &mut pending_mcp_auth,
                            selected: &mut provider_connect_selected,
                            filter: &mut provider_connect_filter,
                            draft: &mut draft,
                            live_draft: &mut live_draft,
                            shell_focus: &mut shell_focus,
                            paths: &paths,
                            server_name: server_name.clone(),
                        });
                        if let Some(name) = server_name {
                            let auto = pending_mcp_auth.read().as_ref().and_then(|p| {
                                let matches: Vec<_> = p
                                    .servers
                                    .iter()
                                    .filter(|s| s.name.eq_ignore_ascii_case(&name))
                                    .map(|s| s.name.clone())
                                    .collect();
                                (matches.len() == 1).then(|| matches[0].clone())
                            });
                            if let Some(server) = auto {
                                let _ = start_mcp_oauth_for_server(pending_mcp_auth, &paths, &server);
                            }
                        }
                        approval_selected.set(0);
                        suppress_enter_newline.set(true);
                        force_editor_clear.set(true);
                        return;
                    }
                    SlashOutcome::OpenProviderDisconnectDialog { provider_id } => {
                        let auth_store_path = paths.auth_store_path();
                        open_provider_disconnect_dialog(
                            crate::tui::provider_connect_dialog::OpenProviderDisconnectDialogArgs {
                                pending: &mut pending_provider_disconnect,
                                auth_store_path: &auth_store_path,
                                draft: &mut draft,
                                live_draft: &mut live_draft,
                                shell_focus: &mut shell_focus,
                                provider_id,
                            },
                        );
                        approval_selected.set(0);
                        suppress_enter_newline.set(true);
                        force_editor_clear.set(true);
                        return;
                    }
                    SlashOutcome::OverlayDeferred(overlay) => {
                        push_transcript_message_synced(
                            &mut messages,
                            messages_arc,
                            &mut messages_revision,
                            &mut prompt_history,
                            TranscriptMessage::text(overlay_deferred_message(&overlay), TranscriptStyle::Meta),
                        );
                    }
                    SlashOutcome::ResumeSession { session_id } => {
                        if let Some(session) = agent_session.as_ref() {
                            let session = Arc::clone(session);
                            tokio::spawn(async move {
                                session.shutdown_workers().await;
                            });
                        }
                        draft.set(String::new());
                        live_draft.set(String::new());
                        force_editor_clear.set(true);
                        suppress_enter_newline.set(true);
                        push_transcript_message_synced(
                            &mut messages,
                            messages_arc,
                            &mut messages_revision,
                            &mut prompt_history,
                            TranscriptMessage::text(format!("Resuming session {session_id}…"), TranscriptStyle::Meta),
                        );
                        resume_session_requested.set(Some(session_id));
                    }
                    SlashOutcome::Status(message) => {
                        push_transcript_message_synced(
                            &mut messages,
                            messages_arc,
                            &mut messages_revision,
                            &mut prompt_history,
                            TranscriptMessage::text(message, TranscriptStyle::Meta),
                        );
                    }
                    SlashOutcome::Unimplemented(message) => {
                        push_transcript_message_synced(
                            &mut messages,
                            messages_arc,
                            &mut messages_revision,
                            &mut prompt_history,
                            TranscriptMessage::text(message, TranscriptStyle::Meta),
                        );
                    }
                    SlashOutcome::SpawnAgentTurn
                    | SlashOutcome::SpawnAgentTurnSkill { .. }
                    | SlashOutcome::SpawnAgentTurnPromptTemplate { .. }
                    | SlashOutcome::SpawnAgentTurnQuiet => {
                        if agent_turn_active.get() {
                            // The command was already dispatched by handle_slash_submit
                            // (turn_gate queues it behind the active turn). Tell the user —
                            // do NOT push raw slash text as a follow-up prompt to the model.
                            let notice = format!("Command {slash_input} queued — runs after the current task.");
                            push_transcript_message_synced(
                                &mut messages,
                                messages_arc,
                                &mut messages_revision,
                                &mut prompt_history,
                                TranscriptMessage::text(notice, TranscriptStyle::Meta),
                            );
                        } else if agent_session.is_some() {
                            // Skill/compact may already be running via handle_slash_submit.
                            agent_turn_active.set(true);
                            chrome_refresh_pending.set(true);
                            idle_status_notice.set(None);
                            turn_cancel_requested.set(false);
                            mark_busy(
                                &mut BusyActivation {
                                    busy: &mut busy,
                                    busy_started_at: &mut busy_started_at,
                                    activity_started_at: &mut activity_started_at,
                                    activity_label: &mut activity_label,
                                    last_activity_label: &mut last_activity_label,
                                },
                                false,
                                None,
                            );
                            begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                        }
                    }
                    _ => {}
                }
            }
        }
        return;
    }

    let mention_index_ref = mention_index.read();
    let picker_index = mention_index_ref.as_ref().map(|arc| arc.as_ref());
    let file_picker_snapshot = build_file_picker_snapshot(
        &draft_body,
        editor_cursor,
        screen_height,
        file_picker_show_hidden.get(),
        picker_index,
    );
    if picker_open {
        file_picker_active.set(true);
    }
    if !status_dialog_open
        && !palette_snapshot.visible
        && input_prefix_kind.get() == InputPrefixKind::Default
        && file_picker_snapshot.visible
        && modifiers.contains(KeyModifiers::CONTROL)
        && matches!(code, KeyCode::Char('.'))
        && let Some(action) = resolve_file_picker_key_action(
            &draft_body,
            editor_cursor,
            &file_picker_snapshot,
            file_picker_index.get(),
            code,
            modifiers,
        )
        && action == FilePickerKeyAction::ToggleHiddenFiles
    {
        let next = !file_picker_show_hidden.get();
        file_picker_show_hidden.set(next);
        if let Ok(paths) = Paths::resolve()
            && let Ok(mut settings) = Settings::load_home(&paths)
        {
            settings.ui.file_picker.show_hidden_files = next;
            let _ = Settings::save(&paths, &settings);
        }
        // Transcript ephemeral notice (subtle grey `transient:*` styling; auto-clears after TTL).
        publish_ephemeral_transcript_notice(
            &mut messages,
            &mut messages_revision,
            &mut pending_transcript_notice_expires,
            FILE_PICKER_HIDDEN_NOTICE_KEY,
            file_picker_hidden_notice_text(next),
        );
        return;
    }

    if !status_dialog_open
        && shell_focus.get() == ShellFocus::Transcript
        && let Some(ch) = prompt_focus_char(code, modifiers)
    {
        shell_focus.set(ShellFocus::Prompt);
        let body = live_draft.read().clone();
        if let Some(next_kind) = try_consume_trigger(input_prefix_kind.get(), &body, ch, prefix_config.enabled) {
            input_prefix_kind.set(next_kind);
        } else {
            let mut text = body;
            text.push(ch);
            let (kind, normalized) = absorb_inline_triggers(input_prefix_kind.get(), &text, &prefix_config);
            input_prefix_kind.set(kind);
            draft.set(normalized.clone());
            live_draft.set(normalized);
        }
        suppress_enter_newline.set(false);
        return;
    }

    let palette_tab_reserved = palette_snapshot.visible
        || slash_palette_active.get()
        || picker_open
        || file_picker_active.get()
        || file_picker_snapshot.visible;

    match (modifiers, code) {
        // Ctrl+I — open Session info dialog.
        (m, KeyCode::Char('i')) | (m, KeyCode::Char('I'))
            if m.contains(KeyModifiers::CONTROL)
                && !m.contains(KeyModifiers::ALT)
                && !m.contains(KeyModifiers::META)
                && pending_user_question.read().is_none()
                && pending_model_selector.read().is_none()
                && pending_scoped_models.read().is_none()
                && pending_rename.read().is_none()
                && pending_confetti.read().is_none()
                && pending_provider_connect.read().is_none()
                && pending_provider_disconnect.read().is_none()
                && pending_provider_api_key.read().is_none() =>
        {
            if pending_system_prompt.read().is_some() {
                close_system_prompt_dialog(
                    &mut pending_system_prompt,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                    &mut force_editor_clear,
                );
            } else {
                let skills_snapshot = skills.read().clone();
                match session_info_slash_message(agent_session.as_ref(), Some(&skills_snapshot)) {
                    Ok(text) => {
                        open_scroll_text_dialog(OpenScrollTextDialogArgs {
                            pending: &mut pending_system_prompt,
                            shell_focus: &mut shell_focus,
                            title: "Session".to_string(),
                            text,
                            width_pct: DEFAULT_SCROLL_TEXT_WIDTH_PCT,
                            body_height: None,
                            show_copy: true,
                        });
                        force_editor_clear.set(true);
                    }
                    Err(msg) => {
                        let expire_tx = ephemeral_expire.read().tx.clone();
                        show_ephemeral_banner(
                            &mut ephemeral_banner,
                            &mut ephemeral_banner_generation,
                            &expire_tx,
                            EphemeralBanner {
                                key: "transient:session_info",
                                text: msg,
                                kind: EphemeralBannerKind::Error,
                                expires_at: Some(Instant::now() + AGENT_MODE_NOTICE_TTL),
                            },
                        );
                    }
                }
            }
        }
        (m, KeyCode::Char('l')) | (m, KeyCode::Char('L'))
            if m.contains(KeyModifiers::CONTROL) && pending_user_question.read().is_none() =>
        {
            if pending_confetti.read().is_none() {
                if pending_system_prompt.read().is_some() {
                    close_system_prompt_dialog(
                        &mut pending_system_prompt,
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                        &mut force_editor_clear,
                    );
                } else if pending_scoped_models.read().is_some() {
                    cancel_scoped_models(
                        &mut pending_scoped_models,
                        &mut session_scoped_items.write(),
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                    );
                } else if pending_model_selector.read().is_some() {
                    close_model_selector(&mut pending_model_selector, &mut draft, &mut live_draft, &mut shell_focus);
                } else {
                    let settings = Settings::load(&paths).ok();
                    let default_pm = settings.as_ref().and_then(|s| s.models.default_provider_and_model());
                    let live_pm = agent_session.as_ref().map(|s| (s.model_provider(), s.model_id()));
                    let (sel_provider, sel_model) = match live_pm {
                        Some((p, m)) => (Some(p), Some(m)),
                        None => match default_pm {
                            Some((p, m)) => (Some(p), Some(m)),
                            None => (None, None),
                        },
                    };
                    open_model_selector(OpenModelSelectorArgs {
                        pending: &mut pending_model_selector,
                        provider_index: &mut model_provider_index,
                        model_index: &mut model_selected_index,
                        filter: &mut model_filter,
                        input_focus: &mut model_input_focus,
                        draft: &mut draft,
                        live_draft: &mut live_draft,
                        shell_focus: &mut shell_focus,
                        initial_filter: String::new(),
                        paths: &paths,
                        provider_id: sel_provider.as_deref(),
                        model_id: sel_model.as_deref(),
                        session_scoped: Some(session_scoped_items.read().as_slice()),
                    });
                }
            }
        }
        // Ctrl+H — play confetti rain (if no overlays open).
        (m, KeyCode::Char('h')) | (m, KeyCode::Char('H'))
            if m.contains(KeyModifiers::CONTROL)
                && !m.contains(KeyModifiers::ALT)
                && !m.contains(KeyModifiers::META)
                && pending_confetti.read().is_none()
                && pending_tool_approval.read().is_none()
                && pending_mode_change.read().is_none()
                && pending_user_question.read().is_none()
                && pending_model_selector.read().is_none()
                && pending_scoped_models.read().is_none()
                && pending_system_prompt.read().is_none()
                && pending_rename.read().is_none()
                && pending_provider_connect.read().is_none()
                && pending_provider_disconnect.read().is_none()
                && pending_provider_api_key.read().is_none() =>
        {
            open_confetti(OpenConfettiArgs {
                pending: &mut pending_confetti,
                state: &mut confetti_runtime,
                draft: &mut draft,
                live_draft: &mut live_draft,
                shell_focus: &mut shell_focus,
                mode: ConfettiMode::Confetti,
            });
        }
        // Ctrl+F — play fireworks (if no overlays open).
        (m, KeyCode::Char('f')) | (m, KeyCode::Char('F'))
            if m.contains(KeyModifiers::CONTROL)
                && !m.contains(KeyModifiers::ALT)
                && !m.contains(KeyModifiers::META)
                && pending_confetti.read().is_none()
                && pending_tool_approval.read().is_none()
                && pending_mode_change.read().is_none()
                && pending_user_question.read().is_none()
                && pending_model_selector.read().is_none()
                && pending_scoped_models.read().is_none()
                && pending_system_prompt.read().is_none()
                && pending_rename.read().is_none()
                && pending_provider_connect.read().is_none()
                && pending_provider_disconnect.read().is_none()
                && pending_provider_api_key.read().is_none() =>
        {
            open_confetti(OpenConfettiArgs {
                pending: &mut pending_confetti,
                state: &mut confetti_runtime,
                draft: &mut draft,
                live_draft: &mut live_draft,
                shell_focus: &mut shell_focus,
                mode: ConfettiMode::Firework,
            });
        }
        // Ctrl+Y — always copy the full prompt body (not the selection).
        // Plain `y` yanks selected text in the Textarea (separate toast path).
        (m, KeyCode::Char('y')) | (m, KeyCode::Char('Y'))
            if m.contains(KeyModifiers::CONTROL)
                && !m.contains(KeyModifiers::SHIFT)
                && !m.contains(KeyModifiers::ALT)
                && !status_dialog_open
                && pending_user_question.read().is_none() =>
        {
            let expire_tx = ephemeral_expire.read().tx.clone();
            let banner = if draft_body.is_empty() {
                prompt_copy_banner(0)
            } else {
                match copy_to_clipboard(&draft_body) {
                    Ok(()) => prompt_copy_banner(draft_body.chars().count()),
                    Err(err) => {
                        log::warn!("copy prompt failed: {err}");
                        prompt_copy_failed_banner()
                    }
                }
            };
            show_ephemeral_banner(&mut ephemeral_banner, &mut ephemeral_banner_generation, &expire_tx, banner);
        }
        // Ctrl+Shift+T — roll theme Auto → Light → Dark (persist + reinstall palette).
        (m, KeyCode::Char('t')) | (m, KeyCode::Char('T'))
            if m.contains(KeyModifiers::CONTROL)
                && m.contains(KeyModifiers::SHIFT)
                && !status_dialog_open
                && pending_user_question.read().is_none() =>
        {
            if let Some(next) = cycle_and_persist_theme_mode(&paths) {
                let expire_tx = ephemeral_expire.read().tx.clone();
                show_ephemeral_banner(
                    &mut ephemeral_banner,
                    &mut ephemeral_banner_generation,
                    &expire_tx,
                    theme_mode_banner(next.label()),
                );
            }
        }
        // Ctrl+P / Shift+Ctrl+P — cycle scoped models (pi parity).
        (m, KeyCode::Char('p')) | (m, KeyCode::Char('P'))
            if m.contains(KeyModifiers::CONTROL) && !status_dialog_open && pending_user_question.read().is_none() =>
        {
            let reverse = m.contains(KeyModifiers::SHIFT);
            let agent = agent_session.clone();
            let (provider, model) = agent
                .as_ref()
                .map(|s| (Some(s.model_provider()), Some(s.model_id())))
                .unwrap_or((None, None));
            let mut stats = chrome_stats.read().clone();
            match cycle_scoped_model_selection(
                &paths,
                &mut session_scoped_items.write(),
                provider.as_deref(),
                model.as_deref(),
                reverse,
                &mut stats,
            ) {
                Ok((_label, value)) => {
                    publish_chrome_stats(&mut chrome_stats, &mut chrome_ui_revision, stats);
                    chrome_refresh_pending.set(true);
                    let clamped = clamp_thinking_for_model_value(thinking_level.get(), &value);
                    if clamped != thinking_level.get() {
                        thinking_level.set(clamped);
                    }
                    // Scoped cycle: `Model set to MODEL_ID (PROVIDER)`.
                    publish_ephemeral_transcript_notice(
                        &mut messages,
                        &mut messages_revision,
                        &mut pending_transcript_notice_expires,
                        MODEL_SET_NOTICE_KEY,
                        model_set_notice_from_value(&value),
                    );
                    if let Some(session) = agent {
                        spawn_runtime_model_switch(session, value, thinking_level.get());
                    }
                }
                Err(err) => {
                    push_transcript_message_synced(
                        &mut messages,
                        messages_arc,
                        &mut messages_revision,
                        &mut prompt_history,
                        TranscriptMessage::text(format!("{err}"), TranscriptStyle::Meta),
                    );
                }
            }
        }
        (m, KeyCode::Esc) if m.is_empty() => {
            // Escape cancels Shift-based text selection mode — the temporary Ctrl+S
            // toggle (e.g. after Shift+↑/↓ redirected focus to the transcript). The
            // persistent Ctrl+S toggle keeps its own mechanism and is never
            // cancelled by Escape.
            if shift_held.get() {
                shift_held.set(false);
                shift_last_pressed.set(None);
            }
            if shell_focus.get() == ShellFocus::Transcript {
                shell_focus.set(ShellFocus::Prompt);
            }
        }
        // Tab: toggle focus between prompt textarea and transcript.
        (m, KeyCode::Tab) if m.is_empty() && !status_dialog_open && !palette_tab_reserved => match shell_focus.get() {
            ShellFocus::Prompt => shell_focus.set(ShellFocus::Transcript),
            ShellFocus::Transcript => shell_focus.set(ShellFocus::Prompt),
            ShellFocus::StatusDialog => {}
        },
        // Shift+Tab: cycle agent mode (BackTab is the usual Shift+Tab code).
        (m, KeyCode::BackTab) | (m, KeyCode::Tab)
            if !status_dialog_open
                && !palette_tab_reserved
                && (matches!(code, KeyCode::BackTab) || m.contains(KeyModifiers::SHIFT)) =>
        {
            if busy.get() && !allow_mode_change_while_busy.get() {
                // Block mode changes during stream/tool work; toast clears async (TTL).
                let expire_tx = ephemeral_expire.read().tx.clone();
                show_ephemeral_banner(
                    &mut ephemeral_banner,
                    &mut ephemeral_banner_generation,
                    &expire_tx,
                    agent_mode_busy_banner(),
                );
            } else {
                let next = agent_mode.get().next();
                agent_mode.set(next);
                let expire_tx = ephemeral_expire.read().tx.clone();
                show_ephemeral_banner(
                    &mut ephemeral_banner,
                    &mut ephemeral_banner_generation,
                    &expire_tx,
                    agent_mode_banner(next),
                );
                if let Some(session) = agent_session.as_ref() {
                    // Eagerly invalidate cache and set mode_state so
                    // /system-prompt and the next harness turn see the
                    // new mode before the background task completes.
                    session.invalidate_system_prompt_cache();
                    session.try_set_mode_sync(next);
                    let session = Arc::clone(session);
                    let mode = next;
                    tokio::spawn(async move {
                        if let Err(err) = session.set_agent_mode(mode).await {
                            log::warn!("failed to set agent mode: {err}");
                        }
                    });
                }
            }
        }
        // Ctrl+.: cycle thinking level from the active model's catalog (thinkingLevelMap).
        (m, KeyCode::Char('.')) if m.contains(KeyModifiers::CONTROL) => {
            let current = thinking_level.get();
            let next = {
                let (provider, model_id) = if let Some(session) = agent_session.as_ref() {
                    (session.model_provider(), session.model_id())
                } else {
                    // Pre-session: parse footer model label (`provider/model`).
                    let label = chrome_stats.read().model_label.clone();
                    match crate::agent::parse_model_value(&label) {
                        Ok((p, m)) => (p, m),
                        Err(_) => (String::new(), String::new()),
                    }
                };
                if let Some(model) = elph_ai::get_builtin_model(&provider, &model_id) {
                    // Only catalog-supported levels (+ Off). Stale levels re-enter the cycle from Off.
                    current.next_for_model(&model)
                } else {
                    current.next()
                }
            };
            thinking_level.set(next);
            if let Some(session) = agent_session.as_ref() {
                let session = Arc::clone(session);
                let level = next;
                tokio::spawn(async move {
                    if let Err(err) = session.set_thinking_level(level).await {
                        log::warn!("failed to set thinking level: {err}");
                    }
                });
            }
        }
        // Ctrl+O: expand/collapse the most recent finished thinking / tool / response block.
        // Click a process header to toggle that specific older result (iocraft Button hit-test).
        (m, KeyCode::Char(c)) if m.contains(KeyModifiers::CONTROL) && matches!(c, 'o' | 'O') => {
            let mut msgs = messages.write();
            if toggle_latest_collapsible_detail(&mut msgs) {
                drop(msgs);
                messages_revision.set(messages_revision.get().wrapping_add(1));
            }
        }
        // Ctrl+Q: open/close prompt queue manager (cancel / edit numbered items).
        (m, KeyCode::Char('q')) | (m, KeyCode::Char('Q'))
            if m.contains(KeyModifiers::CONTROL)
                && !m.contains(KeyModifiers::ALT)
                && pending_tool_approval.read().is_none()
                && pending_mode_change.read().is_none()
                && pending_user_question.read().is_none()
                && pending_model_selector.read().is_none()
                && pending_scoped_models.read().is_none()
                && pending_system_prompt.read().is_none()
                && pending_rename.read().is_none()
                && pending_confetti.read().is_none() =>
        {
            if queue_manager_open.get() {
                queue_manager_open.set(false);
                queue_manager_selected.set(0);
                queue_manager_action.set(PromptQueueAction::SendNow);
                shell_focus.set(ShellFocus::Prompt);
            } else if prompt_queue.read().is_empty() {
                let expire_tx = ephemeral_expire.read().tx.clone();
                show_ephemeral_banner(
                    &mut ephemeral_banner,
                    &mut ephemeral_banner_generation,
                    &expire_tx,
                    EphemeralBanner {
                        key: "transient:prompt_queue_empty",
                        text: "Prompt queue is empty".into(),
                        kind: EphemeralBannerKind::Notice,
                        expires_at: Some(std::time::Instant::now() + AGENT_MODE_NOTICE_TTL),
                    },
                );
            } else {
                queue_manager_selected.set(0);
                queue_manager_action.set(PromptQueueAction::SendNow);
                queue_manager_open.set(true);
                shell_focus.set(ShellFocus::StatusDialog);
            }
        }
        // Ctrl+Enter is handled early (before status dialogs) — see is_ctrl_enter_interject.
        (m, KeyCode::Char('d')) if m.contains(KeyModifiers::CONTROL) => {
            let expire_tx = ephemeral_expire.read().tx.clone();
            let _ = request_quit(
                PendingQuitAction {
                    pending_quit_confirm: &mut pending_quit_confirm,
                    should_exit: &mut should_exit,
                    busy: &busy,
                    turn_cancel_requested: &mut turn_cancel_requested,
                    prompt_queue: &mut prompt_queue,
                    pending_tool_approval: &mut pending_tool_approval,
                    pending_user_question: &mut pending_user_question,
                    agent_session: &agent_session,
                },
                &mut ephemeral_banner,
                &mut ephemeral_banner_generation,
                &expire_tx,
                false,
            );
        }
        (m, KeyCode::Char('c')) if m.contains(KeyModifiers::CONTROL) && pending_tool_approval.read().is_none() => {
            // Ctrl+C: if textarea has content → clear it; if empty and busy → cancel stream.
            // Never used for yank (`y` = selection, Ctrl+Y = full prompt).
            if !draft_body.is_empty() {
                draft.set(String::new());
                live_draft.set(String::new());
                force_editor_clear.set(true);
                slash_palette_index.set(0);
                slash_palette_query.write().clear();
                suppress_enter_newline.set(true);
            } else if busy.get() {
                turn_cancel_requested.set(true);
                activity_label.set("Cancelling…".to_string());
                prompt_queue.write().clear();
                queue_ui_revision.set(queue_ui_revision.get().wrapping_add(1));
                pre_echoed_user_prompts.set(0);
                agent_turn_active.set(false);
                queue_manager_open.set(false);
                queue_manager_selected.set(0);
                if let Some(pending) = pending_tool_approval.write().take() {
                    pending.respond(ToolApprovalChoice::Reject);
                }
                if let Some(mode_change) = pending_mode_change.write().take() {
                    mode_change.respond(false);
                }
                let _ = pending_plan_confirmation.write().take();
                let _ = pending_memory_flush.write().take();
                if let Some(question) = pending_user_question.write().take() {
                    question.cancel();
                }
                shell_focus.set(ShellFocus::Prompt);
                question_answer.set(String::new());
                question_input_focus.set(QuestionInputFocus::Choices);
                if let Some(token) = user_shell_abort.read().clone() {
                    token.cancel();
                }
                if let Some(session) = agent_session.as_ref() {
                    TurnDispatcher::spawn_abort(Arc::clone(session));
                } else if user_shell_abort.read().is_none() {
                    let canceled_elapsed = busy_started_at
                        .read()
                        .as_ref()
                        .map(|started| format_elapsed_secs(*started))
                        .unwrap_or(0.0);
                    session_elapsed_secs.set(accumulate_session_elapsed(session_elapsed_secs.get(), canceled_elapsed));
                    busy.set(false);
                    busy_started_at.set(None);
                    activity_started_at.set(None);
                    turn_token_tracker.set(None);
                    turn_cancel_requested.set(false);
                    idle_status_notice.set(Some(IdleStatusNotice {
                        text: format_turn_canceled_notice(canceled_elapsed),
                        since: Instant::now(),
                    }));
                }
            }
        }
        _ => {}
    }
}
