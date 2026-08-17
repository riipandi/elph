# Files

- [Agent Loop — Turn Cycle](agent-loop.md) - The turn cycle of Elph's agent loop — from AgentHarness::prompt() through run_agent_loop() to tool execution and compaction
- [Auth — Credential Store, Models Store, and OAuth](auth.md) - Authentication and credential management in Elph — CredentialStore trait, InMemoryCredentialStore, ModelsStore, OAuth providers, resolve_provider_auth
- [Compaction — Context Window Management](compaction.md) - Context compaction in Elph — token estimation, cut-point selection, LLM summarization, and timestamp-gated estimates
- [Handover — Foreign Session Import](handover.md) - Foreign session handover in Elph — importing transcripts from Claude Code and Codex sessions with inert safety boundary
- [Multi-Process Worker Coordination](workers.md) - Multi-process worker coordination in Elph — session leases, file leases, mailbox, worker registry, and agent tools
