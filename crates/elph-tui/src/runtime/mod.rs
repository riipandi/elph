//! tuie runtime configuration and shell entry point.

use std::io;
use std::process::ExitCode;

use tuie::prelude::*;

use crate::terminal::{disable_keyboard_enhancement, enable_keyboard_enhancement};

/// Registers the async spawner used by tuie background tasks.
pub fn configure_runtime() {
    tuie::config::update(|cfg| cfg.hover_events = false);
    tuie::set_spawner(|fut| {
        std::thread::spawn(move || {
            futures::executor::block_on(fut);
        });
    });
}

struct KeyboardEnhancementGuard {
    inner: Box<dyn Widget>,
    enabled: bool,
}

impl KeyboardEnhancementGuard {
    fn new(inner: Box<dyn Widget>) -> Box<Self> {
        Box::new(Self { inner, enabled: false })
    }
}

impl DelegateWidget for KeyboardEnhancementGuard {
    tuie::delegate_widget!(inner);

    fn after_before_layout(&mut self) {
        if !self.enabled {
            let _ = enable_keyboard_enhancement();
            self.enabled = true;
        }
    }
}

/// Configures the runtime, installs keyboard-enhancement guards, and runs the shell.
pub fn start_shell(root: Box<dyn Widget>) -> io::Result<ExitCode> {
    configure_runtime();
    tuie::on_quit(|_| {
        let _ = disable_keyboard_enhancement();
    });
    let root = KeyboardEnhancementGuard::new(root);
    let result = tuie::start_tui(root);
    let _ = disable_keyboard_enhancement();
    result
}
