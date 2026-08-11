//! Build chat model catalogs from models.dev (origin) + Elph provider overlays.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;

use super::models_dev::{default_cache_dir, find_model, find_model_fuzzy, load_models_dev, models_for_provider_keys};
use super::normalize::{enrich_existing, from_models_dev};
use super::pricing::{
    aimd_reasoning, apply_cost, fetch_ai_model_directory, fetch_all_live_data, fetch_nara_pricing, resolve_cost,
};
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
    pub builtin_rs: PathBuf,
    pub offline: bool,
    pub no_live_pricing: bool,
    pub force: bool,
}

/// Extract effort levels from live thinking data for a model.
/// Returns None when the model has boolean reasoning but no discrete effort levels.
fn live_efforts_for(
    live: &HashMap<String, super::pricing::LiveProbeResult>,
    provider_id: &str,
    model_id: &str,
) -> Option<Vec<String>> {
    live.get(provider_id)
        .and_then(|r| r.thinking.get(model_id))
        .filter(|t| !t.supported_efforts.is_empty())
        .map(|t| t.supported_efforts.clone())
}

pub fn generate_chat(options: ChatOptions) -> Result<()> {
    term::header("generate-models chat · models.dev origin");
    fs::create_dir_all(&options.models_dir).context("create models output directory")?;
    let cache_dir = default_cache_dir(&options.models_dir);
    let models_dev = load_models_dev(&cache_dir, options.offline, options.force)?;
    let mut live = fetch_all_live_data(options.no_live_pricing);
    let aimd = fetch_ai_model_directory(&cache_dir, options.offline);
    // Nara's /v1/models exposes no pricing; merge the official /api/pricing catalog
    // (USD per million tokens) into the live map so resolve_cost picks it up first.
    let nara_pricing = fetch_nara_pricing(&cache_dir, options.offline);
    if !nara_pricing.is_empty() {
        let nara_result = live.entry("nara-router".to_string()).or_default();
        for (id, prices) in nara_pricing {
            nara_result.pricing.insert(id, prices);
        }
    }

    let mut index: Vec<CatalogIndexEntry> = Vec::new();
    let mut total_models = 0usize;
    let mut maps_ok = 0usize;
    let mut maps_bad = 0usize;

    let mut cost_live = 0usize;
    let mut cost_mdev = 0usize;
    let mut cost_aimd = 0usize;
    let mut cost_prev = 0usize;
    let mut cost_none = 0usize;

    term::header("providers");
    for src in all_provider_sources() {
        let rust_mod = src.id.replace('-', "_");
        let out_path = options.models_dir.join(format!("{rust_mod}.json"));
        let previous = load_previous_catalog(&out_path)?;
        let mut catalog = BTreeMap::new();

        if src.gateway_preserve_ids || models_for_provider_keys(&models_dev, src.models_dev_keys).is_none() {
            let prev_map = previous.as_ref().and_then(|v| v.as_object());
            let live_ids = super::pricing::fetch_live_model_ids(src);
            if let Some(live_ids) = &live_ids {
                term::live_pricing(src.id, live_ids.len());
            }

            let mut ids: Vec<String> = Vec::new();
            if let Some(live_ids) = &live_ids {
                ids.extend(live_ids.iter().cloned());
            } else if let Some(prev_map) = prev_map {
                ids.extend(prev_map.keys().cloned());
            }
            ids.sort();
            ids.dedup();

            if ids.is_empty() {
                if let Some((_, mdev_models)) = models_for_provider_keys(&models_dev, src.models_dev_keys) {
                    for (mid, mdev) in mdev_models {
                        let _live_map: HashMap<String, super::pricing::PriceTriple> = HashMap::new();
                        let rich = models_dev.rich_model(src.id, mid);
                        let aimd_reasoning = aimd_reasoning(&aimd, src.id, mid);
                        let mut entry = from_models_dev(src, mid, mdev, None, None, rich, aimd_reasoning);
                        let (i, o, cr, cw, csrc) = resolve_cost(src, mid, &models_dev, &live, &aimd, None);
                        tally_cost(
                            csrc,
                            &mut cost_live,
                            &mut cost_mdev,
                            &mut cost_aimd,
                            &mut cost_prev,
                            &mut cost_none,
                        );
                        apply_cost(&mut entry, i, o, cr, cw);
                        tally_map(&entry, &mut maps_ok, &mut maps_bad);
                        catalog.insert(mid.clone(), entry);
                    }
                } else {
                    term::warn(format!(
                        "{}: no previous catalog, no live ids, not on models.dev — skipped",
                        src.id
                    ));
                    continue;
                }
            } else {
                for mid in ids {
                    let prev_entry = prev_map.and_then(|m| m.get(&mid));
                    let prev_ref = prev_entry.unwrap_or(&Value::Null);
                    let live_efforts = live_efforts_for(&live, src.id, &mid);
                    let _live_map: HashMap<String, super::pricing::PriceTriple> =
                        live.get(src.id).map(|r| r.pricing.clone()).unwrap_or_default();

                    let rich = models_dev.rich_model(src.id, &mid);
                    let aimd_reasoning = aimd_reasoning(&aimd, src.id, &mid);
                    let mut entry = if let Some(m) = find_model(&models_dev, src.models_dev_keys, &mid) {
                        enrich_existing(
                            src,
                            &mid,
                            prev_ref,
                            Some(m),
                            live_efforts.as_ref().map(|v| v.as_slice()),
                            rich,
                            aimd_reasoning,
                        )
                    } else if let Some((_, m)) = find_model_fuzzy(&models_dev, &mid) {
                        enrich_existing(
                            src,
                            &mid,
                            prev_ref,
                            Some(&m),
                            live_efforts.as_ref().map(|v| v.as_slice()),
                            rich,
                            aimd_reasoning,
                        )
                    } else {
                        enrich_existing(
                            src,
                            &mid,
                            prev_ref,
                            None,
                            live_efforts.as_ref().map(|v| v.as_slice()),
                            rich,
                            aimd_reasoning,
                        )
                    };
                    let (i, o, cr, cw, csrc) = resolve_cost(src, &mid, &models_dev, &live, &aimd, entry.get("cost"));
                    tally_cost(
                        csrc,
                        &mut cost_live,
                        &mut cost_mdev,
                        &mut cost_aimd,
                        &mut cost_prev,
                        &mut cost_none,
                    );
                    apply_cost(&mut entry, i, o, cr, cw);
                    tally_map(&entry, &mut maps_ok, &mut maps_bad);
                    catalog.insert(mid.clone(), entry);
                }
            }
        } else if let Some((_, mdev_models)) = models_for_provider_keys(&models_dev, src.models_dev_keys) {
            let prev_map = previous.as_ref().and_then(|v| v.as_object());
            let _live_map: HashMap<String, super::pricing::PriceTriple> =
                live.get(src.id).map(|r| r.pricing.clone()).unwrap_or_default();
            for (mid, mdev) in mdev_models {
                let prev = prev_map.and_then(|m| m.get(mid));
                let live_efforts = live_efforts_for(&live, src.id, mid);
                let rich = models_dev.rich_model(src.id, mid);
                let aimd_reasoning = aimd_reasoning(&aimd, src.id, mid);
                let mut entry = from_models_dev(
                    src,
                    mid,
                    mdev,
                    prev,
                    live_efforts.as_ref().map(|v| v.as_slice()),
                    rich,
                    aimd_reasoning,
                );
                let (i, o, cr, cw, csrc) = resolve_cost(src, mid, &models_dev, &live, &aimd, entry.get("cost"));
                tally_cost(
                    csrc,
                    &mut cost_live,
                    &mut cost_mdev,
                    &mut cost_aimd,
                    &mut cost_prev,
                    &mut cost_none,
                );
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
    term::success(format!("Wrote {} providers / {total_models} models", index.len()));
    term::info(format!("{}", options.models_dir.display()));
    term::note("build.rs compresses models/*.json into the binary on the next build".to_string());

    term::metric("thinkingLevelMap", maps_ok, maps_bad);
    if maps_bad > 0 {
        term::err(format!("{maps_bad} models missing complete thinkingLevelMap"));
        bail!("{maps_bad} models missing complete thinkingLevelMap");
    }

    term::cost_breakdown(cost_live, cost_mdev, cost_aimd, cost_prev, cost_none);

    verify_providers_registered(&index, &options.builtin_rs)?;

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

/// Tally a resolved cost source tag into the running counters.
fn tally_cost(src: &str, live: &mut usize, mdev: &mut usize, aimd: &mut usize, previous: &mut usize, none: &mut usize) {
    match src {
        "live-api" => *live += 1,
        "models.dev" => *mdev += 1,
        "ai-model-directory" => *aimd += 1,
        "previous" => *previous += 1,
        _ => *none += 1,
    }
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
        "infron_provider" => "infron",
        "kimi_coding_provider" => "kimi-coding",
        "mistral_provider" => "mistral",
        "neuralwatt_provider" => "neuralwatt",
        "nvidia_provider" => "nvidia",
        "openai_provider" => "openai",
        "openai_codex_provider" => "openai-codex",
        "opencode_provider" => "opencode",
        "opencode_go_provider" => "opencode-go",
        "sumopod_provider" => "sumopod",
        "wafer_provider" => "wafer",
        "xai_provider" => "xai",
        _ => return None,
    })
}

fn parse_registered_provider_ids(builtin_src: &str) -> Result<std::collections::BTreeSet<String>> {
    use std::collections::BTreeSet;

    let start = builtin_src
        .find("pub fn builtin_providers()")
        .context("builtin_providers() not found in providers/builtin.rs")?;
    let after = &builtin_src[start..];
    let body_start = after.find("vec![").context("builtin_providers vec![ not found")?;
    let body = &after[body_start..];
    let end = body
        .find("\n    ]")
        .context("could not find end of builtin_providers vec")?;
    let body = &body[..end];

    let mut ids = BTreeSet::new();
    for cap in regex_lite_simple_provider_ids(body) {
        ids.insert(cap);
    }
    for name in body.split_whitespace() {
        let name = name.trim_end_matches([',', '(', ')']);
        if name.ends_with("_provider")
            && let Some(id) = named_factory_provider_id(name)
        {
            ids.insert(id.to_string());
        }
    }
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
