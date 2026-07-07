use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;

pub fn find_matching_brace(text: &str, open_index: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in text[open_index..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open_index + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Convert a TypeScript object literal (with unquoted keys and `satisfies` clauses) to JSON.
pub fn ts_object_to_json(body: &str) -> Result<Value> {
    let mut body = body.trim().to_string();
    if !body.starts_with('{') {
        bail!("expected object literal starting with '{{'");
    }

    let satisfies = Regex::new(r"\}\s*satisfies\s+\w+(?:<[^>]+>)?").context("compile satisfies regex")?;
    body = satisfies.replace_all(&body, "}").to_string();

    let unquoted_keys = Regex::new(r#"([\{,]\s*)([A-Za-z_][A-Za-z0-9_]*)(\s*:)"#).context("compile keys regex")?;
    body = unquoted_keys.replace_all(&body, r#"$1"$2"$3"#).to_string();

    let trailing_commas = Regex::new(r",(\s*[\}\]])").context("compile trailing comma regex")?;
    body = trailing_commas.replace_all(&body, "$1").to_string();

    serde_json::from_str(&body).context("parse converted object json")
}

/// Convert a pi-ai `*.models.ts` catalog export to JSON.
pub fn ts_catalog_to_json(ts: &str) -> Result<Value> {
    let start = ts.find('=').context("missing catalog assignment")?;
    let brace_start = ts[start..].find('{').context("missing catalog object")? + start;
    let brace_end = find_matching_brace(ts, brace_start).context("unterminated catalog object")?;
    ts_object_to_json(&ts[brace_start..=brace_end])
}

pub fn run_pi_npm_script(pi_dir: &Path, script: &str) -> Result<()> {
    println!("Running pi-ai {script} in {}...", pi_dir.display());

    if Command::new("npm")
        .args(["run", script, "--silent"])
        .current_dir(pi_dir)
        .status()
        .with_context(|| format!("spawn npm run {script}"))?
        .success()
    {
        return Ok(());
    }

    let ts_script = format!("scripts/{script}.ts");
    for (bin, args) in [
        ("npx", vec!["tsx", &ts_script]),
        ("node", vec!["--experimental-strip-types", &ts_script]),
        ("node", vec![&ts_script]),
    ] {
        let status = Command::new(bin)
            .args(&args)
            .current_dir(pi_dir)
            .status()
            .with_context(|| format!("spawn {bin} {}", args.join(" ")))?;
        if status.success() {
            return Ok(());
        }
    }

    bail!(
        "failed to run pi-ai `{script}`; install deps with `npm install` in {}",
        pi_dir.display()
    );
}

pub fn catalog_const_name(provider_id: &str) -> String {
    let mut out = String::new();
    for ch in provider_id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    format!("{out}_IMAGE_MODELS")
}
