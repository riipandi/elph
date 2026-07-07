use crate::diff::hyperlink;
use iocraft::prelude::*;

/// OAuth/login flow status for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthStatus {
    #[default]
    Idle,
    Waiting,
    Success,
    Error,
}

#[derive(Props)]
pub struct LoginDialogProps {
    pub provider: String,
    pub auth_url: String,
    pub status: AuthStatus,
    pub error_message: String,
    pub on_cancel: HandlerMut<'static, ()>,
}

#[component]
pub fn LoginDialog(props: &LoginDialogProps) -> impl Into<AnyElement<'static>> {
    let link = hyperlink(&props.auth_url, "Open authorization URL");
    let body = match props.status {
        AuthStatus::Idle => format!("Connect to {} to continue.", props.provider),
        AuthStatus::Waiting => format!(
            "Waiting for {} authorization...\n{link}\nPress Esc to cancel.",
            props.provider
        ),
        AuthStatus::Success => format!("Successfully connected to {}.", props.provider),
        AuthStatus::Error => format!(
            "Failed to connect to {}.\n{}\nPress Esc to retry.",
            props.provider, props.error_message
        ),
    };

    element! {
        View(
            border_style: BorderStyle::Round,
            border_color: Color::Yellow,
            padding: 2,
            width: 80pct,
            flex_direction: FlexDirection::Column,
            gap: Gap::Length(1),
        ) {
            Text(content: format!("Login — {}", props.provider))
            Text(content: body)
            #(if props.status == AuthStatus::Waiting {
                Some(element! {
                    Text(content: "⠋ Waiting for browser callback...".to_string())
                })
            } else {
                None
            })
        }
    }
}
