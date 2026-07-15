use elph_tui::types::*;

#[test]
fn select_option_new() {
    let opt = SelectOption::new("name", "description");
    assert_eq!(opt.name, "name");
    assert_eq!(opt.description, "description");
}

#[test]
fn tab_item_new() {
    let tab = TabItem::new("Tab", "Body");
    assert_eq!(tab.label, "Tab");
    assert_eq!(tab.content, "Body");
}
