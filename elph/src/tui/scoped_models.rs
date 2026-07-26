//! Scoped models selector — enable/disable models for Ctrl+P cycling.
//!
//! UX mirrors pi `/scoped-models`:
//! - Enter toggles the highlighted model
//! - Ctrl+A enable all (or all matching the filter)
//! - Ctrl+X clear all (or clear matching the filter)
//! - Ctrl+P toggles every model for the selected item's provider
//! - Alt+↑/↓ reorders enabled models (cycle order)
//! - Ctrl+S persists to home `settings.models.scoped`
//! - Esc cancels without saving
//!
//! Changes are session-only until Ctrl+S (`is_dirty`).

use elph_ai::{get_builtin_model, get_builtin_models, get_builtin_providers};

use crate::agent::parse_model_value;
use crate::tui::model_selector::{ModelRow, model_match_score};

/// One row in the scoped-models editor list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedModelItem {
    pub value: String,
    pub name: String,
    pub provider_id: String,
    pub model_id: String,
    pub enabled: bool,
}

/// Open `/scoped-models` session (session-only until saved).
#[derive(Debug, Clone)]
pub struct PendingScopedModels {
    /// Catalog order (`provider/model_id`).
    pub all_ids: Vec<String>,
    /// Enabled models for Ctrl+P / Scoped tab. Empty = none enabled.
    pub enabled_ids: Vec<String>,
    /// Snapshot loaded when the editor opened (for cancel).
    pub baseline_ids: Vec<String>,
    pub filter: String,
    pub selected_index: usize,
    pub is_dirty: bool,
    pub stashed_prompt_draft: Option<String>,
}

impl PendingScopedModels {
    pub fn open(enabled_ids: Vec<String>, stashed_prompt_draft: Option<String>) -> Self {
        let all_ids = catalog_model_values();
        let enabled_ids = sanitize_enabled_ids(&enabled_ids, &all_ids);
        Self {
            all_ids,
            enabled_ids: enabled_ids.clone(),
            baseline_ids: enabled_ids,
            filter: String::new(),
            selected_index: 0,
            is_dirty: false,
            stashed_prompt_draft,
        }
    }

    pub fn items(&self) -> Vec<ScopedModelItem> {
        build_items(&self.all_ids, &self.enabled_ids)
    }

    pub fn filtered_items(&self) -> Vec<ScopedModelItem> {
        filter_items(&self.items(), &self.filter)
    }

    pub fn selected_item(&self) -> Option<ScopedModelItem> {
        self.filtered_items().get(self.selected_index).cloned()
    }

    pub fn clamp_selection(&mut self) {
        let len = self.filtered_items().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        self.clamp_selection();
    }

    pub fn move_selection(&mut self, delta: isize) {
        let len = self.filtered_items().len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }
        let next = (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
        self.selected_index = next;
    }

    pub fn toggle_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            return;
        };
        self.enabled_ids = toggle_enabled(&self.enabled_ids, &item.value);
        self.is_dirty = true;
        self.clamp_selection();
    }

    pub fn enable_all_visible_or_all(&mut self) {
        let targets = if self.filter.trim().is_empty() {
            None
        } else {
            Some(
                self.filtered_items()
                    .into_iter()
                    .map(|item| item.value)
                    .collect::<Vec<_>>(),
            )
        };
        self.enabled_ids = enable_all(&self.enabled_ids, &self.all_ids, targets.as_deref());
        self.is_dirty = true;
        self.clamp_selection();
    }

    pub fn clear_all_visible_or_all(&mut self) {
        let targets = if self.filter.trim().is_empty() {
            None
        } else {
            Some(
                self.filtered_items()
                    .into_iter()
                    .map(|item| item.value)
                    .collect::<Vec<_>>(),
            )
        };
        self.enabled_ids = clear_all(&self.enabled_ids, targets.as_deref());
        self.is_dirty = true;
        self.clamp_selection();
    }

    pub fn toggle_selected_provider(&mut self) {
        let Some(item) = self.selected_item() else {
            return;
        };
        let provider = item.provider_id;
        let provider_ids: Vec<String> = self
            .all_ids
            .iter()
            .filter(|id| id.split_once('/').map(|(p, _)| p) == Some(provider.as_str()))
            .cloned()
            .collect();
        if provider_ids.is_empty() {
            return;
        }
        let all_on = provider_ids.iter().all(|id| self.enabled_ids.iter().any(|e| e == id));
        self.enabled_ids = if all_on {
            clear_all(&self.enabled_ids, Some(&provider_ids))
        } else {
            enable_all(&self.enabled_ids, &self.all_ids, Some(&provider_ids))
        };
        self.is_dirty = true;
        self.clamp_selection();
    }

    /// Reorder the selected **enabled** model within the cycle list.
    pub fn reorder_selected(&mut self, delta: isize) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        if !item.enabled {
            return false;
        }
        let Some(new_enabled) = move_enabled(&self.enabled_ids, &item.value, delta) else {
            return false;
        };
        if new_enabled == self.enabled_ids {
            return false;
        }
        self.enabled_ids = new_enabled;
        self.is_dirty = true;
        // Keep selection on the same model after re-sort.
        let items = self.filtered_items();
        if let Some(idx) = items.iter().position(|row| row.value == item.value) {
            self.selected_index = idx;
        } else {
            self.clamp_selection();
        }
        true
    }

    pub fn mark_saved(&mut self) {
        self.baseline_ids = self.enabled_ids.clone();
        self.is_dirty = false;
    }

    pub fn enabled_count(&self) -> usize {
        self.enabled_ids.len()
    }

    pub fn total_count(&self) -> usize {
        self.all_ids.len()
    }

    pub fn footer_hint(&self) -> String {
        let count = format!("{}/{} enabled", self.enabled_count(), self.total_count());
        let base = format!(
            "Enter toggle · Ctrl+A all · Ctrl+X clear · Ctrl+P provider · Alt+↑/↓ reorder · Ctrl+S save · Esc cancel · {count}"
        );
        if self.is_dirty {
            format!("{base} · (unsaved)")
        } else {
            base
        }
    }
}

/// All builtin models as `provider/model_id`, stable provider then model order.
pub fn catalog_model_values() -> Vec<String> {
    let mut out = Vec::new();
    for provider_id in get_builtin_providers() {
        for model in get_builtin_models(provider_id) {
            out.push(format!("{provider_id}/{}", model.id));
        }
    }
    out
}

pub fn sanitize_enabled_ids(enabled: &[String], all_ids: &[String]) -> Vec<String> {
    let all: std::collections::HashSet<&str> = all_ids.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    for raw in enabled {
        let trimmed = raw.trim();
        if trimmed.is_empty() || !all.contains(trimmed) {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn is_enabled(enabled_ids: &[String], id: &str) -> bool {
    enabled_ids.iter().any(|e| e == id)
}

fn toggle_enabled(enabled_ids: &[String], id: &str) -> Vec<String> {
    if let Some(index) = enabled_ids.iter().position(|e| e == id) {
        let mut next = enabled_ids.to_vec();
        next.remove(index);
        next
    } else {
        let mut next = enabled_ids.to_vec();
        next.push(id.to_string());
        next
    }
}

fn enable_all(enabled_ids: &[String], all_ids: &[String], targets: Option<&[String]>) -> Vec<String> {
    let targets = targets.unwrap_or(all_ids);
    let mut result = enabled_ids.to_vec();
    for id in targets {
        if !result.iter().any(|e| e == id) {
            result.push(id.clone());
        }
    }
    result
}

fn clear_all(enabled_ids: &[String], targets: Option<&[String]>) -> Vec<String> {
    match targets {
        None => Vec::new(),
        Some(targets) => {
            let drop: std::collections::HashSet<&str> = targets.iter().map(String::as_str).collect();
            enabled_ids
                .iter()
                .filter(|id| !drop.contains(id.as_str()))
                .cloned()
                .collect()
        }
    }
}

fn move_enabled(enabled_ids: &[String], id: &str, delta: isize) -> Option<Vec<String>> {
    let index = enabled_ids.iter().position(|e| e == id)?;
    let new_index = index as isize + delta;
    if new_index < 0 || new_index >= enabled_ids.len() as isize {
        return Some(enabled_ids.to_vec());
    }
    let new_index = new_index as usize;
    let mut result = enabled_ids.to_vec();
    result.swap(index, new_index);
    Some(result)
}

/// Enabled models first (user order), then remaining catalog models.
fn sorted_ids(enabled_ids: &[String], all_ids: &[String]) -> Vec<String> {
    let enabled_set: std::collections::HashSet<&str> = enabled_ids.iter().map(String::as_str).collect();
    let mut out = enabled_ids.to_vec();
    for id in all_ids {
        if !enabled_set.contains(id.as_str()) {
            out.push(id.clone());
        }
    }
    out
}

fn model_row_for_value(value: &str) -> Option<ModelRow> {
    let (provider_id, model_id) = parse_model_value(value).ok()?;
    let model = get_builtin_model(&provider_id, &model_id)?;
    Some(ModelRow {
        value: format!("{provider_id}/{}", model.id),
        name: model.name.clone(),
        provider_id,
        model_id: model.id.clone(),
        context_k: model.context_window / 1000,
        reasoning: model.reasoning,
    })
}

fn build_items(all_ids: &[String], enabled_ids: &[String]) -> Vec<ScopedModelItem> {
    sorted_ids(enabled_ids, all_ids)
        .into_iter()
        .filter_map(|value| {
            let row = model_row_for_value(&value)?;
            Some(ScopedModelItem {
                value: row.value,
                name: row.name,
                provider_id: row.provider_id,
                model_id: row.model_id,
                enabled: is_enabled(enabled_ids, &value),
            })
        })
        .collect()
}

fn filter_items(items: &[ScopedModelItem], query: &str) -> Vec<ScopedModelItem> {
    let query = query.trim();
    if query.is_empty() {
        return items.to_vec();
    }
    let mut scored: Vec<(ScopedModelItem, i32)> = items
        .iter()
        .filter_map(|item| {
            let row = ModelRow {
                value: item.value.clone(),
                name: item.name.clone(),
                provider_id: item.provider_id.clone(),
                model_id: item.model_id.clone(),
                context_k: 0,
                reasoning: false,
            };
            model_match_score(&row, query).map(|score| (item.clone(), score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    scored.into_iter().map(|(item, _)| item).collect()
}

/// Cycle to the next (or previous) scoped model after the current selection.
///
/// Returns the next `provider/model_id`, or `None` when the list is empty.
pub fn cycle_scoped_model(enabled_ids: &[String], current: Option<(&str, &str)>, reverse: bool) -> Option<String> {
    let all = catalog_model_values();
    let list = sanitize_enabled_ids(enabled_ids, &all);
    if list.is_empty() {
        return None;
    }
    let current_value = current.map(|(p, m)| format!("{p}/{m}"));
    let idx = current_value
        .as_ref()
        .and_then(|v| list.iter().position(|e| e == v))
        .unwrap_or(if reverse { 0 } else { list.len().wrapping_sub(1) });
    let next = if reverse {
        if idx == 0 { list.len() - 1 } else { idx - 1 }
    } else {
        (idx + 1) % list.len()
    };
    Some(list[next].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_adds_and_removes() {
        let all = catalog_model_values();
        assert!(!all.is_empty());
        let id = all[0].clone();
        let enabled = toggle_enabled(&[], &id);
        assert_eq!(enabled, vec![id.clone()]);
        let enabled = toggle_enabled(&enabled, &id);
        assert!(enabled.is_empty());
    }

    #[test]
    fn enable_all_and_clear() {
        let all = catalog_model_values();
        let sample: Vec<String> = all.iter().take(3).cloned().collect();
        let enabled = enable_all(&[], &all, Some(&sample));
        assert_eq!(enabled.len(), 3);
        let cleared = clear_all(&enabled, Some(&sample[..1]));
        assert_eq!(cleared.len(), 2);
        assert!(clear_all(&enabled, None).is_empty());
    }

    #[test]
    fn reorder_swaps_neighbors() {
        let list = vec!["a/1".into(), "b/2".into(), "c/3".into()];
        let moved = move_enabled(&list, "b/2", -1).expect("move");
        assert_eq!(moved, vec!["b/2".to_string(), "a/1".into(), "c/3".into()]);
        let moved = move_enabled(&list, "a/1", -1).expect("clamp");
        assert_eq!(moved, list);
    }

    #[test]
    fn sorted_ids_puts_enabled_first() {
        let all = vec!["p/a".into(), "p/b".into(), "p/c".into()];
        let enabled = vec!["p/c".into(), "p/a".into()];
        assert_eq!(sorted_ids(&enabled, &all), vec!["p/c".to_string(), "p/a".into(), "p/b".into()]);
    }

    #[test]
    fn cycle_wraps_forward_and_backward() {
        let all = catalog_model_values();
        let list: Vec<String> = all.iter().take(3).cloned().collect();
        assert!(list.len() >= 2);
        let (p0, m0) = parse_model_value(&list[0]).expect("parse");
        let next = cycle_scoped_model(&list, Some((&p0, &m0)), false).expect("next");
        assert_eq!(next, list[1]);
        let prev = cycle_scoped_model(&list, Some((&p0, &m0)), true).expect("prev");
        assert_eq!(prev, list[list.len() - 1]);
    }

    #[test]
    fn open_sanitizes_unknown_entries() {
        let all = catalog_model_values();
        let sample = all[0].clone();
        let pending = PendingScopedModels::open(vec!["not-real".into(), sample.clone(), sample.clone()], None);
        assert_eq!(pending.enabled_ids, vec![sample]);
        assert!(!pending.is_dirty);
    }

    #[test]
    fn footer_marks_unsaved() {
        let mut pending = PendingScopedModels::open(Vec::new(), None);
        assert!(!pending.footer_hint().contains("unsaved"));
        if let Some(id) = pending.all_ids.first().cloned() {
            pending.enabled_ids = vec![id];
            pending.is_dirty = true;
            assert!(pending.footer_hint().contains("unsaved"));
        }
    }
}
