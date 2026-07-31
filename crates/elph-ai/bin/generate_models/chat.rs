//! Model catalog generator. Reads flattened model JSON from the upstream pi-ai data
//! directory (`src/providers/data/*.json`) and writes them as embedded catalog files.
//!
//! Each data JSON is keyed by API type, with models nested inside:
//! ```json
//! { "openai-completions": { "model-id": { ... } } }
//! ```
//! The generator merges all API groups into a single flat object per provider.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::bail;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use super::common::CATALOG_CHAT_SCRIPT;
use super::common::run_catalog_npm_script;

#[derive(Serialize)]
pub struct CatalogIndexEntry {
    #[serde(rename = "providerId")]
    pub provider_id: String,
    #[serde(rename = "rustMod")]
    pub rust_mod: String,
    pub count: usize,
}

pub struct ChatOptions {
    pub catalog_dir: PathBuf,
    pub skip_scripts: bool,
    pub models_dir: PathBuf,
    pub catalog_rs: PathBuf,
    pub no_regenerate_catalog: bool,
}

pub fn generate_chat(options: ChatOptions) -> Result<()> {
    if !options.catalog_dir.join(CATALOG_CHAT_SCRIPT).is_file() {
        bail!(
            "catalog source package not found at {}\n  expected {} under earendil-works/pi (see docs/porting/README.md)",
            options.catalog_dir.display(),
            CATALOG_CHAT_SCRIPT
        );
    }

    if !options.skip_scripts {
        run_catalog_npm_script(&options.catalog_dir, "generate-models")?;
    }

    let data_dir = options.catalog_dir.join("src/providers/data");
    if !data_dir.is_dir() {
        bail!("missing catalog data directory at {}", data_dir.display());
    }

    fs::create_dir_all(&options.models_dir).context("create models output directory")?;

    let mut catalogs: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for entry in fs::read_dir(&data_dir).context("read catalog data directory")? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(provider_id) = file_name.strip_suffix(".json") else {
            continue;
        };
        // Skip the manifest file if present
        if provider_id.starts_with('.') {
            continue;
        }

        let rust_mod = provider_id.replace('-', "_");
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let json: Value = serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;

        // Flatten: merge all api-grouped sub-objects into one (mirrors flattenModelCatalog).
        let flattened = flatten_catalog_json(json, provider_id);
        let count = flattened.as_object().map(|m| m.len()).unwrap_or(0);
        if count == 0 {
            continue;
        }
        catalogs.insert(provider_id.to_string(), (rust_mod, flattened));
        println!("Converted {provider_id}: {count} models");
    }

    if catalogs.is_empty() {
        bail!("no model catalog data files found under {}", data_dir.display());
    }

    let mut index = Vec::new();
    for (provider_id, (rust_mod, json)) in &catalogs {
        let out_path = options.models_dir.join(format!("{rust_mod}.json"));
        let pretty = serde_json::to_string_pretty(json).context("serialize catalog json")?;
        fs::write(&out_path, format!("{pretty}\n")).with_context(|| format!("write {}", out_path.display()))?;
        index.push(CatalogIndexEntry {
            provider_id: provider_id.clone(),
            rust_mod: rust_mod.clone(),
            count: json.as_object().map(|m| m.len()).unwrap_or(0),
        });
    }

    // Keep Elph-only catalogs that live only under models/*.json (not in upstream pi).
    merge_local_only_catalogs(&options.models_dir, &mut index)?;

    let index_path = options.models_dir.join("index.json");
    let index_json = serde_json::to_string_pretty(&index).context("serialize index.json")?;
    fs::write(&index_path, format!("{index_json}\n")).context("write index.json")?;

    if options.no_regenerate_catalog {
        println!(
            "\nWrote {} chat catalogs to {} (skipped catalog.rs regeneration)",
            index.len(),
            options.models_dir.display()
        );
    } else {
        let catalog_source = render_chat_catalog_rs(&index);
        fs::write(&options.catalog_rs, catalog_source).context("write src/models/catalog.rs")?;
        println!(
            "\nWrote {} chat catalogs to {} and regenerated {}",
            index.len(),
            options.models_dir.display(),
            options.catalog_rs.display()
        );
    }

    // Ensure every catalog provider is wired into builtin_providers() so runtime stream/auth works.
    let builtin_rs = options
        .catalog_rs
        .parent() // src/models
        .and_then(|p| p.parent()) // src
        .map(|src| src.join("providers/builtin.rs"))
        .context("resolve providers/builtin.rs path")?;
    verify_providers_registered(&index, &builtin_rs)?;

    Ok(())
}

/// Flatten a nested `{ api_type: { model_id: model } }` structure into
/// a flat `{ model_id: model }` object, injecting the `api` field from the key.
///
/// This mirrors the JavaScript `flattenModelCatalog` function from pi-ai:
/// ```js
/// flattenModelCatalog(provider, groups) {
///   return Object.assign({}, ...Object.values(groups));
/// }
/// ```
///
/// Also normalizes `provider` / `id` so embedded catalogs always load under the
/// correct provider key in elph.
fn flatten_catalog_json(nested: Value, provider_id: &str) -> Value {
    match nested {
        Value::Object(groups) => {
            let mut merged = serde_json::Map::new();
            for (api_type, models) in groups {
                if let Value::Object(models_map) = models {
                    for (mid, mut model) in models_map {
                        if let Value::Object(ref mut fields) = model {
                            if !fields.contains_key("api") {
                                fields.insert("api".to_string(), Value::String(api_type.clone()));
                            }
                            // Always own the provider id from the catalog file name.
                            fields.insert("provider".to_string(), Value::String(provider_id.to_string()));
                            // Keep map key and model.id aligned for lookup.
                            fields.insert("id".to_string(), Value::String(mid.clone()));
                        }
                        merged.insert(mid, model);
                    }
                }
            }
            Value::Object(merged)
        }
        other => other,
    }
}

/// Factory function name → provider id for named `*_provider()` entries in `builtin_providers()`.
fn named_factory_provider_id(fn_name: &str) -> Option<&'static str> {
    Some(match fn_name {
        "amazon_bedrock_provider" => "amazon-bedrock",
        "anthropic_provider" => "anthropic",
        "cloudflare_ai_gateway_provider" => "cloudflare-ai-gateway",
        "cloudflare_workers_ai_provider" => "cloudflare-workers-ai",
        "fireworks_provider" => "fireworks",
        "github_copilot_provider" => "github-copilot",
        "google_vertex_provider" => "google-vertex",
        "hyper_provider" => "hyper",
        "kimi_coding_provider" => "kimi-coding",
        "mistral_provider" => "mistral",
        "neuralwatt_provider" => "neuralwatt",
        "nvidia_provider" => "nvidia",
        "openai_provider" => "openai",
        "openai_codex_provider" => "openai-codex",
        "opencode_provider" => "opencode",
        "opencode_go_provider" => "opencode-go",
        "sumopod_provider" => "sumopod",
        "xai_provider" => "xai",
        _ => return None,
    })
}

/// Parse `builtin_providers()` body for registered provider ids.
fn parse_registered_provider_ids(builtin_src: &str) -> Result<std::collections::BTreeSet<String>> {
    use std::collections::BTreeSet;

    let start = builtin_src
        .find("pub fn builtin_providers()")
        .context("builtin_providers() not found in providers/builtin.rs")?;
    let after = &builtin_src[start..];
    let body_start = after.find("vec![").context("builtin_providers vec![ not found")?;
    let body = &after[body_start..];
    // Take until the matching `]\n}` of the function is fragile; scan until we hit `    ]\n}`
    // after the first vec. Use a simple depth-ish cut at the first `\n    ]\n` after vec![.
    let end = body
        .find("\n    ]")
        .context("could not find end of builtin_providers vec")?;
    let body = &body[..end];

    let mut ids = BTreeSet::new();
    // simple_provider!("id", ...)
    for cap in regex_lite_simple_provider_ids(body) {
        ids.insert(cap);
    }
    // named factories: foo_provider()
    for name in body.split_whitespace() {
        let name = name.trim_end_matches([',', '(', ')']);
        if name.ends_with("_provider")
            && let Some(id) = named_factory_provider_id(name)
        {
            ids.insert(id.to_string());
        }
    }
    // Also catch `openai_provider(),` style with no whitespace split issues
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_suffix("(),") {
            if let Some(id) = named_factory_provider_id(name) {
                ids.insert(id.to_string());
            }
        } else if let Some(name) = trimmed.strip_suffix("()")
            && let Some(id) = named_factory_provider_id(name)
        {
            ids.insert(id.to_string());
        }
    }

    Ok(ids)
}

fn regex_lite_simple_provider_ids(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = body;
    while let Some(idx) = search.find("simple_provider!(") {
        let after = &search[idx + "simple_provider!(".len()..];
        let after = after.trim_start();
        if let Some(rest) = after.strip_prefix('"')
            && let Some(end) = rest.find('"')
        {
            out.push(rest[..end].to_string());
        }
        search = &search[idx + 1..];
    }
    out
}

fn verify_providers_registered(index: &[CatalogIndexEntry], builtin_rs: &std::path::Path) -> Result<()> {
    if !builtin_rs.is_file() {
        bail!("cannot verify provider registration: missing {}", builtin_rs.display());
    }
    let src = fs::read_to_string(builtin_rs).with_context(|| format!("read {}", builtin_rs.display()))?;
    let registered = parse_registered_provider_ids(&src)?;
    let catalog: std::collections::BTreeSet<String> = index.iter().map(|e| e.provider_id.clone()).collect();

    let missing: Vec<_> = catalog.difference(&registered).cloned().collect();
    let extra: Vec<_> = registered.difference(&catalog).cloned().collect();

    if !missing.is_empty() {
        bail!(
            "catalog providers missing from builtin_providers() — models will load in the UI but stream/auth fails with \
             \"Unknown provider\":\n  {}\n\n\
             Register each factory in crates/elph-ai/src/providers/builtin.rs (`builtin_providers`).\n\
             See crates/elph-ai/README.md → Adding a New Provider.",
            missing.join(", ")
        );
    }
    if !extra.is_empty() {
        println!(
            "note: builtin_providers() has entries not in catalog (ok if intentional): {}",
            extra.join(", ")
        );
    }
    println!(
        "Verified {} catalog providers are registered in builtin_providers()",
        catalog.len()
    );
    Ok(())
}

/// Merge local-only `models/*.json` catalogs (Hyper, Kilo, OpenGateway, …) into the index.
///
/// Upstream pi only writes providers present in its data directory; Elph-only JSON
/// files already on disk must stay registered in `index.json` / `catalog.rs`.
fn merge_local_only_catalogs(models_dir: &std::path::Path, index: &mut Vec<CatalogIndexEntry>) -> Result<()> {
    let known: std::collections::HashSet<String> = index.iter().map(|e| e.rust_mod.clone()).collect();
    let mut added = 0usize;
    for entry in fs::read_dir(models_dir).with_context(|| format!("read {}", models_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(rust_mod) = name.strip_suffix(".json") else {
            continue;
        };
        if rust_mod == "index" || known.contains(rust_mod) {
            continue;
        }
        let raw = fs::read_to_string(&path).with_context(|| format!("read local catalog {}", path.display()))?;
        let json: Value =
            serde_json::from_str(&raw).with_context(|| format!("parse local catalog {}", path.display()))?;
        let count = json.as_object().map(|m| m.len()).unwrap_or(0);
        if count == 0 {
            continue;
        }
        let provider_id = rust_mod.replace('_', "-");
        println!("Preserved local-only catalog {provider_id}: {count} models");
        index.push(CatalogIndexEntry {
            provider_id,
            rust_mod: rust_mod.to_string(),
            count,
        });
        added += 1;
    }
    if added > 0 {
        index.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// catalog.rs rendering
// ---------------------------------------------------------------------------

fn render_chat_catalog_rs(index: &[CatalogIndexEntry]) -> String {
    let mut out = String::from(
        "//! Embedded builtin model catalogs (auto-generated by `generate-models chat` — do not edit).\n\n\
         use std::collections::HashMap;\n\
         use std::sync::LazyLock;\n\
         use serde::Deserialize;\n\n\
         use crate::types::{AnthropicMessagesCompat, Model, ModelCost, ModelCostTier};\n\n\
         use crate::types::{OpenAICompletionsCompat, OpenAIResponsesCompat, ThinkingLevelMap};\n\n\
         #[derive(Debug, Deserialize)]\n\
         struct RawModel {\n\
             id: String,\n\
             name: String,\n\
             api: String,\n\
             provider: String,\n\
             #[serde(rename = \"baseUrl\")]\n\
             base_url: String,\n\
             reasoning: bool,\n\
             #[serde(default)]\n\
             #[serde(rename = \"thinkingLevelMap\")]\n\
             thinking_level_map: Option<ThinkingLevelMap>,\n\
             input: Vec<String>,\n\
             cost: RawCost,\n\
             #[serde(rename = \"contextWindow\")]\n\
             context_window: u32,\n\
             #[serde(rename = \"maxTokens\")]\n\
             max_tokens: u32,\n\
             #[serde(default)]\n\
             headers: Option<HashMap<String, String>>,\n\
             #[serde(default)]\n\
             compat: Option<serde_json::Value>,\n\
         }\n\n\
         #[derive(Debug, Deserialize)]\n\
         struct RawCost {\n\
             input: f64,\n\
             output: f64,\n\
             #[serde(rename = \"cacheRead\")]\n\
             cache_read: f64,\n\
             #[serde(rename = \"cacheWrite\")]\n\
             cache_write: f64,\n\
             #[serde(default)]\n\
             tiers: Option<Vec<RawCostTier>>,\n\
         }\n\n\
         #[derive(Debug, Deserialize)]\n\
         struct RawCostTier {\n\
             #[serde(rename = \"inputTokensAbove\")]\n\
             input_tokens_above: u64,\n\
             input: f64,\n\
             output: f64,\n\
             #[serde(rename = \"cacheRead\")]\n\
             cache_read: f64,\n\
             #[serde(rename = \"cacheWrite\")]\n\
             cache_write: f64,\n\
         }\n\n\
         fn parse_models(json: &str) -> Vec<Model> {\n\
             let raw: HashMap<String, RawModel> = serde_json::from_str(json).expect(\"invalid embedded model catalog\");\n\
             raw.into_values().map(convert_model).collect()\n\
         }\n\n\
         fn convert_model(raw: RawModel) -> Model {\n\
             let (openai_completions_compat, openai_responses_compat, anthropic_compat) =\n\
                 parse_compat(&raw.api, raw.compat.as_ref());\n\n\
             Model {\n\
                 id: raw.id,\n\
                 name: raw.name,\n\
                 api: raw.api,\n\
                 provider: raw.provider,\n\
                 base_url: raw.base_url,\n\
                 reasoning: raw.reasoning,\n\
                 thinking_level_map: raw.thinking_level_map,\n\
                 input: raw.input,\n\
                 cost: ModelCost {\n\
                     input: raw.cost.input,\n\
                     output: raw.cost.output,\n\
                     cache_read: raw.cost.cache_read,\n\
                     cache_write: raw.cost.cache_write,\n\
                     tiers: raw.cost.tiers.map(|tiers| {\n\
                         tiers\n\
                             .into_iter()\n\
                             .map(|t| ModelCostTier {\n\
                                 input_tokens_above: t.input_tokens_above,\n\
                                 input: t.input,\n\
                                 output: t.output,\n\
                                 cache_read: t.cache_read,\n\
                                 cache_write: t.cache_write,\n\
                             })\n\
                             .collect()\n\
                     }),\n\
                 },\n\
                 context_window: raw.context_window,\n\
                 max_tokens: raw.max_tokens,\n\
                 headers: raw.headers,\n\
                 openai_completions_compat,\n\
                 openai_responses_compat,\n\
                 anthropic_compat,\n\
             }\n\
         }\n\n\
         fn parse_compat(\n\
             api: &str,\n\
             compat: Option<&serde_json::Value>,\n\
         ) -> (\n\
             Option<OpenAICompletionsCompat>,\n\
             Option<OpenAIResponsesCompat>,\n\
             Option<AnthropicMessagesCompat>,\n\
         ) {\n\
             let Some(compat) = compat else {\n\
                 return (None, None, None);\n\
             };\n\
             match api {\n\
                 \"openai-completions\" => (serde_json::from_value(compat.clone()).ok(), None, None),\n\
                 \"openai-responses\" | \"azure-openai-responses\" | \"openai-codex-responses\" => {\n\
                     (None, serde_json::from_value(compat.clone()).ok(), None)\n\
                 }\n\
                 \"anthropic-messages\" => (None, None, serde_json::from_value(compat.clone()).ok()),\n\
                 _ => (None, None, None),\n\
             }\n\
         }\n\n\
         macro_rules! define_catalog {\n\
             ($name:ident, $file:literal) => {\n\
                 pub static $name: LazyLock<Vec<Model>> = LazyLock::new(|| {\n\
                     parse_models(include_str!(concat!(\n\
                         env!(\"CARGO_MANIFEST_DIR\"),\n\
                         \"/models/\",\n\
                         $file\n\
                     )))\n\
                 });\n\
             };\n\
         }\n\n",
    );

    for entry in index {
        let const_name = chat_catalog_const_name(&entry.provider_id);
        out.push_str(&format!("define_catalog!({const_name}, \"{}.json\");\n", entry.rust_mod));
    }

    out.push_str("\npub fn all_builtin_models() -> HashMap<&'static str, &'static [Model]> {\n    HashMap::from([\n");
    for entry in index {
        let const_name = chat_catalog_const_name(&entry.provider_id);
        out.push_str(&format!("        (\"{}\", {}.as_slice()),\n", entry.provider_id, const_name));
    }
    out.push_str(
        "    ])\n}\n\n\
         pub fn get_builtin_model(provider: &str, id: &str) -> Option<Model> {\n\
             all_builtin_models().get(provider)?.iter().find(|m| m.id == id).cloned()\n\
         }\n\n\
         pub fn get_builtin_models(provider: &str) -> Vec<Model> {\n\
             all_builtin_models()\n\
                 .get(provider)\n\
                 .map(|models| models.to_vec())\n\
                 .unwrap_or_default()\n\
         }\n\n\
         pub fn get_builtin_providers() -> Vec<&'static str> {\n\
             let mut providers: Vec<_> = all_builtin_models().keys().copied().collect();\n\
             providers.sort_unstable();\n\
             providers\n\
         }\n",
    );
    out
}

fn chat_catalog_const_name(provider_id: &str) -> String {
    let mut out = String::new();
    for ch in provider_id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    format!("{out}_MODELS")
}
