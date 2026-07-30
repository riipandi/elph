//! Full-screen confetti / fireworks overlay (iocraft port of [confetty-tui](https://github.com/medopaw/confetty-tui)).

use std::time::{Duration, Instant};

use iocraft::prelude::*;

use super::simulation::{ConfettiMode, System, VisibleParticle};

use crate::tui::focus::ShellFocus;

const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const BURST_INTERVAL: Duration = Duration::from_millis(220);
const SHOW_MIN: Duration = Duration::from_secs(2);
const SHOW_MAX: Duration = Duration::from_secs(5);
const EMPTY_GRACE: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Default)]
pub struct PendingConfetti {}

impl PendingConfetti {
    pub fn open() -> Self {
        Self {}
    }
}

pub struct OpenConfettiArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingConfetti>>,
    pub state: &'a mut Ref<Option<ConfettiRuntime>>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub mode: ConfettiMode,
}

pub fn open_confetti(args: OpenConfettiArgs<'_>) {
    // Slash input is consumed by the effect — never stash/restore it after the overlay.
    args.draft.set(String::new());
    args.live_draft.set(String::new());
    args.state.set(Some(ConfettiRuntime::new(args.mode)));
    args.pending.set(Some(PendingConfetti::open()));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

pub fn close_confetti(
    pending: &mut Ref<Option<PendingConfetti>>,
    state: &mut Ref<Option<ConfettiRuntime>>,
    _draft: &mut State<String>,
    _live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
) {
    state.set(None);
    pending.write().take();
    shell_focus.set(ShellFocus::Prompt);
}

pub fn confetti_mode_from_slash_args(args: &str) -> ConfettiMode {
    match args.trim().to_ascii_lowercase().as_str() {
        "firework" | "fireworks" => ConfettiMode::Firework,
        _ => ConfettiMode::Confetti,
    }
}

pub struct ConfettiRuntime {
    pub system: System,
    pub started_at: Instant,
    pub last_frame: Instant,
    pub last_burst: Instant,
    pub empty_since: Option<Instant>,
}

impl ConfettiRuntime {
    pub fn new(mode: ConfettiMode) -> Self {
        let now = Instant::now();
        Self {
            system: System::new(mode),
            started_at: now,
            last_frame: now,
            last_burst: now - BURST_INTERVAL,
            empty_since: None,
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.system.resize(width.max(1) as i32, height.max(1) as i32);
    }

    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let mut changed = false;

        if now.duration_since(self.last_burst) >= BURST_INTERVAL {
            self.system.spawn_burst();
            self.last_burst = now;
            self.empty_since = None;
            changed = true;
        }

        if now.duration_since(self.last_frame) >= FRAME_INTERVAL {
            self.system.update();
            self.last_frame = now;
            changed = true;
        }

        if self.system.particles.is_empty() {
            if self.empty_since.is_none() {
                self.empty_since = Some(now);
            }
        } else {
            self.empty_since = None;
        }

        changed
    }

    pub fn should_close(&self) -> bool {
        let elapsed = self.started_at.elapsed();
        if elapsed >= SHOW_MAX {
            return true;
        }
        if elapsed < SHOW_MIN {
            return false;
        }
        self.empty_since.is_some_and(|since| since.elapsed() >= EMPTY_GRACE)
    }
}

#[derive(Default, Props)]
pub struct ConfettiOverlayProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub particles: Vec<VisibleParticle>,
}

#[component]
pub fn ConfettiOverlay(props: &ConfettiOverlayProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let cells: Vec<AnyElement<'static>> = props
        .particles
        .iter()
        .map(|p| {
            element! {
                View(
                    position: Position::Absolute,
                    left: p.x as u16,
                    top: p.y as u16,
                ) {
                    Text(
                        content: p.ch.clone(),
                        color: p.color,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            .into()
        })
        .collect();

    element! {
        View(
            width: props.screen_width,
            height: props.screen_height,
            position: Position::Absolute,
            left: 0,
            top: 0,
            background_color: Color::Reset,
        ) {
            #(cells)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confetti_mode_parses_firework_alias() {
        assert_eq!(confetti_mode_from_slash_args("fireworks"), ConfettiMode::Firework);
        assert_eq!(confetti_mode_from_slash_args(""), ConfettiMode::Confetti);
    }

    #[test]
    fn auto_close_waits_for_minimum_show_duration() {
        let mut runtime = ConfettiRuntime::new(ConfettiMode::Confetti);
        runtime.system.particles.clear();
        runtime.empty_since = Some(Instant::now());
        assert!(!runtime.should_close());
    }

    #[test]
    fn auto_close_after_max_duration_even_with_particles() {
        let mut runtime = ConfettiRuntime::new(ConfettiMode::Confetti);
        runtime.started_at = Instant::now() - SHOW_MAX - Duration::from_millis(1);
        runtime.system.spawn_burst();
        assert!(runtime.should_close());
    }
}
