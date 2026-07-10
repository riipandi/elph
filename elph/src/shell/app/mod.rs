//! Interactive TUI application state and bootstrap.

mod events;
mod input;
mod overlays;
mod render;
mod slash;
mod turn;

use std::path::PathBuf;
use std::sync::Arc;

use elph_tui::{
    ActivityState, PromptQueue, PromptState, SelectItem, SessionSelectorState, SlashPaletteState, Theme, ThinkingLevel,
    ToolApprovalState, ToolExecutionState, TranscriptStyle, TreeNavigatorState, default_activity_spinner,
    read_git_branch,
};
use slt::widgets::SpinnerState;
use tokio::sync::mpsc;

use crate::agent::{
    AgentUiEvent, CodingAgentSession, CreateSessionOptions, ToolApprovalChoice, create_coding_session_with_events,
    slash_commands_for_palette,
};
use crate::extensions::ExtensionHost;
use crate::platform::{Paths, Settings};

pub use render::{render_app, run_sigint_watcher, run_tui};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ActiveOverlay {
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
    pub(super) ui_rx: mpsc::UnboundedReceiver<AgentUiEvent>,
    pub(super) live_tools: Vec<ToolExecutionState>,
    pub(super) plan_modal: elph_tui::PlanConfirmationState,
    pub(super) tool_modal: ToolApprovalState,
    pub(super) pending_tool_approval_tx: Option<tokio::sync::oneshot::Sender<ToolApprovalChoice>>,
    pub(super) show_thinking: bool,
    pub(super) last_turn_elapsed_secs: f64,
    pub(super) settings: Settings,
    pub(super) paths: Paths,
    pub(super) cwd: PathBuf,
    pub(super) active_overlay: ActiveOverlay,
    pub(super) model_selector: elph_tui::ModelSelectorState,
    pub(super) session_selector: SessionSelectorState,
    pub(super) tree_navigator: TreeNavigatorState,
    pub(super) overlay_items: Vec<SelectItem>,
    pub(super) extensions: ExtensionHost,
}

impl ElphApp {
    pub async fn bootstrap(settings: Settings) -> anyhow::Result<Self> {
        let paths = crate::platform::Paths::resolve()?;
        let cwd: PathBuf = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let project_dir = cwd.display().to_string();
        let git_branch = read_git_branch(&cwd);
        let thinking = ThinkingLevel::from_setting(&settings.session.thinking_level);

        let extensions = ExtensionHost::new();
        ExtensionHost::ensure_dirs(&paths)?;
        extensions.reload(&paths, true)?;

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
        let agent_mode = crate::agent::agent_mode_from_setting(&settings.session.agent_mode);

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
            slash_commands: {
                let registry = extensions.registry();
                let guard = registry.read();
                slash_commands_for_palette(Some(&guard))
            },
            git_branch,
            collapse: elph_tui::CollapseState::default(),
            prompt_queue: PromptQueue::default(),
            session,
            ui_rx,
            live_tools: Vec::new(),
            plan_modal: elph_tui::PlanConfirmationState::default(),
            tool_modal: ToolApprovalState::default(),
            pending_tool_approval_tx: None,
            show_thinking: settings.show_thinking,
            last_turn_elapsed_secs: 0.0,
            settings,
            paths,
            cwd,
            active_overlay: ActiveOverlay::None,
            model_selector: elph_tui::ModelSelectorState::default(),
            session_selector: SessionSelectorState::default(),
            tree_navigator: TreeNavigatorState::default(),
            overlay_items: Vec::new(),
            extensions,
        })
    }

    pub(super) fn overlay_visible(&self) -> bool {
        self.active_overlay != ActiveOverlay::None
    }
}
