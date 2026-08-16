# Plan: System Prompt Context Efficiency Revamp (elph)

**Scope:** `crates/elph-agent/src/prompt/`, `crates/elph-agent/src/agent/harness/`, `crates/coding-agent/src/agent/prompt/`, `crates/coding-agent/templates/agent/coding_base.txt`
**Goal:** Reduce per-turn system prompt token footprint without degrading structure, tool discipline, or memory/skill recall quality.
**Non-goal:** Do not touch model routing, agent modes' behavioral semantics, or MCP transport code. Prompt copy and gating logic only.

**Ground rule:** every phase must pass `cargo fmt && cargo check && cargo clippy && cargo test` (workspace-wide, or scoped to touched crates if the full run is too slow) before being marked done. Do not use `code_search` as a tool reference anywhere in prompt copy unless it's confirmed registered — see Phase 1.

---

## Phase 0 — Baseline measurement (do this first, don't skip)

Before changing anything, capture a baseline so improvements are provable, not assumed.

1. In a repo with a realistic `.agents/skills/` + `~/.agents/skills/` set (use the current dev machine's actual skill count — 18 skills observed), render `build_coding_system_prompt` for each of the 3 modes (`build`, `plan`/read-only, `brave`) with a typical `active_tool_names` set (native tools + memory tools, no MCP).
2. Record token count (use the same tokenizer/estimator already in `crates/elph-agent/src/compaction/estimation.rs` — reuse it, don't hand-roll a new one) for:
    - Full rendered system prompt
    - `<available_skills>` block alone
    - `<available_tools>` block alone
3. Save these numbers in `docs/archive/prompt-baseline-<date>.md`. Every phase below should re-run this and report delta.

Acceptance: baseline doc exists with concrete token numbers, not estimates.

---

## Phase 1 — Fix confirmed template bugs (low risk, do independent of everything else)

**File:** `crates/coding-agent/prompts/coding_base.txt`

1. `<execution>` step 3 currently hardcodes the literal string `` `code_search` / `grep` / targeted `read_file` `` regardless of whether codegraph tools are actually active this turn. This is inconsistent with the `<codegraph>` block above it, which correctly gates on `${% if codegraph.code_search %}`.
    - Fix: make step 3 conditional the same way — when `codegraph.code_search` is set, mention it; otherwise say `` `grep` / targeted `read_file` `` only. Reuse the existing `codegraph.code_search` template variable, don't introduce a new one.
    - Add/update the existing test in `crates/coding-agent/src/agent/prompt/builder.rs` (`coding_prompt_includes_codegraph_when_tools_present` and a sibling "...without codegraph tools" case) to assert the literal `code_search` string never appears when codegraph tools are absent from `active_tool_names`.
2. Grep the whole `templates/` tree and `elph-agent/src/prompt/` for any other hardcoded tool-name strings that should instead go through the `tools.*` / `codegraph.*` template context (the pattern used everywhere else in this file). Fix each the same way.

Acceptance: `cargo test -p coding-agent` green, no literal tool-name strings outside the `tools.*`/`codegraph.*` template variable pattern.

---

## Phase 2 — Skill relevance gating (highest-impact change)

**Files:** `crates/elph-agent/src/agent/harness/types/options.rs` (`Skill` struct), `crates/elph-agent/src/agent/harness/system_prompt.rs` (`format_skills_for_system_prompt`)

Current behavior: `format_skills_for_system_prompt` includes every skill where `disable_model_invocation == false`. No relevance filtering. With 18 registered skills (mix of global `~/.agents/skills/` and project `.agents/skills/`), every prompt carries full `name` + `description` for skills that have zero chance of matching the current working directory or session type (e.g. Go-specific skills in a Rust-only project).

The `Skill` struct already has two unused-for-this-purpose fields: `compatibility: Option<String>` and `metadata: Option<HashMap<String, Value>>`. Use these — do not add new required fields, that breaks every existing `SKILL.md` frontmatter parser and every already-authored skill file.

1. Decide the tagging mechanism first, don't guess — read how `compatibility` is currently parsed from `SKILL.md` frontmatter (search `crates/elph-agent/src/skills/` for the frontmatter parser) and confirm whether it's free text or structured. If free text: repurpose it, or use `metadata["scope"]` as a soft-typed field (`"project"` | `"global"` | comma-separated glob list) instead. Whichever needs the smaller parser change, use that.
2. Add a filter step in `format_skills_for_system_prompt` (or a new function called before it, e.g. `filter_skills_for_context(skills, cwd, active_tool_names)`) that:
    - Always includes skills with no scope metadata set (backward compatible — unset means "always show", matches today's behavior for every existing skill until authors opt in).
    - When scope metadata IS set, only includes the skill if it matches the current project/cwd or an explicit always-on flag.
3. **Do not silently drop skills** — if a skill is filtered out this turn, that's fine (that's the point), but make sure `list_available_tools`-equivalent discovery for skills (if one exists — check `crates/elph-agent/src/skills/` for a skill-listing tool) still surfaces the full set on demand. If no such on-demand discovery tool exists for skills today, flag this as a follow-up gap in the final report — don't silently ship a regression where niche skills become unreachable.
4. This is a behavior change, not just a token optimization — get explicit user sign-off on the scope-tagging convention (step 1) before implementing step 2, since it affects how every future `SKILL.md` author writes their frontmatter. Use `ask_user_question` if unclear, don't assume.

Acceptance: baseline re-run from Phase 0 shows measurable token reduction in `<available_skills>` for a project-scoped session; full skill set still available via at least one discovery path; existing skills with no scope metadata behave identically to before (regression test for this specifically).

---

## Phase 3 — `list_available_tools` selective filtering (enables real lazy-load, not currently possible)

**File:** `crates/elph-agent/src/tools/list_available_tools.rs`

Current implementation takes no parameters and returns the entire tool catalog snapshot in one call. This means it cannot currently be used as a true "load schema on demand for MCP tool X" mechanism — it's all-or-nothing, and for high-tool-count MCP servers (e.g. an MCP server exposing 15-20 sub-tools) that "on-demand" call is nearly as expensive as just having had the schema active.

1. Add an optional `name_prefix` or `server` string parameter to the tool's JSON schema (`parameters` field), so it can return e.g. only tools matching a substring/MCP-server-name filter.
2. Keep the no-argument case returning everything (backward compatible with the existing prompt line "`list_available_tools` only when you need details about an unfamiliar or dynamically added tool").
3. This phase only matters if Phase 4 (below) determines MCP tool schemas are in fact being kept in `active_tool_names`/the API `tools` param even when unused this session — verify that first before investing time here.

Acceptance: `list_available_tools` supports a filter arg with a passing unit test; no-arg behavior unchanged (existing tests still pass).

---

## Phase 4 — Verify MCP tool activation scope (investigation phase, may produce no code change)

This is the part that actually determines whether Phase 3 is worth doing. `active_tool_names` is passed into `build_coding_system_prompt` from the host caller — find that call site (search `crates/coding-agent/src` and the CLI/ACP entry points for where `tool_names` gets assembled before calling `build_coding_system_prompt`).

1. Confirm: are all configured MCP servers' tools (e.g. a browser-automation server with 15-20 sub-tools) included in `active_tool_names` for every turn of every session, or only when that MCP server is actually connected/relevant to the session?
2. If it's unconditional — this is the real cost center, not the system prompt template. Fixing it means either (a) lazy-registering MCP tool schemas only after the model calls `list_available_tools` and asks for them (requires Phase 3's filter param plus a harness-side re-registration step), or (b) exposing MCP server grouping so a session can be started with only the servers it needs. Scope this as a separate, larger follow-up plan — don't attempt the full lazy-MCP-registration mechanism in this pass, it's a bigger architectural change than "trim the system prompt text."
3. If it's already conditional/session-scoped — good, no action needed, note this in the final report and drop Phase 3 unless the user wants the filter param for other reasons (e.g. a future `/tools` slash command).

Acceptance: a written finding (in the final report, not necessarily new code) stating which case is true, with the call-site file/line as evidence.

---

## Phase 5 — Re-measure and report

1. Re-run Phase 0's baseline script against the same fixture (same skill count, same tool set) after Phases 1-3.
2. Produce a short before/after table: total prompt tokens, `<available_skills>` tokens, `<available_tools>` tokens, per mode.
3. Final report must state: what changed, what was measured (not estimated), what's deferred (Phase 4 finding + any follow-up MCP work), and confirm `cargo fmt/check/clippy/test` pass on the full touched scope.

---

## Explicit constraints for the executing agent

- Do not modify skill file content in `~/.agents/skills/*/SKILL.md` or `.agents/skills/*/SKILL.md` — those are user-authored. Only touch the harness/template Rust code and, if Phase 2's tagging convention is approved, add scope metadata to the _project's own_ skills under `.agents/skills/` as a worked example, not the global `~/.agents/skills/` ones.
- Do not remove the `disable_model_invocation` flag or its current semantics — additive change only.
- Do not change `PromptAssemblyMode::Full` behavior (used by non-coding domain prompts) — this plan is scoped to the coding-agent domain template only.
- If any phase requires a decision with user-facing behavior implications (Phase 2 step 1 and step 4 especially), stop and ask before implementing — don't guess a convention that every future skill author has to live with.
