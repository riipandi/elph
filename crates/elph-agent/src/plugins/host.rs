//! wasmi core-Wasm host for extension guests.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail, ensure};
use parking_lot::Mutex;
use serde_json::Value;
use wasmi::{Caller, Config, Engine, Linker, Module, Store};

use super::abi::{read_len_prefixed, read_utf8, write_guest_bytes};
use super::types::{ExtensionCommand, ExtensionManifest, ExtensionSlashResult, ExtensionToolSpec};
use super::ui::{DenyUi, ExtensionUi};

const FUEL_PER_CALL: u64 = 10_000_000;

pub struct PluginState {
    pub ui: Arc<dyn ExtensionUi>,
    pub commands: Vec<ExtensionCommand>,
    pub tools: Vec<ExtensionToolSpec>,
    pub events: Vec<String>,
    pub extension_name: String,
}

struct InstanceState {
    store: Store<PluginState>,
    instance: wasmi::Instance,
}

pub struct LoadedExtension {
    pub manifest: ExtensionManifest,
    #[allow(dead_code)]
    pub root: std::path::PathBuf,
    inner: Mutex<InstanceState>,
}

impl LoadedExtension {
    pub fn load(
        engine: &Engine,
        root: &std::path::Path,
        manifest: ExtensionManifest,
        ui: Arc<dyn ExtensionUi>,
    ) -> Result<Self> {
        let wasm_path = manifest.wasm_path(root);
        ensure!(wasm_path.is_file(), "wasm not found: {}", wasm_path.display());
        let bytes = std::fs::read(&wasm_path).with_context(|| format!("read {}", wasm_path.display()))?;
        Self::load_bytes(engine, root.to_path_buf(), manifest, ui, &bytes)
    }

    pub fn load_bytes(
        engine: &Engine,
        root: std::path::PathBuf,
        manifest: ExtensionManifest,
        ui: Arc<dyn ExtensionUi>,
        bytes: &[u8],
    ) -> Result<Self> {
        if looks_like_wasi_module(bytes) {
            bail!(
                "extension '{}' imports WASI; Elph guests must target wasm32-unknown-unknown without WASI",
                manifest.name
            );
        }

        let module = Module::new(engine, bytes).map_err(|error| anyhow!("parse wasm: {error}"))?;
        let mut linker = Linker::new(engine);
        define_host_imports(&mut linker)?;

        let state = PluginState {
            ui,
            commands: Vec::new(),
            tools: Vec::new(),
            events: Vec::new(),
            extension_name: manifest.name.clone(),
        };
        let mut store = Store::new(engine, state);
        if store.set_fuel(FUEL_PER_CALL).is_err() {
            log::debug!("wasmi fuel metering unavailable");
        }

        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|error| anyhow!("instantiate: {error}"))?;

        let init = instance
            .get_typed_func::<(), ()>(&store, "elph_init")
            .map_err(|error| anyhow!("missing elph_init: {error}"))?;
        let _ = store.set_fuel(FUEL_PER_CALL);
        init.call(&mut store, ())
            .map_err(|error| anyhow!("elph_init: {error}"))?;

        Ok(Self {
            manifest,
            root,
            inner: Mutex::new(InstanceState { store, instance }),
        })
    }

    pub fn commands(&self) -> Vec<ExtensionCommand> {
        self.inner.lock().store.data().commands.clone()
    }

    pub fn tools(&self) -> Vec<ExtensionToolSpec> {
        self.inner.lock().store.data().tools.clone()
    }

    pub fn subscribed(&self, event: &str) -> bool {
        self.inner.lock().store.data().events.iter().any(|name| name == event)
    }

    pub fn execute_command(&self, name: &str, args: &str) -> Result<ExtensionSlashResult> {
        let mut inner = self.inner.lock();
        let _ = inner.store.set_fuel(FUEL_PER_CALL);
        let memory = inner
            .instance
            .get_memory(&inner.store, "memory")
            .context("guest memory")?;
        let alloc = inner
            .instance
            .get_typed_func::<i32, i32>(&inner.store, "elph_alloc")
            .map_err(|error| anyhow!("elph_alloc: {error}"))?;
        let execute = inner
            .instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&inner.store, "elph_execute_command")
            .map_err(|error| anyhow!("elph_execute_command: {error}"))?;

        let name_ptr = write_guest_bytes(&mut inner.store, memory, &alloc, name.as_bytes())?;
        let args_ptr = write_guest_bytes(&mut inner.store, memory, &alloc, args.as_bytes())?;
        let result_ptr = execute
            .call(&mut inner.store, (name_ptr, name.len() as i32, args_ptr, args.len() as i32))
            .map_err(|error| anyhow!("execute_command: {error}"))?;
        if result_ptr == 0 {
            return Ok(ExtensionSlashResult {
                message: "extension returned null".into(),
                is_error: true,
            });
        }
        let payload = read_len_prefixed(&inner.store, memory, result_ptr)?;
        serde_json::from_slice(&payload).context("parse slash result")
    }

    pub fn execute_tool(&self, name: &str, tool_call_id: &str, input: &Value) -> Result<Value> {
        let mut inner = self.inner.lock();
        let _ = inner.store.set_fuel(FUEL_PER_CALL);
        let memory = inner
            .instance
            .get_memory(&inner.store, "memory")
            .context("guest memory")?;
        let alloc = inner
            .instance
            .get_typed_func::<i32, i32>(&inner.store, "elph_alloc")
            .map_err(|error| anyhow!("elph_alloc: {error}"))?;
        let execute = inner
            .instance
            .get_typed_func::<(i32, i32), i32>(&inner.store, "elph_execute_tool")
            .map_err(|error| anyhow!("elph_execute_tool: {error}"))?;
        let body = serde_json::json!({
            "name": name,
            "tool_call_id": tool_call_id,
            "input": input,
        });
        let bytes = serde_json::to_vec(&body)?;
        let ptr = write_guest_bytes(&mut inner.store, memory, &alloc, &bytes)?;
        let result_ptr = execute
            .call(&mut inner.store, (ptr, bytes.len() as i32))
            .map_err(|error| anyhow!("execute_tool: {error}"))?;
        if result_ptr == 0 {
            bail!("elph_execute_tool returned null");
        }
        let payload = read_len_prefixed(&inner.store, memory, result_ptr)?;
        serde_json::from_slice(&payload).context("parse tool result")
    }

    pub fn on_event(&self, event: &str, payload: &Value) -> Result<Option<Value>> {
        if !self.subscribed(event) {
            return Ok(None);
        }
        let mut inner = self.inner.lock();
        let _ = inner.store.set_fuel(FUEL_PER_CALL);
        let memory = inner
            .instance
            .get_memory(&inner.store, "memory")
            .context("guest memory")?;
        let alloc = inner
            .instance
            .get_typed_func::<i32, i32>(&inner.store, "elph_alloc")
            .map_err(|error| anyhow!("elph_alloc: {error}"))?;
        let on_event = inner
            .instance
            .get_typed_func::<(i32, i32), i32>(&inner.store, "elph_on_event")
            .map_err(|error| anyhow!("elph_on_event: {error}"))?;
        let body = serde_json::json!({ "event": event, "payload": payload });
        let bytes = serde_json::to_vec(&body)?;
        let ptr = write_guest_bytes(&mut inner.store, memory, &alloc, &bytes)?;
        let result_ptr = on_event
            .call(&mut inner.store, (ptr, bytes.len() as i32))
            .map_err(|error| anyhow!("on_event: {error}"))?;
        if result_ptr == 0 {
            return Ok(None);
        }
        let payload = read_len_prefixed(&inner.store, memory, result_ptr)?;
        if payload.is_empty() || payload == b"null" {
            return Ok(None);
        }
        let value: Value = serde_json::from_slice(&payload).context("parse event result")?;
        if value.is_null() { Ok(None) } else { Ok(Some(value)) }
    }
}

pub fn new_engine() -> Result<Engine> {
    let mut config = Config::default();
    config.consume_fuel(true);
    Ok(Engine::new(&config))
}

pub fn default_ui() -> Arc<dyn ExtensionUi> {
    Arc::new(DenyUi)
}

fn define_host_imports(linker: &mut Linker<PluginState>) -> Result<()> {
    linker
        .func_wrap(
            "elph",
            "register_command",
            |mut caller: Caller<'_, PluginState>, ptr: i32, len: i32| {
                if let Ok(raw) = read_utf8(&caller, ptr, len)
                    && let Ok(value) = serde_json::from_str::<Value>(&raw)
                {
                    let name = value.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                    let description = value
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() {
                        let extension = caller.data().extension_name.clone();
                        caller.data_mut().commands.push(ExtensionCommand {
                            extension,
                            name,
                            description,
                        });
                    }
                }
            },
        )
        .map_err(|error| anyhow!("link register_command: {error}"))?;

    linker
        .func_wrap(
            "elph",
            "register_tool",
            |mut caller: Caller<'_, PluginState>, ptr: i32, len: i32| {
                if let Ok(raw) = read_utf8(&caller, ptr, len)
                    && let Ok(spec) = serde_json::from_str::<ExtensionToolSpec>(&raw)
                    && !spec.name.is_empty()
                {
                    caller.data_mut().tools.push(spec);
                }
            },
        )
        .map_err(|error| anyhow!("link register_tool: {error}"))?;

    linker
        .func_wrap(
            "elph",
            "subscribe",
            |mut caller: Caller<'_, PluginState>, ptr: i32, len: i32| {
                if let Ok(raw) = read_utf8(&caller, ptr, len)
                    && let Ok(events) = serde_json::from_str::<Vec<String>>(&raw)
                {
                    caller.data_mut().events.extend(events);
                }
            },
        )
        .map_err(|error| anyhow!("link subscribe: {error}"))?;

    linker
        .func_wrap("elph", "notify", |caller: Caller<'_, PluginState>, ptr: i32, len: i32| {
            if let Ok(raw) = read_utf8(&caller, ptr, len)
                && let Ok(value) = serde_json::from_str::<Value>(&raw)
            {
                let message = value.get("message").and_then(Value::as_str).unwrap_or("");
                let level = value.get("level").and_then(Value::as_str).unwrap_or("info");
                caller.data().ui.notify(message, level);
            }
        })
        .map_err(|error| anyhow!("link notify: {error}"))?;

    linker
        .func_wrap(
            "elph",
            "confirm",
            |caller: Caller<'_, PluginState>, ptr: i32, len: i32| -> i32 {
                let Ok(raw) = read_utf8(&caller, ptr, len) else {
                    return 0;
                };
                let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                    return 0;
                };
                let title = value.get("title").and_then(Value::as_str).unwrap_or("");
                let body = value.get("body").and_then(Value::as_str).unwrap_or("");
                i32::from(caller.data().ui.confirm(title, body))
            },
        )
        .map_err(|error| anyhow!("link confirm: {error}"))?;

    Ok(())
}

fn looks_like_wasi_module(bytes: &[u8]) -> bool {
    // Cheap scan: WASI preview1 modules import from `wasi_snapshot_preview1`.
    let hay = b"wasi_snapshot_preview1";
    bytes.windows(hay.len()).any(|window| window == hay) || {
        let hay2 = b"wasi:";
        bytes.windows(hay2.len()).any(|window| window == hay2)
    }
}
