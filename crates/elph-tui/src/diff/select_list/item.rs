/// One selectable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl SelectItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// ANSI styling for [`super::SelectList`].
#[derive(Debug, Clone, Copy)]
pub struct SelectListTheme {
    pub selected: u8,
    pub description: u8,
    pub scroll_info: u8,
    pub no_match: u8,
}

impl SelectListTheme {
    pub fn dark() -> Self {
        Self {
            selected: 51,
            description: 240,
            scroll_info: 245,
            no_match: 240,
        }
    }
}

/// Callback when a list item is selected.
pub type SelectCallback = Box<dyn FnMut(&SelectItem)>;
/// Callback when list selection changes.
pub type SelectChangeCallback = Box<dyn FnMut(&SelectItem)>;
