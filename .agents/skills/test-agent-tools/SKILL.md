---
name: test-agent-tools
description: >-
    Probe, test, and document every tool the agent harness has access
    to (native tools, subagents, connected MCP servers). Use when the
    user wants to test, audit, or inventory the harness's tool/capability
    set, or asks "what tools do you have".
metadata:
    scope: global
---

# Test Agent Tools

## Purpose

Enumerate available tools/capabilities — including connected MCP servers — exercise each one (web search, subagents, MCP calls), and produce a final report with a status table + subagent summary, under a timestamped `demo/` run folder.

## Workflow

1. Create folder `demo/YYMMDD_hhmm` (current date/time).
2. List available tools/capabilities, including any connected MCP servers/tools.
3. Test each tool:
    - Non-subagent tools (e.g. web search): run directly, write findings flat inside `demo/YYMMDD_hhmm/`.
    - Subagent-based exploration: dispatch one subagent per topic/tool; each writes to its own `demo/YYMMDD_hhmm/<subagent_id_or_name>/findings.md`.
    - MCP tools: if any MCP servers are connected, invoke each connected tool at least once; record request/response/error.
4. Write `demo/YYMMDD_hhmm/REPORT.md`:
    - Table: `| Tool | Type (native/MCP/subagent) | Status (OK/FAIL/SKIPPED) | Notes |`
    - Summary section aggregating each subagent's report (what it tested, key findings).
5. Post the full content of `REPORT.md` directly in the chat reply (not just a file link) — including the tool status table and the subagent summary. If subagents produced findings, include their key results inline too, not just a reference to the file.
    - Files (`REPORT.md`, findings, etc.) stay in English regardless of chat language.
    - The chat-transcript presentation of the report (narration, summary, table labels) uses whatever language the user is currently using in the conversation — translate on the fly, don't just paste the English file verbatim if the chat language differs.

## Mandatory Rules

- Only create files/folders inside `demo/`.
- Never create files/folders at the root directory or outside `demo/`.
- One run = one folder `demo/YYMMDD_hhmm` (current date/time, no suffix/label).
- No subagent: write markdown files flat inside `demo/YYMMDD_hhmm/`.
- With subagent: each subagent writes into its own `demo/YYMMDD_hhmm/<subagent_id_or_name>/`.
- `REPORT.md` always lives flat at `demo/YYMMDD_hhmm/REPORT.md`, never inside a subagent folder.

## Mode Lock

- Never offer, suggest, or ask the user to switch agent mode (Build/Plan/Brave/Ask).
- Always operate in the currently active mode, regardless of task complexity/risk.
- If the task needs another mode's capability (e.g. execution needed but mode = Plan/Ask):
    - Do as much as the active mode allows.
    - Report the blocker factually (e.g. "needs write access, current mode is read-only").
    - Do NOT frame it as "want me to switch to mode X?" — mode switching is user-initiated only.
- This holds even if the user seems unsure or asks "should I switch mode?" — answer factually, but still don't proactively offer/trigger a switch.

## Examples

- Flat (non-subagent/MCP): `demo/260731_1420/mcp-github/findings.md` or `demo/260731_1420/findings.md`
- With subagent: `demo/260731_1420/web-search-agent/findings.md`
- Final report: `demo/260731_1420/REPORT.md`
