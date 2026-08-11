//! Status footer row under the editor: mode/model left, turn/git right.

use iocraft::prelude::*;

use crate::tui::chrome::{
    chrome_footer_widths, fit_footer_status_left, fit_footer_status_right_with_workers, footer_mode_model_width,
};
use crate::tui::labels::GitFooterInfo;
use crate::tui::labels::{
    FOOTER_IMG_INDICATOR, FOOTER_SELECT_MODE_BADGE, FOOTER_SEP, FOOTER_WORKERS_BADGE_PREFIX,
    FOOTER_WORKERS_BADGE_PREFIX_ACTIVE, footer_mode_label,
};
use crate::tui::theme::{
    FOOTER_DIM_FG, FOOTER_GIT_ADD_FG, FOOTER_GIT_DEL_FG, FOOTER_IMG_INDICATOR_FG, FOOTER_WORKERS_IDLE_FG,
    FOOTER_WORKERS_INBOX_FG, FOOTER_WORKERS_REPLY_FG, QUIT_BUSY_NOTICE_FG, rgb_color,
};
use crate::types::{AgentMode, ThinkingLevel};

#[derive(Clone, Default, Props)]
pub struct FooterProps {
    pub screen_width: u16,
    pub agent_mode: AgentMode,
    pub model_label: String,
    pub thinking_level: ThinkingLevel,
    pub supports_images: bool,
    pub turn: u32,
    pub git: Option<GitFooterInfo>,
    /// When true, mode/thinking/git accents are colored; otherwise dimmed grey.
    pub colored_status_footer: bool,
    /// Native text-selection mode (`Ctrl+S`) — shows `sel |` on the right cluster.
    pub select_mode: bool,
    /// Live multi-worker count for peer badge (`⬡ N` when ≥ 2; 0 hides).
    pub worker_live_count: usize,
    /// This process worker memorable name (shown with badge when multi-worker).
    pub worker_name: String,
    /// Bumped when chrome stats/git refresh so footer repaints eagerly.
    pub chrome_revision: u64,
    /// Pending inbound worker messages not yet seen (>0 colors `⬡` yellow).
    pub worker_pending_count: usize,
    /// True while the agent is replying to / sending a response for a peer (colors `⬡` green).
    pub worker_replying: bool,
}

/// Colored segments for the left status footer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FooterLeftParts {
    /// Agent mode (`Build`, `Plan`, …) — mode color.
    mode: String,
    /// ` · ` separator between mode and model — dimmed grey.
    sep: String,
    /// `provider/model (thinking)` — thinking-level color (no leading separator).
    model_thinking: String,
    /// ` ◐` — dimmed when uncolored, blue when colored (includes leading space).
    img: String,
}

/// Colored segments for the right status footer (git stats).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct FooterRightParts {
    /// Select-mode badge (`sel`) — warm accent when active.
    select_badge: String,
    /// Separator after the badge (` | `) when select mode is on.
    select_sep: String,
    /// Worker badge (`⬡ N` or `⬡ N · name`) — colored by messaging state.
    workers_badge: String,
    /// Separator after the worker badge (` · `) when the badge is present.
    workers_sep: String,
    /// Dimmed prefix (`turn: N | ` or empty).
    prefix: String,
    /// `[` — dimmed.
    open: String,
    /// `+files/lines` — soft green.
    added: String,
    /// Single space between add/del — dimmed.
    mid: String,
    /// `-files/lines` — soft red.
    deleted: String,
    /// `]` — dimmed.
    close: String,
    /// Entire right side dimmed when no git stats (e.g. `turn: 3` only).
    plain: String,
}

/// Parse `[+A/B -C/D]` into add/del bodies (without brackets).
fn parse_git_stats_body(stats: &str) -> Option<(String, String)> {
    let inner = stats.strip_prefix('[')?.strip_suffix(']')?;
    let (added, deleted) = inner.split_once(' ')?;
    if !added.starts_with('+') || !deleted.starts_with('-') {
        return None;
    }
    Some((added.to_string(), deleted.to_string()))
}

/// Split a fitted right footer line into optional `sel` / workers / turn / git add / git del for coloring.
fn split_footer_status_right(right: &str) -> FooterRightParts {
    if right.is_empty() {
        return FooterRightParts::default();
    }

    let (select_badge, select_sep, rest) = if right == FOOTER_SELECT_MODE_BADGE {
        return FooterRightParts {
            select_badge: FOOTER_SELECT_MODE_BADGE.to_string(),
            ..FooterRightParts::default()
        };
    } else if let Some(after_sel) = right.strip_prefix(&format!("{FOOTER_SELECT_MODE_BADGE}{}", FOOTER_SEP)) {
        (FOOTER_SELECT_MODE_BADGE.to_string(), FOOTER_SEP.to_string(), after_sel)
    } else {
        (String::new(), String::new(), right)
    };

    if rest.is_empty() {
        return FooterRightParts {
            select_badge,
            select_sep,
            ..FooterRightParts::default()
        };
    }

    // Extract a leading worker badge (`⬡ N` or `⬡ N · name`) so it can be colored
    // independently of the dimmed turn/git cluster. The badge ends right before
    // the ` · turn:` segment (the turn label is always present on the right).
    //
    // `rest` stays a borrow of `right` throughout; only the badge text and the
    // final output fields are cloned (footer strings are tiny).
    let (workers_badge, workers_sep) = if rest.starts_with(FOOTER_WORKERS_BADGE_PREFIX)
        && let Some(idx) = rest.find(&format!("{FOOTER_SEP}turn:"))
    {
        let (badge, _) = rest.split_at(idx);
        (badge.to_string(), FOOTER_SEP.to_string())
    } else {
        (String::new(), String::new())
    };

    // Remaining text after the badge (if any). When a badge was found, skip the
    // ` · ` separator that links it to the turn cluster.
    let rest = if workers_badge.is_empty() {
        rest
    } else {
        // `rest` started with the badge followed by ` · turn:`; drop badge + sep.
        rest.get(workers_badge.len() + workers_sep.len()..).unwrap_or(rest)
    };

    let (prefix, stats) = if let Some((turn, after_turn)) = rest.split_once(FOOTER_SEP) {
        if after_turn.starts_with('[') {
            (format!("{}{}", turn, FOOTER_SEP), after_turn)
        } else {
            return FooterRightParts {
                select_badge,
                select_sep,
                workers_badge,
                workers_sep,
                plain: rest.to_string(),
                ..FooterRightParts::default()
            };
        }
    } else if rest.starts_with('[') {
        (String::new(), rest)
    } else {
        return FooterRightParts {
            select_badge,
            select_sep,
            workers_badge,
            workers_sep,
            plain: rest.to_string(),
            ..FooterRightParts::default()
        };
    };

    if let Some((added, deleted)) = parse_git_stats_body(stats) {
        FooterRightParts {
            select_badge,
            select_sep,
            workers_badge,
            workers_sep,
            prefix,
            open: "[".to_string(),
            added,
            mid: " ".to_string(),
            deleted,
            close: "]".to_string(),
            plain: String::new(),
        }
    } else {
        FooterRightParts {
            select_badge,
            select_sep,
            workers_badge,
            workers_sep,
            plain: rest.to_string(),
            ..FooterRightParts::default()
        }
    }
}

/// Split a fitted left footer line into mode / sep / model-thinking / ◐ for coloring.
fn split_footer_status_left(mode: AgentMode, left: &str) -> FooterLeftParts {
    let mode_s = footer_mode_label(mode);
    let mode_prefix = format!("{mode_s}{}", FOOTER_SEP);
    const VISION_SUFFIX: &str = " ◐";

    if left == mode_s {
        return FooterLeftParts {
            mode: mode_s,
            sep: String::new(),
            model_thinking: String::new(),
            img: String::new(),
        };
    }

    if let Some(after_mode) = left.strip_prefix(&mode_prefix) {
        if after_mode == FOOTER_IMG_INDICATOR {
            // Degenerate: mode + ◐ only (no model segment).
            return FooterLeftParts {
                mode: mode_s,
                sep: FOOTER_SEP.to_string(),
                model_thinking: String::new(),
                img: FOOTER_IMG_INDICATOR.to_string(),
            };
        }
        if let Some(model_part) = after_mode.strip_suffix(VISION_SUFFIX) {
            return FooterLeftParts {
                mode: mode_s,
                sep: FOOTER_SEP.to_string(),
                model_thinking: model_part.to_string(),
                img: VISION_SUFFIX.to_string(),
            };
        }
        return FooterLeftParts {
            mode: mode_s,
            sep: FOOTER_SEP.to_string(),
            model_thinking: after_mode.to_string(),
            img: String::new(),
        };
    }

    if left.starts_with(&mode_s) {
        return FooterLeftParts {
            mode: left.to_string(),
            sep: String::new(),
            model_thinking: String::new(),
            img: String::new(),
        };
    }

    // Fitted string dropped the mode — treat as model/thinking (or bare text), dimmed group color.
    if let Some(model_part) = left.strip_suffix(VISION_SUFFIX) {
        return FooterLeftParts {
            mode: String::new(),
            sep: String::new(),
            model_thinking: model_part.to_string(),
            img: VISION_SUFFIX.to_string(),
        };
    }
    if left == FOOTER_IMG_INDICATOR {
        return FooterLeftParts {
            mode: String::new(),
            sep: String::new(),
            model_thinking: String::new(),
            img: FOOTER_IMG_INDICATOR.to_string(),
        };
    }
    FooterLeftParts {
        mode: String::new(),
        sep: String::new(),
        model_thinking: left.to_string(),
        img: String::new(),
    }
}

#[component]
pub fn Footer(props: &FooterProps) -> impl Into<AnyElement<'static>> {
    let _chrome_revision = props.chrome_revision;

    // Mode + model always win width; git/turn on the right yield when the row is tight.
    let min_left = footer_mode_model_width(props.agent_mode, &props.model_label);
    let (left_w, right_w) = chrome_footer_widths(props.screen_width.max(1), min_left);
    let left = fit_footer_status_left(
        props.agent_mode,
        &props.model_label,
        props.thinking_level,
        props.supports_images,
        left_w.max(1),
    );
    let right = fit_footer_status_right_with_workers(
        props.turn,
        props.git.as_ref(),
        right_w,
        props.select_mode,
        props.worker_live_count,
        &props.worker_name,
    );
    let parts = split_footer_status_left(props.agent_mode, &left);
    let right_parts = split_footer_status_right(&right);
    let colored = props.colored_status_footer;
    let mode_color = if colored {
        rgb_color(props.agent_mode.label_rgb())
    } else {
        FOOTER_DIM_FG
    };
    let model_color = if colored {
        rgb_color(props.thinking_level.border_rgb())
    } else {
        FOOTER_DIM_FG
    };
    let img_color = if colored {
        FOOTER_IMG_INDICATOR_FG
    } else {
        FOOTER_DIM_FG
    };
    let git_add_color = if colored { FOOTER_GIT_ADD_FG } else { FOOTER_DIM_FG };
    let git_del_color = if colored { FOOTER_GIT_DEL_FG } else { FOOTER_DIM_FG };
    // Select badge always uses the warm accent so it stays visible even with dimmed footers.
    let select_badge_color = if props.select_mode {
        QUIT_BUSY_NOTICE_FG
    } else {
        FOOTER_DIM_FG
    };
    // Worker badge color: green while replying → yellow with pending mail → dim when idle.
    let workers_badge_color = if props.worker_replying {
        FOOTER_WORKERS_REPLY_FG
    } else if props.worker_pending_count > 0 {
        FOOTER_WORKERS_INBOX_FG
    } else {
        FOOTER_WORKERS_IDLE_FG
    };
    // Active (messaging) states use a filled hexagon; idle stays hollow.
    let workers_active = props.worker_replying || props.worker_pending_count > 0;

    element! {
        View(
            width: props.screen_width.max(1),
            height: 1,
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
        ) {
            View(
                width: left_w as u16,
                height: 1,
                flex_direction: FlexDirection::Row,
                flex_shrink: 0f32,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Start,
                padding: 0,
            ) {
                #( (!parts.mode.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(
                            color: mode_color,
                            weight: Weight::Bold,
                            wrap: TextWrap::NoWrap,
                            content: parts.mode.clone(),
                        )
                    }
                    .into()
                }))
                #( (!parts.sep.is_empty()).then(|| -> AnyElement<'static> {
                    // Separator stays dimmed grey, independent of agent mode or thinking color.
                    element! {
                        Text(
                            color: FOOTER_DIM_FG,
                            wrap: TextWrap::NoWrap,
                            content: parts.sep.clone(),
                        )
                    }
                    .into()
                }))
                #( (!parts.model_thinking.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: model_color, wrap: TextWrap::NoWrap, content: parts.model_thinking.clone())
                    }
                    .into()
                }))
                #( (!parts.img.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: img_color, wrap: TextWrap::NoWrap, content: parts.img.clone())
                    }
                    .into()
                }))
            }
            View(
                width: right_w as u16,
                height: 1,
                flex_direction: FlexDirection::Row,
                flex_shrink: 0f32,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::End,
                padding: 0,
            ) {
                #( (!right_parts.select_badge.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(
                            color: select_badge_color,
                            weight: Weight::Bold,
                            wrap: TextWrap::NoWrap,
                            content: right_parts.select_badge.clone(),
                        )
                    }
                    .into()
                }))
                #( (!right_parts.select_sep.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: FOOTER_DIM_FG, wrap: TextWrap::NoWrap, content: right_parts.select_sep.clone())
                    }
                    .into()
                }))
                #( (!right_parts.workers_badge.is_empty()).then(|| -> AnyElement<'static> {
                    // Swap the leading hollow hexagon (⬡) for a filled one (⬢) in active states.
                    let badge_content = if workers_active {
                        let mut s = right_parts.workers_badge.clone();
                        if s.starts_with(FOOTER_WORKERS_BADGE_PREFIX) {
                            s.replace_range(..FOOTER_WORKERS_BADGE_PREFIX.len(), FOOTER_WORKERS_BADGE_PREFIX_ACTIVE);
                        }
                        s
                    } else {
                        right_parts.workers_badge.clone()
                    };
                    element! {
                        Text(color: workers_badge_color, wrap: TextWrap::NoWrap, content: badge_content)
                    }
                    .into()
                }))
                #( (!right_parts.workers_sep.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: FOOTER_DIM_FG, wrap: TextWrap::NoWrap, content: right_parts.workers_sep.clone())
                    }
                    .into()
                }))
                #( (!right_parts.plain.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: FOOTER_DIM_FG, wrap: TextWrap::NoWrap, content: right_parts.plain.clone())
                    }
                    .into()
                }))
                #( (!right_parts.prefix.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: FOOTER_DIM_FG, wrap: TextWrap::NoWrap, content: right_parts.prefix.clone())
                    }
                    .into()
                }))
                #( (!right_parts.open.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: FOOTER_DIM_FG, wrap: TextWrap::NoWrap, content: right_parts.open.clone())
                    }
                    .into()
                }))
                #( (!right_parts.added.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: git_add_color, wrap: TextWrap::NoWrap, content: right_parts.added.clone())
                    }
                    .into()
                }))
                #( (!right_parts.mid.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: FOOTER_DIM_FG, wrap: TextWrap::NoWrap, content: right_parts.mid.clone())
                    }
                    .into()
                }))
                #( (!right_parts.deleted.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: git_del_color, wrap: TextWrap::NoWrap, content: right_parts.deleted.clone())
                    }
                    .into()
                }))
                #( (!right_parts.close.is_empty()).then(|| -> AnyElement<'static> {
                    element! {
                        Text(color: FOOTER_DIM_FG, wrap: TextWrap::NoWrap, content: right_parts.close.clone())
                    }
                    .into()
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_footer_status_left_keeps_mode_model_and_dimmed_img() {
        let parts = split_footer_status_left(AgentMode::Plan, "Plan · opencode/deepseek-v4-flash (xhigh) ◐");
        assert_eq!(parts.mode, "Plan");
        assert_eq!(parts.sep, " · ");
        assert_eq!(parts.model_thinking, "opencode/deepseek-v4-flash (xhigh)");
        assert_eq!(parts.img, " ◐");

        let no_img = split_footer_status_left(AgentMode::Build, "Build · openai/gpt-5.6-luna (high)");
        assert_eq!(no_img.mode, "Build");
        assert_eq!(no_img.sep, " · ");
        assert_eq!(no_img.model_thinking, "openai/gpt-5.6-luna (high)");
        assert!(no_img.img.is_empty());

        let mode_only = split_footer_status_left(AgentMode::Ask, "Ask");
        assert_eq!(mode_only.mode, "Ask");
        assert!(mode_only.sep.is_empty());
        assert!(mode_only.model_thinking.is_empty());
        assert!(mode_only.img.is_empty());
    }

    #[test]
    fn split_footer_status_right_colors_git_add_and_del() {
        let full = split_footer_status_right("turn: 2 · [+3/42 -1/7]");
        assert_eq!(full.prefix, "turn: 2 · ");
        assert_eq!(full.open, "[");
        assert_eq!(full.added, "+3/42");
        assert_eq!(full.mid, " ");
        assert_eq!(full.deleted, "-1/7");
        assert_eq!(full.close, "]");
        assert!(full.plain.is_empty());
        assert!(full.select_badge.is_empty());

        let stats_only = split_footer_status_right("[+0/0 -0/0]");
        assert!(stats_only.prefix.is_empty());
        assert_eq!(stats_only.added, "+0/0");
        assert_eq!(stats_only.deleted, "-0/0");

        let turn_only = split_footer_status_right("turn: 3");
        assert_eq!(turn_only.plain, "turn: 3");
        assert!(turn_only.added.is_empty());

        let with_sel = split_footer_status_right("sel · turn: 0 · [+0/0 -0/0]");
        assert_eq!(with_sel.select_badge, "sel");
        assert_eq!(with_sel.select_sep, " · ");
        assert_eq!(with_sel.prefix, "turn: 0 · ");
        assert_eq!(with_sel.added, "+0/0");
        assert_eq!(with_sel.deleted, "-0/0");
    }

    #[test]
    fn split_footer_status_right_extracts_workers_badge() {
        // Badge only, no name, no git → turn lands in `plain`.
        let badge = split_footer_status_right("⬡ 2 · turn: 1");
        assert_eq!(badge.workers_badge, "⬡ 2");
        assert_eq!(badge.workers_sep, " · ");
        assert_eq!(badge.plain, "turn: 1");
        assert!(badge.prefix.is_empty());

        // Badge with worker name + git stats.
        let named = split_footer_status_right("⬡ 3 · calm-fox · turn: 2 · [+1/5 -0/0]");
        assert_eq!(named.workers_badge, "⬡ 3 · calm-fox");
        assert_eq!(named.workers_sep, " · ");
        assert_eq!(named.prefix, "turn: 2 · ");
        assert_eq!(named.added, "+1/5");
        assert_eq!(named.deleted, "-0/0");

        // sel precedes the badge.
        let sel_badge = split_footer_status_right("sel · ⬡ 2 · turn: 0");
        assert_eq!(sel_badge.select_badge, "sel");
        assert_eq!(sel_badge.workers_badge, "⬡ 2");
        assert_eq!(sel_badge.plain, "turn: 0");

        // No badge — fields stay empty.
        let no_badge = split_footer_status_right("turn: 5");
        assert!(no_badge.workers_badge.is_empty());
        assert!(no_badge.workers_sep.is_empty());
        assert_eq!(no_badge.plain, "turn: 5");
    }

    #[test]
    fn footer_render_includes_mode_model_and_turn() {
        let rendered = element! {
            Footer(
                screen_width: 100u16,
                agent_mode: AgentMode::Plan,
                model_label: "opencode/deepseek-v4-flash".to_string(),
                thinking_level: ThinkingLevel::Xhigh,
                supports_images: true,
                turn: 0u32,
                git: None,
                colored_status_footer: true,
                select_mode: false,
                worker_live_count: 0usize,
                worker_name: String::new(),
                chrome_revision: 1u64,
            )
        }
        .to_string();
        assert!(
            rendered.contains("Plan") || rendered.contains("opencode"),
            "left missing: {rendered:?}"
        );
        assert!(rendered.contains("turn:"), "right missing: {rendered:?}");
        assert!(rendered.contains("(xhigh)") || rendered.contains("◐"), "{rendered:?}");
    }

    #[test]
    fn footer_render_shows_git_stats_when_present() {
        let git = GitFooterInfo {
            branch: "main".to_string(),
            files_added: 1,
            lines_added: 2,
            files_deleted: 0,
            lines_deleted: 0,
        };
        let rendered = element! {
            Footer(
                screen_width: 100u16,
                agent_mode: AgentMode::Build,
                model_label: "openai/gpt-5.6-luna".to_string(),
                thinking_level: ThinkingLevel::High,
                supports_images: false,
                turn: 3u32,
                git: Some(git),
                colored_status_footer: true,
                select_mode: false,
                worker_live_count: 0usize,
                worker_name: String::new(),
                chrome_revision: 2u64,
            )
        }
        .to_string();
        assert!(rendered.contains("turn: 3") || rendered.contains("turn:"), "{rendered:?}");
        assert!(rendered.contains("[+") || rendered.contains("Build"), "{rendered:?}");
    }

    #[test]
    fn footer_render_shows_sel_badge_in_select_mode() {
        let git = GitFooterInfo {
            branch: "main".to_string(),
            files_added: 0,
            lines_added: 0,
            files_deleted: 0,
            lines_deleted: 0,
        };
        let rendered = element! {
            Footer(
                screen_width: 100u16,
                agent_mode: AgentMode::Build,
                model_label: "openai/gpt-5.6-luna".to_string(),
                thinking_level: ThinkingLevel::High,
                supports_images: false,
                turn: 0u32,
                git: Some(git),
                colored_status_footer: true,
                select_mode: true,
                worker_live_count: 0usize,
                worker_name: String::new(),
                chrome_revision: 1u64,
            )
        }
        .to_string();
        assert!(rendered.contains("sel"), "select badge missing: {rendered:?}");
        assert!(rendered.contains("turn:"), "{rendered:?}");
    }

    #[test]
    fn footer_render_accepts_uncolored_status_flag() {
        let rendered = element! {
            Footer(
                screen_width: 80u16,
                agent_mode: AgentMode::Brave,
                model_label: "opencode/x".to_string(),
                thinking_level: ThinkingLevel::Max,
                supports_images: false,
                turn: 1u32,
                git: None,
                colored_status_footer: false,
                select_mode: false,
                worker_live_count: 0usize,
                worker_name: String::new(),
                chrome_revision: 1u64,
            )
        }
        .to_string();
        assert!(rendered.contains("Brave") || rendered.contains("opencode"), "{rendered:?}");
    }

    #[test]
    fn footer_render_swaps_icon_for_active_worker_badge() {
        // Idle (no pending, not replying) → hollow hexagon ⬡.
        let idle = element! {
            Footer(
                screen_width: 100u16,
                agent_mode: AgentMode::Build,
                model_label: "openai/gpt-5.6-luna".to_string(),
                thinking_level: ThinkingLevel::High,
                supports_images: false,
                turn: 1u32,
                git: None,
                colored_status_footer: true,
                select_mode: false,
                worker_live_count: 2usize,
                worker_name: String::new(),
                chrome_revision: 1u64,
                worker_pending_count: 0usize,
                worker_replying: false,
            )
        }
        .to_string();
        assert!(idle.contains("⬡"), "idle badge missing hollow icon: {idle:?}");
        assert!(!idle.contains("⬢"), "idle badge should not use filled icon: {idle:?}");

        // Pending inbound messages → filled hexagon ⬢.
        let pending = element! {
            Footer(
                screen_width: 100u16,
                agent_mode: AgentMode::Build,
                model_label: "openai/gpt-5.6-luna".to_string(),
                thinking_level: ThinkingLevel::High,
                supports_images: false,
                turn: 1u32,
                git: None,
                colored_status_footer: true,
                select_mode: false,
                worker_live_count: 2usize,
                worker_name: String::new(),
                chrome_revision: 1u64,
                worker_pending_count: 3usize,
                worker_replying: false,
            )
        }
        .to_string();
        assert!(pending.contains("⬢"), "pending badge missing filled icon: {pending:?}");
        assert!(!pending.contains("⬡"), "pending badge should not use hollow icon: {pending:?}");
    }
}
