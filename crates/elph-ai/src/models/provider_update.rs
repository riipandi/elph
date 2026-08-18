//! Plan and apply model-catalog updates to a providers directory.
//!
//! A catalog lives on disk as `<dir>/<provider-id>.json` (the `CONFIG_DIR/providers`
//! overlay). The embedded binary carries the latest seed for every builtin provider.
//!
//! `plan_provider_update` compares the embedded seed against the on-disk file and
//! classifies each provider as `New`, `UpToDate`, or `Conflict`. `apply_provider_update`
//! writes the chosen policy:
//!
//! - `Merge` keeps the user's file and only *adds* models present in the seed but
//!   missing on disk. Existing entries (including user customizations) are never
//!   overwritten — this is the safe default that preserves custom configuration.
//! - `Overwrite` replaces the file with the seed (discards custom configuration).
//! - `SkipExisting` leaves existing files untouched (bootstrap behaviour).

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::embedded::{embedded_provider_ids, embedded_provider_json};

/// How a conflicting provider file should be reconciled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePolicy {
    /// Leave existing files untouched — only missing files are written.
    SkipExisting,
    /// Keep the user file; add seed models that are missing. Never clobbers.
    Merge,
    /// Replace the file with the embedded seed (discards custom configuration).
    Overwrite,
}

/// Per-provider reconciliation status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderUpdateStatus {
    /// No on-disk file yet; the seed will be written.
    New,
    /// On-disk file already matches the seed; nothing to do.
    UpToDate,
    /// On-disk file exists and differs from the seed; needs a decision.
    Conflict,
}

/// A single provider's update plan entry.
#[derive(Debug, Clone)]
pub struct ProviderUpdatePlanEntry {
    pub provider: String,
    pub status: ProviderUpdateStatus,
    /// Model ids present in the seed but absent from the disk file (would be added).
    pub added: Vec<String>,
    /// Model ids present in both but with differing values (user-customized).
    pub changed: Vec<String>,
    /// True when the existing file could not be parsed as JSON (left untouched on merge).
    pub unparsable: bool,
}

/// The full plan for a `plan_provider_update` call.
#[derive(Debug, Default, Clone)]
pub struct ProviderUpdatePlan {
    pub entries: Vec<ProviderUpdatePlanEntry>,
}

impl ProviderUpdatePlan {
    /// Providers whose on-disk file differs from the seed.
    pub fn conflicts(&self) -> Vec<&ProviderUpdatePlanEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == ProviderUpdateStatus::Conflict)
            .collect()
    }
}

/// Outcome counts for an applied update.
#[derive(Debug, Default, Clone)]
pub struct ProviderUpdateReport {
    /// Seed written for providers with no prior file.
    pub written: usize,
    /// User file kept, with seed-only models merged in.
    pub merged: usize,
    /// File replaced by the seed (custom configuration discarded).
    pub overwritten: usize,
    /// Existing file left untouched (skip policy / unparsable on merge).
    pub skipped: usize,
    /// File already matched the seed.
    pub up_to_date: usize,
}

/// Compare embedded seeds against on-disk catalog files for the given providers.
///
/// Providers without a builtin seed (custom/disk-only) are skipped.
pub fn plan_provider_update(dir: &Path, providers: &[String]) -> Result<ProviderUpdatePlan, String> {
    let mut entries = Vec::new();

    for provider in providers {
        // Only builtin providers carry an embedded seed to update from.
        if !embedded_provider_ids().contains(&provider.as_str()) {
            continue;
        }
        let seed = embedded_provider_json(provider).ok_or_else(|| format!("missing embedded seed for {provider}"))?;
        let seed: Value = serde_json::from_str(&seed).map_err(|e| format!("parse embedded seed {provider}: {e}"))?;

        let path = dir.join(format!("{provider}.json"));
        let (status, added, changed, unparsable) = match fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (ProviderUpdateStatus::New, Vec::new(), Vec::new(), false)
            }
            Err(e) => return Err(format!("read {}: {e}", path.display())),
            Ok(body) => match serde_json::from_str::<Value>(&body) {
                Ok(disk) => {
                    if disk == seed {
                        (ProviderUpdateStatus::UpToDate, Vec::new(), Vec::new(), false)
                    } else {
                        let (added, changed) = diff_models(&seed, &disk);
                        (ProviderUpdateStatus::Conflict, added, changed, false)
                    }
                }
                Err(_) => (ProviderUpdateStatus::Conflict, Vec::new(), Vec::new(), true),
            },
        };

        entries.push(ProviderUpdatePlanEntry {
            provider: provider.clone(),
            status,
            added,
            changed,
            unparsable,
        });
    }

    Ok(ProviderUpdatePlan { entries })
}

/// Write the catalogs according to `resolve(provider_entry)`.
///
/// `resolve` returns the policy for each planned entry; this lets callers apply a
/// single global policy (TUI / non-interactive CLI) or a per-provider choice
/// (interactive CLI).
pub fn apply_provider_update(
    dir: &Path,
    plan: &ProviderUpdatePlan,
    resolve: impl Fn(&ProviderUpdatePlanEntry) -> UpdatePolicy,
) -> Result<ProviderUpdateReport, String> {
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let mut report = ProviderUpdateReport::default();

    for entry in &plan.entries {
        let path = dir.join(format!("{}.json", entry.provider));
        let seed = embedded_provider_json(&entry.provider)
            .ok_or_else(|| format!("missing embedded seed for {}", entry.provider))?;
        let seed: Value = serde_json::from_str(&seed).map_err(|e| format!("parse seed {}: {e}", entry.provider))?;

        let policy = resolve(entry);
        match entry.status {
            ProviderUpdateStatus::UpToDate => {
                report.up_to_date += 1;
            }
            ProviderUpdateStatus::New => {
                write_pretty_json(&path, &seed)?;
                report.written += 1;
            }
            ProviderUpdateStatus::Conflict => match policy {
                UpdatePolicy::SkipExisting => {
                    report.skipped += 1;
                }
                UpdatePolicy::Overwrite => {
                    write_pretty_json(&path, &seed)?;
                    report.overwritten += 1;
                }
                UpdatePolicy::Merge => {
                    if entry.unparsable {
                        // Never clobber content we cannot parse; leave it and note it.
                        log::warn!("provider catalog update skipped unparsable file provider={}", entry.provider);
                        report.skipped += 1;
                    } else {
                        let body = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
                        let disk: Value =
                            serde_json::from_str(&body).map_err(|e| format!("parse {}: {e}", path.display()))?;
                        let merged = merge_keep_disk(&disk, &seed);
                        write_pretty_json(&path, &merged)?;
                        report.merged += 1;
                    }
                }
            },
        }
    }

    log::info!(
        "provider catalog update written={} merged={} overwritten={} skipped={} up_to_date={}",
        report.written,
        report.merged,
        report.overwritten,
        report.skipped,
        report.up_to_date
    );
    Ok(report)
}

/// Model ids added (in seed, not on disk) and changed (in both, differing).
fn diff_models(seed: &Value, disk: &Value) -> (Vec<String>, Vec<String>) {
    let (Some(seed_obj), Some(disk_obj)) = (seed.as_object(), disk.as_object()) else {
        return (Vec::new(), Vec::new());
    };
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for (id, sval) in seed_obj {
        match disk_obj.get(id) {
            None => added.push(id.clone()),
            Some(dval) if dval != sval => changed.push(id.clone()),
            Some(_) => {}
        }
    }
    added.sort();
    changed.sort();
    (added, changed)
}

/// Merge: start from `disk`, then add any seed model id missing from disk.
/// Existing disk entries (including user customizations) are preserved.
fn merge_keep_disk(disk: &Value, seed: &Value) -> Value {
    let mut merged = disk.clone();
    if let (Some(m), Some(s)) = (merged.as_object_mut(), seed.as_object()) {
        for (id, sval) in s {
            m.entry(id.clone()).or_insert(sval.clone());
        }
    }
    merged
}

fn write_pretty_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut body = serde_json::to_string_pretty(value).map_err(|e| format!("serialize {}: {e}", path.display()))?;
    body.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    fs::write(path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Uses the real embedded `anthropic` seed against explicit temp dirs; the plan
    // and apply helpers never touch the global on-disk catalog cache.
    #[test]
    fn plan_marks_new_up_to_date_and_conflict() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();

        let providers = vec!["anthropic".to_string()];
        // No file yet → New.
        let plan = plan_provider_update(dir, &providers).expect("plan");
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(plan.entries[0].status, ProviderUpdateStatus::New);

        // Apply (merge) → writes seed.
        let report = apply_provider_update(dir, &plan, |_| UpdatePolicy::Merge).expect("apply");
        assert_eq!(report.written, 1);

        // Now up to date.
        let plan = plan_provider_update(dir, &providers).expect("plan");
        assert_eq!(plan.entries[0].status, ProviderUpdateStatus::UpToDate);

        // Edit the file: drop one model and customize another → Conflict.
        let path = dir.join("anthropic.json");
        let mut value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        if let Some(obj) = value.as_object_mut() {
            obj.remove("claude-haiku-4-5"); // becomes an "added" in the seed
            if let Some(entry) = obj.get_mut("claude-opus-4-5") {
                entry["name"] = Value::String("Custom Opus".into()); // becomes "changed"
            }
        }
        fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

        let plan = plan_provider_update(dir, &providers).expect("plan");
        let e = &plan.entries[0];
        assert_eq!(e.status, ProviderUpdateStatus::Conflict);
        assert!(e.added.contains(&"claude-haiku-4-5".to_string()));
        assert!(e.changed.contains(&"claude-opus-4-5".to_string()));

        // Merge keeps the customization and re-adds the dropped model.
        let report = apply_provider_update(dir, &plan, |_| UpdatePolicy::Merge).expect("apply");
        assert_eq!(report.merged, 1);
        let restored: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(restored["claude-opus-4-5"]["name"], "Custom Opus");
        assert!(restored.get("claude-haiku-4-5").is_some(), "dropped model re-added by merge");

        // Overwrite discards the customization.
        let report = apply_provider_update(dir, &plan, |_| UpdatePolicy::Overwrite).expect("apply");
        assert_eq!(report.overwritten, 1);
        let overwritten: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_ne!(overwritten["claude-opus-4-5"]["name"], "Custom Opus");
    }

    #[test]
    fn merge_keeps_disk_entries() {
        let disk: Value = serde_json::json!({"x": {"id":"x","name":"X"}});
        let seed: Value = serde_json::json!({"x": {"id":"x","name":"X-edited"}, "y": {"id":"y","name":"Y"}});
        let merged = merge_keep_disk(&disk, &seed);
        assert_eq!(merged["x"]["name"], "X", "disk entry preserved");
        assert_eq!(merged["y"]["name"], "Y", "seed-only model added");
    }

    #[test]
    fn custom_providers_without_seed_are_skipped() {
        let tmp = tempdir().expect("tempdir");
        let plan = plan_provider_update(tmp.path(), &["my-custom-gateway".to_string()]).expect("plan");
        assert!(plan.entries.is_empty(), "no builtin seed → skipped");
    }
}
