use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use jsonschema::Validator;
use parking_lot::Mutex;
use serde_json::Value;

use crate::types::{Tool, ToolCall};

fn validator(schema: &Value) -> Option<Arc<Validator>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<Validator>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        schema.to_string().hash(&mut hasher);
        hasher.finish()
    };
    let mut guard = cache.lock();
    if let std::collections::hash_map::Entry::Vacant(e) = guard.entry(key) {
        let compiled = Arc::new(Validator::new(schema).ok()?);
        e.insert(compiled);
    }
    guard.get(&key).cloned()
}

/// Validate tool call arguments against a tool's JSON Schema parameters.
pub fn validate_tool_call(tool: &Tool, call: &ToolCall) -> Result<(), String> {
    let Some(schema) = validator(&tool.parameters) else {
        return Ok(());
    };
    if let Err(error) = schema.validate(&call.arguments) {
        return Err(error.to_string());
    }
    Ok(())
}
