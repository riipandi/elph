//! Reusable interactive prompts for CLI commands.
//!
//! Wraps [`inquire`] prompts with project-specific formatting and
//! provider-aware display helpers. Also provides a CLI implementation
//! of [`AuthLoginCallbacks`] for OAuth flows.

use std::fmt;

use anstyle::*;
use elph_ai::auth::{AuthEvent, AuthLoginCallbacks, AuthPrompt, BoxFuture};
use inquire::{Confirm, Password, PasswordDisplayMode, Select, Text};

use crate::tui::provider_connect_dialog::{ProviderAuthMethod, ProviderConfigStatus, ProviderOption};

// ── Style helpers ────────────────────────────────────────────────────

/// Render a styled message: `icon message` with the given style.
fn styled(style: Style, icon: &str, message: impl fmt::Display) -> String {
    format!("{}{} {}{}", style.render(), icon, message, style.render_reset())
}

/// Render a dim (muted) prefix.
fn dim(message: impl fmt::Display) -> String {
    format!("{}{}{}", STYLE_DIM.render(), message, STYLE_DIM.render_reset())
}

/// Render an error label.
fn err_label() -> String {
    format!("{}!{}", STYLE_ERROR.render(), STYLE_ERROR.render_reset())
}

// ── Style constants ──────────────────────────────────────────────────

const STYLE_SUCCESS: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const STYLE_ERROR: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const STYLE_DIM: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
const STYLE_ACCENT: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const STYLE_LABEL: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Blue)));
const STYLE_CODE: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const STYLE_URL: Style = Style::new().underline().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));

// ── Provider display wrapper ─────────────────────────────────────────

/// Wraps a [`ProviderOption`] for inquire's `Display` trait, showing
/// name and config status.
pub struct ProviderDisplayItem<'a> {
    pub provider: &'a ProviderOption,
}

impl fmt::Display for ProviderDisplayItem<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = match &self.provider.config_status {
            ProviderConfigStatus::Unconfigured => String::new(),
            _ => " ✓".to_string(),
        };
        write!(
            f,
            "{}{}{}{}",
            self.provider.name,
            STYLE_DIM.render(),
            status,
            STYLE_DIM.render_reset(),
        )
    }
}

// ── Auth method selection ────────────────────────────────────────────

/// Interactive auth method selection (Account vs API key).
/// Returns `None` if the user cancels.
pub fn select_auth_method() -> Option<ProviderAuthMethod> {
    #[derive(Clone, Copy)]
    enum Method {
        Account,
        ApiKey,
    }

    impl fmt::Display for Method {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Method::Account => write!(f, "Sign in with an account (OAuth)"),
                Method::ApiKey => write!(f, "Sign in with an API key"),
            }
        }
    }

    let options = vec![Method::Account, Method::ApiKey];
    let ans = Select::new("Authentication method", options)
        .with_page_size(5)
        .with_help_message("↑↓ navigate · Enter confirm · Esc cancel")
        .prompt_skippable()
        .ok()??;

    Some(match ans {
        Method::Account => ProviderAuthMethod::Account,
        Method::ApiKey => ProviderAuthMethod::ApiKey,
    })
}

// ── Provider selection ───────────────────────────────────────────────

/// Interactive provider selection with built-in fuzzy search (`/` to filter).
///
/// Shows providers filtered by `auth_method`, with config status labels.
/// Returns `None` if the user cancels.
pub fn select_provider(providers: &[ProviderOption], auth_method: ProviderAuthMethod) -> Option<&ProviderOption> {
    let filtered: Vec<&ProviderOption> = providers
        .iter()
        .filter(|p| match auth_method {
            ProviderAuthMethod::ApiKey => p.supports_api_key,
            ProviderAuthMethod::Account => p.supports_oauth,
        })
        .collect();

    if filtered.is_empty() {
        eprintln!(
            "{} No providers available for the selected method.",
            styled(STYLE_ERROR, "error:", "")
        );
        return None;
    }

    let display_items: Vec<ProviderDisplayItem<'_>> =
        filtered.iter().map(|p| ProviderDisplayItem { provider: p }).collect();

    let ans = Select::new("Select provider", display_items)
        .with_page_size(10)
        .with_help_message("↑↓ navigate · / filter · Enter confirm · Esc cancel")
        .prompt_skippable()
        .ok()??;

    // Find the original provider reference from the selected display item
    let selected_id = ans.provider.id.clone();
    filtered.into_iter().find(|p| p.id == selected_id)
}

// ── API key input ────────────────────────────────────────────────────

/// Prompt for an API key. Returns the key string, or empty if cancelled.
pub fn prompt_api_key(provider_name: &str) -> Option<String> {
    Text::new(&format!("Enter API key for {provider_name}"))
        .with_placeholder("sk-...")
        .with_help_message("Enter the API key · Esc to cancel")
        .prompt_skippable()
        .ok()?
        .filter(|s| !s.trim().is_empty())
}

// ── Overwrite confirmation ───────────────────────────────────────────

/// Confirm overwriting existing credentials for a provider.
pub fn confirm_overwrite(provider_name: &str) -> bool {
    Confirm::new(&format!("{provider_name} already has stored credentials. Overwrite?"))
        .with_default(false)
        .with_help_message("y/N")
        .prompt_skippable()
        .ok()
        .flatten()
        .unwrap_or(false)
}

// ── OAuth callbacks (CLI) ────────────────────────────────────────────

/// Open a URL in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    let status = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(url).status()
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open").arg(url).status()
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .status()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = url;
            return Err("opening a browser is not supported on this platform".to_string());
        }
    };
    status
        .map_err(|e| format!("failed to open browser: {e}"))
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(format!("browser process exited with: {s}"))
            }
        })
}

/// CLI implementation of [`AuthLoginCallbacks`] using `inquire` prompts.
pub struct CliAuthCallbacks;

impl AuthLoginCallbacks for CliAuthCallbacks {
    fn prompt<'a>(&'a self, prompt: AuthPrompt) -> BoxFuture<'a, anyhow::Result<String>> {
        Box::pin(async move {
            match prompt {
                AuthPrompt::Text { message, placeholder } => {
                    let message = message.clone();
                    let placeholder = placeholder.clone();
                    tokio::task::spawn_blocking(move || {
                        let mut input = Text::new(&message);
                        if let Some(ref ph) = placeholder {
                            input = input.with_placeholder(ph);
                        }
                        input
                            .with_help_message("Type your response · Esc to cancel")
                            .prompt()
                            .map_err(|e| anyhow::anyhow!("{e}"))
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                }
                AuthPrompt::Secret {
                    message,
                    placeholder: _,
                } => {
                    let message = message.clone();
                    tokio::task::spawn_blocking(move || {
                        Password::new(&message)
                            .without_confirmation()
                            .with_display_mode(PasswordDisplayMode::Masked)
                            .with_help_message("Type your API key · Esc to cancel")
                            .prompt()
                            .map_err(|e| anyhow::anyhow!("{e}"))
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                }
                AuthPrompt::Select { message, options } => {
                    let message = message.clone();
                    let labels: Vec<String> = options.iter().map(|opt| opt.label.clone()).collect();
                    let ids: Vec<String> = options.iter().map(|opt| opt.id.clone()).collect();
                    tokio::task::spawn_blocking(move || {
                        let labels_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
                        Select::new(&message, labels_refs)
                            .with_help_message("↑↓ navigate · Enter confirm · Esc cancel")
                            .raw_prompt()
                            .map(|selected| ids[selected.index].clone())
                            .map_err(|e| anyhow::anyhow!("{e}"))
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                }
                AuthPrompt::ManualCode { message, placeholder } => {
                    let message = message.clone();
                    let _placeholder = placeholder.clone();
                    println!();
                    println!("{message}");
                    // Use tokio::io::stdin so this future is cancellable
                    // by tokio::select! in the OAuth flow (e.g. OpenRouter
                    // callback server completes first).
                    use std::io::Write;
                    use tokio::io::AsyncBufReadExt;
                    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
                    let mut line = String::new();
                    print!("Enter code: ");
                    std::io::stdout().flush().ok();
                    reader.read_line(&mut line).await.map_err(|e| anyhow::anyhow!("{e}"))?;
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        Err(anyhow::anyhow!("Cancelled"))
                    } else {
                        Ok(trimmed)
                    }
                }
            }
        })
    }

    fn notify(&self, event: AuthEvent) {
        match event {
            AuthEvent::AuthUrl { url, instructions } => {
                println!();
                eprintln!(
                    "{}Opening browser for authentication…{}",
                    STYLE_ACCENT.render(),
                    STYLE_ACCENT.render_reset(),
                );
                if let Some(instructions) = instructions {
                    eprintln!("{}{}", STYLE_DIM.render(), instructions);
                }
                eprintln!("{}  {}{}", STYLE_URL.render(), url, STYLE_URL.render_reset(),);
                match open_url(&url) {
                    Ok(()) => {
                        let line = styled(STYLE_SUCCESS, "✓", "Browser opened. Login in your browser.");
                        eprintln!("{line}");
                    }
                    Err(e) => {
                        eprintln!("{} Could not open browser: {e}", err_label());
                        eprintln!(
                            "{}  Open manually:{}{}{}{}",
                            STYLE_DIM.render(),
                            STYLE_DIM.render_reset(),
                            STYLE_URL.render(),
                            url,
                            STYLE_URL.render_reset(),
                        );
                    }
                }
                println!();
            }
            AuthEvent::DeviceCode {
                user_code,
                verification_uri,
                interval_seconds,
                expires_in_seconds,
            } => {
                println!();
                eprintln!(
                    "{}Device code authentication{}",
                    STYLE_ACCENT.render(),
                    STYLE_ACCENT.render_reset(),
                );
                eprintln!(
                    "  {}Open:{}{}  {}{}",
                    STYLE_LABEL.render(),
                    STYLE_LABEL.render_reset(),
                    STYLE_URL.render(),
                    verification_uri,
                    STYLE_URL.render_reset(),
                );
                eprintln!(
                    "  {}Code:{}{}  {}{}",
                    STYLE_LABEL.render(),
                    STYLE_LABEL.render_reset(),
                    STYLE_CODE.render(),
                    user_code,
                    STYLE_CODE.render_reset(),
                );
                match open_url(&verification_uri) {
                    Ok(()) => {
                        let line = styled(STYLE_SUCCESS, "✓", "Browser opened.");
                        eprintln!("{line}");
                    }
                    Err(e) => {
                        eprintln!("{} Could not open browser: {e}", err_label());
                        eprintln!(
                            "{}  Open manually:{}{}{}{}",
                            STYLE_DIM.render(),
                            STYLE_DIM.render_reset(),
                            STYLE_URL.render(),
                            verification_uri,
                            STYLE_URL.render_reset(),
                        );
                    }
                }
                if let Some(interval) = interval_seconds {
                    eprintln!(
                        "{}  Polling every {interval}s — waiting for authentication…",
                        dim("").trim_end()
                    );
                }
                if let Some(expires) = expires_in_seconds {
                    eprintln!("{}  Code expires in {expires}s.", dim("").trim_end());
                }
                println!();
            }
            AuthEvent::Progress { message } => {
                // Silently ignore progress messages in CLI mode to avoid noise.
                let _ = message;
            }
        }
    }
}
