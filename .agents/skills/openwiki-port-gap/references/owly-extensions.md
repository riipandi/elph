# Owly extension scan hints

Starting checklist for **Phase 3 (Owly implementation delta)**. Always verify in
code; the list grows over time. These are **not** automatic OpenWiki port gaps.

For each item found, write **In OpenWiki / In Owly / Implications**.

---

## Stack (intentional)

- **elph-agent runtime** — `owly/src/agent/` instead of LangGraph
- **elph-ai providers** — multi-provider catalog via Elph (OpenCode default, etc.) vs OpenWiki’s provider set
- **elph-tui / shell** — `tui/`, `shell/` interactive host
- **Workspace coupling** — Owly lives in the Elph monorepo; shares crates and release train

## Product / code-mode (confirm vs upstream)

- **Checkpoint / session** — `checkpoint/`, `session/` (design may differ from better-sqlite3 LangGraph checkpointer)
- **Frontmatter + metadata** — `frontmatter.rs`, `metadata.rs`, update no-op / `.last-update.json`
- **Docs pipeline** — `docs.rs`, `prompts.rs`
- **Onboarding / credentials** — `onboarding.rs`, `credentials.rs`, `~/.owly/.env`
- **Ecosystem markers** — `ecosystem.rs` (AGENTS.md / CLAUDE.md openwiki-context blocks)
- **Ask-user / UI events** — `ask_user/`, `ui_events.rs`
- **Diagnostics / redaction** — `diagnostics.rs`, utils redaction patterns
- **Explicit config** — CLI flags + env; lib.rs claims no hidden state outside the working directory

## Often OpenWiki-only (common gaps to check)

- **Personal mode** — `openwiki personal`, `~/.openwiki/wiki`
- **Connectors / ingest** — git-repo, Notion, Gmail, X, web-search, Hacker News, Slack auth
- **ngrok / OAuth helper commands** — `openwiki auth`, `openwiki ngrok`
- **Scheduled LaunchAgents / CI examples** — GitHub Actions, GitLab, Bitbucket sample pipelines
- **LangSmith tracing** — `/langsmith-key`, tracing project
- **openai-chatgpt / ChatGPT OAuth provider** — subscription Codex path
- **Multiple concurrent connector instances** — `web-search-1`, schedules in `onboarding.json`

## How to confirm “missing in OpenWiki” vs “missing in Owly”

```bash
# From OpenWiki clone
ls
rg -n "personal|connector|ingest|auth |ngrok" --glob '!**/node_modules/**' | head
# From owly
ls owly/src
rg -n "personal|connector|ingest|OPENWIKI|elph_agent|elph_ai" owly/src | head
```

If OpenWiki later adds a Rust-friendly or simpler code-mode feature, reclassify from
`[Owly delta]` toward convergence notes under **Parity and nuance**.
