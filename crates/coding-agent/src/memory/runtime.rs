//! Shared memory runtime: one store + async-safe task/turn state for tools and hooks.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::time::timeout;
use turso::Database;

use floppy::{
    Memory, MemoryCategory, MemoryRecord, MemoryStore, ReportCorrectionInput, ReportUserInput, SelfReportEntry,
    StartTaskResult, TaskEndInput,
};

use super::capture::{
    ExplorationScratch, MAX_PATHS_PER_TURN, build_discovery_entries, discovery_area_matches, format_change_entry,
    format_work_entry, is_mutation_tool, is_sensitive_path, now_unix, paths_from_tool_input, record_exploration,
    truncate_chars,
};
use super::pack::{CONTEXT_BUDGET_CHARS, pack_ranked_context};
use super::rank::{
    RankOptions, adaptive_threshold_adjustment, filter_sticky, is_continuation_prompt, merge_and_rank, now_secs,
};
// re-export for callers/tests that used runtime::PER_MEMORY_CHARS
pub use super::pack::PER_MEMORY_CHARS;
use super::store::open_store_with_session;
use crate::platform::{MemorySettings, Paths};

/// How long to wait for the database lock before giving up on startup context.
pub const MEMORY_STARTUP_LOCK_TIMEOUT: Duration = Duration::from_secs(8);

/// Timeout for mid-turn memory store operations (graceful skip on lock).
pub const RECALL_DB_TIMEOUT: Duration = Duration::from_secs(2);

/// Minimum user query length to trigger a memory search (skip greetings, noise).
/// Task-like short prompts still recall via [`crate::memory::rank::is_task_like_prompt`].
pub const MIN_QUERY_LENGTH: usize = 8;

/// Host-side automatic memory policy (from `settings.json` → `memory`).
#[derive(Debug, Clone)]
pub struct MemoryRuntimeOptions {
    pub enabled: bool,
    pub auto_recall: bool,
    pub auto_capture_work: bool,
    pub auto_capture_exploration: bool,
    pub top_k: u32,
    pub context_budget_chars: usize,
    pub min_query_length: usize,
}

impl Default for MemoryRuntimeOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_recall: true,
            auto_capture_work: true,
            auto_capture_exploration: true,
            top_k: 5,
            context_budget_chars: CONTEXT_BUDGET_CHARS,
            min_query_length: MIN_QUERY_LENGTH,
        }
    }
}

impl MemoryRuntimeOptions {
    pub fn from_settings(s: &MemorySettings) -> Self {
        Self {
            enabled: s.enabled,
            auto_recall: s.auto_recall,
            auto_capture_work: s.auto_capture_work,
            auto_capture_exploration: s.auto_capture_exploration,
            top_k: s.top_k.max(1),
            context_budget_chars: s.context_budget_chars.max(500) as usize,
            min_query_length: s.min_query_length.max(1) as usize,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TurnScratch {
    pub user_corrections: u32,
    pub injected_memory_ids: Vec<String>,
    /// (path, tool_name) for successful mutations this turn.
    pub paths_touched: Vec<(String, String)>,
    pub mutation_successes: u32,
    /// First line / snippet of the user prompt for work summary.
    pub prompt_snippet: String,
    /// Exploration counters (list_dir / reads) for project-map discovery.
    pub exploration: ExplorationScratch,
}

/// Session-scoped memory facade shared by tools and automatic hooks.
pub struct MemoryRuntime {
    paths: Paths,
    session_id: String,
    options: MemoryRuntimeOptions,
    store: tokio::sync::Mutex<Option<MemoryStore>>,
    active_task_id: Mutex<Option<String>>,
    turn: Mutex<TurnScratch>,
    /// Shared, already-open database handle. When present, the store connects
    /// from this handle instead of opening the store file itself.
    database: Option<Arc<Database>>,
}

impl MemoryRuntime {
    pub fn new(paths: Paths, session_id: impl Into<String>) -> Self {
        Self::with_options_and_db(paths, session_id, MemoryRuntimeOptions::default(), None)
    }

    pub fn with_options(paths: Paths, session_id: impl Into<String>, options: MemoryRuntimeOptions) -> Self {
        Self::with_options_and_db(paths, session_id, options, None)
    }

    pub fn with_options_and_db(
        paths: Paths,
        session_id: impl Into<String>,
        options: MemoryRuntimeOptions,
        database: Option<Arc<Database>>,
    ) -> Self {
        Self {
            paths,
            session_id: session_id.into(),
            options,
            store: tokio::sync::Mutex::new(None),
            active_task_id: Mutex::new(None),
            turn: Mutex::new(TurnScratch::default()),
            database,
        }
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn options(&self) -> &MemoryRuntimeOptions {
        &self.options
    }

    pub fn is_enabled(&self) -> bool {
        self.options.enabled
    }

    pub fn begin_turn(&self, prompt: &str) {
        let mut turn = self.turn.lock().unwrap();
        *turn = TurnScratch {
            prompt_snippet: truncate_chars(prompt.trim(), 200),
            ..TurnScratch::default()
        };
    }

    pub fn bump_user_correction(&self) {
        let mut turn = self.turn.lock().unwrap();
        turn.user_corrections = turn.user_corrections.saturating_add(1);
    }

    pub fn set_injected_ids(&self, ids: Vec<String>) {
        self.turn.lock().unwrap().injected_memory_ids = ids;
    }

    pub fn take_turn_scratch(&self) -> TurnScratch {
        std::mem::take(&mut *self.turn.lock().unwrap())
    }

    pub fn record_successful_mutation(&self, tool_name: &str, input: &serde_json::Value) {
        if !self.options.auto_capture_work {
            log::debug!("memory.auto_capture mutation skipped_reason=settings_disabled");
            return;
        }
        if !is_mutation_tool(tool_name) {
            return;
        }
        let mut turn = self.turn.lock().unwrap();
        turn.mutation_successes = turn.mutation_successes.saturating_add(1);
        for path in paths_from_tool_input(tool_name, input) {
            if turn.paths_touched.len() >= MAX_PATHS_PER_TURN {
                break;
            }
            // Keep sensitive paths out of the path list entirely (not even redacted spam).
            if is_sensitive_path(&path) {
                continue;
            }
            if turn.paths_touched.iter().any(|(p, t)| p == &path && t == tool_name) {
                continue;
            }
            turn.paths_touched.push((path, tool_name.to_string()));
        }
    }

    /// Record successful exploration tools for project-map discovery flush.
    pub fn record_successful_exploration(&self, tool_name: &str, input: &serde_json::Value) {
        if !self.options.auto_capture_exploration {
            log::debug!("memory.auto_capture exploration skipped_reason=settings_disabled");
            return;
        }
        let mut turn = self.turn.lock().unwrap();
        record_exploration(&mut turn.exploration, tool_name, input);
    }

    pub fn active_task_id(&self) -> Option<String> {
        self.active_task_id.lock().unwrap().clone()
    }

    /// Ensure the embed-capable store is open (shared by tools and hooks).
    /// Schema tables are already created by `platform::datastore::ensure()`.
    pub async fn ensure_store(&self) -> Result<()> {
        {
            let guard = self.store.lock().await;
            if guard.is_some() {
                return Ok(());
            }
        }

        let started = Instant::now();
        let result = open_store_with_session(&self.paths, true, &self.session_id, self.database.clone());
        match result {
            Ok(store) => {
                let init = store.init().await;
                match init {
                    Ok(()) => {
                        log::debug!(
                            "memory.store.init needs_embed=true elapsed_ms={} ok=true",
                            started.elapsed().as_millis()
                        );
                        let mut guard = self.store.lock().await;
                        *guard = Some(store);
                        Ok(())
                    }
                    Err(err) => {
                        log::warn!(
                            "memory.store.init needs_embed=true elapsed_ms={} err={err:#}",
                            started.elapsed().as_millis()
                        );
                        Err(err)
                    }
                }
            }
            Err(err) => {
                log::warn!(
                    "memory.store.init needs_embed=true elapsed_ms={} err={err:#}",
                    started.elapsed().as_millis()
                );
                Err(err)
            }
        }
    }

    /// End any leftover active task before starting a new one.
    pub async fn start_task_for_prompt(&self, prompt: &str) -> Result<StartTaskResult> {
        // Defensive: close a stuck previous task so we keep one task per turn.
        if self.active_task_id.lock().unwrap().is_some() {
            let _ = self
                .end_active_task(TaskEndInput {
                    tokens_used: 0,
                    tool_calls: 0,
                    errors: 0,
                    user_corrections: 0,
                    completed: false,
                    self_report: None,
                })
                .await;
        }

        self.ensure_store().await?;
        let result = timeout(RECALL_DB_TIMEOUT, async {
            let guard = self.store.lock().await;
            let store = guard.as_ref().context("memory store missing")?;
            store.start_task(prompt).await
        })
        .await
        .map_err(|_| anyhow::anyhow!("start_task timed out"))??;

        *self.active_task_id.lock().unwrap() = Some(result.task_id.clone());
        log::debug!(
            "memory.task.start task_id={} description_len={}",
            result.task_id,
            prompt.chars().count()
        );
        Ok(result)
    }

    pub async fn search_memories_only(&self, prompt: &str) -> Result<Vec<Memory>> {
        self.ensure_store().await?;
        timeout(RECALL_DB_TIMEOUT, async {
            let guard = self.store.lock().await;
            let store = guard.as_ref().context("memory store missing")?;
            store.search_memories(prompt).await
        })
        .await
        .map_err(|_| anyhow::anyhow!("search_memories timed out"))?
    }

    pub async fn end_active_task(&self, input: TaskEndInput) -> Result<Option<String>> {
        let task_id = self.active_task_id.lock().unwrap().take();
        let Some(task_id) = task_id else {
            return Ok(None);
        };

        log::debug!(
            "memory.task.end task_id={} completed={} tokens={} tool_calls={} errors={} user_corrections={}",
            task_id,
            input.completed,
            input.tokens_used,
            input.tool_calls,
            input.errors,
            input.user_corrections
        );

        self.ensure_store().await?;
        let end = timeout(RECALL_DB_TIMEOUT, async {
            let guard = self.store.lock().await;
            let store = guard.as_ref().context("memory store missing")?;
            store.end_task(&task_id, input).await
        })
        .await;

        match end {
            Ok(Ok(())) => Ok(Some(task_id)),
            Ok(Err(err)) => {
                log::warn!("memory.task.end failed task_id={task_id}: {err:#}");
                Err(err)
            }
            Err(_) => {
                log::warn!("memory.task.end timed out task_id={task_id}");
                Err(anyhow::anyhow!("end_task timed out"))
            }
        }
    }

    pub async fn report_user_input(&self, input: ReportUserInput) -> Result<String> {
        let kind = "user";
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        match store.report_user_input(input).await {
            Ok(id) => {
                log::debug!("memory.write kind={kind} id={id} ok=true");
                Ok(id)
            }
            Err(err) => {
                log::warn!("memory.write kind={kind} ok=false err={err:#}");
                Err(err)
            }
        }
    }

    pub async fn report_correction(&self, input: ReportCorrectionInput) -> Result<String> {
        let kind = "correction";
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        match store.report_correction(input).await {
            Ok(id) => {
                log::debug!("memory.write kind={kind} id={id} ok=true");
                Ok(id)
            }
            Err(err) => {
                log::warn!("memory.write kind={kind} ok=false err={err:#}");
                Err(err)
            }
        }
    }

    pub async fn insert_work_memory(&self, content: String) -> Result<String> {
        self.insert_category_memory(content, MemoryCategory::Work, "work").await
    }

    pub async fn insert_discovery_memory(&self, content: String) -> Result<String> {
        self.insert_category_memory(content, MemoryCategory::Discovery, "discovery")
            .await
    }

    /// Persist a terminal goal outcome as a work memory (goals bridge).
    pub async fn record_goal_outcome(&self, goal_id: &str, objective: &str, status: &str) -> Result<String> {
        if !self.options.enabled || !self.options.auto_capture_work {
            log::debug!("memory.auto_capture goal skipped_reason=settings_disabled");
            return Ok(String::new());
        }
        let outcome = match status {
            "complete" => "success",
            "blocked" => "blocked",
            other => other,
        };
        let obj = truncate_chars(objective.trim(), 160);
        let content = format!(
            "[work] Goal {status}: {obj}\nPaths: (goal)\nOutcome: {outcome}\nNote: goal_id={goal_id} auto-captured from update_goal"
        );
        self.insert_work_memory(content).await
    }

    async fn insert_category_memory(&self, content: String, category: MemoryCategory, kind: &str) -> Result<String> {
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        match store.insert_raw_memory(&content, category, 1.0).await {
            Ok(id) => {
                log::debug!("memory.write kind={kind} id={id} ok=true");
                Ok(id)
            }
            Err(err) => {
                log::warn!("memory.write kind={kind} ok=false err={err:#}");
                Err(err)
            }
        }
    }

    /// Flush coalesced change + work entries (max 2). Call while task is still active.
    pub async fn flush_turn_work(&self, scratch: &TurnScratch, completed: bool) {
        if !self.options.auto_capture_work {
            log::debug!("memory.auto_capture work flush skipped_reason=settings_disabled");
            return;
        }
        if scratch.paths_touched.is_empty() && scratch.mutation_successes == 0 {
            return;
        }

        let mut writes = 0u32;
        if !scratch.paths_touched.is_empty() && writes < 2 {
            let content = format_change_entry(&scratch.paths_touched);
            if let Err(err) = self.insert_work_memory(content).await {
                log::warn!("memory.auto_capture change failed: {err:#}");
            } else {
                writes += 1;
            }
        }

        if writes < 2 && (!scratch.paths_touched.is_empty() || scratch.mutation_successes > 0) {
            let outcome = if completed { "success" } else { "partial" };
            let content = format_work_entry(&scratch.prompt_snippet, &scratch.paths_touched, outcome);
            if let Err(err) = self.insert_work_memory(content).await {
                log::warn!("memory.auto_capture work failed: {err:#}");
            }
        }
    }

    /// Flush project-map discoveries when exploration thresholds are met (rate-limited).
    pub async fn flush_turn_discoveries(&self, scratch: &TurnScratch) {
        if !self.options.auto_capture_exploration {
            log::debug!("memory.auto_capture discovery flush skipped_reason=settings_disabled");
            return;
        }
        let entries = build_discovery_entries(
            &scratch.exploration.list_dir_roots,
            &scratch.exploration.read_prefixes,
            &scratch.exploration.basename_notes,
            now_unix(),
        );
        if entries.is_empty() {
            return;
        }

        // Dedupe against recent discovery memories.
        let recent = self
            .list_recent_memories(20, Some(MemoryCategory::Discovery))
            .await
            .unwrap_or_default();

        let mut written = 0u32;
        for (area, content) in entries {
            if written >= 2 {
                break;
            }
            if recent.iter().any(|m| discovery_area_matches(&m.content, &area)) {
                log::debug!("memory.auto_capture discovery skipped_reason=duplicate area={area}");
                continue;
            }
            match self.insert_discovery_memory(content).await {
                Ok(_) => written += 1,
                Err(err) => log::warn!("memory.auto_capture discovery failed: {err:#}"),
            }
        }
    }

    /// Multi-source recall + pack for a substantive user turn.
    pub async fn build_turn_context(&self, prompt: &str) -> Result<Option<TurnRecallResult>> {
        if !self.options.enabled || !self.options.auto_recall {
            log::debug!("memory.recall.start skipped_reason=settings_disabled");
            return Ok(None);
        }
        let (semantic, task_id) = match self.start_task_for_prompt(prompt).await {
            Ok(result) => (result.memories, Some(result.task_id)),
            Err(err) => {
                log::debug!("memory.task.start failed, fallback search: {err:#}");
                match self.search_memories_only(prompt).await {
                    Ok(m) => (m, None),
                    Err(e) => {
                        log::debug!("memory.recall.start skipped_reason=search_failed err={e:#}");
                        return Ok(None);
                    }
                }
            }
        };

        let total_count = self.get_stats_total_memories().await;
        let weights: Vec<f64> = semantic.iter().map(|m| m.weight).collect();
        let mut threshold = adaptive_recall_threshold(total_count, &weights);
        threshold = (threshold + adaptive_threshold_adjustment(prompt)).clamp(0.30, 0.85);

        let semantic_filtered: Vec<Memory> = semantic.iter().filter(|m| m.score >= threshold).cloned().collect();

        // Always pull recent work/discovery even if semantic was sparse.
        let recent_work = self
            .list_recent_memories(5, Some(MemoryCategory::Work))
            .await
            .unwrap_or_default();
        let recent_discovery = self
            .list_recent_memories(5, Some(MemoryCategory::Discovery))
            .await
            .unwrap_or_default();
        let mut recent = recent_work;
        recent.extend(recent_discovery);

        let sticky_raw = self.get_top_by_weight(5).await.unwrap_or_default();
        let sticky = filter_sticky(sticky_raw).into_iter().take(3).collect::<Vec<_>>();

        let top_semantic = semantic.iter().map(|m| m.score).fold(0.0_f64, f64::max);
        // If semantic match is weak on a large store, prefer recency/sticky over noise.
        // Pull more semantic hits when top_k is higher (active recall).
        let semantic_cap = self.options.top_k.clamp(5, 12) as usize;
        let semantic_for_merge = if top_semantic < 0.35 && total_count > 50 {
            semantic_filtered
                .into_iter()
                .filter(|m| m.score >= 0.35)
                .take(semantic_cap)
                .collect()
        } else {
            semantic_filtered.into_iter().take(semantic_cap).collect()
        };

        // Prefer sticky/recent a bit more so lessons surface even when semantic is weak.
        let mut opts = RankOptions::default().with_prompt(prompt);
        opts.alpha = 0.45;
        opts.beta = 0.30;
        opts.gamma = 0.25;
        let ranked = merge_and_rank(semantic_for_merge, recent, sticky, now_secs(), &opts);
        if ranked.is_empty() {
            log::debug!(
                "memory.recall.hits raw_count={} after_threshold=0 threshold={threshold:.2} task_id={}",
                semantic.len(),
                task_id.as_deref().unwrap_or("-")
            );
            return Ok(None);
        }

        let packed = pack_ranked_context(&ranked, self.options.context_budget_chars);
        if packed.text.is_empty() {
            return Ok(None);
        }

        log::debug!(
            "memory.recall.hits raw_count={} ranked={} threshold={threshold:.2} task_id={} continuation={}",
            semantic.len(),
            ranked.len(),
            task_id.as_deref().unwrap_or("-"),
            is_continuation_prompt(prompt)
        );
        log::debug!(
            "memory.recall.injected injected_count={} total_chars={} lessons={} work={} map={}",
            packed.injected_ids.len(),
            packed.text.chars().count(),
            packed.sections.lessons,
            packed.sections.recent_work,
            packed.sections.project_map
        );

        Ok(Some(TurnRecallResult {
            context: packed.text,
            injected_ids: packed.injected_ids,
            task_id,
        }))
    }

    pub async fn get_stats_total_memories(&self) -> u64 {
        match timeout(RECALL_DB_TIMEOUT, async {
            self.ensure_store().await?;
            let guard = self.store.lock().await;
            let store = guard.as_ref().context("memory store missing")?;
            store.get_stats().await
        })
        .await
        {
            Ok(Ok(s)) => s.total_memories as u64,
            _ => 0,
        }
    }

    pub async fn get_top_by_weight(&self, limit: u32) -> Result<Vec<Memory>> {
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        store.get_top_by_weight(limit).await
    }

    pub async fn list_recent_memories(
        &self,
        limit: u32,
        category: Option<MemoryCategory>,
    ) -> Result<Vec<MemoryRecord>> {
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        store.list_recent_memories(limit, category).await
    }

    pub async fn search_memories(&self, query: &str) -> Result<Vec<Memory>> {
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        store.search_memories(query).await
    }

    pub async fn start_task(&self, description: &str) -> Result<StartTaskResult> {
        self.ensure_store().await?;
        let result = {
            let guard = self.store.lock().await;
            let store = guard.as_ref().context("memory store missing")?;
            store.start_task(description).await?
        };
        *self.active_task_id.lock().unwrap() = Some(result.task_id.clone());
        log::debug!(
            "memory.task.start task_id={} description_len={} source=tool",
            result.task_id,
            description.chars().count()
        );
        Ok(result)
    }

    pub async fn end_task(&self, task_id: &str, input: TaskEndInput) -> Result<()> {
        self.ensure_store().await?;
        {
            let guard = self.store.lock().await;
            let store = guard.as_ref().context("memory store missing")?;
            store.end_task(task_id, input).await?;
        }
        let mut cur = self.active_task_id.lock().unwrap();
        if cur.as_deref() == Some(task_id) {
            *cur = None;
        }
        Ok(())
    }

    pub async fn report(&self, input: floppy::MemoryReportInput) -> Result<String> {
        let kind = match input.report_type {
            floppy::MemoryReportType::Correction => "correction",
            floppy::MemoryReportType::UserInput => "user",
            floppy::MemoryReportType::Insight => "insight",
        };
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        match store.report(input).await {
            Ok(id) => {
                log::debug!("memory.write kind={kind} id={id} ok=true");
                Ok(id)
            }
            Err(err) => {
                log::warn!("memory.write kind={kind} ok=false err={err:#}");
                Err(err)
            }
        }
    }

    pub async fn contradict_memory(&self, memory_id: &str, correction: Option<&str>) -> Result<(bool, Option<String>)> {
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        store.contradict_memory(memory_id, correction).await
    }

    pub async fn get_stats(&self) -> Result<floppy::MemoryStats> {
        self.ensure_store().await?;
        let guard = self.store.lock().await;
        let store = guard.as_ref().context("memory store missing")?;
        store.get_stats().await
    }

    /// Session-start note only — full recall is turn-based (avoids double injection).
    pub async fn build_bootstrap_context(&self) -> Result<String> {
        if !self.options.enabled {
            log::debug!("memory.recall.start phase=bootstrap skipped_reason=settings_disabled");
            return Ok(String::new());
        }
        // Best-effort warm the store so first turn is faster; ignore errors.
        let _ = timeout(MEMORY_STARTUP_LOCK_TIMEOUT, self.ensure_store()).await;
        if !self.options.auto_recall {
            log::debug!("memory.recall.start phase=bootstrap skipped_reason=auto_recall_off");
            return Ok(String::new());
        }
        log::debug!("memory.recall.start phase=bootstrap mode=turn_only_hint");
        Ok(
            "<memory_context>\nMemory auto-recall is active: relevant lessons, recent work, and \
             project map are injected each turn as a seed. Prefer those blocks over re-scanning \
             known areas. Mid-turn: `memory_search` / `memory_recent` on pivots; `memory_report` \
             as soon as a durable preference, insight, or correction appears.\n\
             </memory_context>"
                .to_string(),
        )
    }

    pub async fn session_end_maintenance(&self) {
        if !self.options.enabled {
            log::debug!("memory session-end: skipped_reason=settings_disabled");
            return;
        }
        if let Err(err) = self.ensure_store().await {
            log::warn!("memory session-end: failed to open store: {err:#}");
            return;
        }

        let guard = self.store.lock().await;
        let Some(store) = guard.as_ref() else {
            return;
        };

        match store.clear_zero_embeddings().await {
            Ok(n) if n > 0 => log::info!("memory session-end: cleared {n} invalid zero embeddings"),
            Ok(_) => {}
            Err(err) => log::warn!("memory session-end: clear_zero_embeddings failed: {err:#}"),
        }

        match store.embed_pending().await {
            Ok(n) if n > 0 => log::info!("memory session-end: embedded {n} pending memories"),
            Ok(_) => {}
            Err(err) => log::warn!("memory session-end: embed_pending failed: {err:#}"),
        }

        // Near-duplicate hygiene before decay (best-effort).
        match store.consolidate_similar(0.08, 10).await {
            Ok(result) if result.merged > 0 => {
                log::info!(
                    "memory session-end: consolidated {} pairs, deleted {}",
                    result.merged,
                    result.deleted
                );
            }
            Ok(_) => {}
            Err(err) => log::warn!("memory session-end: consolidate failed: {err:#}"),
        }

        match store.decay().await {
            Ok(result) => {
                if result.decayed > 0 || result.deleted > 0 {
                    log::info!(
                        "memory session-end: decay applied to {}, deleted {}",
                        result.decayed,
                        result.deleted,
                    );
                }
            }
            Err(err) => log::warn!("memory session-end: decay failed: {err:#}"),
        }

        if let Err(err) = store.close().await {
            log::warn!("memory session-end: close failed: {err:#}");
        }
    }
}

/// Result of multi-source turn recall.
pub struct TurnRecallResult {
    pub context: String,
    pub injected_ids: Vec<String>,
    pub task_id: Option<String>,
}

pub fn adaptive_recall_threshold(total_memories: u64, memory_weights: &[f64]) -> f64 {
    let base: f64 = match total_memories {
        0..=9 => 0.40,
        10..=49 => 0.55,
        50..=199 => 0.65,
        _ => 0.75,
    };
    let max_weight = memory_weights.iter().copied().fold(0.0_f64, f64::max);
    let weight_discount: f64 = if max_weight > 3.0 { 0.10 } else { 0.0 };
    (base - weight_discount).clamp(0.30, 0.85)
}

pub fn build_self_report(scratch: &TurnScratch, completed: bool) -> Option<Vec<SelfReportEntry>> {
    if scratch.injected_memory_ids.is_empty() {
        return None;
    }
    let score: u8 = if !completed {
        0
    } else if scratch.user_corrections > 0 {
        1
    } else {
        2
    };
    Some(
        scratch
            .injected_memory_ids
            .iter()
            .map(|id| SelfReportEntry {
                memory_id: id.clone(),
                score,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_threshold_cold_start() {
        assert!((adaptive_recall_threshold(3, &[]) - 0.40).abs() < f64::EPSILON);
        assert!(adaptive_recall_threshold(100, &[4.0]) < 0.65);
    }

    #[test]
    fn self_report_scores() {
        let mut scratch = TurnScratch {
            injected_memory_ids: vec!["m1".into()],
            user_corrections: 0,
            ..Default::default()
        };
        let r = build_self_report(&scratch, true).unwrap();
        assert_eq!(r[0].score, 2);
        scratch.user_corrections = 1;
        assert_eq!(build_self_report(&scratch, true).unwrap()[0].score, 1);
        assert_eq!(build_self_report(&scratch, false).unwrap()[0].score, 0);
    }

    #[test]
    fn options_from_settings_respect_kill_switch() {
        let s = crate::platform::MemorySettings {
            enabled: false,
            auto_recall: false,
            top_k: 0,
            context_budget_chars: 100,
            ..Default::default()
        };
        let opts = MemoryRuntimeOptions::from_settings(&s);
        assert!(!opts.enabled);
        assert!(!opts.auto_recall);
        assert_eq!(opts.top_k, 1); // clamped
        assert_eq!(opts.context_budget_chars, 500); // clamped floor
    }
}
