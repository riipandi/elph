use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::common::{
    CATALOG_IMAGE_GENERATED, CATALOG_IMAGE_SCRIPT, catalog_const_name, find_matching_brace, run_catalog_npm_script,
    ts_object_to_json,
};

pub struct ImageOptions {
    pub catalog_dir: PathBuf,
    pub skip_scripts: bool,
    pub images_dir: PathBuf,
    pub models_rs: PathBuf,
    pub no_regenerate_catalog: bool,
}

pub fn generate_image(options: ImageOptions) -> Result<()> {
    if !options.catalog_dir.join(CATALOG_IMAGE_SCRIPT).is_file() {
        bail!(
            "catalog source package not found at {} (expected npm catalog scripts)",
            options.catalog_dir.display()
        );
    }

    if !options.skip_scripts {
        run_catalog_npm_script(&options.catalog_dir, "generate-image-models")?;
    }

    let generated = options.catalog_dir.join(CATALOG_IMAGE_GENERATED);
    if !generated.is_file() {
        bail!("missing {} — run catalog generate-image-models first", generated.display());
    }

    let catalog_source = fs::read_to_string(&generated).context("read generated image catalog")?;
    let providers = ts_image_models_to_providers(&catalog_source)?;

    fs::create_dir_all(&options.images_dir).context("create images output directory")?;

    let mut provider_ids: Vec<String> = providers.keys().cloned().collect();
    provider_ids.sort();

    for provider_id in &provider_ids {
        let json = providers.get(provider_id).unwrap();
        let count = json.as_object().map(|m| m.len()).unwrap_or(0);
        let out_path = options.images_dir.join(format!("{provider_id}.json"));
        let pretty = serde_json::to_string_pretty(json).context("serialize image catalog json")?;
        fs::write(&out_path, format!("{pretty}\n")).with_context(|| format!("write {}", out_path.display()))?;
        println!("Converted image provider {provider_id}: {count} models");
    }

    if options.no_regenerate_catalog {
        println!(
            "\nWrote {} image catalogs to {} (skipped models.rs regeneration)",
            provider_ids.len(),
            options.images_dir.display()
        );
    } else {
        let catalog_source = render_image_catalog_rs(&provider_ids);
        fs::write(&options.models_rs, catalog_source).context("write src/images/models.rs")?;
        println!(
            "\nWrote {} image catalogs to {} and regenerated {}",
            provider_ids.len(),
            options.images_dir.display(),
            options.models_rs.display()
        );
    }

    Ok(())
}

pub fn ts_image_models_to_providers(ts: &str) -> Result<BTreeMap<String, Value>> {
    let marker_pos = ts.find("IMAGE_MODELS").context("missing IMAGE_MODELS export")?;
    let after = &ts[marker_pos..];
    let eq = after.find('=').context("missing IMAGE_MODELS assignment")?;
    let brace_start = after[eq..].find('{').context("missing IMAGE_MODELS object")? + eq;
    let abs_start = marker_pos + brace_start;
    let brace_end = find_matching_brace(ts, abs_start).context("unterminated IMAGE_MODELS object")?;
    let outer_body = &ts[abs_start + 1..brace_end];

    let mut result = BTreeMap::new();
    let mut pos = 0usize;
    while pos < outer_body.len() {
        pos = skip_ws_commas(outer_body, pos);
        if pos >= outer_body.len() {
            break;
        }

        let (provider_id, next) = parse_ts_key(outer_body, pos)?;
        pos = next;
        pos = skip_ws(outer_body, pos);
        if outer_body.as_bytes().get(pos) != Some(&b':') {
            bail!("expected ':' after provider key");
        }
        pos += 1;
        pos = skip_ws(outer_body, pos);
        if outer_body.as_bytes().get(pos) != Some(&b'{') {
            bail!("expected '{{' for provider {provider_id}");
        }
        let inner_end = find_matching_brace(outer_body, pos).context("unterminated provider object")?;
        let inner = &outer_body[pos..=inner_end];
        let json =
            ts_object_to_json(inner).with_context(|| format!("convert image models for provider {provider_id}"))?;
        result.insert(provider_id, json);
        pos = inner_end + 1;
    }

    if result.is_empty() {
        bail!("no providers found in IMAGE_MODELS");
    }
    Ok(result)
}

fn skip_ws(text: &str, mut pos: usize) -> usize {
    while pos < text.len() && text.as_bytes()[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn skip_ws_commas(text: &str, mut pos: usize) -> usize {
    loop {
        pos = skip_ws(text, pos);
        if text.as_bytes().get(pos) == Some(&b',') {
            pos += 1;
        } else {
            break;
        }
    }
    pos
}

fn parse_ts_key(text: &str, pos: usize) -> Result<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(pos) == Some(&b'"') {
        let rest = &text[pos + 1..];
        let end = rest.find('"').context("unterminated quoted key")?;
        Ok((rest[..end].to_string(), pos + 1 + end + 1))
    } else {
        let start = pos;
        let mut end = pos;
        while end < text.len() {
            let b = bytes[end];
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                end += 1;
            } else {
                break;
            }
        }
        if end == start {
            bail!("expected provider key at byte {pos}");
        }
        Ok((text[start..end].to_string(), end))
    }
}

fn render_image_catalog_rs(provider_ids: &[String]) -> String {
    let mut out = String::from(
        "//! Embedded builtin image model catalogs (auto-generated by `generate-models image` — do not edit).\n\n\
         use std::collections::HashMap;\n\n\
         use std::sync::LazyLock;\n\
         use serde::Deserialize;\n\n\
         use crate::types::{ImagesModel, ModelCost};\n\n\
         #[derive(Debug, Deserialize)]\n\
         struct RawImageModel {\n\
             id: String,\n\
             name: String,\n\
             api: String,\n\
             provider: String,\n\
             #[serde(rename = \"baseUrl\")]\n\
             base_url: String,\n\
             input: Vec<String>,\n\
             output: Vec<String>,\n\
             cost: RawCost,\n\
         }\n\n\
         #[derive(Debug, Deserialize)]\n\
         struct RawCost {\n\
             input: f64,\n\
             output: f64,\n\
             #[serde(rename = \"cacheRead\")]\n\
             cache_read: f64,\n\
             #[serde(rename = \"cacheWrite\")]\n\
             cache_write: f64,\n\
         }\n\n\
         fn parse_image_models(json: &str) -> Vec<ImagesModel> {\n\
             let raw: HashMap<String, RawImageModel> =\n\
                 serde_json::from_str(json).expect(\"invalid embedded image model catalog\");\n\
             raw.into_values().map(convert_image_model).collect()\n\
         }\n\n\
         fn convert_image_model(raw: RawImageModel) -> ImagesModel {\n\
             ImagesModel {\n\
                 id: raw.id,\n\
                 name: raw.name,\n\
                 api: raw.api,\n\
                 provider: raw.provider,\n\
                 base_url: raw.base_url,\n\
                 input: raw.input,\n\
                 output: raw.output,\n\
                 cost: ModelCost {\n\
                     input: raw.cost.input,\n\
                     output: raw.cost.output,\n\
                     cache_read: raw.cost.cache_read,\n\
                     cache_write: raw.cost.cache_write,\n\
                 },\n\
                 headers: None,\n\
             }\n\
         }\n\n\
         macro_rules! define_image_catalog {\n\
             ($name:ident, $file:literal) => {\n\
                 pub static $name: LazyLock<Vec<ImagesModel>> = LazyLock::new(|| {\n\
                     parse_image_models(include_str!(concat!(\n\
                         env!(\"CARGO_MANIFEST_DIR\"),\n\
                         \"/models/images/\",\n\
                         $file\n\
                     )))\n\
                 });\n\
             };\n\
         }\n\n",
    );

    for provider_id in provider_ids {
        let const_name = catalog_const_name(provider_id);
        out.push_str(&format!("define_image_catalog!({const_name}, \"{provider_id}.json\");\n"));
    }

    out.push_str(
        "\npub fn all_builtin_image_models() -> HashMap<&'static str, &'static [ImagesModel]> {\n\
             HashMap::from([\n",
    );
    for provider_id in provider_ids {
        let const_name = catalog_const_name(provider_id);
        out.push_str(&format!("        (\"{provider_id}\", {const_name}.as_slice()),\n"));
    }
    out.push_str(
        "    ])\n}\n\n\
         pub fn get_builtin_image_models(provider: &str) -> Vec<ImagesModel> {\n\
             all_builtin_image_models()\n\
                 .get(provider)\n\
                 .map(|models| models.to_vec())\n\
                 .unwrap_or_default()\n\
         }\n\n\
         pub fn get_builtin_image_providers() -> Vec<&'static str> {\n\
             let mut providers: Vec<_> = all_builtin_image_models().keys().copied().collect();\n\
             providers.sort_unstable();\n\
             providers\n\
         }\n",
    );
    out
}
