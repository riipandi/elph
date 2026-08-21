//! Extension discovery, ABI, and event dispatch.

use elph_agent::plugins::{ExtensionRegistry, ExtensionsSettings, load_manifest};
use std::path::PathBuf;

#[test]
fn parses_extension_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest_path = dir.path().join("extension.toml");
    std::fs::write(
        &manifest_path,
        r#"
name = "demo"
version = "1.0.0"
description = "Demo extension"
wasm = "plugin.wasm"
"#,
    )
    .expect("write manifest");
    let manifest = load_manifest(&manifest_path).expect("parse manifest");
    assert_eq!(manifest.name, "demo");
    assert_eq!(manifest.wasm, "plugin.wasm");
}

#[test]
fn discovers_manifests_under_extension_roots() {
    let root = tempfile::tempdir().expect("tempdir");
    let ext_dir = root.path().join("demo");
    std::fs::create_dir_all(&ext_dir).expect("mkdir");
    std::fs::write(ext_dir.join("extension.toml"), "name = \"demo\"\nwasm = \"p.wasm\"\n").expect("write");
    let found = elph_agent::plugins::discover_manifests(&[PathBuf::from(root.path())]).expect("discover");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.name, "demo");
}

#[test]
fn registry_loads_without_wasm() {
    let root = tempfile::tempdir().expect("tempdir");
    let config = root.path().join("config");
    let project = root.path().join("project");
    std::fs::create_dir_all(&config).expect("config");
    std::fs::create_dir_all(project.join(".elph")).expect("project elph");
    let registry = ExtensionRegistry::new();
    registry
        .load(&config, &project.join(".elph"), &ExtensionsSettings::default(), true)
        .expect("load empty registry");
    assert!(registry.commands().is_empty());
}

#[test]
fn rejects_wasi_import_module() {
    let wasm = wat::parse_str(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
        )"#,
    )
    .expect("wat");
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config");
    let dest = config.join("extensions/wasi-ext");
    std::fs::create_dir_all(&dest).expect("dest");
    std::fs::write(dest.join("plugin.wasm"), wasm).expect("write wasm");
    std::fs::write(dest.join("extension.toml"), "name = \"wasi-ext\"\nwasm = \"plugin.wasm\"\n").expect("toml");
    let project = dir.path().join("project/.elph");
    std::fs::create_dir_all(&project).expect("project");

    let registry = ExtensionRegistry::new();
    let err = registry
        .load(&config, &project, &ExtensionsSettings::default(), false)
        .expect_err("wasi module must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("WASI") || msg.contains("wasi"), "{msg}");
}

#[test]
fn wat_guest_registers_command_and_blocks_tool() {
    let cmd_json = r#"{"name":"hello","description":"Greet"}"#;
    let sub_json = r#"["tool_call"]"#;
    let slash_json = r#"{"message":"Hello, Ada!","is_error":false}"#;
    let block_json = r#"{"block":true,"reason":"blocked"}"#;

    let wat = format!(
        r#"
(module
  (import "elph" "register_command" (func $register_command (param i32 i32)))
  (import "elph" "subscribe" (func $subscribe (param i32 i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))

  (data (i32.const 16) "{cmd}")
  (data (i32.const 80) "{sub}")
  (data (i32.const 128) "{slash}")
  (data (i32.const 256) "{block}")

  (func $alloc (export "elph_alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (local.get $p) (local.get $n)))
    (local.get $p)
  )
  (func (export "elph_dealloc") (param i32 i32))

  (func (export "elph_init")
    (call $register_command (i32.const 16) (i32.const {cmd_len}))
    (call $subscribe (i32.const 80) (i32.const {sub_len}))
  )

  (func $lenpref (param $src i32) (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (call $alloc (i32.add (local.get $n) (i32.const 4))))
    (i32.store (local.get $p) (local.get $n))
    (memory.copy (i32.add (local.get $p) (i32.const 4)) (local.get $src) (local.get $n))
    (local.get $p)
  )

  (func (export "elph_execute_command") (param i32 i32 i32 i32) (result i32)
    (call $lenpref (i32.const 128) (i32.const {slash_len}))
  )

  (func (export "elph_on_event") (param i32 i32) (result i32)
    (call $lenpref (i32.const 256) (i32.const {block_len}))
  )

  (func (export "elph_execute_tool") (param i32 i32) (result i32)
    (i32.const 0)
  )
)
"#,
        cmd = cmd_json.replace('"', "\\\""),
        sub = sub_json.replace('"', "\\\""),
        slash = slash_json.replace('"', "\\\""),
        block = block_json.replace('"', "\\\""),
        cmd_len = cmd_json.len(),
        sub_len = sub_json.len(),
        slash_len = slash_json.len(),
        block_len = block_json.len(),
    );

    let wasm = wat::parse_str(&wat).expect("wat");
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config");
    let dest = config.join("extensions/demo");
    std::fs::create_dir_all(&dest).expect("dest");
    std::fs::write(dest.join("plugin.wasm"), wasm).expect("wasm");
    std::fs::write(
        dest.join("extension.toml"),
        "name = \"demo\"\nversion = \"1\"\ndescription = \"fixture\"\nwasm = \"plugin.wasm\"\n",
    )
    .expect("toml");
    let project = dir.path().join("project/.elph");
    std::fs::create_dir_all(&project).expect("project");

    let registry = ExtensionRegistry::new();
    registry
        .load(&config, &project, &ExtensionsSettings::default(), false)
        .expect("load");
    let commands = registry.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].name, "hello");

    let result = registry.dispatch_slash("hello", "Ada").expect("dispatch").expect("ok");
    assert!(!result.is_error);
    assert!(result.message.contains("Ada"), "{}", result.message);

    let blocked = registry.dispatch_event(
        "tool_call",
        &serde_json::json!({ "tool_name": "shell_exec", "input": { "command": "rm -rf /" } }),
    );
    assert_eq!(blocked.and_then(|v| v.get("block").cloned()), Some(serde_json::json!(true)));
}
