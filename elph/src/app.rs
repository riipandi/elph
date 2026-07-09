use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use elph_agent::PlanConfirmationChoice;
use elph_tui::{
    ActivityState, FooterInfo, FooterTokenDisplay, ModelSelectorAction, ModelSelectorState, PlanConfirmationAction,
    PlanConfirmationState, PromptAction, PromptOpts, PromptQueue, PromptState, SelectItem, SessionSelectorAction,
    SessionSelectorState, ShellChrome, ShellRegion, SlashPaletteState, StatusBarInfo, Theme, ThinkingLevel,
    ToolApprovalAction, ToolApprovalState, ToolExecutionState, TranscriptEntry, TranscriptStyle, TreeNavigatorAction,
    TreeNavigatorState, TuiPlanConfirmationChoice, TuiToolApprovalChoice, consume_ctrl_char, consume_key_code_mod,
    default_activity_spinner, default_run_config, disable_keyboard_enhancement, enable_keyboard_enhancement,
    handle_model_selector_input, handle_plan_confirmation_input, handle_prompt_input, handle_session_selector_input,
    handle_slash_palette_keys, handle_tool_approval_input, handle_tree_navigator_input, is_quit_command, push_capped,
    read_git_branch, render_agent_shell, render_chat_stream_with_agent, render_model_selector,
    render_plan_confirmation, render_prompt, render_session_selector, render_tool_approval, render_tree_navigator,
    sigint_channel, slash_palette_visible,
};
use slt::{Context, KeyCode, KeyModifiers, widgets::SpinnerState};
use tokio::sync::mpsc;

use crate::coding_agent::{
    AgentUiEvent, CodingAgentSession, CreateSessionOptions, SlashDispatch, ToolApprovalChoice,
    create_coding_session_with_events, dispatch_slash_command, list_model_select_items, list_session_select_items,
    list_tree_select_items, slash_commands_for_palette,
};
use crate::runtime::exit_message::ExitSnapshot;
use crate::runtime::{Paths, Settings, WAS_INTERRUPTED, exit_message, handle_prompt_interrupt};
use crate::tui::{TranscriptApplier, TurnDispatcher, transcript_from_branch};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ActiveOverlay {
    #[default]
    None,
    Model,
    Session,
    Tree,
}

pub struct ElphApp {
    pub prompt: PromptState,
    pub chat: elph_tui::ChatStreamState,
    pub theme: Theme,
    pub should_exit: bool,
    pub session_id: String,
    pub turn: u32,
    pub project_dir: String,
    pub thinking: ThinkingLevel,
    pub agent_running: bool,
    pub activity: ActivityState,
    pub spinner: SpinnerState,
    pub slash_palette: SlashPaletteState,
    pub slash_commands: Vec<elph_tui::SlashCommand>,
    pub git_branch: Option<String>,
    pub collapse: elph_tui::CollapseState,
    pub prompt_queue: PromptQueue,
    pub session: Arc<CodingAgentSession>,
    ui_rx: mpsc::UnboundedReceiver<AgentUiEvent>,
    live_tools: Vec<ToolExecutionState>,
    plan_modal: PlanConfirmationState,
    tool_modal: ToolApprovalState,
    pending_tool_approval_tx: Option<tokio::sync::oneshot::Sender<ToolApprovalChoice>>,
    show_thinking: bool,
    last_turn_elapsed_secs: f64,
    settings: Settings,
    paths: Paths,
    cwd: PathBuf,
    active_overlay: ActiveOverlay,
    model_selector: ModelSelectorState,
    session_selector: SessionSelectorState,
    tree_navigator: TreeNavigatorState,
    overlay_items: Vec<SelectItem>,
}

impl ElphApp {
    pub async fn bootstrap(settings: Settings) -> anyhow::Result<Self> {
        let paths = crate::runtime::Paths::resolve()?;
        let cwd: PathBuf = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let project_dir = cwd.display().to_string();
        let git_branch = read_git_branch(&cwd);
        let thinking = ThinkingLevel::from_setting(&settings.session.thinking_level);

        let (session, ui_rx) = create_coding_session_with_events(CreateSessionOptions {
            paths: &paths,
            settings: &settings,
            cwd: &cwd,
            resume_id: None,
            provider_override: None,
            model_override: None,
        })
        .await?;

        let session = Arc::new(session);
        let session_id = session.session_id().to_string();
        let model_name = session.model_display();
        let agent_mode = crate::coding_agent::agent_mode_from_setting(&settings.session.agent_mode);

        let mut chat = elph_tui::ChatStreamState::new();
        chat.style = TranscriptStyle::Composer;
        chat.show_thinking = settings.show_thinking;

        let mut prompt = PromptState::new(&model_name);
        prompt.mode = agent_mode;

        Ok(Self {
            prompt,
            chat,
            theme: Theme::detect(),
            should_exit: false,
            session_id,
            turn: 0,
            project_dir,
            thinking,
            agent_running: false,
            activity: ActivityState::default(),
            spinner: default_activity_spinner(),
            slash_palette: SlashPaletteState::default(),
            slash_commands: slash_commands_for_palette(),
            git_branch,
            collapse: elph_tui::CollapseState::default(),
            prompt_queue: PromptQueue::default(),
            session,
            ui_rx,
            live_tools: Vec::new(),
            plan_modal: PlanConfirmationState::default(),
            tool_modal: ToolApprovalState::default(),
            pending_tool_approval_tx: None,
            show_thinking: settings.show_thinking,
            last_turn_elapsed_secs: 0.0,
            settings,
            paths,
            cwd,
            active_overlay: ActiveOverlay::None,
            model_selector: ModelSelectorState::default(),
            session_selector: SessionSelectorState::default(),
            tree_navigator: TreeNavigatorState::default(),
            overlay_items: Vec::new(),
        })
    }

    fn overlay_visible(&self) -> bool {
        self.active_overlay != ActiveOverlay::None
    }

    fn close_overlay(&mut self) {
        self.active_overlay = ActiveOverlay::None;
        self.overlay_items.clear();
        self.model_selector = ModelSelectorState::default();
        self.session_selector = SessionSelectorState::default();
        self.tree_navigator = TreeNavigatorState::default();
    }

    fn rebuild_transcript_from_session(&mut self) {
        let session = Arc::clone(&self.session);
        let show_thinking = self.show_thinking;
        match elph_agent::block_on(async move { session.branch_entries().await }) {
            Ok(entries) => {
                self.chat.entries = transcript_from_branch(&entries, show_thinking);
                self.live_tools.clear();
                self.chat.pin_to_tail();
            }
            Err(err) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("Failed to load transcript: {err}")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }

    fn swap_session(&mut self, resume_id: Option<&str>) {
        if self.agent_running {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("Cannot switch session while agent is running"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }

        let paths = self.paths.clone();
        let settings = self.settings.clone();
        let cwd = self.cwd.clone();
        let resume_id_owned = resume_id.map(str::to_string);

        match elph_agent::block_on(async move {
            create_coding_session_with_events(CreateSessionOptions {
                paths: &paths,
                settings: &settings,
                cwd: &cwd,
                resume_id: resume_id_owned.as_deref(),
                provider_override: None,
                model_override: None,
            })
            .await
        }) {
            Ok((session, ui_rx)) => {
                self.session = Arc::new(session);
                self.ui_rx = ui_rx;
                self.session_id = self.session.session_id().to_string();
                self.prompt.model_name = self.session.model_display();
                self.turn = 0;
                self.prompt_queue.clear();
                self.rebuild_transcript_from_session();
                let label = if resume_id.is_some() {
                    "Resumed session"
                } else {
                    "Started new session"
                };
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(label),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            Err(err) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("Session switch failed: {err}")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }

    fn open_model_selector(&mut self) {
        self.overlay_items = list_model_select_items();
        if self.overlay_items.is_empty() {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("No models available"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }
        self.model_selector = ModelSelectorState::default();
        self.active_overlay = ActiveOverlay::Model;
    }

    fn open_session_selector(&mut self) {
        let session = Arc::clone(&self.session);
        match elph_agent::block_on(async move { list_session_select_items(session.session_manager()).await }) {
            Ok(items) if items.is_empty() => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system("No sessions to resume"),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            Ok(items) => {
                self.overlay_items = items;
                self.session_selector = SessionSelectorState::default();
                self.active_overlay = ActiveOverlay::Session;
            }
            Err(err) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("Failed to list sessions: {err}")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }

    fn open_tree_navigator(&mut self) {
        if self.agent_running {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("Cannot navigate tree while agent is running"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }
        let session = Arc::clone(&self.session);
        let entries = elph_agent::block_on(async move { session.harness().session_entries().await });
        self.overlay_items = list_tree_select_items(&entries);
        if self.overlay_items.is_empty() {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("No navigable entries in session tree"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }
        self.tree_navigator = TreeNavigatorState::default();
        self.active_overlay = ActiveOverlay::Tree;
    }

    fn handle_overlay_input(&mut self, ui: &Context) -> bool {
        if !self.overlay_visible() {
            return false;
        }

        match self.active_overlay {
            ActiveOverlay::Model => {
                match handle_model_selector_input(ui, &mut self.model_selector, &self.overlay_items, true) {
                    ModelSelectorAction::Selected(item) => {
                        let value = item.value.clone();
                        let session = Arc::clone(&self.session);
                        match elph_agent::block_on(async move { session.set_model_from_value(&value).await }) {
                            Ok(display) => {
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system(format!("Model set to {display}")),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                                self.prompt.model_name = display;
                            }
                            Err(err) => {
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system(format!("Failed to set model: {err}")),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                            }
                        }
                        self.close_overlay();
                    }
                    ModelSelectorAction::Cancelled => self.close_overlay(),
                    ModelSelectorAction::None => {}
                }
            }
            ActiveOverlay::Session => {
                match handle_session_selector_input(ui, &mut self.session_selector, &self.overlay_items, true) {
                    SessionSelectorAction::Selected(item) => {
                        let resume_id = item.value.clone();
                        self.close_overlay();
                        self.swap_session(Some(&resume_id));
                    }
                    SessionSelectorAction::Cancelled => self.close_overlay(),
                    SessionSelectorAction::None => {}
                }
            }
            ActiveOverlay::Tree => {
                match handle_tree_navigator_input(ui, &mut self.tree_navigator, &self.overlay_items, true) {
                    TreeNavigatorAction::Selected(item) => {
                        let entry_id = item.value.clone();
                        let session = Arc::clone(&self.session);
                        match elph_agent::block_on(async move { session.navigate_tree_to(&entry_id).await }) {
                            Ok(()) => {
                                self.rebuild_transcript_from_session();
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system("Navigated session tree"),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                            }
                            Err(err) => {
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system(format!("Tree navigation failed: {err}")),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                            }
                        }
                        self.close_overlay();
                    }
                    TreeNavigatorAction::Cancelled => self.close_overlay(),
                    TreeNavigatorAction::None => {}
                }
            }
            ActiveOverlay::None => {}
        }
        true
    }

    fn poll_ui_events(&mut self) {
        while let Ok(event) = self.ui_rx.try_recv() {
            match event {
                AgentUiEvent::PlanConfirmationRequired(req) => {
                    self.plan_modal = PlanConfirmationState::open(req.plan_id, req.plan_text);
                }
                AgentUiEvent::ToolApprovalRequired(req) => {
                    self.tool_modal = ToolApprovalState::open(req.tool_call_id, req.tool_name, req.args_summary);
                    self.pending_tool_approval_tx = Some(req.response_tx);
                }
                AgentUiEvent::RunCompleted { elapsed_secs } => {
                    self.agent_running = false;
                    self.last_turn_elapsed_secs = elapsed_secs;
                    self.activity.clear();
                    self.drain_prompt_queue();
                }
                other => {
                    let mut applier =
                        TranscriptApplier::new(&mut self.chat.entries, &mut self.live_tools, self.show_thinking);
                    applier.apply(other);
                }
            }
        }
    }

    pub fn handle_global_keys(&mut self, ui: &mut Context) {
        if self.plan_modal.visible {
            return;
        }
        if self.tool_modal.visible {
            return;
        }
        if self.overlay_visible() {
            return;
        }

        if self.agent_running {
            if consume_ctrl_char(ui, 'c') {
                self.activity.request_cancel();
                TurnDispatcher::spawn_abort(Arc::clone(&self.session));
            }
        } else if consume_ctrl_char(ui, 'c') && handle_prompt_interrupt(&mut self.prompt.textarea) {
            self.should_exit = true;
            return;
        }

        if !self.agent_running {
            if consume_ctrl_char(ui, 'x') || consume_ctrl_char(ui, 'd') {
                self.should_exit = true;
                return;
            }
            if consume_ctrl_char(ui, 'q') {
                self.should_exit = true;
                use std::sync::atomic::Ordering;
                WAS_INTERRUPTED.store(true, Ordering::Relaxed);
                #[cfg(unix)]
                crate::runtime::SHOULD_KILL_PARENT.store(true, Ordering::Relaxed);
                return;
            }
        }

        if consume_ctrl_char(ui, 'a') && !self.agent_running {
            self.prompt.cycle_mode();
            let mode = self.prompt.mode;
            let session = Arc::clone(&self.session);
            elph_agent::block_on(async move {
                let _ = session.set_agent_mode(mode).await;
            });
        }
        if consume_ctrl_char(ui, 't') {
            self.theme = self.theme.toggle();
        }
        if consume_ctrl_char(ui, 'o') {
            let len = self.chat.entries.len();
            self.collapse.toggle_newest(len);
            self.chat.collapse = self.collapse.clone();
        }
        if consume_key_code_mod(ui, KeyCode::Tab, KeyModifiers::SHIFT) {
            self.thinking = self.thinking.next();
            let level = self.thinking;
            let session = Arc::clone(&self.session);
            elph_agent::block_on(async move {
                let _ = session.set_thinking_level(level).await;
            });
        }
    }

    fn start_turn(&mut self, user_text: &str, steer: bool) {
        self.turn = self.turn.saturating_add(1);
        self.agent_running = true;
        self.activity = ActivityState::responding();
        push_capped(
            &mut self.chat.entries,
            TranscriptEntry::user(user_text),
            elph_tui::DEFAULT_TRANSCRIPT_CAP,
        );
        self.chat.pin_to_tail();
        TurnDispatcher::spawn_turn(Arc::clone(&self.session), user_text.to_string(), steer);
    }

    fn drain_prompt_queue(&mut self) {
        if self.agent_running {
            return;
        }
        if let Some(next) = self.prompt_queue.pop_front() {
            self.start_turn(&next, false);
        }
    }

    fn handle_slash(&mut self, input: &str) {
        let Some(dispatch) = dispatch_slash_command(input) else {
            return;
        };
        match dispatch {
            SlashDispatch::Quit => self.should_exit = true,
            SlashDispatch::Compact => {
                let session = Arc::clone(&self.session);
                elph_agent::block_on(async move {
                    let _ = session.compact().await;
                });
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system("Compacting session…"),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            SlashDispatch::Message(msg) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(msg),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            SlashDispatch::NotImplemented(cmd) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("{cmd} — not yet implemented")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            SlashDispatch::OpenModelSelector => self.open_model_selector(),
            SlashDispatch::OpenSessionSelector => self.open_session_selector(),
            SlashDispatch::OpenTree => self.open_tree_navigator(),
            SlashDispatch::NewSession => self.swap_session(None),
            SlashDispatch::OpenSettings
            | SlashDispatch::OpenLogin
            | SlashDispatch::Reload
            | SlashDispatch::ShowSession => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system("Command recognized — overlay wiring pending"),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }

    pub fn handle_prompt(&mut self, ui: &mut Context) {
        if self.handle_overlay_input(ui) {
            return;
        }

        if self.plan_modal.visible {
            match handle_plan_confirmation_input(ui, &mut self.plan_modal) {
                PlanConfirmationAction::Resolved(choice) => {
                    let mapped = match choice {
                        TuiPlanConfirmationChoice::StayInPlan => PlanConfirmationChoice::StayInPlan,
                        TuiPlanConfirmationChoice::Implement => PlanConfirmationChoice::Implement,
                        TuiPlanConfirmationChoice::ImplementFresh => PlanConfirmationChoice::ImplementFresh,
                    };
                    let session = Arc::clone(&self.session);
                    elph_agent::block_on(async move {
                        let _ = session.resolve_plan(mapped).await;
                    });
                }
                PlanConfirmationAction::Cancelled => {}
                PlanConfirmationAction::None => {}
            }
            return;
        }

        if self.tool_modal.visible {
            if let ToolApprovalAction::Resolved(choice) = handle_tool_approval_input(ui, &mut self.tool_modal) {
                let mapped = match choice {
                    TuiToolApprovalChoice::Approve => ToolApprovalChoice::Approve,
                    TuiToolApprovalChoice::Reject => ToolApprovalChoice::Reject,
                    TuiToolApprovalChoice::AllowSession => ToolApprovalChoice::AllowSession,
                };
                if let Some(tx) = self.pending_tool_approval_tx.take() {
                    let _ = tx.send(mapped);
                }
            }
            return;
        }

        let input = self.prompt.value();
        if slash_palette_visible(&input) {
            match handle_slash_palette_keys(ui, &mut self.slash_palette, &input, &self.slash_commands) {
                elph_tui::SlashPaletteAction::Complete(cmd) => {
                    self.prompt.textarea.set_value(&cmd);
                    return;
                }
                elph_tui::SlashPaletteAction::Run(cmd) => {
                    self.prompt.textarea.set_value(&cmd);
                }
                _ => {}
            }
        }

        match handle_prompt_input(ui, &mut self.prompt, self.agent_running) {
            PromptAction::Submit(text) => {
                if is_quit_command(&text) {
                    self.prompt.clear();
                    self.should_exit = true;
                    return;
                }
                if text.trim_start().starts_with('/') {
                    self.handle_slash(&text);
                    self.prompt.clear();
                    return;
                }
                self.start_turn(&text, false);
            }
            PromptAction::Queue(text) => {
                if is_quit_command(&text) {
                    self.prompt.clear();
                    self.should_exit = true;
                    return;
                }
                self.prompt_queue.push_back(text);
            }
            PromptAction::Steer(text) => {
                if is_quit_command(&text) {
                    self.prompt.clear();
                    self.should_exit = true;
                    return;
                }
                self.activity.request_cancel();
                TurnDispatcher::spawn_abort(Arc::clone(&self.session));
                if self.agent_running {
                    self.agent_running = false;
                    self.activity.clear();
                }
                self.start_turn(&text, true);
            }
            PromptAction::Clear => self.prompt.clear(),
            PromptAction::CycleMode | PromptAction::None => {}
        }
    }
}

pub fn render_app(ui: &mut Context, app: &mut ElphApp) {
    app.poll_ui_events();
    app.handle_global_keys(ui);
    app.theme.apply_to(ui);

    if app.plan_modal.visible {
        render_plan_confirmation(ui, &app.plan_modal, app.theme);
        return;
    }
    if app.tool_modal.visible {
        render_tool_approval(ui, &app.tool_modal, app.theme);
        return;
    }

    let overlay = app.active_overlay;
    let overlay_items = app.overlay_items.clone();
    let overlay_visible = app.overlay_visible();

    let project_dir = app.project_dir.clone();
    let project_name = elph_tui::path_basename(&project_dir).to_string();
    let model_name = app.prompt.model_name.clone();
    let session_id = app.session_id.clone();
    let thinking = app.thinking.label();
    let branch = app.git_branch.clone();
    let branch_ref = branch.as_deref();
    let model_ref = if model_name.is_empty() {
        None
    } else {
        Some(model_name.as_str())
    };

    let footer = FooterInfo {
        model_name: model_ref,
        provider: None,
        thinking_level: thinking,
        supports_images: false,
        cost_usd: 0.0,
        tokens_used: 0,
        context_pct: 0.0,
        context_limit: 200_000,
        token_display: FooterTokenDisplay::Both,
        project_dir: &project_name,
        session_id: &session_id,
        mode: app.prompt.mode,
        turn: app.turn,
        branch: branch_ref,
        git_additions: 0,
        git_deletions: 0,
    };

    let status_bar = StatusBarInfo {
        branch: branch_ref,
        directory: &project_dir,
        tokens_used: footer.tokens_used,
        context_limit: footer.context_limit,
        git_additions: footer.git_additions,
        git_deletions: footer.git_deletions,
        turn: app.turn.max(1),
        turn_total: None,
    };

    let input = app.prompt.value();
    app.chat.collapse = app.collapse.clone();

    if app.agent_running && !app.activity.visible {
        app.activity = ActivityState::responding();
    }

    let slash_commands = app.slash_commands.clone();
    let slash_palette = app.slash_palette.clone();
    let theme = app.theme;
    let agent_running = app.agent_running;

    let chrome = ShellChrome::composer(
        status_bar,
        footer,
        &input,
        &slash_commands,
        &slash_palette,
        agent_running,
        if agent_running && app.activity.visible {
            Some(app.activity.clone())
        } else {
            None
        },
        app.spinner.clone(),
    );

    render_agent_shell(ui, theme, chrome, |ui, region| match region {
        ShellRegion::Chat => {
            render_chat_stream_with_agent(ui, &mut app.chat, theme, agent_running);
        }
        ShellRegion::Input => {
            app.handle_prompt(ui);
            if !overlay_visible {
                render_prompt(
                    ui,
                    &mut app.prompt,
                    theme,
                    PromptOpts {
                        running: agent_running,
                        composer: true,
                        queued_count: app.prompt_queue.len(),
                    },
                );
            }
        }
    });

    if overlay_visible {
        match overlay {
            ActiveOverlay::Model => {
                let current = app.prompt.model_name.clone();
                render_model_selector(ui, &overlay_items, &current, &mut app.model_selector, true);
            }
            ActiveOverlay::Session => {
                render_session_selector(ui, &overlay_items, &mut app.session_selector, true);
            }
            ActiveOverlay::Tree => {
                render_tree_navigator(ui, &overlay_items, &mut app.tree_navigator, true);
            }
            ActiveOverlay::None => {}
        }
    }
}

pub async fn run_sigint_watcher(app: Arc<Mutex<ElphApp>>) {
    let mut sigint = sigint_channel();
    while sigint.recv().await {
        if let Ok(mut guard) = app.lock() {
            if guard.agent_running {
                guard.activity.request_cancel();
                TurnDispatcher::spawn_abort(Arc::clone(&guard.session));
            } else if handle_prompt_interrupt(&mut guard.prompt.textarea) {
                guard.should_exit = true;
            }
        }
    }
}

pub fn run_tui() -> std::io::Result<()> {
    let _ = enable_keyboard_enhancement();
    struct KeyboardGuard;
    impl Drop for KeyboardGuard {
        fn drop(&mut self) {
            let _ = disable_keyboard_enhancement();
        }
    }
    let _guard = KeyboardGuard;

    let settings = crate::runtime::Paths::resolve()
        .and_then(|paths| Settings::load(&paths))
        .map_err(std::io::Error::other)?;

    let app = elph_agent::block_on(ElphApp::bootstrap(settings)).map_err(std::io::Error::other)?;
    let app = Arc::new(Mutex::new(app));
    let watcher_app = Arc::clone(&app);

    std::thread::spawn(move || {
        elph_agent::block_on(run_sigint_watcher(watcher_app));
    });

    let config = default_run_config();
    slt::run_with(config, move |ui: &mut Context| {
        let mut guard = app.lock().expect("elph app lock");
        if guard.should_exit {
            exit_message::record(ExitSnapshot {
                session_id: guard.session_id.clone(),
                has_history: !guard.chat.entries.is_empty(),
            });
            ui.quit();
        }
        render_app(ui, &mut guard);
    })
}
