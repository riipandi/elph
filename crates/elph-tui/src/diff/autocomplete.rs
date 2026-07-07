use std::path::{Path, PathBuf};

use super::component::{InputResult, LineComponent};
use super::fuzzy::fuzzy_filter;
use super::select_list::{SelectItem, SelectList, SelectListTheme};

/// Slash command entry for prompt autocomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
}

impl SlashCommand {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }
}

/// Provides slash commands and filesystem path completions.
pub trait AutocompleteProvider: Send {
    fn slash_commands(&self) -> &[SlashCommand];
    fn complete_path(&self, prefix: &str, cwd: &Path) -> Vec<String>;
}

/// Combined slash + file autocomplete.
pub struct CombinedAutocompleteProvider {
    commands: Vec<SlashCommand>,
}

impl CombinedAutocompleteProvider {
    pub fn new(commands: Vec<SlashCommand>) -> Self {
        Self { commands }
    }
}

impl AutocompleteProvider for CombinedAutocompleteProvider {
    fn slash_commands(&self) -> &[SlashCommand] {
        &self.commands
    }

    fn complete_path(&self, prefix: &str, cwd: &Path) -> Vec<String> {
        let base = if prefix.starts_with('@') {
            cwd.to_path_buf()
        } else if prefix.starts_with("~/") {
            dirs_home().join(prefix.trim_start_matches("~/"))
        } else {
            cwd.join(prefix)
        };

        let parent = base.parent().unwrap_or(cwd);
        let file_name = base.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if file_name.is_empty() || name.starts_with(file_name) {
                    out.push(name);
                }
            }
        }
        out.sort();
        out.truncate(20);
        out
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Active autocomplete popup hosted on a [`SelectList`].
pub struct AutocompletePopup {
    list: SelectList,
    kind: AutocompleteKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutocompleteKind {
    Slash,
    Path,
}

impl AutocompletePopup {
    pub fn slash_commands(commands: &[SlashCommand], filter: &str) -> Self {
        let items: Vec<SelectItem> = fuzzy_filter(commands, filter, |cmd| format!("{} {}", cmd.name, cmd.description))
            .into_iter()
            .map(|cmd| SelectItem::new(&cmd.name, &cmd.name).with_description(&cmd.description))
            .collect();
        Self {
            list: SelectList::new(items, 6, SelectListTheme::dark()),
            kind: AutocompleteKind::Slash,
        }
    }

    pub fn paths(paths: Vec<String>) -> Self {
        let items = paths.into_iter().map(|p| SelectItem::new(&p, &p)).collect();
        Self {
            list: SelectList::new(items, 6, SelectListTheme::dark()),
            kind: AutocompleteKind::Path,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self.kind {
            AutocompleteKind::Slash => "slash",
            AutocompleteKind::Path => "path",
        }
    }

    pub fn set_on_select(&mut self, cb: super::select_list::SelectCallback) {
        self.list.on_select = Some(cb);
    }

    pub fn set_on_cancel(&mut self, cb: Box<dyn FnMut()>) {
        self.list.on_cancel = Some(cb);
    }
}

impl LineComponent for AutocompletePopup {
    fn render(&mut self, width: u16) -> Vec<super::component::Line> {
        self.list.render(width)
    }

    fn invalidate(&mut self) {
        self.list.invalidate();
    }

    fn handle_input(&mut self, data: &str) -> InputResult {
        self.list.handle_input(data)
    }

    fn set_focused(&mut self, focused: bool) {
        self.list.set_focused(focused);
    }

    fn is_focused(&self) -> bool {
        self.list.is_focused()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_slash_popup() {
        let popup = AutocompletePopup::slash_commands(&[SlashCommand::new("help", "Show help")], "he");
        assert_eq!(popup.kind(), "slash");
    }
}
