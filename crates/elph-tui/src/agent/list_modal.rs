use crate::diff::SelectItem;
use slt::{Border, Color, Context, ListState};

pub fn select_item_label(item: &SelectItem) -> String {
    match &item.description {
        Some(desc) => format!("{} — {}", item.label, desc),
        None => item.label.clone(),
    }
}

/// Renders a centered modal list and returns the updated selection index.
pub fn render_select_modal(
    ui: &mut Context,
    title: &str,
    items: &[SelectItem],
    selected: usize,
    border_color: Color,
    width_pct: u8,
) -> usize {
    if items.is_empty() {
        return 0;
    }

    let labels: Vec<String> = items.iter().map(select_item_label).collect();
    let mut list = ListState::new(labels);
    list.selected = selected.min(items.len().saturating_sub(1));

    let modal_width = (ui.width().saturating_mul(width_pct as u32) / 100).clamp(24, ui.width());

    let _ = ui.modal(|ui| {
        let _ = ui
            .bordered(Border::Rounded)
            .border_fg(border_color)
            .p(1)
            .w(modal_width)
            .col(|ui| {
                let _ = ui.text(title).bold();
                let _ = ui.list(&mut list);
            });
    });

    list.selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use slt::TestBackend;

    #[test]
    fn render_select_modal_shows_title_and_items() {
        let items = vec![SelectItem::new("a", "Alpha"), SelectItem::new("b", "Beta")];
        let mut tb = TestBackend::new(80, 24);
        tb.render(|ui| {
            ui.text("background");
            render_select_modal(ui, "Pick one", &items, 0, Color::Cyan, 50);
        });
        tb.assert_contains("Pick one");
        tb.assert_contains("Alpha");
        tb.assert_contains("Beta");
    }

    #[test]
    fn render_select_modal_without_background_still_shows_content() {
        let items = vec![SelectItem::new("only", "Only choice")];
        let mut tb = TestBackend::new(80, 24);
        tb.render(|ui| {
            render_select_modal(ui, "Dialog", &items, 0, Color::Blue, 60);
        });
        tb.assert_contains("Dialog");
        tb.assert_contains("Only choice");
    }
}
