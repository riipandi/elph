use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::bail;
use anyhow::{Context, Result};
use serde_json::{Map, Value};

/// JSON Schema URL stamped onto generated `models/*.json` catalogs.
pub const PROVIDER_SCHEMA_URL: &str = "https://elph.space/provider-schema.json";

/// Insert `$schema` as the first key of a catalog object (idempotent).
pub fn with_provider_schema(json: Value) -> Value {
    let Value::Object(map) = json else {
        return json;
    };
    let mut out = Map::new();
    out.insert("$schema".into(), Value::String(PROVIDER_SCHEMA_URL.into()));
    for (key, value) in map {
        if key != "$schema" {
            out.insert(key, value);
        }
    }
    Value::Object(out)
}

/// Optional local pi clone for **image** catalog scripts only (not chat origin).
///
/// Chat catalogs use models.dev. Image generation may still read from a sibling
/// earendil-works/pi checkout when present.
pub const DEFAULT_CATALOG_DIR_FROM_WORKSPACE: &str = "../../earendil-works/pi/packages/ai";

/// Resolve optional pi packages/ai path for image tooling.
pub fn default_catalog_dir(crate_root: &Path) -> PathBuf {
    let workspace_root = crate_root
        .parent()
        .and_then(|crates| crates.parent())
        .unwrap_or(crate_root);
    let raw = workspace_root.join(DEFAULT_CATALOG_DIR_FROM_WORKSPACE);
    raw.canonicalize().unwrap_or(raw)
}

/// Run an npm script in a directory (image catalog helper).
pub fn run_catalog_npm_script(catalog_dir: &Path, script: &str) -> Result<()> {
    super::term::info(format!("Running catalog source {script} in {}…", catalog_dir.display()));

    if Command::new("npm")
        .args(["run", script, "--silent"])
        .current_dir(catalog_dir)
        .status()
        .with_context(|| format!("spawn npm run {script}"))?
        .success()
    {
        return Ok(());
    }

    let script_path = format!("scripts/{script}.ts");
    for (bin, args) in [
        ("npx", vec!["tsx", &script_path]),
        ("node", vec!["--experimental-strip-types", &script_path]),
        ("node", vec![&script_path]),
    ] {
        let status = Command::new(bin)
            .args(&args)
            .current_dir(catalog_dir)
            .status()
            .with_context(|| format!("spawn {bin} {}", args.join(" ")))?;
        if status.success() {
            return Ok(());
        }
    }

    bail!(
        "failed to run catalog source `{script}`; install deps with `npm install` in {}",
        catalog_dir.display()
    );
}

fn block_on<T>(fut: impl std::future::Future<Output = T>) -> T {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime for generate-models")
    })
    .block_on(fut)
}

/// GET `url` and return `(status, body text)`. Uses the crate's async reqwest client.
pub fn http_get_text(url: &str, timeout: Duration, bearer: Option<&str>) -> Result<(reqwest::StatusCode, String)> {
    block_on(async {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .context("build HTTP client")?;
        let mut req = client.get(url);
        if let Some(token) = bearer {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        let text = resp.text().await.context("read response body")?;
        Ok((status, text))
    })
}

/// GET `url` and parse JSON. Returns `None` on any network or parse failure.
pub fn http_get_json(url: &str, timeout: Duration, bearer: Option<&str>) -> Option<Value> {
    block_on(async {
        let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
        let mut req = client.get(url);
        if let Some(token) = bearer {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json().await.ok()
    })
}
