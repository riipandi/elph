//! Placeholder sidebar panel for the tuie shell.

use tuie::prelude::*;

/// Right-hand sidebar placeholder shown while the real panel is migrated.
pub struct SidebarPlaceholder {
    root: Box<Pane>,
}

impl SidebarPlaceholder {
    /// Builds the bordered placeholder panel.
    pub fn new(theme: crate::theme::Theme) -> Box<Self> {
        let root = Pane::new()
            .bordered()
            .border_style(Style::new().fg(theme.frame_border))
            .padding(Spacing::balanced(1))
            .vertical()
            .children([
                Text::new()
                    .content("Side panel")
                    .style(Style::new().fg(theme.foreground).bold()) as Box<dyn Widget>,
                Text::new()
                    .content("Ctrl+S to toggle")
                    .style(Style::new().fg(theme.muted)) as Box<dyn Widget>,
            ]);
        Box::new(Self { root })
    }
}

impl DelegateWidget for SidebarPlaceholder {
    tuie::delegate_widget!(root);
}
