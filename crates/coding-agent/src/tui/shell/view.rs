//! Shell view tree builder (extracted from the MainShell component tail).

use super::*;

/// Renders the full shell view tree (previously the tail of the MainShell component).
pub(crate) fn build_shell_view(
    mut system: std::cell::RefMut<'_, SystemContext>,
    ctx: ShellCtx,
    sticky_scroll: bool,
) -> impl Into<AnyElement<'static>> {
    let ShellCtx {
        mut activity_label,
        mut activity_started_at,
        agent_mode,
        agent_session,
        mut agent_session_slot,
        mut agent_turn_active,
        mut approval_selected,
        auto_expand_thinking,
        mut busy,
        mut busy_started_at,
        mut chrome_full_redraw_pending,
        mut chrome_refresh_pending,
        chrome_stats,
        chrome_ui_revision,
        mut clipboard_toast,
        colored_status_footer,
        confetti_frame,
        mut confetti_runtime,
        cwd,
        mut draft,
        mut ephemeral_banner,
        mut ephemeral_banner_generation,
        ephemeral_expire,
        mut event_applier,
        execution_env,
        extension_host,
        fallback_model_label,
        mut file_picker_active,
        mut file_picker_index,
        file_picker_key_handled,
        mut file_picker_query,
        file_picker_show_hidden,
        mut file_picker_suppressed,
        footer_token_display,
        mut force_editor_clear,
        mut force_palette_sync,
        git_footer,
        mut idle_status_notice,
        mut input_prefix_kind,
        mut last_activity_label,
        mut live_cursor,
        mut live_draft,
        mention_index,
        mut mention_index_requested,
        mut messages,
        mut messages_arc,
        mut messages_revision,
        mut model_filter,
        mut model_input_focus,
        mut model_provider_index,
        mut model_selected_index,
        density,
        mut new_session_requested,
        mut resume_session_requested,
        on_queue_action_click,
        paths,
        mut pending_confetti,
        mut pending_feedback,
        mut pending_memory_flush,
        pending_mode_change,
        mut pending_model_selector,
        pending_plan_confirmation,
        mut pending_mcp_auth,
        pending_provider_api_key,
        mut pending_provider_connect,
        mut pending_provider_disconnect,
        mut pending_queue_click,
        mut pending_quit_confirm,
        mut pending_rename,
        mut pending_item_selector,
        mut item_selector_selected,
        mut pending_scoped_models,
        pending_subagent_output,
        mut pending_system_prompt,
        mut pending_tool_approval,
        mut pending_user_question,
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
        provider_disconnect_selected,
        mut question_answer,
        mut question_confirm_focus,
        mut question_input_focus,
        mut question_multi_checked,
        mut question_selected,
        mut question_validation_error,
        mut queue_manager_action,
        mut queue_manager_open,
        mut queue_manager_selected,
        mut queue_ui_revision,
        mut rename_value,
        mut scoped_filter,
        mut scoped_selected_index,
        screen_height,
        screen_width,
        select_mode,
        mut session_elapsed_secs,
        session_id,
        mut session_scoped_items,
        mut session_wall_started_at,
        mut shell_focus,
        shift_held,
        mut should_exit,
        show_thinking,
        skills,
        skills_count,
        slash_commands,
        mut slash_palette_active,
        mut slash_palette_index,
        mut slash_palette_query,
        mut styled_content,
        subagent_output_buffers: _subagent_output_buffers,
        subagent_output_buffers_state,
        subagent_output_scroll,
        subagent_output_scroll_tick,
        mut suppress_enter_newline,
        system_prompt_scroll,
        system_prompt_scroll_tick,
        thinking_level,
        mut turn_cancel_requested,
        mut turn_token_tracker,
        mut ui_events_slot,
        mut user_shell_abort,
        user_shell_channel,
        ..
    } = ctx;

    // Drain plain-`y` selection yank toast from Textarea into the status-row ephemeral banner.
    // Must run in the component body (not only a future tick) so the toast paints the same
    // frame State is set.
    {
        let pending = clipboard_toast.read().clone();
        if let Some(notice) = pending {
            clipboard_toast.set(None);
            let expire_tx = ephemeral_expire.read().tx.clone();
            let banner = clipboard_notice_banner(&notice);
            idle_status_notice.set(Some(IdleStatusNotice {
                text: banner.text.clone(),
                since: Instant::now(),
            }));
            show_ephemeral_banner(&mut ephemeral_banner, &mut ephemeral_banner_generation, &expire_tx, banner);
        }
    }

    // Drain mouse clicks on queue [Send]/[Edit]/[Cancel] chips.
    if let Some((idx, action)) = pending_queue_click.get() {
        pending_queue_click.set(None);
        // Ensure chips work even before Ctrl+Q opens "interactive" keyboard mode.
        if !queue_manager_open.get() {
            queue_manager_open.set(true);
            queue_manager_selected.set(idx);
            shell_focus.set(ShellFocus::StatusDialog);
        } else {
            queue_manager_selected.set(idx);
        }
        queue_manager_action.set(action);
        let session = agent_session.clone();
        let turn_active = agent_turn_active.get();
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
    }

    if should_exit.get() {
        let chrome = chrome_stats.read().clone();
        let api_duration_secs = accumulate_session_elapsed(
            session_elapsed_secs.get(),
            live_turn_elapsed_secs(busy.get(), &busy_started_at.read()),
        );
        let wall_duration_secs = session_wall_started_at.read().elapsed().as_secs_f64();
        let (lines_added, lines_removed) = crate::utils::git::read_worktree_stats(paths.read().project_dir())
            .map(|stats| (stats.lines_added, stats.lines_deleted))
            .unwrap_or((0, 0));
        // Best-effort: title resolution is bounded (400 ms) so exiting is never blocked.
        let session_title = crate::agent::session_title_for_rename(agent_session.as_ref())
            .ok()
            .filter(|title| !title.trim().is_empty());
        record_if_active(
            ExitSnapshot {
                session_id: session_id.clone(),
                session_title,
                cost_usd: chrome.cost_usd,
                api_duration_secs,
                wall_duration_secs,
                lines_added,
                lines_removed,
                usage: Default::default(),
            },
            count_submitted_user_prompts(&messages.read()),
            chrome.turn_count,
        );
        system.exit();
    }

    if chrome_full_redraw_pending.get() {
        chrome_full_redraw_pending.set(false);
        system.request_full_redraw();
    }

    let (accent_r, accent_g, accent_b) = agent_mode.get().label_rgb();
    let scanner_accent = rgb(accent_r, accent_g, accent_b);
    let chrome = chrome_stats.read().clone();
    let mcp_connected = agent_session
        .as_ref()
        .and_then(|session| session.mcp_registry())
        .map(|registry| registry.load_report().servers_ok)
        .unwrap_or(0);
    let paths_snapshot = paths.read().clone();
    let project_name = paths_snapshot
        .project_dir()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();
    let git = git_footer.read().clone();
    let model_label = if chrome.model_label.is_empty() {
        fallback_model_label.clone()
    } else {
        chrome.model_label.clone()
    };
    let supports_images = chrome.supports_images;
    let user_question_open = pending_user_question.read().is_some();
    let model_selector_open = pending_model_selector.read().is_some();
    let scoped_models_open = pending_scoped_models.read().is_some();
    let system_prompt_open = pending_system_prompt.read().is_some();
    let session_info_open = pending_system_prompt
        .read()
        .as_ref()
        .is_some_and(|d| d.title == "Session");
    let rename_open = pending_rename.read().is_some();
    let item_selector_open = pending_item_selector.read().is_some();
    let confetti_open = pending_confetti.read().is_some();
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
        || user_question_open
        || model_selector_open
        || scoped_models_open
        || system_prompt_open
        || rename_open
        || item_selector_open
        || confetti_open
        || provider_connect_open
        || mcp_auth_open
        || provider_disconnect_open
        || provider_api_key_open
        || queue_manager_is_open;
    let prompt_focused =
        !status_dialog_open && matches!(shell_focus.get(), ShellFocus::Prompt | ShellFocus::StatusDialog);
    let transcript_focused = !status_dialog_open && shell_focus.get() == ShellFocus::Transcript;
    let question_has_focus = user_question_open;
    let model_selector_has_focus = model_selector_open
        && !user_question_open
        && !system_prompt_open
        && !rename_open
        && !confetti_open
        && !scoped_models_open
        && !provider_connect_open
        && !mcp_auth_open
        && !provider_disconnect_open;
    let scoped_models_has_focus = scoped_models_open
        && !user_question_open
        && !system_prompt_open
        && !rename_open
        && !confetti_open
        && !model_selector_open
        && !provider_connect_open
        && !mcp_auth_open
        && !provider_disconnect_open;
    let system_prompt_has_focus = system_prompt_open
        && !rename_open
        && !confetti_open
        && !provider_connect_open
        && !mcp_auth_open
        && !provider_disconnect_open;
    let rename_has_focus = rename_open
        && !user_question_open
        && !system_prompt_open
        && !confetti_open
        && !model_selector_open
        && !item_selector_open;
    let item_selector_has_focus = item_selector_open
        && !user_question_open
        && !system_prompt_open
        && !rename_open
        && !confetti_open
        && !model_selector_open
        && !scoped_models_open;
    let approval_has_focus = (pending_tool_approval.read().is_some()
        || pending_mode_change.read().is_some()
        || pending_plan_confirmation.read().is_some()
        || pending_memory_flush.read().is_some()
        || *pending_feedback.read()
        || provider_connect_open
        || mcp_auth_open
        || provider_disconnect_open
        || provider_api_key_open)
        && !user_question_open
        && !model_selector_open
        && !scoped_models_open
        && !system_prompt_open
        && !rename_open
        && !confetti_open
        && !queue_manager_is_open;
    if let Some(pending) = pending_model_selector.write().as_mut() {
        let next_filter = model_selector_sanitize_filter(&model_filter.read());
        if next_filter != model_filter.read().as_str() {
            model_filter.set(next_filter.clone());
        }
        if pending.filter != next_filter {
            pending.model_index = 0;
            model_selected_index.set(0);
        }
        pending.provider_index = model_provider_index.get();
        pending.model_index = model_selected_index.get();
        pending.filter = next_filter;
        pending.input_focus = model_input_focus.get();
        pending.clamp_indices();
        if pending.provider_index != model_provider_index.get() {
            model_provider_index.set(pending.provider_index);
        }
        if pending.model_index != model_selected_index.get() {
            model_selected_index.set(pending.model_index);
        }
    }
    if let Some(pending) = pending_scoped_models.write().as_mut() {
        let next_filter = scoped_filter.read().clone();
        if pending.filter != next_filter {
            pending.set_filter(next_filter);
        }
        pending.selected_index = scoped_selected_index.get();
        pending.clamp_selection();
        if pending.selected_index != scoped_selected_index.get() {
            scoped_selected_index.set(pending.selected_index);
        }
    }
    // Sync provider connect filter from State into pending dialog so re-renders
    // pick up characters typed through the Input component.
    if let Some(pending) = pending_provider_connect.write().as_mut() {
        let next_filter = provider_connect_filter.read().clone();
        if pending.filter != next_filter {
            let providers =
                get_provider_options_for_auth_method(provider_auth_method_from_index(pending.selected_auth_method));
            let count = crate::tui::provider_connect_dialog::count_filtered(&providers, &next_filter);
            pending.filter = next_filter;
            pending.selected_provider = pending.selected_provider.min(count.saturating_sub(1));
            provider_connect_selected.set(pending.selected_provider);
        }
    }
    let model_selector_view = pending_model_selector
        .read()
        .as_ref()
        .map(ModelSelectorView::from_pending);
    let model_selector_overlay = model_selector_view.map(|view| -> AnyElement<'static> {
        element! {
            ModelSelectorBar(
                screen_width: screen_width,
                screen_height: screen_height,
                view: view,
                provider_index: Some(model_provider_index),
                model_index: Some(model_selected_index),
                filter: Some(model_filter),
                input_focus: model_input_focus.get(),
                has_focus: model_selector_has_focus,
                on_filter_submit: move |_| {
                    model_input_focus.set(ModelSelectorFocus::List);
                    if let Some(pending) = pending_model_selector.write().as_mut() {
                        pending.input_focus = ModelSelectorFocus::List;
                    }
                },
                on_confirm: move |_| {},
                on_cancel: move |_| {
                    close_model_selector(
                        &mut pending_model_selector,
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                    );
                },
            )
        }
        .into()
    });
    let scoped_models_view = pending_scoped_models
        .read()
        .as_ref()
        .map(ScopedModelsView::from_pending);
    let scoped_models_overlay = scoped_models_view.map(|view| -> AnyElement<'static> {
        element! {
            ScopedModelsBar(
                screen_width: screen_width,
                screen_height: screen_height,
                view: view,
                selected_index: Some(scoped_selected_index),
                filter: Some(scoped_filter),
                has_focus: scoped_models_has_focus,
                on_filter_submit: move |_| {
                    if let Some(pending) = pending_scoped_models.write().as_mut() {
                        sync_scoped_filter(pending, &scoped_filter.read());
                        pending.toggle_selected();
                        apply_scoped_session(pending, &mut session_scoped_items.write());
                        scoped_selected_index.set(pending.selected_index);
                    }
                },
                on_cancel: move |_| {
                    cancel_scoped_models(
                        &mut pending_scoped_models,
                        &mut session_scoped_items.write(),
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                    );
                },
            )
        }
        .into()
    });
    let rename_overlay = if rename_open {
        let rename_session = agent_session.clone();
        Some(
            element! {
                RenameDialogBar(
                    screen_width: screen_width,
                    has_focus: rename_has_focus,
                    value: Some(rename_value),
                    on_submit: move |_| {
                        let title = rename_value.read().clone();
                        let result = rename_session
                            .as_ref()
                            .map(|session| rename_session_title(session, &title))
                            .unwrap_or_else(|| Err("Agent session required for this command.".into()));
                        close_rename_dialog(
                            &mut pending_rename,
                            &mut rename_value,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                            false,
                        );
                        force_editor_clear.set(true);
                        match result {
                            Ok(()) => {
                                let notice = format!("Session renamed to “{}”.", title.trim());
                                push_transcript_message_synced(
                                    &mut messages,
                                    messages_arc,
                                    &mut messages_revision,
                                    &mut prompt_history,
                                    TranscriptMessage::text(notice, TranscriptStyle::Meta),
                                );
                            }
                            Err(message) => {
                                push_transcript_message_synced(
                                    &mut messages,
                                    messages_arc,
                                    &mut messages_revision,
                                    &mut prompt_history,
                                    TranscriptMessage::text(message, TranscriptStyle::Meta),
                                );
                            }
                        }
                    },
                    on_cancel: move |_| {
                        close_rename_dialog(
                            &mut pending_rename,
                            &mut rename_value,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                            true,
                        );
                        force_editor_clear.set(true);
                    },
                )
            }
            .into(),
        )
    } else {
        None
    };
    // Sync SelectList highlight with filtered selection.
    if let Some(pending) = pending_item_selector.read().as_ref() {
        item_selector_selected.set(pending.filtered_selected());
    }
    let item_selector_overlay = if item_selector_open {
        let pending_snap = pending_item_selector.read().clone();
        Some(
            element! {
                ItemSelectorBar(
                    screen_width: screen_width,
                    screen_height: screen_height,
                    has_focus: item_selector_has_focus,
                    pending: pending_snap,
                    selected_index: Some(item_selector_selected),
                    on_cancel: move |_| {
                        close_item_selector(
                            &mut pending_item_selector,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                            true,
                        );
                        force_editor_clear.set(true);
                    },
                )
            }
            .into(),
        )
    } else {
        None
    };
    // Same slot as slash palette / model picker: above the editor, below the status row.
    let editor_overlay = rename_overlay
        .or(item_selector_overlay)
        .or(model_selector_overlay)
        .or(scoped_models_overlay);
    let _confetti_frame = confetti_frame.get();
    let confetti_overlay = pending_confetti.read().as_ref().map(|_| -> AnyElement<'static> {
        let particles = if let Some(runtime) = confetti_runtime.write().as_mut() {
            runtime.resize(screen_width, screen_height);
            runtime.system.visible_particles()
        } else {
            Vec::new()
        };
        element! {
            ConfettiOverlay(
                screen_width: screen_width,
                screen_height: screen_height,
                particles: particles,
            )
        }
        .into()
    });
    let system_prompt_overlay = pending_system_prompt
        .read()
        .as_ref()
        .map(|pending| -> AnyElement<'static> {
            let (chrome, body_height) = if let Some(fixed) = pending.body_height {
                // Fixed body height from caller (e.g. session info).
                let outer = crate::tui::scroll_text_dialog::scroll_text_dialog_width(screen_width, pending.width_pct);
                let chrome = elph_tui::components::DialogChrome {
                    width: outer,
                    slim_header: true,
                    padding_horizontal: 1,
                    min_content_height: fixed,
                    ..Default::default()
                };
                (chrome, fixed)
            } else {
                system_prompt_dialog_chrome(screen_width, screen_height, pending.width_pct)
            };
            let mut pending_system_prompt = pending_system_prompt;
            let mut draft = draft;
            let mut live_draft = live_draft;
            let mut shell_focus = shell_focus;
            let mut force_editor_clear = force_editor_clear;
            let text_for_copy = pending.text.clone();
            let show_copy = pending.show_copy;
            let mut copy_banner = ephemeral_banner;
            let mut copy_banner_gen = ephemeral_banner_generation;
            let copy_expire = ephemeral_expire;
            element! {
                ScrollTextDialogOverlay(
                    screen_width: screen_width,
                    screen_height: screen_height,
                    title: pending.title.clone(),
                    text: pending.text.clone(),
                    body_height: body_height,
                    chrome: chrome,
                    scroll_handle: Some(system_prompt_scroll),
                    scroll_tick: system_prompt_scroll_tick.get(),
                    has_focus: system_prompt_has_focus,
                    on_esc: move |_| {
                        close_system_prompt_dialog(
                            &mut pending_system_prompt,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                            &mut force_editor_clear,
                        );
                    },
                    on_copy: if show_copy {
                        Some(HandlerMut::from(move |_| {
                            let text = &text_for_copy;
                            let banner = match copy_to_clipboard(text) {
                                Ok(()) => prompt_copy_banner(text.chars().count()),
                                Err(err) => {
                                    log::warn!("copy system prompt failed: {err}");
                                    prompt_copy_failed_banner()
                                }
                            };
                            let expire_tx = copy_expire.read().tx.clone();
                            show_ephemeral_banner(&mut copy_banner, &mut copy_banner_gen, &expire_tx, banner);
                        }))
                    } else {
                        None
                    },
                )
            }
            .into()
        });
    let user_question_view = pending_user_question.read().as_ref().map(|pending| {
        UserQuestionView::from_pending(
            pending,
            question_input_focus.get(),
            question_selected.get(),
            &question_multi_checked.read(),
            question_validation_error.read().clone(),
        )
    });
    // Depend on queue_ui_revision so Ref-backed queue mutations re-render the list/badge.
    let _queue_ui_revision = queue_ui_revision.get();
    // Tool approval takes precedence over the prompt-queue list (visible whenever non-empty).
    let status_dialog = build_status_dialog_kind(pending_tool_approval.read().as_ref())
        .or_else(|| build_mode_change_dialog_kind(pending_mode_change.read().as_ref()))
        .or_else(|| build_plan_confirmation_dialog_kind(pending_plan_confirmation.read().as_ref()))
        .or_else(|| build_memory_flush_dialog_kind(pending_memory_flush.read().as_ref()))
        .or_else(|| build_feedback_dialog_kind(*pending_feedback.read()))
        .or_else(|| {
            let pending = pending_provider_connect.read();
            let pending_ref = pending.as_ref();
            let provider_id = pending_ref.and_then(|p| p.provider_id.clone());
            let step = pending_ref.map(|p| p.step);
            let input_focus = pending_ref
                .map(|p| p.input_focus)
                .unwrap_or(ProviderConnectFocus::AuthMethodList);
            let oauth_url = pending_ref.map(|p| p.oauth_url.clone()).unwrap_or_default();
            let oauth_code = pending_ref.map(|p| p.oauth_code.clone()).unwrap_or_default();
            let oauth_provider_name = pending_ref.map(|p| p.oauth_provider_name.clone()).unwrap_or_default();
            let selected_auth_method = pending_ref.map(|p| p.selected_auth_method).unwrap_or(0);
            let fresh_open = pending_ref.map(|p| p.fresh_open).unwrap_or(false);
            let oauth_select_labels = pending_ref.map(|p| p.oauth_select_labels.clone()).unwrap_or_default();
            let oauth_select_index = pending_ref.map(|p| p.oauth_select_index).unwrap_or(0);
            let oauth_is_prompt = pending_ref.map(|p| p.oauth_is_prompt).unwrap_or(false);
            let oauth_prompt_message = pending_ref.map(|p| p.oauth_prompt_message.clone()).unwrap_or_default();
            drop(pending);

            let dialog = build_provider_connect_dialog_kind(
                provider_id,
                step,
                approval_has_focus,
                input_focus,
                selected_auth_method,
                oauth_url,
                oauth_code,
                oauth_provider_name,
                fresh_open,
                oauth_select_labels,
                oauth_select_index,
                oauth_is_prompt,
                oauth_prompt_message,
            );
            if fresh_open && let Some(ref mut pending) = *pending_provider_connect.write() {
                pending.fresh_open = false;
            }
            dialog
        })
        .or_else(|| build_provider_api_key_dialog_kind(pending_provider_api_key.read().as_ref(), approval_has_focus))
        .or_else(|| build_mcp_auth_dialog_kind(pending_mcp_auth.read().as_ref(), approval_has_focus))
        .or_else(|| {
            let pending = pending_provider_disconnect.read();
            pending.as_ref().map(|p| StatusDialogKind::ProviderDisconnect {
                provider_ids: p.provider_ids.clone(),
            })
        })
        .or_else(|| {
            build_prompt_queue_dialog_kind(
                prompt_queue.read().items(),
                queue_manager_selected.get(),
                queue_manager_action.get(),
                queue_manager_is_open,
            )
        });
    let queue_count = prompt_queue.read().len() as u32;
    let draft_for_palette = compose_palette_draft(input_prefix_kind.get(), &live_draft.read());
    let draft_body = live_draft.read().clone();
    let editor_cursor = live_cursor.get();
    let slash_palette_snapshot = build_snapshot(&draft_for_palette, &slash_commands.read(), screen_height);
    slash_palette_active.set(slash_palette_snapshot.visible);
    // Close prompt history when other palettes open or draft is non-empty.
    let picker_for_history_close =
        input_prefix_kind.get() == InputPrefixKind::Default && file_picker_open(&draft_body, editor_cursor);
    if prompt_history_open.get()
        && (slash_palette_snapshot.visible || picker_for_history_close || !live_draft.read().trim().is_empty())
    {
        prompt_history_open.set(false);
        prompt_history_index.set(0);
    }
    let prompt_history_snapshot =
        build_prompt_history_snapshot(prompt_history_open.get(), &prompt_history.read(), screen_height);
    {
        let old_index = slash_palette_index.get();
        let mut query = slash_palette_query.write();
        let mut index = old_index;
        sync_selection(&mut query, &mut index, &slash_palette_snapshot);
        // iocraft marks state dirty on every `.set()` even when the value is unchanged;
        // calling set during render without this guard causes an infinite re-render loop.
        if index != old_index {
            slash_palette_index.set(index);
        }
    }

    if file_picker_suppressed.get() {
        if let Some(mention) = active_mention_at_cursor(&draft_body, editor_cursor)
            && !mention.query.is_empty()
        {
            file_picker_suppressed.set(false);
        } else if !mention_picker_visible(&draft_body, editor_cursor) {
            file_picker_suppressed.set(false);
        }
    }
    if mention_picker_visible(&draft_body, editor_cursor) {
        mention_index_requested.set(true);
    }
    let picker_eligible = input_prefix_kind.get() == InputPrefixKind::Default
        && !slash_palette_snapshot.visible
        && !file_picker_suppressed.get()
        && file_picker_open(&draft_body, editor_cursor);
    let file_picker_snapshot = if picker_eligible {
        build_file_picker_snapshot(
            &draft_body,
            editor_cursor,
            screen_height,
            file_picker_show_hidden.get(),
            mention_index.read().as_ref().map(|arc| arc.as_ref()),
        )
    } else {
        FilePickerSnapshot::hidden()
    };
    file_picker_active.set(picker_eligible);
    styled_content.set(mention_highlight_ansi(&draft_body, editor_cursor));
    {
        let old_index = file_picker_index.get();
        let mut query = file_picker_query.write();
        let mut index = old_index;
        sync_file_picker_selection(&mut query, &mut index, &file_picker_snapshot);
        if index != old_index {
            file_picker_index.set(index);
        }
    }

    element! {
        View(
            width: screen_width,
            height: screen_height,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Center,
            margin: 0,
            padding: 0,
            position: Position::Relative,
            // Keep chrome (incl. footer) inside the terminal; overflow would push the
            // footer row past the last visible line on short screens / first paint.
            overflow: Overflow::Hidden,
        ) {
            Header(
                screen_width: screen_width,
                session_id: session_id.clone(),
                mcp_connected: mcp_connected,
                skills_count: skills_count.get(),
                cost_usd: chrome.cost_usd,
                tokens_used: chrome.tokens_used,
                context_pct: chrome.context_pct,
                context_limit: chrome.context_limit,
                token_display: footer_token_display.clone(),
            )
            TranscriptPanel(
                screen_width: screen_width,
                messages: Some(messages),
                messages_revision: Some(messages_revision),
                sticky_scroll: sticky_scroll,
                has_focus: transcript_focused,
                // Modal dialogs own the wheel; keep the transcript still underneath.
                mouse_scroll: Some(!status_dialog_open),
                text_select_mode: select_mode.get() || shift_held.get(),
                streaming_active: Some(busy.get()),
                messages_arc: Some(messages_arc.read().clone()),
                density: density,
                on_subagent_click: {
                    let mut pending_subagent_output = pending_subagent_output;
                    let subagent_output_buffers = subagent_output_buffers_state.read().clone();
                    Some(HandlerMut::from(move |(agent_id, title): (String, String)| {
                        let buffers = subagent_output_buffers.read().expect("subagent output buffers lock");
                        if let Some((text, is_running)) = buffers.get(&agent_id) {
                            let pending = PendingSubagentOutputDialog::open(
                                &agent_id,
                                &title,
                                text.clone(),
                                is_running.clone(),
                            );
                            pending_subagent_output.set(Some(pending));
                        }
                    }))
                },
            )
            #(user_question_view.map(|view| -> AnyElement<'static> {
                element! {
                    UserQuestionBar(
                        screen_width: screen_width,
                        screen_height: screen_height,
                        view: view,
                        selected_index: Some(question_selected),
                        multi_checked: Some(question_multi_checked),
                        confirm_focus: Some(question_confirm_focus),
                        answer: Some(question_answer),
                        input_focus: question_input_focus.get(),
                        has_focus: question_has_focus,
                        on_confirm_yes: {
                            move |_| {
                                let outcome = pending_user_question
                                    .write()
                                    .take()
                                    .map(|question| question.respond_confirm(true));
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
                            }
                        },
                        on_confirm_no: {
                            move |_| {
                                let outcome = pending_user_question
                                    .write()
                                    .take()
                                    .map(|question| question.respond_confirm(false));
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
                            }
                        },
                        on_text_submit: move |_| {
                            let answer = {
                                let pending_ref = pending_user_question.read();
                                let Some(pending) = pending_ref.as_ref() else {
                                    return;
                                };
                                let text = question_answer.read().clone();
                                match try_resolve_submittable_answer(
                                    pending,
                                    &text,
                                    question_selected.get(),
                                    &question_multi_checked.read(),
                                ) {
                                    Ok(answer) => answer,
                                    Err(err) => {
                                        question_validation_error.set(Some(err));
                                        return;
                                    }
                                }
                            };
                            let outcome = pending_user_question
                                .write()
                                .take()
                                .map(|question| question.respond(answer));
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
                        },
                        on_text_cancel: move |_| {
                            if pending_user_question.read().as_ref().is_some_and(|pending| {
                                pending.needs_custom_input()
                                    && !pending.needs_text_input()
                                    && question_input_focus.get().is_custom()
                            }) {
                                question_input_focus.set(QuestionInputFocus::Choices);
                                question_validation_error.set(None);
                                return;
                            }
                            let required = pending_user_question
                                .read()
                                .as_ref()
                                .is_some_and(|pending| pending.needs_text_input() && pending.is_required());
                            let optional_text = pending_user_question
                                .read()
                                .as_ref()
                                .is_some_and(|pending| pending.needs_text_input() && !pending.is_required());
                            if !required && !optional_text {
                                return;
                            }
                            if required {
                                question_answer.set(String::new());
                                question_validation_error.set(None);
                                return;
                            }
                            let outcome = pending_user_question
                                .write()
                                .take()
                                .map(|question| question.respond(String::new()));
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
                        },
                    )
                }.into()
            }))
            StatusZone(
                screen_width: screen_width,
                screen_height: screen_height,
                busy: busy.get(),
                activity_label: activity_label.read().clone(),
                accent: scanner_accent,
                activity_started_at: *activity_started_at.read(),
                busy_started_at: *busy_started_at.read(),
                session_elapsed_secs: session_elapsed_secs.get(),
                idle_notice: idle_status_notice.read().as_ref().map(|notice| notice.text.clone()),
                ephemeral_banner: ephemeral_banner
                    .read()
                    .as_ref()
                    .map(|banner| (banner.text.clone(), banner.color())),
                quit_confirm_pending: pending_quit_confirm.get(),
                select_mode: select_mode.get(),
                dialog: status_dialog,
                approval_selected: Some(approval_selected),
                approval_has_focus: approval_has_focus,
                api_key_input: Some(provider_connect_api_key),
                provider_connect_selected: Some(provider_connect_selected),
                provider_disconnect_selected: Some(provider_disconnect_selected),
                provider_connect_filter: Some(provider_connect_filter),
                provider_connect_input_focus: Some(provider_connect_input_focus),
                provider_connect_oauth_url: Some(
                    pending_provider_connect
                        .read()
                        .as_ref()
                        .map(|p| p.oauth_url.clone())
                        .unwrap_or_default(),
                ),
                provider_connect_oauth_code: Some(
                    pending_provider_connect
                        .read()
                        .as_ref()
                        .map(|p| p.oauth_code.clone())
                        .unwrap_or_default(),
                ),
                provider_connect_oauth_provider_name: Some(
                    pending_provider_connect
                        .read()
                        .as_ref()
                        .map(|p| p.oauth_provider_name.clone())
                        .unwrap_or_default(),
                ),
                queue_count: queue_count,
                on_queue_action: on_queue_action_click,
            )
            PromptChrome(
                screen_width: screen_width,
                screen_height: screen_height,
                agent_mode: agent_mode.get(),
                thinking_level: thinking_level.get(),
                // Select mode only disables mouse capture for native terminal drag-select;
                // the prompt stays focused and fully interactive.
                has_focus: prompt_focused,
                project_name: project_name.clone(),
                git: git.clone(),
                turn: chrome.turn_count,
                model_label: model_label.clone(),
                supports_images: supports_images,
                colored_status_footer: colored_status_footer,
                worker_live_count: agent_session
                    .as_ref()
                    .map(|s| s.worker_tui_badge_count())
                    .unwrap_or(0),
                worker_name: agent_session
                    .as_ref()
                    .and_then(|s| s.worker_name())
                    .unwrap_or("")
                    .to_string(),
                chrome_revision: chrome_ui_revision.get(),
                draft: Some(draft),
                live_draft: Some(live_draft),
                input_prefix_kind: Some(input_prefix_kind),
                suppress_enter_newline: Some(suppress_enter_newline),
                slash_palette_active: Some(slash_palette_active),
                file_picker_active: Some(file_picker_active),
                styled_content: Some(styled_content),
                live_cursor: Some(live_cursor),
                clipboard_toast: Some(clipboard_toast),
                prompt_editor_mirror: Some(prompt_editor_mirror),
                force_palette_sync: Some(force_palette_sync),
                force_editor_clear: Some(force_editor_clear),
                slash_palette_snapshot: slash_palette_snapshot,
                slash_palette_selected: Some(slash_palette_index),
                file_picker_snapshot: file_picker_snapshot,
                file_picker_selected: Some(file_picker_index),
                file_picker_show_hidden: file_picker_show_hidden.get(),
                prompt_history_snapshot: prompt_history_snapshot,
                prompt_history_selected: Some(prompt_history_index),
                editor_overlay: editor_overlay,
                text_select_mode: select_mode.get() || shift_held.get(),
                blocked_hint: if system_prompt_open {
                    if session_info_open {
                        Some("Viewing session info — Esc to close".to_string())
                    } else {
                        Some("Viewing system prompt — Esc to close".to_string())
                    }
                } else if user_question_open {
                    Some("Answer the question above".to_string())
                } else if rename_open {
                    Some("Rename session — Enter save · Esc cancel".to_string())
                } else if model_selector_open {
                    Some("Select a model above".to_string())
                } else if scoped_models_open {
                    Some("Edit scoped models above — Ctrl+S save · Esc cancel".to_string())
                } else {
                    None
                },
                on_escape: move |_| {
                    shell_focus.set(ShellFocus::Transcript);
                },
                on_file_picker_key: {
                    let mut draft = draft;
                    let mut live_draft = live_draft;
                    let mut live_cursor = live_cursor;
                    let mut file_picker_index = file_picker_index;
                    let mut file_picker_query = file_picker_query;
                    let mut file_picker_active = file_picker_active;
                    let mut file_picker_suppressed = file_picker_suppressed;
                    let mut file_picker_key_handled = file_picker_key_handled;
                    let mut suppress_enter_newline = suppress_enter_newline;
                    let mut force_palette_sync = force_palette_sync;
                    let mut shell_focus = shell_focus;
                    let show_hidden = file_picker_show_hidden.get();
                    move |input: PaletteKeyInput| {
                        let index_ref = mention_index.read();
                        apply_file_picker_key(
                            input,
                            &mut FilePickerApplyContext {
                                screen_height,
                                show_hidden,
                                mention_index: index_ref.as_ref().map(|arc| arc.as_ref()),
                                draft: &mut draft,
                                live_draft: &mut live_draft,
                                live_cursor: &mut live_cursor,
                                file_picker_index: &mut file_picker_index,
                                file_picker_query: &mut file_picker_query,
                                file_picker_active: &mut file_picker_active,
                                file_picker_suppressed: &mut file_picker_suppressed,
                                file_picker_key_handled: &mut file_picker_key_handled,
                                suppress_enter_newline: &mut suppress_enter_newline,
                                force_palette_sync: &mut force_palette_sync,
                                shell_focus: &mut shell_focus,
                            },
                        );
                    }
                },
                file_picker_key_handled: Some(file_picker_key_handled),
                on_submit: move |text: String| {
                        shell_focus.set(ShellFocus::Prompt);
                        if is_force_quit_command(&text) || is_quit_command(&text) {
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
                                is_force_quit_command(&text),
                            );
                            draft.set(String::new());
                            live_draft.set(String::new());
                            suppress_enter_newline.set(true);
                            return;
                        }
                        if text.trim().is_empty() {
                            return;
                        }

                        let (prefix_kind, body) =
                            resolve_submit_draft(input_prefix_kind.get(), &text, &PromptPrefixConfig::default());
                        if body.trim().is_empty() {
                            push_transcript_message_synced(
                                &mut messages,
                                messages_arc,
                                &mut messages_revision,
                                &mut prompt_history,
                                TranscriptMessage::text("Empty command.", TranscriptStyle::Meta),
                            );
                            draft.set(String::new());
                            live_draft.set(String::new());
                            suppress_enter_newline.set(true);
                            return;
                        }

                        if matches!(
                            prefix_kind,
                            InputPrefixKind::ShellWithContext | InputPrefixKind::ShellNoContext
                        ) {
                            let with_context = prefix_kind == InputPrefixKind::ShellWithContext;
                            let mut submitted = TranscriptMessage::text(body.clone(), TranscriptStyle::User);
                            submitted.submitted_at = Some(chrono::Utc::now());
                            push_transcript_message_synced(
                                &mut messages,
                                messages_arc,
                                &mut messages_revision,
                                &mut prompt_history,
                                submitted);

                            let tool_id = next_user_shell_tool_id();
                            {
                                let mut msgs = messages.write();
                                if event_applier.write().apply(
                                    &mut msgs,
                                    AgentUiEvent::ToolStart {
                                        id: tool_id.clone(),
                                        name: "shell_exec".into(),
                                        args_summary: shell_exec_args_summary(&body),
                                        user_shell: true,
                                    },
                                ) {
                                    // Sync to shared arc so background ToolUpdate/ToolEnd
                                    // (which read from messages_arc_inner) find the message.
                                    *messages_arc.write().write().unwrap() = msgs.clone();
                                    messages_revision.set(messages_revision.get().wrapping_add(1));
                                }
                            }
                            let shell_activity = user_shell_activity_label(&body);
                            mark_busy(
                                &mut BusyActivation {
                                busy: &mut busy,
                                busy_started_at: &mut busy_started_at,
                                activity_started_at: &mut activity_started_at,
                                activity_label: &mut activity_label,
                                last_activity_label: &mut last_activity_label,
                            },
                                false,
                                Some(&shell_activity),
                            );
                            let abort_token = CancellationToken::new();
                            user_shell_abort.set(Some(abort_token.clone()));
                            spawn_user_shell(
                                Arc::clone(&execution_env),
                                tool_id,
                                body,
                                with_context,
                                abort_token,
                                user_shell_channel.read().tx.clone(),
                            );
                            draft.set(String::new());
                            live_draft.set(String::new());
                            suppress_enter_newline.set(true);
                            return;
                        }

                        let slash_input = if prefix_kind == InputPrefixKind::Slash {
                            format!("/{body}")
                        } else {
                            body.clone()
                        };
                        let is_slash = prefix_kind == InputPrefixKind::Slash;

                        let extension_registry = extension_host.registry();
                        let ext_registry = extension_registry.read();
                        let templates = prompt_templates.read().clone();
                        let loaded_skills = skills.read().clone();
                        let paths_snapshot = paths.read().clone();

                        // `/tools` and `/system-prompt` are safe during a streaming turn
                        // (detached tool snapshot + fallback, or cached system prompt), so the
                        // "still responding" banner is intentionally not shown for them.

                        let outcome = handle_slash_submit(SlashContext {
                            input: &slash_input,
                            extensions: Some(&ext_registry),
                            prompt_templates: Some(&templates),
                            skills: Some(&loaded_skills),
                            agent_session: agent_session.clone(),
                            extension_host: Some(&extension_host),
                            paths: Some(&paths_snapshot),
                            cwd: Some(&cwd),
                        });

                        // The handler now ALWAYS dispatches turn-spawning work (Continue,
                        // compact, skill, template) on a background task; `turn_gate`
                        // inside the session queues it behind the active turn. When the
                        // agent is busy, suppress the pre-echo — the busy arm below shows a
                        // "queued" notice instead, and no raw slash text reaches the model.
                        let queue_follow_up = agent_turn_active.get();
                        if slash_echoes_prompt_in_transcript(&outcome) && !queue_follow_up {
                            let echo = if is_slash {
                                // Keep leading `/` so history / skill cards restore as `/name` or `/cmd`.
                                if slash_input.trim().starts_with('/') {
                                    slash_input.trim().to_string()
                                } else {
                                    format!("/{}", slash_input.trim())
                                }
                            } else {
                                // Normal prompt — no forced `/` prefix.
                                body.clone()
                            };
                            let mut submitted = TranscriptMessage::text(
                                echo,
                                TranscriptStyle::for_slash_turn_echo(&slash_input),
                            );
                            if submitted.style.is_user_input_card() {
                                submitted.submitted_at = Some(chrono::Utc::now());
                                // Sync to shared arc so the arc-to-state sync never loses this pre-echoed prompt.
                                messages_arc.write().write().unwrap().push(submitted.clone());
                                pre_echoed_user_prompts.set(pre_echoed_user_prompts.get().saturating_add(1));
                            }
                            push_transcript_message(
                                &mut messages,
                                &mut messages_revision,
                                &mut prompt_history, submitted);
                        }

                        match outcome {
                            SlashOutcome::Quit => {
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
                            SlashOutcome::NewSession => {
                                // Abort current agent turn if active
                                if let Some(session) = agent_session.as_ref() {
                                    TurnDispatcher::spawn_abort(Arc::clone(session));
                                }

                                // Clear the session slot immediately so no command can
                                // access the stale session during the async bootstrap
                                // transition (preventing /reload from restoring old history).
                                agent_session_slot.set(None);

                                // Clear the old session's event receiver so its abort/cleanup
                                // events don't leak into the new transcript.
                                ui_events_slot.set(None);

                                // Clear pending dialogs
                                pending_tool_approval.set(None);
                                if let Some(question) = pending_user_question.write().take() {
                                    question.respond(String::new());
                                }

                                // Clear prompt queue
                                prompt_queue.write().clear();
                                queue_ui_revision.set(queue_ui_revision.get().wrapping_add(1));

                                // Reset event applier so old transcript state is discarded
                                event_applier.set(TranscriptEventApplier::new(
                                    show_thinking,
                                    auto_expand_thinking,
                                ));

                                // Reset transcript to a clean "Starting new session…" line
                                messages.set(vec![TranscriptMessage::startup_status(
                                    crate::tui::startup::STARTUP_KEY_PHASE,
                                    "Starting new session…".to_string(),
                                    TranscriptStyle::StatusRunning,
                                )]);
                                messages_revision.set(messages_revision.get().wrapping_add(1));

                                // Flush the shared arc so the arc-to-state sync does not
                                // restore the old transcript on the next tick.
                                *messages_arc.write().write().unwrap() = messages.read().clone();

                                // Clear prompt history (Arrow Up) so old entries don't
                                // reappear in the new session.
                                prompt_history.set(Vec::new());

                                // Reset timing / busy / tracking state
                                busy.set(false);
                                agent_turn_active.set(false);
                                activity_label.set(String::new());
                                session_elapsed_secs.set(0.0);
                                *session_wall_started_at.write() = Instant::now();
                                busy_started_at.set(None);
                                activity_started_at.set(None);
                                last_activity_label.set(String::new());
                                turn_cancel_requested.set(false);
                                turn_token_tracker.set(None);
                                pre_echoed_user_prompts.set(0);
                                idle_status_notice.set(None);

                                // Clear ephemeral banner
                                ephemeral_banner.set(None);

                                // Clear draft / editor
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);

                                // Signal the tick loop to reload resources and restart bootstrap
                                new_session_requested.set(true);
                            }
                            SlashOutcome::ResumeSession { session_id } => {
                                // Graceful multi-worker release before rebinding the session.
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
                                    TranscriptMessage::text(
                                        format!("Resuming session {session_id}…"),
                                        TranscriptStyle::Meta,
                                    ),
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
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
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
                                    paths: &paths_snapshot,
                                    server_name: server_name.clone(),
                                });
                                // Auto-start when a unique server matches the prefill (e.g. `/mcp auth figma`).
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
                                        let _ = start_mcp_oauth_for_server(
                                            pending_mcp_auth,
                                            &paths_snapshot,
                                            &server,
                                        );
                                    }
                                }
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenProviderDisconnectDialog { provider_id } => {
                                let auth_store_path = paths_snapshot.auth_store_path();
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
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenModelSelector { filter } => {
                                let settings = Settings::load(&paths_snapshot).ok();
                                let default_pm = settings
                                    .as_ref()
                                    .and_then(|s| s.models.default_provider_and_model());
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
                                    paths: &paths_snapshot,
                                    provider_id: sel_provider.as_deref(),
                                    model_id: sel_model.as_deref(),
                                    session_scoped: Some(session_scoped_items.read().as_slice()),
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenScopedModels => {
                                open_scoped_models(OpenScopedModelsArgs {
                                    pending: &mut pending_scoped_models,
                                    selected_index: &mut scoped_selected_index,
                                    filter: &mut scoped_filter,
                                    draft: &mut draft,
                                    live_draft: &mut live_draft,
                                    shell_focus: &mut shell_focus,
                                    paths: &paths_snapshot,
                                    session_scoped: &session_scoped_items.read(),
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenSystemPromptDialog { text } => {
                                open_system_prompt_dialog(OpenSystemPromptDialogArgs {
                                    pending: &mut pending_system_prompt,
                                    shell_focus: &mut shell_focus,
                                    text,
                                    width_pct: None,
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
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
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenSessionInfoDialog { text } => {
                                open_scroll_text_dialog(OpenScrollTextDialogArgs {
                                    pending: &mut pending_system_prompt,
                                    shell_focus: &mut shell_focus,
                                    title: "Session".to_string(),
                                    text,
                                    width_pct: DEFAULT_SCROLL_TEXT_WIDTH_PCT,
                                    body_height: None,
                                    show_copy: true,
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
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
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
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
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
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
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
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
                                    purpose,
                                    title,
                                    items,
                                    preferred_value,
                                    footer_hint,
                                });
                                if let Some(p) = pending_item_selector.read().as_ref() {
                                    item_selector_selected.set(p.filtered_selected());
                                }
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
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
                                suppress_enter_newline.set(true);
                                return;
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
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenMemoryFlushConfirm {
                                memory_count,
                                task_count,
                            } => {
                                *pending_memory_flush.write() = Some(PendingMemoryFlush {
                                    memory_count,
                                    task_count,
                                });
                                approval_selected.set(1); // Cancel by default
                                shell_focus.set(ShellFocus::StatusDialog);
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenFeedbackDialog => {
                                *pending_feedback.write() = true;
                                approval_selected.set(FEEDBACK_DEFAULT_INDEX);
                                shell_focus.set(ShellFocus::StatusDialog);
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
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
                            SlashOutcome::BackgroundTask => {
                                // Background task already dispatched by handle_slash_submit.
                                // No busy/turn state needed — the task will emit Status events
                                // when done.
                            }
                            SlashOutcome::BackgroundTaskQuiet => {
                                // Like BackgroundTask, but no slash input is echoed as a user
                                // card — the handover task delivers its own transcript events
                                // (slim meta line / stream) and derives busy state from the
                                // agent loop, so a read failure never strands a stale busy UI.
                            }
                            SlashOutcome::SpawnAgentTurn | SlashOutcome::SpawnAgentTurnQuiet if is_slash => {
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
                                    // Skill/compact already spawned via handle_slash_submit.
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
                            SlashOutcome::SpawnAgentTurn | SlashOutcome::SpawnAgentTurnQuiet => {
                                debug_assert!(!slash_outcome_is_ui_only(&SlashOutcome::SpawnAgentTurn));
                                if agent_turn_active.get() {
                                    prompt_queue.write().push_follow_up_local(body.clone());
                                    queue_ui_revision.set(queue_ui_revision.get().wrapping_add(1));
                                    if let Some(session) = agent_session.as_ref() {
                                        TurnDispatcher::spawn_follow_up(Arc::clone(session), body.clone());
                                    }
                                } else if let Some(session) = agent_session.as_ref() {
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
                                    TurnDispatcher::spawn_turn(Arc::clone(session), body.clone(), false);
                                } else {
                                    push_transcript_message_synced(
                                &mut messages,
                                messages_arc,
                                &mut messages_revision,
                                &mut prompt_history,
                                TranscriptMessage::text(
                                            "Agent session unavailable — check logs or run `elph doctor`.",
                                            TranscriptStyle::Meta,
                                        ),
                                    );
                                }
                            }
                        }

                    draft.set(String::new());
                    live_draft.set(String::new());
                    // Avoid a stuck Slash/! prefix making the next submit look like a slash command.
                    input_prefix_kind.set(InputPrefixKind::Default);
                    suppress_enter_newline.set(true);
                },
            )
            #(confetti_overlay)
            #(system_prompt_overlay)
            #(pending_subagent_output.read().as_ref().map(|pending| -> AnyElement<'static> {
                let (chrome, body_height) = crate::tui::subagent_output_dialog::subagent_output_dialog_chrome(
                    screen_width, screen_height, pending.width_pct
                );
                let mut pending_subagent_output = pending_subagent_output;
                let mut shell_focus = shell_focus;
                element! {
                    SubagentOutputDialogOverlay(
                        screen_width: screen_width,
                        screen_height: screen_height,
                        agent_id: pending.agent_id.clone(),
                        title: pending.title.clone(),
                        text: pending.text.clone(),
                        is_running: pending.is_running.clone(),
                        body_height: body_height,
                        chrome: chrome,
                        scroll_handle: Some(subagent_output_scroll),
                        scroll_tick: subagent_output_scroll_tick.get(),
                        has_focus: true,
                        on_esc: move |_| {
                            // Free the output buffer when closing the dialog — these
                            // buffers otherwise accumulate across the whole session.
                            if let Some(id) = pending_subagent_output
                                .read()
                                .as_ref()
                                .map(|p| p.agent_id.clone())
                            {
                                let buffers_arc = subagent_output_buffers_state.read().clone();
                                let mut buffers = buffers_arc.write().expect("subagent output buffers lock");
                                buffers.remove(&id);
                            }
                            pending_subagent_output.set(None);
                            shell_focus.set(ShellFocus::Prompt);
                        },
                    )
                }
                .into()
            }))
        }
    }
}
