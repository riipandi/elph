//! Build chat model catalogs from models.dev (origin) + Elph provider overlays.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;

use super::models_dev::{default_cache_dir, find_model, find_model_fuzzy, load_models_dev, models_for_provider_keys};
use super::normalize::{enrich_existing, from_models_dev};
use super::pricing::{apply_cost, fetch_all_live_pricing, resolve_cost};
use super::provider_sources::all_provider_sources;
use super::term;

#[derive(Serialize)]
pub struct CatalogIndexEntry {
    #[serde(rename = "providerId")]
    pub provider_id: String,
    #[serde(rename = "rustMod")]
    pub rust_mod: String,
    pub count: usize,
}

pub struct ChatOptions {
    pub models_dir: PathBuf,
    pub catalog_rs: PathBuf,
    pub no_regenerate_catalog: bool,
    pub offline: bool,
    pub no_live_pricing: bool,
}

pub fn generate_chat(options: ChatOptions) -> Result<()> {
    term::header("generate-models chat · models.dev origin");
    fs::create_dir_all(&options.models_dir).context("create models output directory")?;
    let cache_dir = default_cache_dir(&options.models_dir);
    let models_dev = load_models_dev(&cache_dir, options.offline)?;
    let live = fetch_all_live_pricing(options.no_live_pricing);

    let mut index: Vec<CatalogIndexEntry> = Vec::new();
    let mut total_models = 0usize;
    let mut maps_ok = 0usize;
    let mut maps_bad = 0usize;

    term::header("providers");
    for src in all_provider_sources() {
        let rust_mod = src.id.replace('-', "_");
        let out_path = options.models_dir.join(format!("{rust_mod}.json"));
        let previous = load_previous_catalog(&out_path)?;

        let mut catalog = BTreeMap::new();

        if src.gateway_preserve_ids || models_for_provider_keys(&models_dev, src.models_dev_keys).is_none() {
            // Keep existing model ids (gateway / Elph-only) and enrich from models.dev.
            let prev_map = previous.as_ref().and_then(|v| v.as_object());
            if let Some(prev_map) = prev_map {
                for (mid, prev_entry) in prev_map {
                    let mut entry = if let Some(m) = find_model(&models_dev, src.models_dev_keys, mid) {
                        enrich_existing(src, mid, prev_entry, Some(m))
                    } else if let Some((_, m)) = find_model_fuzzy(&models_dev, mid) {
                        enrich_existing(src, mid, prev_entry, Some(&m))
                    } else {
                        enrich_existing(src, mid, prev_entry, None)
                    };
                    let (i, o, cr, cw, _) = resolve_cost(src, mid, &models_dev, &live, entry.get("cost"));
                    apply_cost(&mut entry, i, o, cr, cw);
                    tally_map(&entry, &mut maps_ok, &mut maps_bad);
                    catalog.insert(mid.clone(), entry);
                }
            } else if let Some((_, mdev_models)) = models_for_provider_keys(&models_dev, src.models_dev_keys) {
                for (mid, mdev) in mdev_models {
                    let mut entry = from_models_dev(src, mid, mdev, None);
                    let (i, o, cr, cw, _) = resolve_cost(src, mid, &models_dev, &live, None);
                    apply_cost(&mut entry, i, o, cr, cw);
                    tally_map(&entry, &mut maps_ok, &mut maps_bad);
                    catalog.insert(mid.clone(), entry);
                }
            } else {
                term::warn(format!("{}: no previous catalog and not on models.dev — skipped", src.id));
                continue;
            }
        } else if let Some((_, mdev_models)) = models_for_provider_keys(&models_dev, src.models_dev_keys) {
            // Origin: models.dev list
            let prev_map = previous.as_ref().and_then(|v| v.as_object());
            for (mid, mdev) in mdev_models {
                let prev = prev_map.and_then(|m| m.get(mid));
                let mut entry = from_models_dev(src, mid, mdev, prev);
                let (i, o, cr, cw, _) = resolve_cost(src, mid, &models_dev, &live, entry.get("cost"));
                apply_cost(&mut entry, i, o, cr, cw);
                tally_map(&entry, &mut maps_ok, &mut maps_bad);
                catalog.insert(mid.clone(), entry);
            }
        } else {
            term::warn(format!("{}: not on models.dev and not gateway — skipped", src.id));
            continue;
        }

        if catalog.is_empty() {
            term::warn(format!("{}: empty catalog — skipped", src.id));
            continue;
        }

        let count = catalog.len();
        total_models += count;
        let json = Value::Object(catalog.into_iter().collect());
        let pretty = serde_json::to_string_pretty(&json).context("serialize catalog")?;
        fs::write(&out_path, format!("{pretty}\n")).with_context(|| format!("write {}", out_path.display()))?;
        let file_name = out_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        term::provider_ok(src.id, count, file_name);
        index.push(CatalogIndexEntry {
            provider_id: src.id.to_string(),
            rust_mod,
            count,
        });
    }

    index.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
    let index_path = options.models_dir.join("index.json");
    let index_json = serde_json::to_string_pretty(&index).context("serialize index")?;
    fs::write(&index_path, format!("{index_json}\n")).context("write index.json")?;

    term::header("summary");
    if options.no_regenerate_catalog {
        term::success(format!(
            "Wrote {} providers / {total_models} models (skipped catalog.rs)",
            index.len()
        ));
    } else {
        let catalog_source = render_chat_catalog_rs(&index);
        fs::write(&options.catalog_rs, catalog_source).context("write catalog.rs")?;
        let embedded_json_path = options
            .catalog_rs
            .parent()
            .map(|p| p.join("embedded_json.rs"))
            .context("resolve embedded_json.rs")?;
        fs::write(&embedded_json_path, render_embedded_json_rs(&index)).context("write embedded_json.rs")?;
        term::success(format!(
            "Wrote {} providers / {total_models} models + catalog.rs + embedded_json.rs",
            index.len()
        ));
        term::info(format!("{}", options.catalog_rs.display()));
    }

    term::metric("thinkingLevelMap", maps_ok, maps_bad);
    if maps_bad > 0 {
        term::err(format!("{maps_bad} models missing complete thinkingLevelMap"));
        bail!("{maps_bad} models missing complete thinkingLevelMap");
    }

    let builtin_rs = options
        .catalog_rs
        .parent()
        .and_then(|p| p.parent())
        .map(|src| src.join("providers/builtin.rs"))
        .context("resolve builtin.rs")?;
    verify_providers_registered(&index, &builtin_rs)?;

    Ok(())
}

fn load_previous_catalog(path: &std::path::Path) -> Result<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let json: Value = serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(json))
}

fn tally_map(entry: &Value, ok: &mut usize, bad: &mut usize) {
    const KEYS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
    let Some(map) = entry.get("thinkingLevelMap").and_then(|v| v.as_object()) else {
        *bad += 1;
        return;
    };
    if KEYS.iter().all(|k| map.contains_key(*k)) {
        *ok += 1;
    } else {
        *bad += 1;
    }
}

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
        term::note(format!(
            "builtin_providers() has entries not in catalog (ok if intentional): {}",
            extra.join(", ")
        ));
    }
    term::verified(format!(
        "Verified {} catalog providers are registered in builtin_providers()",
        catalog.len()
    ));
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

/// Raw JSON files for `CONFIG_DIR/providers/` unpack (kebab-case ids).
fn render_embedded_json_rs(index: &[CatalogIndexEntry]) -> String {
    let mut out = String::from(
        "//! Embedded raw provider catalog JSON for CONFIG_DIR unpack (auto-generated by \
         `generate-models chat` — do not edit).\n\n\
         /// `(provider_id kebab-case, raw JSON file body)`.\n\
         pub fn embedded_provider_json() -> &'static [(&'static str, &'static str)] {\n\
             &[\n",
    );
    for entry in index {
        out.push_str(&format!(
            "        (\"{}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/models/{}.json\"))),\n",
            entry.provider_id, entry.rust_mod
        ));
    }
    out.push_str("    ]\n}\n");
    out
}
