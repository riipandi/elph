//! Main shell tick loop (extracted from the MainShell `use_future` closure).

use super::*;

/// Runs the shell's main tick loop (extracted from the MainShell `use_future` closure).
pub(crate) async fn shell_tick_loop(ctx: ShellCtx) {
    let ShellCtx {
        mut active_plan_file,
        mut activity_label,
        mut activity_started_at,
        mut agent_session_slot,
        mut agent_turn_active,
        allow_mode_change_while_busy,
        mut approval_selected,
        mut bootstrap_config,
        mut bootstrap_phase,
        mut bootstrap_rx,
        mut bootstrap_worker_started,
        mut busy,
        mut busy_started_at,
        mut chrome_eager_paint_done,
        mut chrome_full_redraw_pending,
        mut chrome_refresh_pending,
        mut chrome_stats,
        mut chrome_tick,
        mut chrome_ui_revision,
        mut confetti_frame,
        mut confetti_runtime,
        cwd_for_loop,
        cwd_for_mention_index,
        mut draft,
        mut ephemeral_banner,
        mut ephemeral_banner_generation,
        mut ephemeral_expire,
        mut event_applier,
        extension_host_for_loop,
        extension_host_for_palette,
        fallback_context_limit,
        fallback_model_label_for_chrome,
        fallback_supports_images,
        mut git_footer,
        mut idle_status_notice,
        mut last_activity_label,
        mut last_event_burst,
        mut pending_approval_label,
        mut last_transcript_publish,
        mut layout_screen_size_for_loop,
        mut live_draft,
        mut live_session_id,
        mut mention_index,
        mention_index_requested,
        mut messages,
        messages_arc_inner,
        mut messages_for_tick,
        mut messages_revision,
        mut messages_revision_for_tick,
        mut new_session_requested,
        mut resume_session_requested,
        mut palette_refresh_pending,
        paths,
        mut pending_confetti,
        mut pending_mode_change,
        mut pending_retry_prompt,
        mut pending_plan_confirmation,
        mut pending_mcp_auth_for_tick,
        mut pending_provider_connect_for_tick,
        mut pending_provider_disconnect_for_tick,
        mut pending_quit_confirm,
        mut pending_system_prompt,
        mut pending_aside,
        mut aside_tick,
        mut pending_tool_approval,
        mut pending_transcript_notice_expires,
        mut pending_user_question,
        mut pre_echoed_user_prompts,
        mut prompt_history,
        mut prompt_queue,
        mut prompt_templates,
        mut provider_connect_api_key_for_tick,
        mut provider_connect_input_focus_for_tick,
        mut question_answer,
        mut question_confirm_focus,
        mut question_input_focus,
        mut question_multi_checked,
        mut question_selected,
        mut queue_manager_open,
        mut queue_manager_selected,
        mut queue_ui_revision,
        mut session_elapsed_secs,
        session_id: _session_id,
        mut shell_focus,
        mut shell_focus_for_tick,
        mut shift_held,
        mut shift_last_pressed,
        show_thinking,
        mut skills,
        mut skills_count,
        mut slash_commands,
        subagent_output_buffers_state,
        mut subagent_output_scroll_tick,
        mut transcript_pending,
        mut turn_cancel_requested,
        mut turn_token_tracker,
        mut last_turn_stats,
        turn_stats_enabled,
        mut ui_events_slot,
        mut user_shell_abort,
        mut user_shell_channel,
        mut todos,
        mut todo_panel_tick,
        mut thinking_level,
        pending_subagent_output,
        ..
    } = ctx;
    loop {
        tokio::time::sleep(Duration::from_millis(SHELL_TICK_MS)).await;

        poll_layout_screen_size(&mut layout_screen_size_for_loop);

        // Time-based debounce: auto-clear shift-held after 10 seconds of no Shift
        // key press. The user holds Shift, selects text for several seconds,
        // then has 10s after the last Shift press to press Ctrl+C/Cmd+V
        // (which arrive without modifiers on macOS terminals). After 10s
        // of no Shift key, the scrollbar comes back.
        let shift_timed_out = shift_held.get()
            && shift_last_pressed
                .read()
                .as_ref()
                .is_some_and(|last| last.elapsed() > Duration::from_secs(10));
        if shift_timed_out {
            shift_held.set(false);
            shift_last_pressed.set(None);
        }

        if bootstrap_phase.get() == BootstrapPhase::Pending && !bootstrap_worker_started.get() {
            if let Some(config) = bootstrap_config.read().clone() {
                bootstrap_worker_started.set(true);
                let paths_snapshot = paths.read().clone();
                bootstrap_rx.set(Some(spawn_bootstrap_worker(config, paths_snapshot)));
                bootstrap_phase.set(BootstrapPhase::Running);
                busy.set(true);
                activity_started_at.set(Some(Instant::now()));
                activity_label.set(bootstrap_activity_label(BootstrapPhase::Running, Some("Preparing agent")));
                {
                    let mut msgs = messages.write();
                    begin_agent_startup(&mut msgs);
                }
                publish_transcript_now(&mut messages_revision, &mut transcript_pending, &mut last_transcript_publish);
            } else {
                bootstrap_phase.set(BootstrapPhase::Done);
            }
        }

        if let Some(rx) = bootstrap_rx.write().as_mut() {
            let mut bootstrap_events = 0usize;
            while bootstrap_events < MAX_BOOTSTRAP_EVENTS_PER_TICK {
                let Ok(event) = rx.try_recv() else {
                    break;
                };
                bootstrap_events += 1;
                // Desktop notification for bootstrap events
                match &event {
                    BootstrapUiEvent::AgentReady(_) => {
                        if let Ok(settings) = Settings::load(&paths.read().clone()) {
                            notifier::notify(&settings.notifications, notifier::NotifKind::StartupReady);
                        }
                    }
                    BootstrapUiEvent::AgentFailed(msg) => {
                        if let Ok(settings) = Settings::load(&paths.read().clone()) {
                            notifier::notify(
                                &settings.notifications,
                                notifier::NotifKind::Error { message: msg.as_str() },
                            );
                        }
                    }
                    _ => {}
                }
                apply_bootstrap_ui_event(
                    event,
                    &mut bootstrap_phase,
                    &mut busy,
                    &mut activity_label,
                    &mut activity_started_at,
                    &mut live_session_id,
                    &mut chrome_refresh_pending,
                    &mut chrome_stats,
                    &mut chrome_ui_revision,
                    fallback_context_limit,
                    &mut palette_refresh_pending,
                    &mut agent_session_slot,
                    &mut ui_events_slot,
                    &mut messages,
                    &mut prompt_history,
                    &mut thinking_level,
                )
                .await;
                chrome_full_redraw_pending.set(true);
                publish_transcript_now(&mut messages_revision, &mut transcript_pending, &mut last_transcript_publish);
            }
        }

        // Sync bootstrap messages from State back to the arc so the arc sync
        // (which runs on the next agent event) does not overwrite them.
        *messages_arc_inner.write().unwrap() = messages.read().clone();

        // Handle `/new` and `/resume <id>`: reload resources + restart bootstrap without exiting TUI.
        let resume_id_req = resume_session_requested.read().clone();
        let want_new = *new_session_requested.read();
        if want_new || resume_id_req.is_some() {
            *new_session_requested.write() = false;
            *resume_session_requested.write() = None;

            let paths_for_load = paths.read().clone();
            let cwd_for_load = cwd_for_loop.clone();
            let settings = Settings::load(&paths_for_load).ok();
            if let Some(settings) = settings {
                let env = Arc::new(LocalExecutionEnv::new(&cwd_for_load));
                let loaded = load_resources(&paths_for_load, &cwd_for_load, &env).await;

                let new_templates = loaded.resources.prompt_templates.clone();
                let new_skills = loaded.resources.skills.clone();
                prompt_templates.set(new_templates);
                skills.set(new_skills);
                {
                    let ext_registry = extension_host_for_loop.registry();
                    let reg = ext_registry.read();
                    slash_commands.set(slash_commands_for_palette(
                        Some(&reg),
                        Some(&prompt_templates.read()),
                        Some(&skills.read()),
                    ));
                }

                let boot =
                    crate::tui::resolve_boot_model(&settings, &paths_for_load, &cwd_for_load, resume_id_req.as_deref())
                        .await;
                let new_config = TuiBootstrapConfig {
                    paths: paths_for_load,
                    settings,
                    resume_id: resume_id_req,
                    model_override: boot.ok().map(|(provider, model_id)| format!("{provider}/{model_id}")),
                    preloaded_resources: loaded,
                };
                bootstrap_config.set(Some(new_config));
            }

            bootstrap_phase.set(BootstrapPhase::Pending);
            bootstrap_worker_started.set(false);
            bootstrap_rx.set(None);
            chrome_refresh_pending.set(true);
            // Clear the old live session slot so UI does not keep talking to a dead worker.
            agent_session_slot.set(None);
            messages.set(Vec::new());
            *messages_arc_inner.write().unwrap() = Vec::new();
        }

        let agent_session_for_loop = agent_session_slot.read().clone();
        let agent_session_for_chrome = agent_session_slot.read().clone();
        let agent_session_for_palette = agent_session_slot.read().clone();
        let ui_events = ui_events_slot.read().clone();

        if mention_index_requested.get() && mention_index.read().is_none() {
            let base = cwd_for_mention_index.to_string_lossy().into_owned();
            if let Ok(Ok(index)) = tokio::task::spawn_blocking(move || MentionSearchIndex::build(&base)).await {
                mention_index.set(Some(Arc::new(index)));
            }
        }

        if palette_refresh_pending.get() {
            if let Some(session) = agent_session_for_palette.as_ref() {
                let resources = session.harness().get_resources().await;
                let templates = resources.prompt_templates.clone();
                let loaded_skills = resources.skills.clone();
                prompt_templates.set(templates.clone());
                skills.set(loaded_skills.clone());
                slash_commands.set(slash_commands_for_palette(
                    Some(&extension_host_for_palette.registry().read()),
                    Some(&templates),
                    Some(&loaded_skills),
                ));
            }
            palette_refresh_pending.set(false);
        }

        chrome_tick.set(chrome_tick.get().wrapping_add(1));

        // ── MCP OAuth completed: close dialog ────────────────────
        if pending_mcp_auth_for_tick.read().as_ref().is_some_and(|p| p.done) {
            let notice = pending_mcp_auth_for_tick
                .read()
                .as_ref()
                .and_then(|p| p.success_notice.clone());
            pending_mcp_auth_for_tick.set(None);
            shell_focus_for_tick.set(ShellFocus::Prompt);
            if let Some(notice) = notice {
                let mut msgs = messages_for_tick.write().clone();
                msgs.push(TranscriptMessage::text(notice, TranscriptStyle::Meta));
                messages_for_tick.set(msgs);
                messages_revision_for_tick.set(messages_revision_for_tick.get().wrapping_add(1));
            }
        }

        // ── OAuth completed: close dialog + ensure live creds reloaded ──
        if pending_provider_connect_for_tick
            .read()
            .as_ref()
            .is_some_and(|p| p.done)
        {
            let notice = pending_provider_connect_for_tick.read().as_ref().and_then(|p| {
                let url = &p.oauth_url;
                if url.starts_with("Signed in to ") {
                    Some(url.clone())
                } else {
                    None
                }
            });
            // Best-effort: re-read auth.json into the live session models store
            // for the provider that just connected (not only the current model).
            let completed_pid = pending_provider_connect_for_tick
                .read()
                .as_ref()
                .and_then(|p| p.completed_provider_id.clone());
            if let (Some(session), Some(provider)) = (agent_session_slot.read().clone(), completed_pid) {
                let path = paths.read().auth_store_path();
                tokio::spawn(async move {
                    if let Err(e) = session.reload_provider_credential_from_disk(&path, &provider).await {
                        log::warn!("reload credential after OAuth for {provider}: {e:#}");
                    }
                });
            }
            pending_provider_connect_for_tick.set(None);
            provider_connect_api_key_for_tick.set(String::new());
            provider_connect_input_focus_for_tick.set(ProviderConnectFocus::default());
            shell_focus_for_tick.set(ShellFocus::Prompt);
            if let Some(notice) = notice {
                let mut msgs = messages_for_tick.write().clone();
                msgs.push(TranscriptMessage::text(notice, TranscriptStyle::Meta));
                messages_for_tick.set(msgs);
                messages_revision_for_tick.set(messages_revision_for_tick.get().wrapping_add(1));
            }
        }

        // ── Provider disconnect completed: close dialog ─────────────
        if pending_provider_disconnect_for_tick
            .read()
            .as_ref()
            .is_some_and(|p| p.done)
        {
            pending_provider_disconnect_for_tick.set(None);
            shell_focus_for_tick.set(ShellFocus::Prompt);
            // Push transcript notification
            let mut msgs = messages_for_tick.write().clone();
            msgs.push(TranscriptMessage::text(
                "Signed out from all providers".to_string(),
                TranscriptStyle::Meta,
            ));
            messages_for_tick.set(msgs);
            messages_revision_for_tick.set(messages_revision_for_tick.get().wrapping_add(1));
        }

        let chrome_due = chrome_refresh_pending.get() || chrome_tick.get() % CHROME_REFRESH_TICKS == 0;
        if chrome_due {
            let paths = paths.read().clone();
            let next_git_footer = read_git_footer_info(paths.project_dir());
            if git_footer.read().clone() != next_git_footer {
                git_footer.set(next_git_footer);
                bump_chrome_ui_revision(&mut chrome_ui_revision);
            }

            if let Some(session) = agent_session_for_chrome.as_ref() {
                chrome_refresh_pending.set(false);
                let resources = session.harness().get_resources().await;
                skills_count.set(resources.skills.len());
                let stats = refresh_chrome_stats(
                    Arc::clone(session),
                    fallback_context_limit,
                    &fallback_model_label_for_chrome,
                    fallback_supports_images,
                )
                .await;
                publish_chrome_stats(&mut chrome_stats, &mut chrome_ui_revision, stats.clone());
                if busy.get()
                    && let Some(tracker) = turn_token_tracker.write().as_mut()
                {
                    tracker.sync_baseline(stats.tokens_used);
                }
            } else {
                // No session yet: still finish the pending git/chrome snapshot so the
                // bootstrap footer (project + model) paints without waiting for AgentReady.
                // Previously pending stayed true forever and re-ran git I/O every tick.
                chrome_refresh_pending.set(false);
            }

            // One-shot eager repaint after the first chrome pass (layout size is settled
            // and bootstrap labels are on the tree). Without this, iocraft can leave the
            // footer blank until the first stats mutation (model pick / first turn).
            if !chrome_eager_paint_done.get() {
                chrome_eager_paint_done.set(true);
                chrome_full_redraw_pending.set(true);
                bump_chrome_ui_revision(&mut chrome_ui_revision);
            }
        }

        // Phase-timer reset only — spinner/elapsed animate inside StatusRow (no shell re-render).
        if busy.get() {
            let current_label = activity_label.read().clone();
            if current_label != *last_activity_label.read() {
                last_activity_label.set(current_label);
                activity_started_at.set(Some(Instant::now()));
            }
        }

        let idle_notice_expired = idle_status_notice
            .read()
            .as_ref()
            .is_some_and(|notice| notice.since.elapsed() >= Duration::from_millis(TURN_COMPLETE_NOTICE_MS));
        if idle_notice_expired {
            idle_status_notice.set(None);
        }

        if pending_confetti.read().is_some() {
            let (frame_changed, should_close) = {
                if let Some(runtime) = confetti_runtime.write().as_mut() {
                    let frame_changed = runtime.tick();
                    (frame_changed, runtime.should_close())
                } else {
                    (false, false)
                }
            };
            if should_close {
                close_confetti(
                    &mut pending_confetti,
                    &mut confetti_runtime,
                    &mut draft,
                    &mut live_draft,
                    &mut shell_focus,
                );
            } else if frame_changed {
                confetti_frame.set(confetti_frame.get().wrapping_add(1));
            }
        }

        {
            let mut channel = ephemeral_expire.write();
            poll_ephemeral_banner_expiry(&mut ephemeral_banner, &ephemeral_banner_generation, &mut channel.rx);
        }
        // Wall-clock expiry for transcript ephemeral notices (independent of status-row toasts).
        poll_ephemeral_transcript_notices(
            &mut messages,
            &mut messages_revision,
            &mut pending_transcript_notice_expires,
        );

        let mut transcript_changed = false;
        let mut run_completed = false;
        let mut run_completed_elapsed: Option<f64> = None;
        // True when a live-activity event (deltas / tool / Retrying) arrived after
        // RunCompleted in the same drained batch — the turn is still alive.
        let mut live_after_run_completed = false;

        let drained_events: Vec<AgentUiEvent> = if let Some(rx) = ui_events.as_ref() {
            if let Ok(mut guard) = rx.lock() {
                let mut raw = Vec::with_capacity(MAX_UI_EVENTS_PER_TICK);
                while raw.len() < MAX_UI_EVENTS_PER_TICK {
                    let Ok(event) = guard.try_recv() else {
                        break;
                    };
                    raw.push(event);
                }
                drop(guard);
                last_event_burst.set(raw.len());
                crate::tui::agent_bridge::coalesce_agent_ui_events(raw)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        for event in drained_events {
            if agent_event_keeps_busy(&event) {
                // Stream/tool activity means a real harness turn (not bootstrap chrome).
                agent_turn_active.set(true);
                if !busy.get() {
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
                }
                // A live-activity event arriving *after* RunCompleted in the same batch
                // means the turn is still running (auto-retry, late subagent output) — the
                // turn-completion block below must not tear the busy state back down.
                if run_completed {
                    live_after_run_completed = true;
                }
            }
            if let AgentUiEvent::RunCompleted {
                elapsed_secs,
                usage,
                provider_id,
                model_id,
            } = &event
            {
                run_completed = true;
                run_completed_elapsed = Some(*elapsed_secs);
                // Only render a stats card for real agent/chat-assistant turns. System
                // operations that spin the UI without an AI response (e.g. `/compact`
                // "History is already up to date") carry no usage/model and are skipped.
                if usage.is_some() || provider_id.is_some() || model_id.is_some() {
                    last_turn_stats.set(Some(TurnCompleteStats::from_event(
                        *elapsed_secs,
                        usage.as_ref(),
                        provider_id.as_deref(),
                        model_id.as_deref(),
                    )));
                } else {
                    last_turn_stats.set(None);
                }
            }

            match &event {
                AgentUiEvent::TextDelta(delta) => {
                    if let Some(tracker) = turn_token_tracker.write().as_mut() {
                        tracker.record_delta(delta);
                    }
                }
                AgentUiEvent::ThinkingDelta(delta) if show_thinking => {
                    if let Some(tracker) = turn_token_tracker.write().as_mut() {
                        tracker.record_delta(delta);
                    }
                }
                _ => {}
            }

            if let AgentUiEvent::Status(ref message) = event {
                if message.to_ascii_lowercase().contains("reloaded") {
                    // `/reload` refreshes skills/templates — rebuild slash palette.
                    palette_refresh_pending.set(true);
                }
                // Sticky red toast — friendly text only (no raw JSON); transcript keeps fuller line.
                if crate::tui::api_error_display::is_user_facing_api_error_line(message) {
                    // Drop the trailing "Press Ctrl+R to retry this prompt." marker (and the
                    // blank line that precedes it) so the single-line toast stays clean; the
                    // transcript error card renders that hint as its own dedicated row.
                    let cleaned = message
                        .replace(crate::tui::api_error_display::RETRY_HINT, "")
                        .trim()
                        .to_string();
                    let toast = crate::tui::api_error_display::format_ephemeral_api_error(&cleaned);
                    show_ephemeral_banner(
                        &mut ephemeral_banner,
                        &mut ephemeral_banner_generation,
                        &ephemeral_expire.read().tx,
                        api_error_banner(toast),
                    );
                }
            }

            // Transient stream/API failure — stash the recovery prompt so the Ctrl+R key can
            // re-submit it without the user re-typing (error card shows the hint).
            if let AgentUiEvent::RetryablePrompt(prompt) = &event {
                pending_retry_prompt.set(Some(prompt.clone()));
            }

            if let AgentUiEvent::MemoryResult(ref text) = event {
                let body_height = (text.lines().count() as u16).saturating_add(3).clamp(8, 40);
                open_scroll_text_dialog(OpenScrollTextDialogArgs {
                    pending: &mut pending_system_prompt,
                    shell_focus: &mut shell_focus,
                    title: "Memory".to_string(),
                    text: text.clone(),
                    width_pct: 80,
                    body_height: Some(body_height),
                    show_copy: false,
                });
                continue;
            }

            if let AgentUiEvent::AsideStarted { request_id, question } = &event {
                *pending_aside.write() =
                    Some(crate::tui::aside_panel::AsidePanelState::loading(*request_id, question.clone()));
                activity_label.set(format!("Aside: {question}"));
                continue;
            }
            if let AgentUiEvent::AsideFinished {
                request_id,
                question,
                answer,
            } = &event
            {
                let accept = pending_aside
                    .read()
                    .as_ref()
                    .is_none_or(|s| s.request_id() == *request_id);
                if accept {
                    *pending_aside.write() = Some(crate::tui::aside_panel::AsidePanelState::done(
                        *request_id,
                        question.clone(),
                        answer.clone(),
                    ));
                }
                continue;
            }
            if let AgentUiEvent::AsideFailed { request_id, error, .. } = &event {
                let question = pending_aside
                    .read()
                    .as_ref()
                    .map(|s| s.question().to_string())
                    .unwrap_or_default();
                let accept = pending_aside
                    .read()
                    .as_ref()
                    .is_none_or(|s| s.request_id() == *request_id);
                if accept {
                    *pending_aside.write() = Some(crate::tui::aside_panel::AsidePanelState::error(
                        *request_id,
                        question,
                        error.clone(),
                    ));
                }
                continue;
            }
            // Non-interrupting inbound worker messages: same aside panel as `/aside`,
            // never a harness steer — the user's current task keeps running.
            if let AgentUiEvent::WorkerInboundStarted {
                request_id,
                from_worker,
                message,
            } = &event
            {
                *pending_aside.write() = Some(crate::tui::aside_panel::worker_inbound_loading(
                    *request_id,
                    from_worker,
                    message,
                ));
                activity_label.set(format!("Worker {from_worker}: {message}"));
                continue;
            }
            if let AgentUiEvent::WorkerInboundAnswered {
                request_id,
                from_worker,
                message,
                answer,
            } = &event
            {
                if pending_aside
                    .read()
                    .as_ref()
                    .is_none_or(|s| s.request_id() == *request_id)
                {
                    *pending_aside.write() = Some(crate::tui::aside_panel::AsidePanelState::done(
                        *request_id,
                        format!("{from_worker}: {message}"),
                        answer.clone(),
                    ));
                }
                continue;
            }
            if let AgentUiEvent::WorkerInboundFailed {
                request_id,
                from_worker,
                message,
                error,
            } = &event
            {
                if pending_aside
                    .read()
                    .as_ref()
                    .is_none_or(|s| s.request_id() == *request_id)
                {
                    *pending_aside.write() = Some(crate::tui::aside_panel::AsidePanelState::error(
                        *request_id,
                        format!("{from_worker}: {message}"),
                        error.clone(),
                    ));
                }
                continue;
            }

            if let AgentUiEvent::TodoUpdated { items } = &event {
                todos.set(items.clone());
                // Re-render loop for the todo panel spinner: while a todo is
                // in_progress the shell re-renders (this State read makes the
                // view depend on the tick value), so the running row animates.
                if todos
                    .read()
                    .iter()
                    .any(|t| t.status == elph_agent::TodoStatus::InProgress)
                {
                    todo_panel_tick.set(todo_panel_tick.get().wrapping_add(1));
                }
                continue;
            }

            if let AgentUiEvent::ToolApprovalRequired(req) = event {
                let tool_name = req.tool_name.clone();
                let tool_call_id = req.tool_call_id.clone();
                let verb = tool_display_verb(&tool_name);
                activity_label.set(format!("Approve: {verb}"));
                approval_selected.set(TOOL_APPROVAL_DEFAULT_INDEX);
                shell_focus.set(ShellFocus::StatusDialog);
                pending_tool_approval.set(Some(PendingToolApproval::from_request(req)));
                // Mark that we set a pending approval label so we can clear it on completion
                pending_approval_label.set(true);
                // Desktop notification: tool permission request
                {
                    let paths = paths.read().clone();
                    if let Ok(settings) = Settings::load(&paths) {
                        notifier::notify(
                            &settings.notifications,
                            notifier::NotifKind::ToolPermission { tool_name: &tool_name },
                        );
                    }
                }
                {
                    let mut msgs = messages_arc_inner.write().unwrap();
                    // Process status line (colored, consistent gaps) — not a flush Meta dump.
                    let key = tool_approval_transcript_key(&tool_call_id);
                    if let Some(existing) = msgs.iter_mut().find(|m| m.startup_key.as_deref() == Some(key.as_str())) {
                        existing.content = "Tool approval".to_string();
                        existing.status_detail = Some(verb.clone());
                        existing.style = TranscriptStyle::StatusRunning;
                    } else {
                        let mut row = TranscriptMessage::startup_status(
                            key,
                            "Tool approval".to_string(),
                            TranscriptStyle::StatusRunning,
                        );
                        row.status_detail = Some(verb);
                        msgs.push(row);
                    }
                }
                transcript_changed = true;
                continue;
            }

            if let AgentUiEvent::UserQuestionRequired(req) = event {
                let question_summary: String = req
                    .steps
                    .first()
                    .map(|s| s.question.clone())
                    .unwrap_or_else(|| "Agent has a question".into());
                let pending = PendingUserQuestion::from_request(req);
                activity_label.set(step_activity_label(&pending));
                reset_ui_for_step(
                    &pending,
                    &mut question_selected,
                    &mut question_confirm_focus,
                    &mut question_answer,
                    &mut question_multi_checked,
                    &mut question_input_focus,
                );
                shell_focus.set(ShellFocus::StatusDialog);
                pending_user_question.set(Some(pending));
                // Desktop notification: user question
                {
                    let paths = paths.read().clone();
                    if let Ok(settings) = Settings::load(&paths) {
                        notifier::notify(
                            &settings.notifications,
                            notifier::NotifKind::UserQuestion {
                                summary: question_summary,
                            },
                        );
                    }
                }
                transcript_changed = true;
                continue;
            }

            if let AgentUiEvent::ModeChangeRequired(req) = event {
                if busy.get() && !allow_mode_change_while_busy.get() {
                    // Auto-reject mode change while busy when setting disallows it.
                    let _ = req.response_tx.send("false".to_string());
                    continue;
                }
                let mode_label = req.target_mode.to_ascii_uppercase();
                activity_label.set(format!("Approve: switch to {mode_label}"));
                approval_selected.set(0);
                shell_focus.set(ShellFocus::StatusDialog);
                pending_mode_change.set(Some(PendingModeChange {
                    target_mode: req.target_mode.clone(),
                    reason: req.reason.clone(),
                    response_tx: req.response_tx,
                }));
                // Push a status row for the transcript.
                {
                    let mut msgs = messages_arc_inner.write().unwrap();
                    let key = "mode-change:pending".to_string();
                    let mut row = TranscriptMessage::startup_status(
                        key,
                        format!("Switch to {mode_label} mode?"),
                        TranscriptStyle::StatusRunning,
                    );
                    row.status_detail = Some(req.reason);
                    msgs.push(row);
                }
                transcript_changed = true;
                // Mark that we set a pending approval label so we can clear it on completion
                pending_approval_label.set(true);
                continue;
            }

            if let AgentUiEvent::PlanConfirmationRequired(req) = event {
                // Save plan to disk FIRST so user can read it before deciding.
                let plan_file = {
                    let paths = paths.read().clone();
                    let sid = agent_session_for_loop.as_ref().map(|s| s.session_id().to_string());
                    crate::agent::plan_files::save_plan_to_disk(&req.plan_text, &paths, sid.as_deref())
                        .map_err(|e| log::error!("Failed to save plan: {e}"))
                        .ok()
                };

                activity_label.set("Plan proposed".to_string());
                approval_selected.set(PLAN_CONFIRM_DEFAULT_INDEX);
                shell_focus.set(ShellFocus::StatusDialog);
                pending_plan_confirmation.set(Some(PendingPlanConfirmation {
                    plan_text: req.plan_text.clone(),
                    plan_file,
                    session: agent_session_for_loop.clone(),
                }));
                // Push a status row for the transcript.
                {
                    let mut msgs = messages_arc_inner.write().unwrap();
                    let key = plan_confirmation_transcript_key();
                    let mut row = TranscriptMessage::startup_status(
                        key,
                        "Plan confirmation".to_string(),
                        TranscriptStyle::StatusRunning,
                    );
                    row.status_detail = Some("Review the proposed plan".to_string());
                    msgs.push(row);
                }
                transcript_changed = true;
                continue;
            }

            if let AgentUiEvent::QueueUpdate { items } = event {
                prompt_queue.write().replace(items);
                queue_ui_revision.set(queue_ui_revision.get().wrapping_add(1));
                if prompt_queue.read().is_empty() {
                    if queue_manager_open.get() {
                        queue_manager_open.set(false);
                        queue_manager_selected.set(0);
                        if pending_tool_approval.read().is_none() && pending_user_question.read().is_none() {
                            shell_focus.set(ShellFocus::Prompt);
                        }
                    }
                } else {
                    let len = prompt_queue.read().len();
                    let idx = queue_manager_selected.get().min(len.saturating_sub(1));
                    queue_manager_selected.set(idx);
                }
                continue;
            }

            if let AgentUiEvent::UserPromptCommitted { text } = event {
                // Idle submit and Ctrl+Enter (and Ctrl+R retry) already painted their row.
                let pending = pre_echoed_user_prompts.get();
                if pending > 0 {
                    pre_echoed_user_prompts.set(pending.saturating_sub(1));
                    continue;
                }
                // Auto-retry recovery prompt (not pre-echoed by the shell) — render as a
                // slim sticky status label ("Continuing tasks…") instead of a user bubble card,
                // and skip Arrow-Up history.
                if text.trim() == RETRY_CONTINUE_PROMPT {
                    let mut notice = TranscriptMessage::text("Continuing tasks…", TranscriptStyle::Meta);
                    notice.sticky_meta = true;
                    {
                        let mut msgs = messages_arc_inner.write().unwrap();
                        msgs.push(notice);
                    }
                    transcript_changed = true;
                    continue;
                }
                // Handover prompt injected by `/handover claude` — render as a slim
                // sticky meta label instead of flooding the transcript with a giant
                // inert-JSON user card.
                if text.starts_with(HANDOVER_PROMPT_PREFIX) {
                    let mut notice = TranscriptMessage::text("Handover from Claude Code…", TranscriptStyle::Meta);
                    notice.sticky_meta = true;
                    {
                        let mut msgs = messages_arc_inner.write().unwrap();
                        msgs.push(notice);
                    }
                    transcript_changed = true;
                    continue;
                }
                // Handover prompt injected by `/handover codex`.
                if text.starts_with(CODEX_HANDOVER_PROMPT_PREFIX) {
                    let mut notice = TranscriptMessage::text("Handover from Codex…", TranscriptStyle::Meta);
                    notice.sticky_meta = true;
                    {
                        let mut msgs = messages_arc_inner.write().unwrap();
                        msgs.push(notice);
                    }
                    transcript_changed = true;
                    continue;
                }
                // Goal steering prompts (continuation / budget limit) queued internally by the
                // harness — render as a slim meta label instead of a user bubble card.
                if text.starts_with(CONTINUATION_PROMPT_PREFIX) || text.starts_with(BUDGET_LIMIT_PROMPT_PREFIX) {
                    let label = if text.starts_with(CONTINUATION_PROMPT_PREFIX) {
                        "Continuing tasks…"
                    } else {
                        "Goal budget limit reached"
                    };
                    let mut notice = TranscriptMessage::text(label, TranscriptStyle::Meta);
                    notice.sticky_meta = true;
                    {
                        let mut msgs = messages_arc_inner.write().unwrap();
                        msgs.push(notice);
                    }
                    transcript_changed = true;
                    continue;
                }
                let mut submitted = TranscriptMessage::text(text, TranscriptStyle::User);
                submitted.submitted_at = Some(chrono::Utc::now());
                // Write to arc directly (no State dirty mark);
                // sync to messages State happens at end of tick.
                {
                    let mut msgs = messages_arc_inner.write().unwrap();
                    if matches!(submitted.style, TranscriptStyle::User | TranscriptStyle::SkillPrompt) {
                        crate::tui::prompt_history::push_history_entry_styled(
                            &mut prompt_history.write(),
                            &submitted.content,
                            submitted.style,
                        );
                    }
                    msgs.push(submitted);
                }
                transcript_changed = true;
                continue;
            }

            if let Some(label) = activity_label_for_event(&event, show_thinking) {
                activity_label.set(label);
            }
            // Handle SubagentStatus: init/cleanup output buffers for real-time dialog.
            if let AgentUiEvent::SubagentStatus { agent_id, phase, .. } = &event {
                let buffers_arc = subagent_output_buffers_state.read().clone();
                let mut buffers = buffers_arc.write().expect("subagent output buffers lock");
                let phase = *phase;
                match phase {
                    crate::agent::SubagentUiPhase::Running => {
                        use std::collections::hash_map::Entry;
                        if let Entry::Vacant(e) = buffers.entry(agent_id.clone()) {
                            e.insert((
                                Arc::new(RwLock::new(String::new())),
                                Arc::new(std::sync::atomic::AtomicBool::new(true)),
                            ));
                        }
                    }
                    // When a subagent finishes AND its output dialog is not open, free the
                    // output buffer — these grow unbounded over a long session otherwise.
                    crate::agent::SubagentUiPhase::Done | crate::agent::SubagentUiPhase::Error => {
                        if let Some((_text, is_running)) = buffers.get(agent_id) {
                            is_running.store(false, std::sync::atomic::Ordering::Relaxed);
                        }
                        let dialog_open = pending_subagent_output
                            .read()
                            .as_ref()
                            .is_some_and(|p| p.agent_id == *agent_id);
                        if !dialog_open {
                            buffers.remove(agent_id);
                        }
                    }
                    _ => {}
                }
            }
            // Handle SubagentOutput: update shared output buffer for real-time dialog.
            if let AgentUiEvent::SubagentOutput { agent_id, content } = &event {
                let buffers_arc = subagent_output_buffers_state.read().clone();
                let buffers = buffers_arc.read().expect("subagent output buffers lock");
                if let Some((text_arc, _is_running_arc)) = buffers.get(agent_id) {
                    if let Ok(mut current) = text_arc.write() {
                        current.push_str(content);
                    }
                    subagent_output_scroll_tick.set(subagent_output_scroll_tick.get().wrapping_add(1));
                }
            }
            {
                let mut msgs = messages_arc_inner.write().unwrap();
                if event_applier.write().apply(&mut msgs, event) {
                    transcript_changed = true;
                }
            }
        }

        while let Ok(event) = user_shell_channel.write().rx.try_recv() {
            match event {
                UserShellEvent::ToolUpdate { id, chunk } => {
                    let mut msgs = messages_arc_inner.write().unwrap();
                    if event_applier
                        .write()
                        .apply(&mut msgs, AgentUiEvent::ToolUpdate { id, output: chunk })
                    {
                        transcript_changed = true;
                    }
                }
                UserShellEvent::ToolEnd {
                    id,
                    exit_code,
                    output,
                    cancelled,
                    with_context,
                    command,
                } => {
                    let is_error = !cancelled && exit_code != Some(0);
                    {
                        let mut msgs = messages_arc_inner.write().unwrap();
                        if event_applier.write().apply(
                            &mut msgs,
                            AgentUiEvent::ToolEnd {
                                id,
                                is_error,
                                output: output.clone(),
                                details: serde_json::json!({}),
                            },
                        ) {
                            transcript_changed = true;
                        }
                    }
                    let shell_elapsed = busy_started_at
                        .read()
                        .as_ref()
                        .map(|started| format_elapsed_secs(*started))
                        .unwrap_or(0.0);
                    user_shell_abort.set(None);
                    turn_cancel_requested.set(false);
                    busy.set(false);
                    busy_started_at.set(None);
                    activity_started_at.set(None);
                    activity_label.set(String::new());
                    if cancelled {
                        idle_status_notice.set(Some(IdleStatusNotice {
                            text: format_shell_canceled_notice(shell_elapsed),
                            since: Instant::now(),
                        }));
                    }
                    if with_context
                        && !cancelled
                        && let Some(session) = agent_session_for_loop.as_ref()
                    {
                        let context = format_shell_agent_context(&command, &output);
                        agent_turn_active.set(true);
                        TurnDispatcher::spawn_turn(Arc::clone(session), context, false);
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
                    }
                }
            }
        }

        // Drive spinner while /aside is loading (shell re-render, independent of busy turn).
        if matches!(
            pending_aside.read().as_ref(),
            Some(crate::tui::aside_panel::AsidePanelState::Loading { .. })
        ) {
            aside_tick.set(aside_tick.get().wrapping_add(1));
        }

        if transcript_changed {
            // Sync the arc to State at controlled interval (one dirty per tick
            // instead of per-token). Panel reads from the arc directly.
            *messages.write() = messages_arc_inner.read().unwrap().clone();
            transcript_pending.set(true);
        }

        let transcript_publish_ms =
            transcript_publish_interval_ms(bootstrap_is_active(bootstrap_phase.get()), last_event_burst.get());
        if transcript_pending.get()
            && (run_completed || last_transcript_publish.get().elapsed().as_millis() >= transcript_publish_ms as u128)
        {
            messages_revision.set(messages_revision.get().wrapping_add(1));
            transcript_pending.set(false);
            last_transcript_publish.set(Instant::now());
        }

        if run_completed {
            // Bound in-memory transcript for live TUI. Full history is reconstructed from
            // session_entries on resume — no separate SQLite transcript archive.
            let should_trim = {
                let msgs = messages_arc_inner.read().unwrap();
                msgs.len() > MAX_MESSAGES_BEFORE_ARCHIVE
            };
            if should_trim {
                let keep = KEEP_MESSAGES;
                {
                    let mut msgs = messages_arc_inner.write().unwrap();
                    let archive_count = msgs.len().saturating_sub(keep);
                    if archive_count > 0 {
                        msgs.drain(..archive_count);
                    }
                    // Drop parsed markdown documents and tool diff text from retained
                    // messages beyond the cache window. This sheds the two biggest memory
                    // consumers for old messages:
                    //   - Parsed MarkdownDocument (styled spans + tables): 1-5 MB per message
                    //   - Tool diff text (old_text/new_text): ~500 KB per edit_file tool
                    // Keeps AssistantMarkdownBuffer metadata (stable_end, stream_complete,
                    // row counts) so layout stays correct.
                    let markdown_keep = super::MARKED_MESSAGES_WITH_MARKDOWN_CACHE;
                    let n = msgs.len();
                    if n > markdown_keep {
                        for msg in msgs[..n - markdown_keep].iter_mut() {
                            if let Some(ref mut md) = msg.markdown {
                                std::sync::Arc::make_mut(md).drop_cached_documents();
                            }
                            if let Some(ref mut tool) = msg.tool {
                                tool.strip_diff_text();
                            }
                        }
                    }
                }
                // Re-sync the State copy so it also drops the markdown caches.
                *messages.write() = messages_arc_inner.read().unwrap().clone();
            }

            pending_quit_confirm.set(false);
            clear_quit_busy_banner(&mut ephemeral_banner, &mut ephemeral_banner_generation);

            // Transition plan from in_progress → completed (or leave as-is on cancel).
            if !turn_cancel_requested.get()
                && let Some(plan_path) = active_plan_file.write().take()
            {
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
                if let Err(err) = crate::agent::plan_files::update_plan_frontmatter(&plan_path, "completed", &now, None)
                {
                    log::error!("Failed to mark plan as completed: {err}");
                }
            }

            if let Some(turn_elapsed) = run_completed_elapsed {
                session_elapsed_secs.set(accumulate_session_elapsed(session_elapsed_secs.get(), turn_elapsed));
            }
            // Keep the busy/turn state alive when a retry (or late subagent output) started
            // in the same batch right after RunCompleted — the spinner + "Retrying…" label
            // must survive until that continuation finishes and emits its own RunCompleted.
            if !live_after_run_completed {
                busy.set(false);
                agent_turn_active.set(false);
                busy_started_at.set(None);
                activity_started_at.set(None);
                activity_label.set(String::new());
                turn_token_tracker.set(None);
                pending_approval_label.set(false);
                chrome_refresh_pending.set(true);
            }
            // Follow-up prompts are drained inside the harness agent loop; no TUI re-spawn.
            // History is durable via session_entries (MessageEnd); no separate UI snapshot.

            if turn_cancel_requested.get() {
                turn_cancel_requested.set(false);
                let elapsed = run_completed_elapsed.unwrap_or(0.0);
                idle_status_notice.set(Some(IdleStatusNotice {
                    text: format_turn_canceled_notice(elapsed),
                    since: Instant::now(),
                }));
                // Desktop notification
                if let Ok(settings) = Settings::load(&paths.read().clone()) {
                    notifier::notify(
                        &settings.notifications,
                        notifier::NotifKind::TurnCancel { elapsed_secs: elapsed },
                    );
                }
                // Canceled turns do not get a stats card.
                last_turn_stats.set(None);
            } else if let Some(elapsed_secs) = run_completed_elapsed {
                idle_status_notice.set(Some(IdleStatusNotice {
                    text: format_turn_complete_notice(elapsed_secs),
                    since: Instant::now(),
                }));
                // Desktop notification
                if let Ok(settings) = Settings::load(&paths.read().clone()) {
                    notifier::notify(&settings.notifications, notifier::NotifKind::TurnComplete { elapsed_secs });
                }
                // Dimmed per-turn stats card under the last assistant reply
                // (`ui.turnStats`, default on). Falls back to duration-only when
                // no usage/model was reported.
                if !live_after_run_completed
                    && turn_stats_enabled
                    && let Some(stats) = last_turn_stats.read().clone()
                {
                    let mut msg =
                        TranscriptMessage::text(format_turn_complete_stats_line(&stats), TranscriptStyle::Meta);
                    msg.sticky_meta = true;
                    {
                        let mut msgs = messages_arc_inner.write().unwrap();
                        msgs.push(msg);
                    }
                    // Repaint immediately: the transcript sync already ran this tick.
                    *messages.write() = messages_arc_inner.read().unwrap().clone();
                    messages_revision.set(messages_revision.get().wrapping_add(1));
                }
            }
        }
    }
}
