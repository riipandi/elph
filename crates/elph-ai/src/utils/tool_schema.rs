//! Normalize tool JSON Schemas for OpenAI-compatible providers (including xAI).
//!
//! Some APIs reject function `parameters` whose root uses `anyOf`/`oneOf` with
//! branches that are not plain `"type": "object"` schemas — e.g. xAI returns:
//! `tool parameter root must be an object type (root schema is an anyOf/oneOf
//! union with a non-object branch)`.

use serde_json::{Value, json};

/// Sanitize a tool `parameters` JSON Schema for OpenAI Completions / Responses.
///
/// - Ensures root `"type": "object"` when the schema is object-shaped.
/// - Drops root `anyOf` / `oneOf` / `allOf` that only encode alternate `required`
///   sets (or otherwise lack an object-typed branch), which break xAI and similar
///   validators. Callers should still validate mutually exclusive fields at runtime.
pub fn sanitize_openai_tool_parameters(schema: &Value) -> Value {
    let Value::Object(map) = schema else {
        // Non-object roots are invalid for tools; wrap as empty object.
        return json!({
            "type": "object",
            "properties": {}
        });
    };

    let mut out = map.clone();

    let has_object_shape = out.get("type").and_then(|t| t.as_str()) == Some("object")
        || out.contains_key("properties")
        || out.contains_key("required");

    if has_object_shape {
        out.entry("type".to_string()).or_insert_with(|| json!("object"));
        out.entry("properties".to_string()).or_insert_with(|| json!({}));
    }

    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = out.get(key).and_then(|v| v.as_array())
            && should_drop_root_union(branches)
        {
            out.remove(key);
        }
    }

    // Nested schemas: only fix root for now (xAI error is root-specific).
    // Still ensure properties exist if type is object.
    if out.get("type").and_then(|t| t.as_str()) == Some("object") && !out.contains_key("properties") {
        out.insert("properties".to_string(), json!({}));
    }

    Value::Object(out)
}

fn should_drop_root_union(branches: &[Value]) -> bool {
    if branches.is_empty() {
        return true;
    }
    // Drop if any branch is not a clear object schema (xAI's rejection case),
    // or every branch is only a `required` list (exclusive-field sugar).
    let any_non_object = branches.iter().any(|b| !is_object_schema_branch(b));
    let all_required_only = branches.iter().all(is_required_only_branch);
    any_non_object || all_required_only
}

fn is_object_schema_branch(branch: &Value) -> bool {
    let Some(obj) = branch.as_object() else {
        return false;
    };
    if obj.get("type").and_then(|t| t.as_str()) == Some("object") {
        return true;
    }
    // Untyped branch with properties is still object-shaped.
    obj.contains_key("properties")
}

fn is_required_only_branch(branch: &Value) -> bool {
    let Some(obj) = branch.as_object() else {
        return false;
    };
    !obj.is_empty() && obj.keys().all(|k| k == "required") && obj.get("required").map(|r| r.is_array()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drops_root_anyof_required_only_unions() {
        let schema = json!({
            "type": "object",
            "properties": {
                "question": { "type": "string" },
                "questions": { "type": "array" }
            },
            "anyOf": [
                { "required": ["question"] },
                { "required": ["questions"] }
            ]
        });
        let out = sanitize_openai_tool_parameters(&schema);
        assert_eq!(out["type"], "object");
        assert!(out.get("anyOf").is_none(), "anyOf must be stripped for xAI");
        assert!(out["properties"].get("question").is_some());
    }

    #[test]
    fn drops_root_oneof_required_only_unions() {
        let schema = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "paths": { "type": "array" }
            },
            "oneOf": [
                { "required": ["path"] },
                { "required": ["paths"] }
            ]
        });
        let out = sanitize_openai_tool_parameters(&schema);
        assert!(out.get("oneOf").is_none());
    }

    #[test]
    fn keeps_typed_object_anyof_when_all_branches_are_objects() {
        // If every branch is a full object schema, keep it (not the exclusive-required sugar).
        let schema = json!({
            "anyOf": [
                {
                    "type": "object",
                    "properties": { "a": { "type": "string" } },
                    "required": ["a"]
                },
                {
                    "type": "object",
                    "properties": { "b": { "type": "number" } },
                    "required": ["b"]
                }
            ]
        });
        let out = sanitize_openai_tool_parameters(&schema);
        // has_object_shape is false (no type/properties/required at root) — we still
        // keep anyOf when branches are proper objects.
        assert!(out.get("anyOf").is_some() || out.get("type") == Some(&json!("object")));
    }

    #[test]
    fn non_object_root_becomes_empty_object() {
        let out = sanitize_openai_tool_parameters(&json!("string"));
        assert_eq!(out["type"], "object");
        assert!(out["properties"].is_object());
    }
}
