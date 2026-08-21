//! Example extension: `/say-hello` plus a `tool_call` gate for `rm -rf`.

use elph_extension_pdk as pdk;
use serde_json::Value;

#[unsafe(no_mangle)]
pub extern "C" fn elph_init() {
    pdk::register_command("say-hello", "Greet someone by name");
    pdk::subscribe(&["tool_call"]);
    pdk::notify("say-hello extension loaded", "info");
}

#[unsafe(no_mangle)]
pub extern "C" fn elph_execute_command(name_ptr: i32, name_len: i32, args_ptr: i32, args_len: i32) -> i32 {
    let name = pdk::read_utf8(name_ptr, name_len);
    if name != "say-hello" {
        return pdk::return_json(&serde_json::json!({
            "message": format!("unknown command: {name}"),
            "is_error": true,
        }));
    }
    let args = pdk::read_utf8(args_ptr, args_len);
    let target = args.trim();
    let message = if target.is_empty() {
        "Hello, world!".to_string()
    } else {
        format!("Hello, {target}!")
    };
    pdk::return_json(&serde_json::json!({ "message": message, "is_error": false }))
}

#[unsafe(no_mangle)]
pub extern "C" fn elph_on_event(ptr: i32, len: i32) -> i32 {
    let raw = pdk::read_utf8(ptr, len);
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return 0;
    };
    let event = value.get("event").and_then(Value::as_str).unwrap_or("");
    if event != "tool_call" {
        return 0;
    }
    let payload = value.get("payload").cloned().unwrap_or(Value::Null);
    let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let command = payload
        .get("input")
        .and_then(|input| input.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if tool == "shell_exec" && command.contains("rm -rf") {
        let ok = pdk::confirm("Dangerous command", "Allow rm -rf?");
        if !ok {
            return pdk::return_json(&serde_json::json!({
                "block": true,
                "reason": "Blocked by extension",
            }));
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn elph_execute_tool(_ptr: i32, _len: i32) -> i32 {
    0
}
