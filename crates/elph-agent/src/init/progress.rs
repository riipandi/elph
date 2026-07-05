use std::borrow::Cow;
use std::io::{IsTerminal, stderr};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

pub struct InitProgress {
    bar: ProgressBar,
    quiet_env: Option<&'static str>,
}

impl InitProgress {
    pub fn new(steps: u64) -> Self {
        let bar = if Self::enabled(None) {
            let bar = ProgressBar::new(steps);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} {wide_msg:.cyan} [{bar:24.cyan/blue}] {pos}/{len}")
                    .expect("valid init progress template")
                    .progress_chars("━━╸─"),
            );
            bar.enable_steady_tick(Duration::from_millis(80));
            bar
        } else {
            ProgressBar::hidden()
        };

        Self { bar, quiet_env: None }
    }

    pub fn with_quiet_env(mut self, env: &'static str) -> Self {
        self.quiet_env = Some(env);
        if !Self::enabled(self.quiet_env) {
            self.bar = ProgressBar::hidden();
        }
        self
    }

    pub fn advance(&self, message: impl Into<Cow<'static, str>>) {
        self.bar.inc(1);
        self.bar.set_message(message);
    }

    pub fn finish(&self) {
        self.bar.finish_and_clear();
    }

    fn enabled(quiet_env: Option<&'static str>) -> bool {
        if cfg!(test) {
            return false;
        }
        if quiet_env.is_some_and(|name| std::env::var_os(name).is_some()) {
            return false;
        }
        if std::env::var("NO_COLOR").as_deref() == Ok("true") {
            return false;
        }
        stderr().is_terminal()
    }
}
