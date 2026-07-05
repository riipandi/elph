use elph_tui::{LineComponent, SettingItem, SettingsList, SettingsListTheme};

#[test]
fn settings_list_navigates_and_cycles() {
    let mut list = SettingsList::new(
        vec![
            SettingItem::new("theme", "Theme", "dark").with_values(vec!["dark".into(), "light".into()]),
            SettingItem::new("thinking", "Thinking", "high").with_values(vec!["low".into(), "high".into()]),
        ],
        4,
        SettingsListTheme::dark(),
    );
    list.set_focused(true);
    list.handle_input("\x1b[B");
    assert_eq!(list.selected_item().unwrap().id, "thinking");
    list.handle_input("\r");
    assert_eq!(list.selected_item().unwrap().current_value, "low");
}
