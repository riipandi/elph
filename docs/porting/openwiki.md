# Porting status: OpenWiki → owly

**Last audited:** 2026-07-12T10:30:00Z
**Upstream:** [langchain-ai/openwiki](https://github.com/langchain-ai/openwiki) **v0.1.1** @ `14f1281`
**Elph crate:** `owly/` **v0.0.6**
**Depends on:** `elph-agent`, `elph-ai`

---

## Purpose

Track how far **Owly** lags or leads mainstream **OpenWiki** on the Elph stack.

---

## At a glance

- Personal + code modes — **[Parity]** (`~/.owly/wiki` vs `./openwiki/`)
- Init / update / chat / print / stream — **[Parity]** (terminal-only; bare `owly` is a placeholder)
- Connectors: `git-repo`, `web-search`, `hackernews`, `x` (manual token) — **[Partial]**
- `auth configure`, `ingest`, `cron` — **[Parity]** (subset of upstream connectors)
- Docs snapshot + `.last-update.json` on change — **[Parity]**
- Code-mode update no-op from git evidence — **[Parity]**
- Refreshable AGENTS.md / CLAUDE.md — **[Parity]**
- Optional `.github/workflows/owly-update.yml` — **[Parity]**
- `--dry-run` / `--credentials` — **[Parity]**
- CI examples under `owly/examples/` — **[Parity]**
- Ink / Node interactive UI — **[Gap]** (Owly: terminal + dialoguer onboarding)
- `ngrok`, Slack/Gmail/Notion OAuth, `auth tools` — **[N/A]** (explicit rejection)
- ChatGPT OAuth subscription login — **[Gap P2]**
- LangSmith tracing setup — **[Gap P2]**

---

## Architecture delta

- **OpenWiki:** Node CLI, Ink UI, LangGraph SQLite checkpoints, personal + code + connectors.
- **Owly:** Rust crate on `elph-agent` / `elph-ai`, Turso checkpoint saver, terminal streaming, dialoguer onboarding. Personal wiki at `~/.owly/wiki`; code wiki at `openwiki/`.

---

## Timeline

### 2026-07-12T10:30:00Z — terminal product pass

- Personal mode default; `owly personal` / `owly code` positional modes
- Terminal streaming (default on chat), compact chat header, `fff_search` warn suppressed
- Trailing-flag recovery (`owly personal --init --dry-run`)
- E2E script: `owly/scripts/e2e_cli.sh`
- README + this page updated for personal + connector scope

### 2026-07-12T09:15:00Z — code-mode parity pass

- `code_mode.rs` — agent snippet refresh + optional `owly-update.yml`
- credentials diagnostics, `--dry-run`, NVIDIA NIM onboarding label
- examples CI YAML

### 2026-07-12 — porting doc + skill scaffold

Initial `/openwiki-port-gap` skill and this page.

---

## Remaining / watch

- Interactive REPL/TUI for bare `owly`
- ChatGPT OAuth / LangSmith if product needs them on elph-ai
- Re-run `/openwiki-port-gap` after OpenWiki releases

---

## Related

- [`owly/README.md`](../../owly/README.md)
- [`owly/scripts/e2e_cli.sh`](../../owly/scripts/e2e_cli.sh)
- [`owly/examples/`](../../owly/examples/)
- Skill: `.agents/skills/openwiki-port-gap/`