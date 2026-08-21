//! Guest-side helpers for Elph core-Wasm extensions.

use std::alloc::{Layout, alloc, dealloc};
use std::slice;

#[link(wasm_import_module = "elph")]
unsafe extern "C" {
    #[link_name = "register_command"]
    fn host_register_command(ptr: i32, len: i32);
    #[link_name = "register_tool"]
    fn host_register_tool(ptr: i32, len: i32);
    #[link_name = "subscribe"]
    fn host_subscribe(ptr: i32, len: i32);
    #[link_name = "notify"]
    fn host_notify(ptr: i32, len: i32);
    #[link_name = "confirm"]
    fn host_confirm(ptr: i32, len: i32) -> i32;
}

#[unsafe(no_mangle)]
pub extern "C" fn elph_alloc(size: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    let Ok(layout) = Layout::from_size_align(size as usize, 8) else {
        return 0;
    };
    let ptr = unsafe { alloc(layout) };
    ptr as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn elph_dealloc(ptr: i32, size: i32) {
    if ptr == 0 || size <= 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(size as usize, 8) {
        unsafe { dealloc(ptr as *mut u8, layout) };
    }
}

fn call_host(f: unsafe extern "C" fn(i32, i32), json: &str) {
    unsafe { f(json.as_ptr() as i32, json.len() as i32) };
}

pub fn register_command(name: &str, description: &str) {
    let body = serde_json::json!({ "name": name, "description": description }).to_string();
    call_host(host_register_command, &body);
}

pub fn register_tool(name: &str, label: &str, description: &str, parameters: serde_json::Value) {
    let body = serde_json::json!({
        "name": name,
        "label": label,
        "description": description,
        "parameters": parameters,
    })
    .to_string();
    call_host(host_register_tool, &body);
}

pub fn subscribe(events: &[&str]) {
    let body = serde_json::to_string(events).unwrap_or_else(|_| "[]".into());
    call_host(host_subscribe, &body);
}

pub fn notify(message: &str, level: &str) {
    let body = serde_json::json!({ "message": message, "level": level }).to_string();
    call_host(host_notify, &body);
}

pub fn confirm(title: &str, body: &str) -> bool {
    let json = serde_json::json!({ "title": title, "body": body }).to_string();
    unsafe { host_confirm(json.as_ptr() as i32, json.len() as i32) != 0 }
}

/// Encode a JSON value as length-prefixed bytes in guest memory; returns pointer.
pub fn return_json(value: &serde_json::Value) -> i32 {
    let bytes = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec());
    let total = bytes.len() + 4;
    let ptr = elph_alloc(total as i32);
    if ptr == 0 {
        return 0;
    }
    unsafe {
        let dest = slice::from_raw_parts_mut(ptr as *mut u8, total);
        dest[..4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        dest[4..].copy_from_slice(&bytes);
    }
    ptr
}

pub fn read_utf8(ptr: i32, len: i32) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let bytes = unsafe { slice::from_raw_parts(ptr as *const u8, len as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}
