//! Full workspace reload for `/reload` (providers, settings, resources).

use std::path::Path;

use anyhow::Result;

use super::model_registry::{ModelSelection, resolve_model};
use super::resource_loader::{format_resource_conflict_notice, format_resource_load_warnings};
use super::session::CodingAgentSession;

use crate::platform::{Paths, Settings};
use crate::utils::path::AppPaths;

/// Inputs for a workspace reload.
#[derive(Debug, Clone)]
pub struct WorkspaceReloadRequest<'a> {
    pub paths: &'a Paths,
    pub cwd: &'a Path,
}

/// Structured outcome of [`CodingAgentSession::reload_workspace`].
#[derive(Debug, Clone, Default)]
pub struct WorkspaceReloadReport {
    /// Short status lines (providers, settings, resources counts).
    pub summary: Vec<String>,
    /// Durable transcript notices (conflicts, load warnings).
    pub notices: Vec<String>,
    /// Catalog files seen under `providers/`.
    pub provider_catalog_files: usize,
    /// Disk-only providers registered with streaming adapters this apply.
    pub disk_providers_registered: usize,
    pub skill_count: usize,
    pub template_count: usize,
}

impl WorkspaceReloadReport {
    pub fn summary_text(&self) -> String {
        if self.summary.is_empty() {
            "Reload unavailable.".into()
        } else {
            self.summary.join("\n")
        }
    }

    pub fn push_summary(&mut self, line: impl Into<String>) {
        self.summary.push(line.into());
    }

    pub fn push_notice(&mut self, line: impl Into<String>) {
        let line = line.into();
        if !line.trim().is_empty() {
            self.notices.push(line);
        }
    }
}

impl CodingAgentSession {
    /// Reload providers (disk catalogs), settings/model runtime, and workspace resources.
    ///
    /// Does **not** reload WASM extensions — the host calls `ExtensionHost::reload` and
    /// merges those lines into the report.
    pub async fn reload_workspace(&self, request: WorkspaceReloadRequest<'_>) -> WorkspaceReloadReport {
        let mut report = WorkspaceReloadReport::default();
        let paths = request.paths;
        let cwd = request.cwd;

        // ── Providers ──────────────────────────────────────────────────────
        match crate::agent::install_providers_dir(&paths.providers_dir()) {
            Ok(n) => {
                report.provider_catalog_files = n;
                report.push_summary(format!("Providers reloaded ({n} catalog file(s))."));
            }
            Err(err) => report.push_summary(format!("Provider catalog reload failed: {err}")),
        }

        // ── Settings + model runtime (includes disk-only provider adapters) ─
        match Settings::load(paths) {
            Ok(settings) => {
                let auth_path = paths.auth_store_path();
                match resolve_model(&settings, None, None, Some(&auth_path)).await {
                    Ok((selection, overlay_stats)) => {
                        report.disk_providers_registered = overlay_stats.registered;
                        match self.apply_reloaded_selection(selection, &settings).await {
                            Ok(()) => {
                                if overlay_stats.registered > 0 {
                                    report.push_summary(format!(
                                        "Registered {} disk-only provider adapter(s) for streaming.",
                                        overlay_stats.registered
                                    ));
                                }
                                if overlay_stats.skipped > 0 {
                                    report.push_notice(format!(
                                        "Provider load warnings:\n  • {} disk provider(s) skipped (unsupported API kind)",
                                        overlay_stats.skipped
                                    ));
                                }
                                report.push_summary("Settings reloaded.");
                            }
                            Err(err) => report.push_summary(format!("Settings apply failed: {err}")),
                        }
                    }
                    Err(err) => report.push_summary(format!("Settings reload (model resolve) failed: {err}")),
                }
            }
            Err(err) => report.push_summary(format!("Settings reload failed: {err}")),
        }

        // ── Skills / templates / agent conflict scan ───────────────────────
        match self.reload_resources(paths, cwd).await {
            Ok(loaded) => {
                report.skill_count = loaded.skill_count();
                report.template_count = loaded.template_count();
                report.push_summary(format!(
                    "Resources reloaded ({} skill(s), {} template(s)).",
                    loaded.skill_count(),
                    loaded.template_count()
                ));
                if let Some(notice) = format_resource_conflict_notice(&loaded) {
                    report.push_notice(notice);
                }
                if let Some(warn) = format_resource_load_warnings(&loaded) {
                    report.push_notice(warn);
                }
            }
            Err(err) => report.push_summary(format!("Resource reload failed: {err}")),
        }

        if report.summary.is_empty() {
            report.push_summary("Reload unavailable.");
        }
        report
    }

    async fn apply_reloaded_selection(&self, selection: ModelSelection, settings: &Settings) -> Result<()> {
        // Keep live thinking / agent mode — reload only adapters, catalog, and stream options.
        // Prefer keeping the same provider/model id when present in the reloaded catalog.
        let live = self.selection.read().clone();
        let model_for_harness = if live.provider == selection.provider && live.model_id == selection.model_id {
            selection.model.clone()
        } else if let Some(m) = elph_ai::get_builtin_model(&live.provider, &live.model_id) {
            m
        } else {
            selection.model.clone()
        };
        self.harness()
            .set_model(model_for_harness.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.harness()
            .set_stream_options(elph_agent::AgentHarnessStreamOptions {
                timeout_ms: settings.provider_timeout_ms(),
                max_retries: Some(settings.max_retries),
                thinking_budgets: settings.models.thinking_budgets.clone(),
                ..elph_agent::AgentHarnessStreamOptions::default()
            })
            .await;

        let mut next = selection;
        if next.provider != live.provider || next.model_id != live.model_id {
            // Preserve live model identity; only swap Models Arc / catalog entry.
            next.provider = live.provider;
            next.model_id = live.model_id;
            next.model = model_for_harness;
            next.display_name = live.display_name;
        }
        self.replace_selection(next);
        Ok(())
    }
}
