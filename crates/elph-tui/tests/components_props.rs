use elph_tui::components::line_numbers::LineNumbersProps;
use elph_tui::components::text::StyledTextProps;
use elph_tui::types::SelectOption;

#[test]
fn counts_lines() {
    let props = LineNumbersProps {
        line_count: 3,
        start_line: 0,
        gutter_width: 4,
        ..Default::default()
    };
    assert_eq!(props.line_count, 3);
}

#[test]
fn props_default() {
    let props = StyledTextProps::default();
    assert!(props.content.is_empty());
}

#[test]
fn option_construct() {
    let opt = SelectOption::new("Save", "Save file");
    assert_eq!(opt.name, "Save");
}
