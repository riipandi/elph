//! Interactive TUI application shell.

mod events;
mod overlays;
mod render;
mod shell_host;
mod slash;
mod transcript_render;
mod turn;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use elph_tui::{
    ActivityState, PromptQueue, PromptState, SelectItem, SessionSelectorState, Theme, ThinkingLevel, ToolApprovalState,
    ToolExecutionState, TranscriptStyle, TreeNavigatorState, read_git_branch,
};
use tokio::sync::mpsc;

use crate::agent::{
    AgentUiEvent, CodingAgentSession, CreateSessionOptions, ToolApprovalChoice, create_coding_session_with_events,
    slash_commands_for_palette,
};
use crate::extensions::ExtensionHost;
use crate::platform::{Paths, Settings};

pub use render::{run_sigint_watcher, run_tui};
pub use shell_host::ElphShellHost;
pub use transcript_render::{TranscriptRenderOptions, entries_to_lines, entries_to_lines_simple};

/// Launch options for the interactive TUI.
#[derive(Debug, Clone, Default)]
pub struct TuiOptions {
    pub resume_id: Option<String>,
}

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
    pub(super) total_api_secs: f64,
    pub(super) started_at: Instant,
    // INVARIANT: used by overlay session swap once tuie modals land.
    #[allow(dead_code)]
    pub(super) settings: Settings,
    #[allow(dead_code)]
    pub(super) paths: Paths,
    #[allow(dead_code)]
    pub(super) cwd: PathBuf,
    pub(super) active_overlay: ActiveOverlay,
    pub(super) model_selector: elph_tui::ModelSelectorState,
    pub(super) session_selector: SessionSelectorState,
    pub(super) tree_navigator: TreeNavigatorState,
    pub(super) overlay_items: Vec<SelectItem>,
}

impl ElphApp {
    pub async fn bootstrap(settings: Settings, resume_id: Option<&str>) -> anyhow::Result<Self> {
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
            resume_id,
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
            total_api_secs: 0.0,
            started_at: Instant::now(),
            settings,
            paths,
            cwd,
            active_overlay: ActiveOverlay::None,
            model_selector: elph_tui::ModelSelectorState::default(),
            session_selector: SessionSelectorState::default(),
            tree_navigator: TreeNavigatorState::default(),
            overlay_items: Vec::new(),
        })
    }

    pub(super) fn overlay_visible(&self) -> bool {
        self.active_overlay != ActiveOverlay::None
    }

    /// Builds an exit snapshot without holding unrelated locks across `block_on`.
    pub(super) fn exit_snapshot_from(
        session_id: &str,
        total_api_secs: f64,
        started_at: Instant,
        project_dir: &str,
        session: &Arc<CodingAgentSession>,
    ) -> crate::platform::exit_message::ExitSnapshot {
        use std::path::Path;

        use elph_tui::read_git_diff_stats;

        let wall_duration_secs = started_at.elapsed().as_secs_f64();
        let (lines_added, lines_removed) = read_git_diff_stats(Path::new(project_dir));

        let (usage, cost_usd) = match elph_agent::block_on(async { session.branch_entries().await }) {
            Ok(entries) => crate::platform::exit_message::aggregate_usage_from_entries(&entries),
            Err(_) => (crate::platform::exit_message::UsageTotals::default(), 0.0),
        };

        crate::platform::exit_message::ExitSnapshot {
            session_id: session_id.to_string(),
            cost_usd,
            api_duration_secs: total_api_secs,
            wall_duration_secs,
            lines_added,
            lines_removed,
            usage,
        }
    }
}

#[cfg(test)]
mod prompt_dispatch_tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use elph_tui::{PromptAction, ShellHost};

    use super::ElphShellHost;
    use crate::platform::{self, Paths};

    async fn bootstrap_test_app(tmp: &tempfile::TempDir) -> ElphApp {
        let home = tmp.path().join("home");
        let data = tmp.path().join("data");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&home).expect("home dir");
        std::fs::create_dir_all(&project).expect("project dir");

        // SAFETY: single-threaded test runtime; vars scoped to this test process.
        unsafe {
            std::env::set_var("ELPH_HOME", &home);
            std::env::set_var("ELPH_DATA_DIR", &data);
            std::env::set_var("ELPH_PROJECT_DIR", &project);
        }

        let paths = Paths::from_dirs(home, data, project);
        platform::bootstrap::ensure_with_paths(&paths, "test")
            .await
            .expect("bootstrap home");
        let settings = platform::Settings::load(&paths).expect("load settings");
        ElphApp::bootstrap(settings, None).await.expect("bootstrap app")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn clear_empties_prompt_draft() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut app = bootstrap_test_app(&tmp).await;
        app.prompt.set_value("draft text");
        app.dispatch_prompt_action(PromptAction::Clear);
        assert!(app.prompt.value().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queue_stores_follow_up_while_busy() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut app = bootstrap_test_app(&tmp).await;
        app.agent_running = true;
        app.dispatch_prompt_action(PromptAction::Queue("follow up".into()));
        assert_eq!(app.prompt_queue.len(), 1);
        assert_eq!(app.prompt_queue.pop_front().as_deref(), Some("follow up"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quit_command_sets_should_exit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut app = bootstrap_test_app(&tmp).await;
        app.dispatch_prompt_action(PromptAction::Submit(":q".into()));
        assert!(app.should_exit);
        assert!(app.prompt.value().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slash_submit_appends_stub_and_clears_prompt() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut app = bootstrap_test_app(&tmp).await;
        app.dispatch_prompt_action(PromptAction::Submit("/help".into()));
        assert!(app.prompt.value().is_empty());
        let system = app
            .chat
            .entries
            .iter()
            .find(|entry| entry.role == elph_tui::TranscriptRole::System)
            .expect("slash stub system line");
        assert!(system.content.contains("help"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn overlay_blocks_clear_via_host() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let app = Arc::new(Mutex::new(bootstrap_test_app(&tmp).await));
        {
            let mut guard = app.lock().expect("lock");
            guard.prompt.set_value("blocked");
            guard.active_overlay = ActiveOverlay::Model;
        }

        let mut host = ElphShellHost::new(app);
        host.on_prompt_action(PromptAction::Clear);
        assert_eq!(host.prompt_text(), "blocked");
    }
}
