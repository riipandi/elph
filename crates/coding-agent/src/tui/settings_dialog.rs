//! Interactive settings editor for the TUI.
//!
//! The editor deliberately works on the typed [`Settings`] value instead of
//! exposing JSON text. This keeps values validated and preserves the existing
//! home/project layering rule: the editor writes the home layer only.

use elph_tui::components::{
    DialogChrome, DialogHeader, DialogShellOverlay, UiTheme, dialog_body_min_height, dialog_max_content_height,
};
use elph_tui::install_theme_config;
use iocraft::prelude::*;

use crate::platform::{Paths, Settings};
use crate::tui::focus::ShellFocus;

const MIN_DIALOG_WIDTH: u16 = 56;
const MAX_DIALOG_WIDTH: u16 = 104;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    General,
    Appearance,
    Models,
    Memory,
    Advanced,
}

impl SettingsCategory {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Appearance,
        Self::Models,
        Self::Memory,
        Self::Advanced,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Models => "Models",
            Self::Memory => "Memory",
            Self::Advanced => "Advanced",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|category| *category == self).unwrap_or(0)
    }

    fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldSection {
    General,
    Appearance,
    Models,
    PromptEncoding,
    Memory,
    Notifications,
    Compaction,
    Sessions,
    Workers,
    Resources,
    Logging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Toggle,
    Text,
    Number,
    Choice(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
struct FieldSpec {
    key: &'static str,
    label: &'static str,
    description: &'static str,
    kind: FieldKind,
}

const THEME_CHOICES: &[&str] = &["auto", "dark", "light"];
const FOOTER_CHOICES: &[&str] = &["both", "percentage", "count"];
const DENSITY_CHOICES: &[&str] = &["compact", "loose"];
const THINKING_CHOICES: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const GPU_CHOICES: &[&str] = &["auto", "on", "off"];
const ROTATION_CHOICES: &[&str] = &["daily", "hourly", "size"];
const ENCODING_CHOICES: &[&str] = &["off", "toon", "auto"];
const DELIMITER_CHOICES: &[&str] = &["comma", "tab", "pipe"];

fn section_fields(category: FieldSection) -> &'static [FieldSpec] {
    match category {
        FieldSection::General => &[
            FieldSpec {
                key: "preferredChatLanguage",
                label: "Chat language",
                description: "Language used for AI responses. Code and docs remain English.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "simplifiedTechnicalEnglish",
                label: "Technical English",
                description: "Follow ASD-STE100 simplified technical English in responses.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "maxRetries",
                label: "Provider retries",
                description: "Retries for temporary provider and network failures.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "defaultTimeout",
                label: "Provider timeout",
                description: "Inactivity limit for streams, for example 120s or 2m.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "shellPath",
                label: "Shell path",
                description: "Optional custom shell binary. A leading ~ is expanded.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "shellCommandPrefix",
                label: "Shell prefix",
                description: "Text prepended to every shell command.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "httpProxy",
                label: "HTTP proxy",
                description: "Proxy URL used when HTTP_PROXY and HTTPS_PROXY are unset.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "quietStartup",
                label: "Quiet startup",
                description: "Hide bootstrap spinner and startup chatter.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "defaultTools",
                label: "Default tools",
                description: "Comma-separated builtin tool names. Empty means all tools.",
                kind: FieldKind::Text,
            },
        ],
        FieldSection::Appearance => &[
            FieldSpec {
                key: "ui.theme",
                label: "Theme",
                description: "Auto follows the terminal; dark and light force a palette.",
                kind: FieldKind::Choice(THEME_CHOICES),
            },
            FieldSpec {
                key: "ui.showThinking",
                label: "Show thinking",
                description: "Show model reasoning blocks in the transcript.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "ui.autoExpandThinking",
                label: "Expand thinking",
                description: "Start thinking blocks expanded in the transcript.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "ui.stickyScroll",
                label: "Sticky scroll",
                description: "Keep the latest user prompt visible while scrolling replies.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "ui.footerTokenDisplay",
                label: "Footer tokens",
                description: "Show both percentage and count, or just one.",
                kind: FieldKind::Choice(FOOTER_CHOICES),
            },
            FieldSpec {
                key: "ui.coloredStatusFooter",
                label: "Colored footer",
                description: "Use mode, thinking, and git accent colors in the footer.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "ui.density",
                label: "Transcript density",
                description: "Compact packs collapsed tool calls; loose adds breathing room.",
                kind: FieldKind::Choice(DENSITY_CHOICES),
            },
            FieldSpec {
                key: "ui.showHiddenFiles",
                label: "Show hidden files",
                description: "Include dotfiles in @ file search.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "ui.allowModeChangeWhileBusy",
                label: "Mode changes while busy",
                description: "Allow mode changes while the agent is streaming or running tools.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "ui.turnStats",
                label: "Turn statistics",
                description: "Show token and model stats below completed turns.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "ui.atomicPaste",
                label: "Atomic paste",
                description: "Represent long clipboard pastes as expandable markers.",
                kind: FieldKind::Toggle,
            },
        ],
        FieldSection::Models => &[
            FieldSpec {
                key: "models.defaultModel",
                label: "New-session model",
                description: "provider/model_id seed for new sessions. Empty lets you choose later.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "models.sessionTitleModel",
                label: "Session title model",
                description: "Model for titles, or inherit the active session model.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "models.compactionModel",
                label: "Compaction model",
                description: "Model for summaries, or inherit the active session model.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "models.treeBranchSummaries",
                label: "Branch summary model",
                description: "Model for tree branch summaries, or inherit.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "models.defaultThinkingLevel",
                label: "New-session thinking",
                description: "Reasoning level seeded into new sessions.",
                kind: FieldKind::Choice(THINKING_CHOICES),
            },
            FieldSpec {
                key: "models.showConfiguredOnly",
                label: "Configured providers only",
                description: "Limit model picker lists to providers with credentials.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "models.scopedModels",
                label: "Scoped models",
                description: "Comma-separated provider/model_id values used by Ctrl+P.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "models.enabled",
                label: "Model filter",
                description: "Comma-separated model globs. Use ! or - to exclude; empty means all.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "models.embedModel",
                label: "Embedding model",
                description: "Local model used for memory search; supports a Hugging Face id.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "models.embedQuantized",
                label: "Quantized embeddings",
                description: "Prefer smaller quantized embedding weights when available.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "models.embedGpuAcceleration",
                label: "Embedding acceleration",
                description: "Use GPU always, never, or auto-detect.",
                kind: FieldKind::Choice(GPU_CHOICES),
            },
            FieldSpec {
                key: "models.thinkingBudgets.minimal",
                label: "Minimal thinking budget",
                description: "Optional custom token budget for minimal reasoning.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "models.thinkingBudgets.low",
                label: "Low thinking budget",
                description: "Optional custom token budget for low reasoning.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "models.thinkingBudgets.medium",
                label: "Medium thinking budget",
                description: "Optional custom token budget for medium reasoning.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "models.thinkingBudgets.high",
                label: "High thinking budget",
                description: "Optional custom token budget for high reasoning.",
                kind: FieldKind::Number,
            },
        ],
        FieldSection::PromptEncoding => &[
            FieldSpec {
                key: "promptEncoding.mode",
                label: "Prompt encoding",
                description: "Encode structured tool results as TOON, automatically or always.",
                kind: FieldKind::Choice(ENCODING_CHOICES),
            },
            FieldSpec {
                key: "promptEncoding.minBytes",
                label: "Encoding minimum bytes",
                description: "Only encode payloads at or above this size.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "promptEncoding.minSavingsRatio",
                label: "Encoding savings ratio",
                description: "Minimum compression ratio required before encoding.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "promptEncoding.delimiter",
                label: "Encoding delimiter",
                description: "Delimiter used for TOON tabular arrays.",
                kind: FieldKind::Choice(DELIMITER_CHOICES),
            },
            FieldSpec {
                key: "promptEncoding.tabularDelimiter",
                label: "Tabular delimiter",
                description: "Optional delimiter override for tabular arrays.",
                kind: FieldKind::Choice(DELIMITER_CHOICES),
            },
            FieldSpec {
                key: "promptEncoding.targets.toolResultText",
                label: "Encode tool-result text",
                description: "Allow TOON to rewrite eligible tool-result text blocks.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "promptEncoding.targets.structuredDetails",
                label: "Encode structured details",
                description: "Allow TOON to rewrite MCP structured_content details.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "promptEncoding.preamble",
                label: "Encoding preamble",
                description: "Optional hint placed above TOON fenced blocks.",
                kind: FieldKind::Text,
            },
        ],
        FieldSection::Memory => &[
            FieldSpec {
                key: "memory.enabled",
                label: "Memory hooks",
                description: "Enable automatic memory hooks and bootstrap injection.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "memory.autoRecall",
                label: "Auto recall",
                description: "Inject relevant memories into each turn.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "memory.autoCaptureWork",
                label: "Capture work",
                description: "Journal successful file mutations as work memories.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "memory.autoCaptureExploration",
                label: "Capture exploration",
                description: "Capture discovery and project-map exploration.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "memory.topK",
                label: "Recall top K",
                description: "Maximum semantic memory hits per retrieval.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "memory.contextBudgetChars",
                label: "Context budget",
                description: "Maximum characters for injected memory blocks.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "memory.minQueryLength",
                label: "Minimum query length",
                description: "Prompt length required to trigger automatic recall.",
                kind: FieldKind::Number,
            },
        ],
        FieldSection::Notifications => &[
            FieldSpec {
                key: "notifications.enabled",
                label: "Notifications",
                description: "Master switch for desktop notifications.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "notifications.onTurnComplete",
                label: "Turn complete",
                description: "Notify when the agent finishes a turn.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "notifications.onToolPermission",
                label: "Tool permission",
                description: "Notify when the agent asks to run a tool.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "notifications.onUserQuestion",
                label: "User question",
                description: "Notify when the agent asks you a question.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "notifications.onError",
                label: "Errors",
                description: "Notify on agent, MCP, and bootstrap errors.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "notifications.onTurnCancel",
                label: "Canceled turns",
                description: "Notify when a running turn is canceled.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "notifications.onStartupReady",
                label: "Startup ready",
                description: "Notify when bootstrap completes.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "notifications.minTurnDurationSecs",
                label: "Minimum turn duration",
                description: "Seconds before a turn-complete notification is sent.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "notifications.appName",
                label: "Notification app name",
                description: "Application name shown in notification banners.",
                kind: FieldKind::Text,
            },
        ],
        FieldSection::Compaction => &[
            FieldSpec {
                key: "compaction.thresholdPct",
                label: "Trigger threshold",
                description: "Context usage percentage that triggers compaction (1–100).",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "compaction.keepRecentTokens",
                label: "Recent tokens",
                description: "Tokens to retain after compaction.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "compaction.reserveTokens",
                label: "Reserved tokens",
                description: "Tokens reserved for the next model response.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "compaction.physicalPrune",
                label: "Prune old entries",
                description: "Delete entries before the compaction boundary.",
                kind: FieldKind::Toggle,
            },
        ],
        FieldSection::Sessions => &[
            FieldSpec {
                key: "session.enabled",
                label: "Session storage",
                description: "Enable automatic session GC and size enforcement.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "session.gcOnOpen",
                label: "GC on open",
                description: "Run cleanup when opening the project store.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "session.maxSessionsPerCwd",
                label: "Sessions per project",
                description: "Maximum non-pinned sessions; 0 means unlimited.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "session.maxSessionAgeDays",
                label: "Session age (days)",
                description: "Delete old non-pinned sessions; 0 means no age limit.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "session.maxEntriesPerSession",
                label: "Entries per session",
                description: "Soft pressure threshold for tree entries; 0 means unlimited.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "session.maxStoreDbBytes",
                label: "Store size (bytes)",
                description: "Soft .elph/store.db budget; 0 means unlimited.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "session.protectLatestPerCwd",
                label: "Protect latest",
                description: "Never auto-delete the latest session for a project.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "session.maxEntryPayloadBytes",
                label: "Entry payload (bytes)",
                description: "Truncate oversized stored entry payloads.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "session.journalKeepTurns",
                label: "Journal turns",
                description: "Recent turns covered by harness journal entries.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "session.maxTerminalFilesPerSession",
                label: "Terminal files",
                description: "Terminal output files retained per session; 0 means unlimited.",
                kind: FieldKind::Number,
            },
        ],
        FieldSection::Workers => &[
            FieldSpec {
                key: "workers.enabled",
                label: "Workers",
                description: "Enable leases, registry, worker tools, and heartbeat.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "workers.name",
                label: "Worker name",
                description: "Preferred name shown to other live workers. Empty auto-allocates.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "workers.purpose",
                label: "Worker purpose",
                description: "Short purpose shown in worker_list.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "workers.heartbeatSecs",
                label: "Heartbeat (seconds)",
                description: "Interval for leases, registry, and file claims.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "workers.leaseStaleSecs",
                label: "Stale lease (seconds)",
                description: "Window before a dead worker is reclaimed.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "workers.inboxPollMs",
                label: "Inbox poll (ms)",
                description: "Interval for durable mailbox delivery.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "workers.askTimeoutMs",
                label: "Ask timeout (ms)",
                description: "Time before a worker ask is marked timed out.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "workers.maxHops",
                label: "Max message hops",
                description: "Maximum forwarding hops for worker messages.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "workers.tuiShowPeers",
                label: "Show peer badge",
                description: "Show the compact peer badge in the TUI.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "workers.fileLeases",
                label: "File leases",
                description: "Protect shared-cwd mutations with cross-process claims.",
                kind: FieldKind::Toggle,
            },
        ],
        FieldSection::Resources => &[
            FieldSpec {
                key: "resources.skills",
                label: "Extra skills",
                description: "Comma-separated skill paths or include/exclude patterns.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "resources.prompts",
                label: "Extra prompts",
                description: "Comma-separated prompt template paths.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "resources.disabledSkills",
                label: "Disabled skills",
                description: "Comma-separated skill names or globs to disable.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "resources.disabledPrompts",
                label: "Disabled prompts",
                description: "Comma-separated prompt names or globs to disable.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "resources.enableSkillCommands",
                label: "Skill slash commands",
                description: "Register discovered skills as /name commands.",
                kind: FieldKind::Toggle,
            },
        ],
        FieldSection::Logging => &[
            FieldSpec {
                key: "logging.level",
                label: "Log level",
                description: "Global level or a rustlog spec such as info.",
                kind: FieldKind::Text,
            },
            FieldSpec {
                key: "logging.file",
                label: "File logging",
                description: "Write rolling JSONL logs to the application data directory.",
                kind: FieldKind::Toggle,
            },
            FieldSpec {
                key: "logging.rotation",
                label: "Rotation",
                description: "Rotate logs hourly, daily, or when they reach a size.",
                kind: FieldKind::Choice(ROTATION_CHOICES),
            },
            FieldSpec {
                key: "logging.maxFiles",
                label: "Maximum files",
                description: "Optional cap on retained rotated log files.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "logging.maxBytes",
                label: "Maximum bytes",
                description: "Optional size cap for log rotation.",
                kind: FieldKind::Number,
            },
            FieldSpec {
                key: "logging.trace",
                label: "Trace file",
                description: "Write fastrace events to a separate JSONL file.",
                kind: FieldKind::Toggle,
            },
        ],
    }
}

fn fields(category: SettingsCategory) -> Vec<FieldSpec> {
    let sections: &[FieldSection] = match category {
        SettingsCategory::General => &[FieldSection::General],
        SettingsCategory::Appearance => &[FieldSection::Appearance, FieldSection::Notifications],
        SettingsCategory::Models => &[
            FieldSection::Models,
            FieldSection::PromptEncoding,
            FieldSection::Compaction,
        ],
        SettingsCategory::Memory => &[FieldSection::Memory, FieldSection::Sessions],
        SettingsCategory::Advanced => &[FieldSection::Workers, FieldSection::Resources, FieldSection::Logging],
    };
    sections
        .iter()
        .flat_map(|section| section_fields(*section).iter().copied())
        .collect()
}

#[derive(Debug, Clone)]
pub struct PendingSettings {
    pub settings: Settings,
    pub category: SettingsCategory,
    pub selected: usize,
    pub editing: bool,
    pub edit_buffer: String,
    pub dirty: bool,
    pub error: Option<String>,
    pub project_layer_present: bool,
}

impl PendingSettings {
    pub fn open(paths: &Paths) -> Self {
        let settings = Settings::load_home(paths).unwrap_or_else(|_| Settings::defaults());
        Self {
            settings,
            category: SettingsCategory::General,
            selected: 0,
            editing: false,
            edit_buffer: String::new(),
            dirty: false,
            error: None,
            project_layer_present: paths.project_settings_path().is_file(),
        }
    }

    fn current_field(&self) -> FieldSpec {
        let list = fields(self.category);
        list[self.selected.min(list.len().saturating_sub(1))]
    }

    fn select_category(&mut self, category: SettingsCategory) {
        self.category = category;
        self.selected = 0;
        self.editing = false;
        self.error = None;
    }

    fn change_category(&mut self, delta: isize) {
        let next = (self.category.index() as isize + delta).rem_euclid(SettingsCategory::ALL.len() as isize) as usize;
        self.select_category(SettingsCategory::from_index(next));
    }
}

pub fn open_settings_dialog(
    pending: &mut Ref<Option<PendingSettings>>,
    paths: &Paths,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
) {
    pending.set(Some(PendingSettings::open(paths)));
    draft.set(String::new());
    live_draft.set(String::new());
    shell_focus.set(ShellFocus::StatusDialog);
}

pub fn handle_settings_key(
    pending: &mut Ref<Option<PendingSettings>>,
    revision: &mut State<u64>,
    paths: &Paths,
    shell_focus: &mut State<ShellFocus>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> bool {
    let mut pending_guard = pending.write();
    let Some(state) = pending_guard.as_mut() else {
        return false;
    };

    if state.editing {
        if modifiers.is_empty() && code == KeyCode::Esc {
            state.editing = false;
            state.error = None;
        } else if modifiers.is_empty() && code == KeyCode::Backspace {
            state.edit_buffer.pop();
        } else if modifiers.is_empty() && code == KeyCode::Enter {
            let field = state.current_field();
            match set_value(&mut state.settings, field.key, &state.edit_buffer) {
                Ok(()) => {
                    state.dirty = true;
                    state.editing = false;
                    state.error = None;
                }
                Err(message) => state.error = Some(message),
            }
        } else if modifiers.is_empty()
            && let KeyCode::Char(ch) = code
            && !ch.is_control()
        {
            state.edit_buffer.push(ch);
            state.error = None;
        }
        revision.set(revision.get().wrapping_add(1));
        return true;
    }

    if modifiers.is_empty() && code == KeyCode::Esc {
        if state.dirty {
            if state.error.as_deref() == Some("Unsaved changes — press Esc again to discard or Ctrl+S to save.") {
                *pending_guard = None;
                shell_focus.set(ShellFocus::Prompt);
            } else {
                state.error = Some("Unsaved changes — press Esc again to discard or Ctrl+S to save.".into());
            }
        } else {
            *pending_guard = None;
            shell_focus.set(ShellFocus::Prompt);
        }
        revision.set(revision.get().wrapping_add(1));
        return true;
    }

    if modifiers.contains(KeyModifiers::CONTROL)
        && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META)
        && matches!(code, KeyCode::Char('s') | KeyCode::Char('S'))
    {
        match Settings::save(paths, &state.settings) {
            Ok(()) => {
                install_theme_config(&state.settings.ui.theme_config());
                state.dirty = false;
                state.error = Some("Saved to home settings. Restart to apply process-level changes.".into());
            }
            Err(err) => state.error = Some(format!("Could not save settings: {err:#}")),
        }
        revision.set(revision.get().wrapping_add(1));
        return true;
    }

    if modifiers.is_empty() {
        match code {
            KeyCode::Left | KeyCode::Char('[') => state.change_category(-1),
            KeyCode::Right | KeyCode::Tab | KeyCode::Char(']') => state.change_category(1),
            KeyCode::Up => {
                state.selected = state.selected.saturating_sub(1);
                state.error = None;
            }
            KeyCode::Down => {
                state.selected = (state.selected + 1).min(fields(state.category).len().saturating_sub(1));
                state.error = None;
            }
            KeyCode::Enter => {
                let field = state.current_field();
                match field.kind {
                    FieldKind::Toggle => {
                        let next = if value_for(&state.settings, field.key) == "true" {
                            "false"
                        } else {
                            "true"
                        };
                        let _ = set_value(&mut state.settings, field.key, next);
                        state.dirty = true;
                    }
                    FieldKind::Choice(choices) => {
                        let current = value_for(&state.settings, field.key);
                        let index = choices.iter().position(|choice| *choice == current).unwrap_or(0);
                        let _ = set_value(&mut state.settings, field.key, choices[(index + 1) % choices.len()]);
                        state.dirty = true;
                    }
                    FieldKind::Text | FieldKind::Number => {
                        state.editing = true;
                        state.edit_buffer = value_for(&state.settings, field.key);
                    }
                }
                state.error = None;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                let field = state.current_field();
                let default_value = value_for(&Settings::defaults(), field.key);
                let before = state.settings.clone();
                match set_value(&mut state.settings, field.key, &default_value) {
                    Ok(()) if state.settings != before => {
                        state.dirty = true;
                        state.error = Some(format!("{} reset to default.", field.label));
                    }
                    Ok(()) => {
                        state.error = Some(format!("{} is already at its default.", field.label));
                    }
                    Err(message) => state.error = Some(message),
                }
            }
            KeyCode::Char(ch) if ('1'..='5').contains(&ch) => {
                state.select_category(SettingsCategory::from_index((ch as usize) - '1' as usize))
            }
            _ => return true,
        }
        revision.set(revision.get().wrapping_add(1));
    }
    true
}

#[derive(Props)]
pub struct SettingsDialogOverlayProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub pending: PendingSettings,
    pub revision: u64,
    pub on_esc: HandlerMut<'static, ()>,
}

impl Default for SettingsDialogOverlayProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            pending: PendingSettings::open(&Paths::resolve().expect("resolve paths")),
            revision: 0,
            on_esc: HandlerMut::default(),
        }
    }
}

#[component]
pub fn SettingsDialogOverlay(props: &mut SettingsDialogOverlayProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let theme = UiTheme::default();
    let outer = ((props.screen_width.saturating_sub(2) as u32 * 94 / 100) as u16).clamp(
        MIN_DIALOG_WIDTH.min(props.screen_width.saturating_sub(2)),
        MAX_DIALOG_WIDTH.min(props.screen_width.saturating_sub(2)).max(1),
    );
    let chrome = DialogChrome {
        width: outer.max(1),
        padding_horizontal: 1,
        slim_header: true,
        min_content_height: 8,
        ..DialogChrome::default()
    };
    let body_width = chrome.inner_body_width().max(1);
    let max_body = dialog_max_content_height(props.screen_height, &chrome, 2);
    let body_height = dialog_body_min_height(max_body).max(8);
    let fields_for_category = fields(props.pending.category);
    let current = props.pending.current_field();
    let value = if props.pending.editing {
        props.pending.edit_buffer.clone()
    } else {
        value_for(&props.pending.settings, current.key)
    };

    let tabs = SettingsCategory::ALL
        .iter()
        .enumerate()
        .map(|(index, category)| {
            if *category == props.pending.category {
                format!("[{} {}]", index + 1, category.label())
            } else {
                format!(" {} {} ", index + 1, category.label())
            }
        })
        .collect::<Vec<_>>()
        .join("  ");

    let tab_rows = ((tabs.chars().count() as u16).saturating_add(body_width.saturating_sub(1)) / body_width).max(1);
    let field_viewport = body_height.saturating_sub(6 + tab_rows.saturating_sub(1)).max(1) as usize;
    let visible_start = props.pending.selected.saturating_sub(field_viewport.saturating_sub(1));
    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    for (index, field) in fields_for_category
        .iter()
        .enumerate()
        .skip(visible_start)
        .take(field_viewport)
    {
        let selected = index == props.pending.selected;
        let marker = if selected { "›" } else { " " };
        let field_value = value_for(&props.pending.settings, field.key);
        let display_value = if selected && props.pending.editing {
            value.clone()
        } else {
            field_value
        };
        let display_value = if display_value.is_empty() {
            "—".to_string()
        } else {
            display_value
        };
        let value_color = if selected && props.pending.editing {
            theme.accent
        } else if selected {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let label_color = if selected && props.pending.editing {
            theme.accent
        } else if selected {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        rows.push(
            element! {
                View(width: body_width, height: 1u16, padding_left: 1, padding_right: 1, flex_direction: FlexDirection::Row, flex_shrink: 0f32) {
                    Text(content: format!("{marker}  {:<28}", field.label), color: label_color, weight: if selected { Weight::Bold } else { Weight::Normal }, wrap: TextWrap::NoWrap)
                    Text(content: display_value, color: value_color, wrap: TextWrap::NoWrap)
                }
            }
            .into(),
        );
    }

    let status = if let Some(error) = props.pending.error.clone() {
        error
    } else if props.pending.editing {
        "Editing value · Enter apply · Esc cancel".to_string()
    } else {
        format!(
            "field {}/{} · {}{}",
            props.pending.selected + 1,
            fields_for_category.len(),
            if props.pending.dirty { "unsaved changes · " } else { "" },
            if props.pending.project_layer_present {
                "project overrides present"
            } else {
                "home settings"
            }
        )
    };

    let on_esc = props.on_esc.take();
    element! {
        DialogShellOverlay(
            screen_width: props.screen_width,
            screen_height: props.screen_height,
            chrome: chrome,
            header: DialogHeader::title(format!("Settings · {}", props.pending.category.label())),
            theme: Some(theme),
            on_esc: on_esc,
            on_copy: None,
        ) {
            View(width: body_width, height: body_height, flex_direction: FlexDirection::Column, overflow: Overflow::Hidden, flex_shrink: 0f32) {
                Text(content: tabs, color: theme.accent, wrap: TextWrap::Wrap)
                Text(content: "[/]/Tab category · ↑/↓ field · Enter edit/toggle · R reset · 1–5 jump", color: theme.text_muted, wrap: TextWrap::NoWrap)
                View(width: body_width, height: 1u16, padding_top: 1, flex_shrink: 0f32) {
                    Text(content: format!("{}  {}", current.label, current.description), color: theme.text_secondary, wrap: TextWrap::Wrap)
                }
                View(width: body_width, flex_direction: FlexDirection::Column, padding_top: 1, flex_shrink: 0f32) {
                    #(rows)
                }
                View(width: body_width, padding_top: 1, flex_shrink: 0f32) {
                    Text(content: status, color: if props.pending.error.is_some() { theme.warning } else { theme.text_muted }, wrap: TextWrap::Wrap)
                }
                Text(content: "Ctrl+S save · R reset selected · Esc close (Esc twice discards unsaved changes)", color: theme.text_hint, wrap: TextWrap::NoWrap)
            }
        }
    }
}

fn value_for(settings: &Settings, key: &str) -> String {
    match key {
        "preferredChatLanguage" => settings.preferred_chat_language.clone(),
        "simplifiedTechnicalEnglish" => settings.simplified_technical_english.to_string(),
        "maxRetries" => settings.max_retries.to_string(),
        "defaultTimeout" => settings.default_timeout.clone(),
        "shellPath" => settings.shell_path.clone().unwrap_or_default(),
        "shellCommandPrefix" => settings.shell_command_prefix.clone().unwrap_or_default(),
        "httpProxy" => settings.http_proxy.clone().unwrap_or_default(),
        "quietStartup" => settings.quiet_startup.to_string(),
        "defaultTools" => join_list(settings.default_tools.as_deref()),
        "promptEncoding.mode" => settings
            .prompt_encoding
            .as_ref()
            .map(|config| format!("{:?}", config.mode).to_ascii_lowercase())
            .unwrap_or_else(|| "off".into()),
        "promptEncoding.minBytes" => settings
            .prompt_encoding
            .as_ref()
            .map(|config| config.min_bytes.to_string())
            .unwrap_or_else(|| {
                elph_agent::prompt::PromptEncodingConfig::default()
                    .min_bytes
                    .to_string()
            }),
        "promptEncoding.minSavingsRatio" => settings
            .prompt_encoding
            .as_ref()
            .map(|config| config.min_savings_ratio.to_string())
            .unwrap_or_else(|| {
                elph_agent::prompt::PromptEncodingConfig::default()
                    .min_savings_ratio
                    .to_string()
            }),
        "promptEncoding.delimiter" => settings
            .prompt_encoding
            .as_ref()
            .map(|config| format!("{:?}", config.delimiter).to_ascii_lowercase())
            .unwrap_or_else(|| "comma".into()),
        "promptEncoding.tabularDelimiter" => settings
            .prompt_encoding
            .as_ref()
            .and_then(|config| config.tabular_delimiter)
            .map(|value| format!("{value:?}").to_ascii_lowercase())
            .unwrap_or_else(|| "tab".into()),
        "promptEncoding.targets.toolResultText" => settings
            .prompt_encoding
            .as_ref()
            .map(|config| config.targets.tool_result_text.to_string())
            .unwrap_or_else(|| "true".into()),
        "promptEncoding.targets.structuredDetails" => settings
            .prompt_encoding
            .as_ref()
            .map(|config| config.targets.structured_details.to_string())
            .unwrap_or_else(|| "true".into()),
        "promptEncoding.preamble" => settings
            .prompt_encoding
            .as_ref()
            .and_then(|config| config.preamble.clone())
            .unwrap_or_default(),
        "ui.theme" => settings.ui.theme.clone(),
        "ui.showThinking" => settings.ui.show_thinking.to_string(),
        "ui.autoExpandThinking" => settings.ui.auto_expand_thinking.to_string(),
        "ui.stickyScroll" => settings.ui.sticky_scroll.to_string(),
        "ui.footerTokenDisplay" => settings.ui.footer_token_display.clone(),
        "ui.coloredStatusFooter" => settings.ui.colored_status_footer.to_string(),
        "ui.density" => settings.ui.density.clone(),
        "ui.showHiddenFiles" => settings.ui.show_hidden_files.to_string(),
        "ui.allowModeChangeWhileBusy" => settings.ui.allow_mode_change_while_busy.to_string(),
        "ui.turnStats" => settings.ui.turn_stats.to_string(),
        "ui.atomicPaste" => settings.ui.atomic_paste.to_string(),
        "models.defaultModel" => settings.models.default_model.clone().unwrap_or_default(),
        "models.sessionTitleModel" => settings.models.session_title_model.clone(),
        "models.compactionModel" => settings.models.compaction_model.clone(),
        "models.treeBranchSummaries" => settings.models.tree_branch_summaries.clone(),
        "models.defaultThinkingLevel" => settings.models.default_thinking_level.clone(),
        "models.showConfiguredOnly" => settings.models.show_configured_only.to_string(),
        "models.scopedModels" => join_list(Some(&settings.models.scoped_models)),
        "models.enabled" => join_list(Some(&settings.models.enabled)),
        "models.embedModel" => settings.models.embed_model.to_model_id(),
        "models.embedQuantized" => settings.models.embed_quantized.to_string(),
        "models.embedGpuAcceleration" => settings.models.embed_gpu_acceleration.to_string(),
        "models.thinkingBudgets.minimal" => settings
            .models
            .thinking_budgets
            .as_ref()
            .and_then(|budgets| budgets.minimal)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        "models.thinkingBudgets.low" => settings
            .models
            .thinking_budgets
            .as_ref()
            .and_then(|budgets| budgets.low)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        "models.thinkingBudgets.medium" => settings
            .models
            .thinking_budgets
            .as_ref()
            .and_then(|budgets| budgets.medium)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        "models.thinkingBudgets.high" => settings
            .models
            .thinking_budgets
            .as_ref()
            .and_then(|budgets| budgets.high)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        "memory.enabled" => settings.memory.enabled.to_string(),
        "memory.autoRecall" => settings.memory.auto_recall.to_string(),
        "memory.autoCaptureWork" => settings.memory.auto_capture_work.to_string(),
        "memory.autoCaptureExploration" => settings.memory.auto_capture_exploration.to_string(),
        "memory.topK" => settings.memory.top_k.to_string(),
        "memory.contextBudgetChars" => settings.memory.context_budget_chars.to_string(),
        "memory.minQueryLength" => settings.memory.min_query_length.to_string(),
        "notifications.enabled" => settings.notifications.enabled.to_string(),
        "notifications.onTurnComplete" => settings.notifications.on_turn_complete.to_string(),
        "notifications.onToolPermission" => settings.notifications.on_tool_permission.to_string(),
        "notifications.onUserQuestion" => settings.notifications.on_user_question.to_string(),
        "notifications.onError" => settings.notifications.on_error.to_string(),
        "notifications.onTurnCancel" => settings.notifications.on_turn_cancel.to_string(),
        "notifications.onStartupReady" => settings.notifications.on_startup_ready.to_string(),
        "notifications.minTurnDurationSecs" => settings.notifications.min_turn_duration_secs.to_string(),
        "notifications.appName" => settings.notifications.app_name.clone(),
        "compaction.thresholdPct" => settings.compaction.threshold_pct.to_string(),
        "compaction.keepRecentTokens" => settings.compaction.keep_recent_tokens.to_string(),
        "compaction.reserveTokens" => settings.compaction.reserve_tokens.to_string(),
        "compaction.physicalPrune" => settings.compaction.physical_prune.to_string(),
        "session.enabled" => settings.session.enabled.to_string(),
        "session.gcOnOpen" => settings.session.gc_on_open.to_string(),
        "session.maxSessionsPerCwd" => settings.session.max_sessions_per_cwd.to_string(),
        "session.maxSessionAgeDays" => settings.session.max_session_age_days.to_string(),
        "session.maxEntriesPerSession" => settings.session.max_entries_per_session.to_string(),
        "session.maxStoreDbBytes" => settings.session.max_store_db_bytes.to_string(),
        "session.protectLatestPerCwd" => settings.session.protect_latest_per_cwd.to_string(),
        "session.maxEntryPayloadBytes" => settings.session.max_entry_payload_bytes.to_string(),
        "session.journalKeepTurns" => settings.session.journal_keep_turns.to_string(),
        "session.maxTerminalFilesPerSession" => settings.session.max_terminal_files_per_session.to_string(),
        "workers.enabled" => settings.workers.enabled.to_string(),
        "workers.name" => settings.workers.name.clone().unwrap_or_default(),
        "workers.purpose" => settings.workers.purpose.clone(),
        "workers.heartbeatSecs" => settings.workers.heartbeat_secs.to_string(),
        "workers.leaseStaleSecs" => settings.workers.lease_stale_secs.to_string(),
        "workers.inboxPollMs" => settings.workers.inbox_poll_ms.to_string(),
        "workers.askTimeoutMs" => settings.workers.ask_timeout_ms.to_string(),
        "workers.maxHops" => settings.workers.max_hops.to_string(),
        "workers.tuiShowPeers" => settings.workers.tui_show_peers.to_string(),
        "workers.fileLeases" => settings.workers.file_leases.to_string(),
        "resources.skills" => join_list(Some(&settings.resources.skills)),
        "resources.prompts" => join_list(Some(&settings.resources.prompts)),
        "resources.disabledSkills" => join_list(Some(&settings.resources.disabled_skills)),
        "resources.disabledPrompts" => join_list(Some(&settings.resources.disabled_prompts)),
        "resources.enableSkillCommands" => settings.resources.enable_skill_commands.to_string(),
        "logging.level" => settings.logging.level.clone().unwrap_or_default(),
        "logging.file" => settings.logging.file.map(|value| value.to_string()).unwrap_or_default(),
        "logging.rotation" => settings
            .logging
            .rotation
            .map(|value| format!("{value:?}").to_ascii_lowercase())
            .unwrap_or_default(),
        "logging.maxFiles" => settings.logging.max_files.map(|n| n.to_string()).unwrap_or_default(),
        "logging.maxBytes" => settings.logging.max_bytes.map(|n| n.to_string()).unwrap_or_default(),
        "logging.trace" => settings
            .logging
            .trace
            .map(|value| value.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn join_list(values: Option<&[String]>) -> String {
    values.map(|items| items.join(", ")).unwrap_or_default()
}

fn parse_bool(value: &str, key: &str) -> Result<bool, String> {
    value
        .parse::<bool>()
        .map_err(|_| format!("{key} must be true or false"))
}

fn parse_number<T>(value: &str, key: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    value
        .trim()
        .parse::<T>()
        .map_err(|_| format!("{key} must be a non-negative number"))
}

fn parse_positive_number<T>(value: &str, key: &str) -> Result<T, String>
where
    T: std::str::FromStr + PartialOrd + Default,
{
    let parsed = parse_number(value, key)?;
    if parsed > T::default() {
        Ok(parsed)
    } else {
        Err(format!("{key} must be greater than zero"))
    }
}

fn parse_non_negative_float(value: &str, key: &str) -> Result<f64, String> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{key} must be a finite, non-negative number"))?;
    if parsed.is_finite() && parsed >= 0.0 {
        Ok(parsed)
    } else {
        Err(format!("{key} must be a finite, non-negative number"))
    }
}

fn list(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn set_value(settings: &mut Settings, key: &str, value: &str) -> Result<(), String> {
    match key {
        "preferredChatLanguage" => settings.preferred_chat_language = value.trim().to_string(),
        "simplifiedTechnicalEnglish" => settings.simplified_technical_english = parse_bool(value, key)?,
        "maxRetries" => settings.max_retries = parse_number(value, key)?,
        "defaultTimeout" => settings.default_timeout = value.trim().to_string(),
        "shellPath" => settings.shell_path = optional_text(value),
        "shellCommandPrefix" => settings.shell_command_prefix = optional_text(value),
        "httpProxy" => settings.http_proxy = optional_text(value),
        "quietStartup" => settings.quiet_startup = parse_bool(value, key)?,
        "defaultTools" => settings.default_tools = optional_list(value),
        "promptEncoding.mode" => {
            let mode = match value.trim() {
                "off" => elph_agent::prompt::PromptEncodingMode::Off,
                "toon" => elph_agent::prompt::PromptEncodingMode::Toon,
                "auto" => elph_agent::prompt::PromptEncodingMode::Auto,
                _ => return Err(format!("{key} must be off, toon, or auto")),
            };
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .mode = mode;
        }
        "promptEncoding.minBytes" => {
            let min_bytes = parse_number(value, key)?;
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .min_bytes = min_bytes;
        }
        "promptEncoding.minSavingsRatio" => {
            let min_savings_ratio = parse_non_negative_float(value, key)?;
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .min_savings_ratio = min_savings_ratio;
        }
        "promptEncoding.delimiter" => {
            let delimiter = match value.trim() {
                "comma" => elph_agent::prompt::PromptEncodingDelimiter::Comma,
                "tab" => elph_agent::prompt::PromptEncodingDelimiter::Tab,
                "pipe" => elph_agent::prompt::PromptEncodingDelimiter::Pipe,
                _ => return Err(format!("{key} must be comma, tab, or pipe")),
            };
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .delimiter = delimiter;
        }
        "promptEncoding.tabularDelimiter" => {
            let delimiter = match value.trim() {
                "comma" => Some(elph_agent::prompt::PromptEncodingDelimiter::Comma),
                "tab" => Some(elph_agent::prompt::PromptEncodingDelimiter::Tab),
                "pipe" => Some(elph_agent::prompt::PromptEncodingDelimiter::Pipe),
                "" => None,
                _ => return Err(format!("{key} must be comma, tab, pipe, or empty")),
            };
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .tabular_delimiter = delimiter;
        }
        "promptEncoding.targets.toolResultText" => {
            let enabled = parse_bool(value, key)?;
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .targets
                .tool_result_text = enabled;
        }
        "promptEncoding.targets.structuredDetails" => {
            let enabled = parse_bool(value, key)?;
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .targets
                .structured_details = enabled;
        }
        "promptEncoding.preamble" => {
            settings
                .prompt_encoding
                .get_or_insert_with(elph_agent::prompt::PromptEncodingConfig::default)
                .preamble = optional_text(value);
        }
        "ui.theme" => settings.ui.theme = value.trim().to_string(),
        "ui.showThinking" => settings.ui.show_thinking = parse_bool(value, key)?,
        "ui.autoExpandThinking" => settings.ui.auto_expand_thinking = parse_bool(value, key)?,
        "ui.stickyScroll" => settings.ui.sticky_scroll = parse_bool(value, key)?,
        "ui.footerTokenDisplay" => settings.ui.footer_token_display = value.trim().to_string(),
        "ui.coloredStatusFooter" => settings.ui.colored_status_footer = parse_bool(value, key)?,
        "ui.density" => settings.ui.density = value.trim().to_string(),
        "ui.showHiddenFiles" => settings.ui.show_hidden_files = parse_bool(value, key)?,
        "ui.allowModeChangeWhileBusy" => settings.ui.allow_mode_change_while_busy = parse_bool(value, key)?,
        "ui.turnStats" => settings.ui.turn_stats = parse_bool(value, key)?,
        "ui.atomicPaste" => settings.ui.atomic_paste = parse_bool(value, key)?,
        "models.defaultModel" => settings.models.default_model = optional_text(value),
        "models.sessionTitleModel" => settings.models.session_title_model = value.trim().to_string(),
        "models.compactionModel" => settings.models.compaction_model = value.trim().to_string(),
        "models.treeBranchSummaries" => settings.models.tree_branch_summaries = value.trim().to_string(),
        "models.defaultThinkingLevel" => settings.models.default_thinking_level = value.trim().to_string(),
        "models.showConfiguredOnly" => settings.models.show_configured_only = parse_bool(value, key)?,
        "models.scopedModels" => settings.models.scoped_models = list(value),
        "models.enabled" => settings.models.enabled = list(value),
        "models.embedModel" => {
            settings.models.embed_model = crate::platform::settings::EmbedModel::from_string(value.trim())
        }
        "models.embedQuantized" => settings.models.embed_quantized = parse_bool(value, key)?,
        "models.embedGpuAcceleration" => {
            settings.models.embed_gpu_acceleration = match value.trim() {
                "auto" => crate::platform::settings::GpuAcceleration::Auto,
                "on" => crate::platform::settings::GpuAcceleration::On,
                "off" => crate::platform::settings::GpuAcceleration::Off,
                _ => return Err(format!("{key} must be auto, on, or off")),
            }
        }
        "models.thinkingBudgets.minimal" => set_thinking_budget(settings, "minimal", value, key)?,
        "models.thinkingBudgets.low" => set_thinking_budget(settings, "low", value, key)?,
        "models.thinkingBudgets.medium" => set_thinking_budget(settings, "medium", value, key)?,
        "models.thinkingBudgets.high" => set_thinking_budget(settings, "high", value, key)?,
        "memory.enabled" => settings.memory.enabled = parse_bool(value, key)?,
        "memory.autoRecall" => settings.memory.auto_recall = parse_bool(value, key)?,
        "memory.autoCaptureWork" => settings.memory.auto_capture_work = parse_bool(value, key)?,
        "memory.autoCaptureExploration" => settings.memory.auto_capture_exploration = parse_bool(value, key)?,
        "memory.topK" => settings.memory.top_k = parse_number(value, key)?,
        "memory.contextBudgetChars" => settings.memory.context_budget_chars = parse_number(value, key)?,
        "memory.minQueryLength" => settings.memory.min_query_length = parse_number(value, key)?,
        "notifications.enabled" => settings.notifications.enabled = parse_bool(value, key)?,
        "notifications.onTurnComplete" => settings.notifications.on_turn_complete = parse_bool(value, key)?,
        "notifications.onToolPermission" => settings.notifications.on_tool_permission = parse_bool(value, key)?,
        "notifications.onUserQuestion" => settings.notifications.on_user_question = parse_bool(value, key)?,
        "notifications.onError" => settings.notifications.on_error = parse_bool(value, key)?,
        "notifications.onTurnCancel" => settings.notifications.on_turn_cancel = parse_bool(value, key)?,
        "notifications.onStartupReady" => settings.notifications.on_startup_ready = parse_bool(value, key)?,
        "notifications.minTurnDurationSecs" => {
            settings.notifications.min_turn_duration_secs = parse_non_negative_float(value, key)?
        }
        "notifications.appName" => settings.notifications.app_name = value.trim().to_string(),
        "compaction.thresholdPct" => {
            let threshold = parse_number::<u8>(value, key)?;
            if !(1..=100).contains(&threshold) {
                return Err(format!("{key} must be between 1 and 100"));
            }
            settings.compaction.threshold_pct = threshold;
        }
        "compaction.keepRecentTokens" => settings.compaction.keep_recent_tokens = parse_number(value, key)?,
        "compaction.reserveTokens" => settings.compaction.reserve_tokens = parse_number(value, key)?,
        "compaction.physicalPrune" => settings.compaction.physical_prune = parse_bool(value, key)?,
        "session.enabled" => settings.session.enabled = parse_bool(value, key)?,
        "session.gcOnOpen" => settings.session.gc_on_open = parse_bool(value, key)?,
        "session.maxSessionsPerCwd" => settings.session.max_sessions_per_cwd = parse_number(value, key)?,
        "session.maxSessionAgeDays" => settings.session.max_session_age_days = parse_number(value, key)?,
        "session.maxEntriesPerSession" => settings.session.max_entries_per_session = parse_number(value, key)?,
        "session.maxStoreDbBytes" => settings.session.max_store_db_bytes = parse_number(value, key)?,
        "session.protectLatestPerCwd" => settings.session.protect_latest_per_cwd = parse_bool(value, key)?,
        "session.maxEntryPayloadBytes" => settings.session.max_entry_payload_bytes = parse_number(value, key)?,
        "session.journalKeepTurns" => settings.session.journal_keep_turns = parse_number(value, key)?,
        "session.maxTerminalFilesPerSession" => {
            settings.session.max_terminal_files_per_session = parse_number(value, key)?
        }
        "workers.enabled" => settings.workers.enabled = parse_bool(value, key)?,
        "workers.name" => settings.workers.name = optional_text(value),
        "workers.purpose" => settings.workers.purpose = value.trim().to_string(),
        "workers.heartbeatSecs" => {
            let heartbeat_secs = parse_positive_number(value, key)?;
            if settings.workers.lease_stale_secs <= heartbeat_secs {
                return Err("workers.heartbeatSecs must be less than workers.leaseStaleSecs".into());
            }
            settings.workers.heartbeat_secs = heartbeat_secs;
        }
        "workers.leaseStaleSecs" => {
            let lease_stale_secs = parse_positive_number(value, key)?;
            if lease_stale_secs <= settings.workers.heartbeat_secs {
                return Err("workers.leaseStaleSecs must be greater than workers.heartbeatSecs".into());
            }
            settings.workers.lease_stale_secs = lease_stale_secs;
        }
        "workers.inboxPollMs" => settings.workers.inbox_poll_ms = parse_positive_number(value, key)?,
        "workers.askTimeoutMs" => settings.workers.ask_timeout_ms = parse_positive_number(value, key)?,
        "workers.maxHops" => settings.workers.max_hops = parse_positive_number(value, key)?,
        "workers.tuiShowPeers" => settings.workers.tui_show_peers = parse_bool(value, key)?,
        "workers.fileLeases" => settings.workers.file_leases = parse_bool(value, key)?,
        "resources.skills" => settings.resources.skills = list(value),
        "resources.prompts" => settings.resources.prompts = list(value),
        "resources.disabledSkills" => settings.resources.disabled_skills = list(value),
        "resources.disabledPrompts" => settings.resources.disabled_prompts = list(value),
        "resources.enableSkillCommands" => settings.resources.enable_skill_commands = parse_bool(value, key)?,
        "logging.level" => settings.logging.level = optional_text(value),
        "logging.file" => {
            settings.logging.file = if value.trim().is_empty() {
                None
            } else {
                Some(parse_bool(value, key)?)
            }
        }
        "logging.rotation" => {
            settings.logging.rotation = if value.trim().is_empty() {
                None
            } else {
                Some(elph_agent::logger::LogRotation::parse(Some(value)))
            }
        }
        "logging.maxFiles" => settings.logging.max_files = optional_positive_number(value, key)?,
        "logging.maxBytes" => settings.logging.max_bytes = optional_positive_number(value, key)?,
        "logging.trace" => {
            settings.logging.trace = if value.trim().is_empty() {
                None
            } else {
                Some(parse_bool(value, key)?)
            }
        }
        _ => return Err(format!("Unknown setting: {key}")),
    }
    Ok(())
}

fn optional_text(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

fn optional_list(value: &str) -> Option<Vec<String>> {
    let values = list(value);
    (!values.is_empty()).then_some(values)
}

fn set_thinking_budget(settings: &mut Settings, level: &str, value: &str, key: &str) -> Result<(), String> {
    let budget = if value.trim().is_empty() {
        None
    } else {
        Some(parse_number(value, key)?)
    };
    let budgets = settings
        .models
        .thinking_budgets
        .get_or_insert(elph_ai::ThinkingBudgets {
            minimal: None,
            low: None,
            medium: None,
            high: None,
        });
    match level {
        "minimal" => budgets.minimal = budget,
        "low" => budgets.low = budget,
        "medium" => budgets.medium = budget,
        "high" => budgets.high = budget,
        _ => return Err(format!("Unknown thinking level: {level}")),
    }
    if budgets.minimal.is_none() && budgets.low.is_none() && budgets.medium.is_none() && budgets.high.is_none() {
        settings.models.thinking_budgets = None;
    }
    Ok(())
}

fn optional_positive_number<T>(value: &str, key: &str) -> Result<Option<T>, String>
where
    T: std::str::FromStr + PartialOrd + Default,
{
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_positive_number(value, key).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_visible_schema_field_round_trips_its_default_value() {
        let settings = Settings::defaults();
        for category in SettingsCategory::ALL {
            for field in fields(category) {
                let value = value_for(&settings, field.key);
                let mut copy = settings.clone();
                set_value(&mut copy, field.key, &value)
                    .unwrap_or_else(|err| panic!("{} ({}) should accept `{value}`: {err}", field.key, field.label));
            }
        }
    }

    #[test]
    fn lists_accept_commas_and_newlines_and_drop_empty_items() {
        assert_eq!(list("one, two\nthree,, "), vec!["one", "two", "three"]);
        assert_eq!(optional_list(" , \n "), None);
    }

    #[test]
    fn invalid_numbers_are_rejected_without_mutating_the_value() {
        let mut settings = Settings::defaults();
        let before = settings.max_retries;
        assert!(set_value(&mut settings, "maxRetries", "not-a-number").is_err());
        assert_eq!(settings.max_retries, before);
        let before = settings.notifications.min_turn_duration_secs;
        assert!(set_value(&mut settings, "notifications.minTurnDurationSecs", "-1").is_err());
        assert!(set_value(&mut settings, "notifications.minTurnDurationSecs", "NaN").is_err());
        assert_eq!(settings.notifications.min_turn_duration_secs, before);
        assert!(set_value(&mut settings, "compaction.thresholdPct", "101").is_err());
        assert!(set_value(&mut settings, "workers.heartbeatSecs", "0").is_err());
        assert!(set_value(&mut settings, "logging.maxFiles", "0").is_err());
    }

    #[test]
    fn optional_logging_values_can_be_cleared() {
        let mut settings = Settings::defaults();
        settings.logging.max_files = Some(3);
        set_value(&mut settings, "logging.maxFiles", "").unwrap();
        assert_eq!(settings.logging.max_files, None);
    }
}
