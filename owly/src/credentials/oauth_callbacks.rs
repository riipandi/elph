//! Dialoguer-backed [`AuthLoginCallbacks`] for elph-ai OAuth flows.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use dialoguer::{Input, Select};
use elph_ai::BoxFuture;
use elph_ai::auth::oauth::oauth_provider_login;
use elph_ai::auth::types::{AuthEvent, AuthLoginCallbacks, AuthPrompt};
use indicatif::ProgressBar;

use crate::interactive;

use super::OwlyCredentialStore;

pub struct DialoguerAuthCallbacks {
    progress: Arc<Mutex<Option<ProgressBar>>>,
    browser_opened: AtomicBool,
}

impl DialoguerAuthCallbacks {
    pub fn new() -> Self {
        Self {
            progress: Arc::new(Mutex::new(None)),
            browser_opened: AtomicBool::new(false),
        }
    }

    fn set_progress(&self, message: impl Into<String>) {
        let message = message.into();
        let mut guard = self.progress.lock().expect("oauth progress lock");
        if let Some(pb) = guard.as_ref() {
            pb.set_message(message);
        } else {
            *guard = Some(interactive::progress_spinner(message));
        }
    }

    pub fn finish(&self) {
        if let Some(pb) = self.progress.lock().expect("oauth progress lock").take() {
            pb.finish_and_clear();
        }
    }
}

impl Default for DialoguerAuthCallbacks {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthLoginCallbacks for DialoguerAuthCallbacks {
    fn prompt<'a>(&'a self, prompt: AuthPrompt) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || match prompt {
                AuthPrompt::Text { message, placeholder } => {
                    let mut input = Input::new().with_prompt(message);
                    if let Some(ph) = placeholder {
                        input = input.with_initial_text(ph);
                    }
                    input.interact_text().context("input cancelled")
                }
                AuthPrompt::Secret { message, placeholder } => {
                    let mut input = Input::new().with_prompt(message);
                    if let Some(ph) = placeholder {
                        input = input.with_initial_text(ph);
                    }
                    input.interact_text().context("secret input cancelled")
                }
                AuthPrompt::Select { message, options } => {
                    let labels: Vec<String> = options
                        .iter()
                        .map(|o| {
                            if let Some(desc) = &o.description {
                                format!("{} — {}", o.label, desc)
                            } else {
                                o.label.clone()
                            }
                        })
                        .collect();
                    let idx = Select::new()
                        .with_prompt(message)
                        .items(&labels)
                        .default(0)
                        .interact()
                        .context("selection cancelled")?;
                    Ok(options[idx].id.clone())
                }
                AuthPrompt::ManualCode { message, placeholder } => {
                    let mut input = Input::new().with_prompt(message);
                    if let Some(ph) = placeholder {
                        input = input.with_initial_text(ph);
                    }
                    input.interact_text().context("manual code input cancelled")
                }
            })
            .await
            .context("prompt interrupted")?
        })
    }

    fn notify(&self, event: AuthEvent) {
        match event {
            AuthEvent::AuthUrl { url, instructions } => {
                if let Some(msg) = instructions {
                    println!("{msg}");
                }
                println!("Open: {url}");
                if !self.browser_opened.swap(true, Ordering::SeqCst) {
                    open_browser(&url);
                }
                self.set_progress("Waiting for browser sign-in...");
            }
            AuthEvent::DeviceCode {
                user_code,
                verification_uri,
                interval_seconds,
                expires_in_seconds,
            } => {
                println!("Visit {verification_uri}");
                println!("Enter code: {user_code}");
                if let Some(interval) = interval_seconds {
                    println!("Poll interval: {interval}s");
                }
                if let Some(expires) = expires_in_seconds {
                    println!("Expires in: {expires}s");
                }
                if !self.browser_opened.swap(true, Ordering::SeqCst) {
                    open_browser(&verification_uri);
                }
                self.set_progress("Waiting for device authorization...");
            }
            AuthEvent::Progress { message } => {
                self.set_progress(message);
            }
        }
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
}

/// Run an elph-ai OAuth login and persist tokens to the Owly credential store.
pub async fn run_oauth_login(provider_id: &str, store: &OwlyCredentialStore) -> Result<()> {
    let callbacks = Arc::new(DialoguerAuthCallbacks::new());
    let callbacks_for_login = Arc::clone(&callbacks);
    let credential = oauth_provider_login(provider_id, callbacks_for_login)
        .await
        .with_context(|| format!("OAuth login failed for {provider_id}"))?;
    callbacks.finish();
    store.store_oauth(provider_id, credential).await
}
