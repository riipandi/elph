use chrono::Local;
use elph_tui::{Label, frame};
use iocraft::prelude::*;
use std::time::Duration;

#[component]
pub fn Example(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut time = hooks.use_state(Local::now);
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_future(async move {
        loop {
            smol::Timer::after(Duration::from_secs(1)).await;
            time.set(Local::now());
        }
    });

    hooks.use_terminal_events(move |event| {
        if let TerminalEvent::Key(KeyEvent {
            code: KeyCode::Char('q'),
            kind,
            ..
        }) = event
            && kind != KeyEventKind::Release
        {
            should_exit.set(true);
            #[cfg(unix)]
            {
                use std::sync::atomic::Ordering;
                crate::app::SHOULD_KILL_PARENT.store(true, Ordering::Relaxed);
            }
        }
    });

    if should_exit.get() {
        system.exit();
    }

    let time_box = frame(vec![
        element!(Label(
            content: format!("Current Time: {}", time.get().format("%r")),
        ))
        .into_any(),
    ]);

    element! {
        View(
            width,
            height,
            background_color: Color::Reset,
            border_style: BorderStyle::Round,
            border_color: Color::Blue,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        ) {
            View(margin_bottom: 2) {
                #(time_box)
            }
            Label(content: String::from("Press \"q\" to quit."))
        }
    }
}
