//! Extension discovery and manifest parsing.

use elph_agent::{ExtensionRegistry, ExtensionsSettings, discover_manifests, load_manifest};
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
component = "component.wasm"
"#,
    )
    .expect("write manifest");
    let manifest = load_manifest(&manifest_path).expect("parse manifest");
    assert_eq!(manifest.name, "demo");
    assert_eq!(manifest.component, "component.wasm");
}

#[test]
fn discovers_manifests_under_extension_roots() {
    let root = tempfile::tempdir().expect("tempdir");
    let ext_dir = root.path().join("demo");
    std::fs::create_dir_all(&ext_dir).expect("mkdir");
    std::fs::write(
        ext_dir.join("extension.toml"),
        "name = \"demo\"\ncomponent = \"c.wasm\"\n",
    )
    .expect("write");
    let found = discover_manifests(&[PathBuf::from(root.path())]).expect("discover");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.name, "demo");
}

#[test]
fn registry_loads_without_wasm_components() {
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
