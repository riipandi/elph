use slt::{Border, Color, Context};

/// OAuth/login flow status for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthStatus {
    #[default]
    Idle,
    Waiting,
    Success,
    Error,
}

/// Renders the OAuth login dialog inside a bordered panel.
pub fn render_login_dialog(ui: &mut Context, provider: &str, auth_url: &str, status: AuthStatus, error_message: &str) {
    let _ = ui
        .bordered(Border::Rounded)
        .border_fg(Color::Yellow)
        .p(2)
        .w_pct(80)
        .col(|ui| {
            let _ = ui.text(format!("Login — {provider}")).bold();
            match status {
                AuthStatus::Idle => {
                    let _ = ui.text(format!("Connect to {provider} to continue."));
                }
                AuthStatus::Waiting => {
                    let _ = ui.text(format!("Waiting for {provider} authorization..."));
                    let _ = ui.text(auth_url).fg(Color::Cyan).underline();
                    let _ = ui.text("Press Esc to cancel.").dim();
                    let _ = ui.text("⠋ Waiting for browser callback...");
                }
                AuthStatus::Success => {
                    let _ = ui
                        .text(format!("Successfully connected to {provider}."))
                        .fg(Color::Green);
                }
                AuthStatus::Error => {
                    let _ = ui.text(format!("Failed to connect to {provider}.")).fg(Color::Red);
                    if !error_message.is_empty() {
                        let _ = ui.text(error_message);
                    }
                    let _ = ui.text("Press Esc to retry.").dim();
                }
            }
        });
}
