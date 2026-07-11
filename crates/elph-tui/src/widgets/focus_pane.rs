//! Bordered pane that highlights when focused (tuie-demo pattern).

use tuie::render::border;
use tuie::{delegate_field, field, prelude::*};

use crate::theme::Theme;

/// Bordered [`Pane`] wrapper that highlights its border when focused.
pub struct FocusPane {
    pane: Box<Pane>,
    border_style: Style,
    selected_border_style: Option<Style>,
}

impl FocusPane {
    fn refresh_border(&mut self) {
        let cfg = border::config::get();
        let focused = tuie::in_focus_chain(self.pane.get_id());
        if focused {
            let style = self
                .selected_border_style
                .unwrap_or_else(|| cfg.selected_style.apply(Style::new().fg(Color::BLUE)));
            self.pane.set_border_style(style);
            self.pane.set_border(Some(cfg.selected_border));
        } else {
            self.pane.set_border_style(self.border_style);
            self.pane.set_border(Some(cfg.border));
        }
    }

    fn sync_border_style(&mut self) {
        self.refresh_border();
    }
}

impl DelegateWidget for FocusPane {
    tuie::delegate_widget!(pane);

    fn after_on_state_change(&mut self, _widget_state: WidgetState) {
        self.pane.dirty_layout();
    }

    fn after_before_layout(&mut self) {
        self.refresh_border();
    }
}

impl FocusPane {
    pub fn new(theme: Theme) -> Box<Self> {
        let mut pane = Pane::new();
        pane.set_bordered(true);
        pane.set_border_style(Style::new().fg(theme.frame_border));
        Box::new(Self {
            pane,
            border_style: Style::new().fg(theme.frame_border),
            selected_border_style: None,
        })
    }

    pub fn child(mut self: Box<Self>, widget: Box<dyn Widget>) -> Box<Self> {
        self.pane.add_child(widget);
        self
    }

    pub fn padding(mut self: Box<Self>, spacing: Spacing) -> Box<Self> {
        self.pane.set_padding(spacing);
        self
    }

    field!(border_style: Style; sync_border_style);
    field!(selected_border_style: Option<Style>);

    delegate_field!(orientation: Axis2D => pane);
    delegate_field!(gap: u8 => pane);

    pub fn get_widget_mut<T: Widget>(&mut self, id: WidgetId<T>) -> Option<&mut T> {
        self.pane.get_widget_mut(id)
    }

    pub fn get_widget<T: Widget>(&self, id: WidgetId<T>) -> Option<&T> {
        self.pane.get_widget(id)
    }
}
