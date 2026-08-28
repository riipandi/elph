//! Tool-result detail helpers for the TUI transcript.

use serde_json::json;

/// Nested tool-result details key for elph TUI metadata (duration, …).
pub const ELPH_UI_DETAILS_KEY: &str = "_elph_ui";

/// Read `duration_secs` from tool-result details (`_elph_ui.duration_secs`).
pub fn duration_from_tool_details(details: &serde_json::Value) -> Option<f64> {
    details
        .get(ELPH_UI_DETAILS_KEY)
        .and_then(|ui| ui.get("duration_secs"))
        .and_then(|v| v.as_f64())
        .filter(|s| s.is_finite() && *s >= 0.0)
}

/// Merge wall duration into tool-result details under `_elph_ui`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn merge_duration_into_details(details: &mut serde_json::Value, duration_secs: f64) {
    if !duration_secs.is_finite() || duration_secs < 0.0 {
        return;
    }
    if !details.is_object() {
        *details = json!({});
    }
    let Some(obj) = details.as_object_mut() else {
        return;
    };
    let ui = obj.entry(ELPH_UI_DETAILS_KEY.to_string()).or_insert_with(|| json!({}));
    if let Some(ui_obj) = ui.as_object_mut() {
        ui_obj.insert("duration_secs".into(), json!(duration_secs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_and_read_duration_in_details() {
        let mut details = json!({ "old_content": "a", "new_content": "b" });
        merge_duration_into_details(&mut details, 2.5);
        assert_eq!(duration_from_tool_details(&details), Some(2.5));
        assert_eq!(details.get("old_content").and_then(|v| v.as_str()), Some("a"));
    }

    #[test]
    fn duration_rejects_non_finite_and_negative() {
        assert_eq!(duration_from_tool_details(&json!({})), None);
        assert_eq!(
            duration_from_tool_details(&json!({ "_elph_ui": { "duration_secs": -1.0 } })),
            None
        );
        assert_eq!(
            duration_from_tool_details(&json!({ "_elph_ui": { "duration_secs": "x" } })),
            None
        );
    }
}
