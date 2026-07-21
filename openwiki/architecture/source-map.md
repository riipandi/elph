---
type: Reference
title: Elph Source Map
description: Comprehensive crate-by-crate module map with file paths and responsibilities for the Elph workspace.
tags: [source-map, rust, crates, workspace]
resource: /
---

# Source Map

## `elph` (binary + library crate) — `/elph/`

The main application. `main.rs` parses args → `cli::run()`. Library crate exposes modules for integration tests.

```
elph/src/
├── main.rs               # Entry: clap parse → cli::run()
├── lib.rs                # Public modules
│
├── cli/                  # CLI subcommands (clap-based)
│   ├── mod.rs            # Cli struct + Commands enum (17+ subcommands)
│   ├── acp.rs            # Agent Client Protocol server
│   ├── codegraph.rs      # code-review-graph integration
│   ├── completions.rs    # Shell completion generation
│   ├── default.rs        # Default/interactive mode handler
│   ├── doctor.rs         # Configuration diagnostics
│   ├── export.rs         # Session export
│   ├── extensions.rs     # WASM extension management
│   ├── help.rs           # Help display
│   ├── import.rs         # Session import
│   ├── mcp.rs            # MCP server management (add/remove/list/doctor)
│   ├── memory.rs         # Agent memory inspection
│   ├── models.rs         # Model catalog inspection
│   ├── provider.rs       # Provider config management
│   ├── run.rs            # Non-interactive run
│   ├── server.rs         # ACP server listener
│   ├── session.rs        # Session management (list/delete/export)
│   ├── stats.rs          # Usage statistics
│   ├── tools.rs          # Tool listing and inspection
│   ├── update.rs         # Self-update
│   ├── version.rs        # Version info
│   └── worktree.rs       # Worktree management
│
├── agent/                # Coding agent product logic
│   ├── mod.rs            # Module declarations
│   ├── runtime.rs        # create_coding_session_with_events factory
│   ├── session/          # CodingAgentSession (harness → UI bridge)
│   │   ├── mod.rs
│   │   └── wiring.rs
│   ├── session_manager.rs
│   ├── slash_commands.rs
│   ├── goal_slash.rs
│   ├── tool_policy.rs    # Agent mode → tool approval policies
│   ├── run_mode.rs       # Non-interactive run orchestration
│   ├── mcp_bootstrap.rs  # MCP discovery during session start
│   ├── model_registry.rs # Model resolution from settings
│   ├── resource_loader.rs
│   ├── events.rs         # AgentUiEvent types
│   ├── overlays.rs       # Overlay handler for UI events
│   ├── diagnostics.rs    # Diagnostics tool
│   ├── skills_load.rs    # Skill loading
│   └── prompt/           # System prompt building
│
├── tui/                  # Interactive TUI application
│   ├── mod.rs            # run_tui() entry, TuiOptions
│   ├── shell.rs          # MainShell (iocraft-based)
│   ├── shell_submit.rs   # Submit handler
│   ├── startup.rs        # TUI bootstrap flow
│   ├── agent_bridge.rs   # Agent event → TUI event bridge
│   ├── activity.rs       # Activity indicator management
│   ├── transcript/       # Chat transcript rendering
│   │   ├── mod.rs
│   │   ├── types.rs
│   │   ├── panel.rs
│   │   ├── ephemeral.rs  # Ephemeral message display
│   │   ├── layout.rs
│   │   ├── card/         # Agent message cards
│   │   │   ├── mod.rs
│   │   │   ├── kinds.rs
│   │   │   ├── builder.rs
│   │   │   ├── chrome.rs
│   │   │   ├── frame.rs
│   │   │   ├── tool_format.rs
│   │   │   └── toggle_ctx.rs
│   │   └── markdown/     # Markdown rendering pipeline
│   ├── prompt/           # Input prompt chrome
│   │   ├── mod.rs
│   │   ├── chrome.rs
│   │   ├── editor.rs
│   │   └── footer.rs
│   ├── chrome/           # Header/status row chrome
│   │   ├── mod.rs
│   │   ├── header.rs
│   │   ├── status_row.rs
│   │   ├── stats.rs
│   │   └── fit.rs
│   ├── confetti/         # Confetti overlay
│   ├── model_selector.rs
│   ├── model_selector_bar.rs
│   ├── model_selector_shell.rs
│   ├── scoped_models.rs
│   ├── scoped_models_bar.rs
│   ├── scoped_models_shell.rs
│   ├── session_prefs.rs
│   ├── slash_handler.rs
│   ├── slash_palette.rs
│   ├── status_dialog.rs
│   ├── subagent_display.rs
│   ├── system_prompt_dialog.rs
│   ├── tool_approval.rs  # Tool approval dialogs
│   ├── tool_params.rs    # Tool parameter display
│   ├── theme.rs
│   ├── focus.rs
│   ├── labels.rs
│   ├── clipboard.rs
│   ├── file_picker.rs
│   ├── api_error_display.rs
│   ├── ask_user_tool_card.rs
│   ├── inline_dialog.rs
│   ├── user_question.rs
│   ├── user_question_bar.rs
│   ├── user_question_option_list.rs
│   └── model_option_list.rs
│
├── platform/             # Host environment
│   ├── mod.rs            # Paths, Settings, migrations, MCP relay, hooks
│   ├── paths.rs          # XDG path resolution
│   ├── settings.rs       # Layered settings (defaults → home → project)
│   ├── bootstrap.rs      # App bootstrap
│   ├── mcp.rs            # MCP server relay
│   ├── migrations.rs     # Platform datastore migrations
│   └── exit_message.rs   # Exit message display
│
├── memory/               # Agent memory
│   ├── mod.rs
│   ├── format.rs
│   └── store.rs
│
├── skills/               # Skill loading
├── prompt/               # Prompt structures
├── command/              # Command implementations
├── worktree/             # Worktree management
├── extensions/           # WASM extension host
└── types.rs              # AgentMode, ThinkingLevel types
```

## `elph-agent` — `/crates/elph-agent/`

Generic agent runtime library. The largest crate by feature surface.

```
crates/elph-agent/src/
├── lib.rs                    # Public API: AgentBuilder, BuiltinToolsBuilder, harness, etc.
├── builder.rs                # AgentBuilder (logging) + BuiltinToolsBuilder (tool catalog)
│
├── agent/                    # Core agent types
│   ├── mod.rs                # Agent struct, events, queue, run, state
│   └── harness/              # AgentHarness
│       ├── mod.rs            # Harness struct + core implementation
│       ├── helpers.rs        # Message builders, validation
│       ├── plan_mode.rs      # Collaboration plan mode
│       ├── prompt_ops.rs     # System prompt operations
│       ├── compaction_ops.rs # History compaction operations
│       ├── tree_nav.rs       # Branch/tree navigation
│       ├── hooks.rs          # Hook system
│       ├── system_prompt.rs  # System prompt builder
│       ├── generic_on.rs     # Event handler wiring
│       ├── types/            # Error, event, option types
│       ├── utils/            # Truncation, shell output
│       └── run_loop/         # Harness turn loop (split by concern)
│
├── runtime/                  # Agent turn loop execution
│   ├── mod.rs                # agent_loop, block_on/try_block_on
│   ├── loop_config.rs        # AgentLoopConfig, AgentContext, callbacks
│   ├── run_loop.rs           # Core turn iteration
│   ├── stream.rs             # Assistant response streaming
│   ├── event_stream.rs       # AgentEventStream + sink
│   ├── env.rs                # Local execution environment
│   ├── local_env/            # Filesystem + shell execution
│   ├── exec/                 # Tool execution pipeline
│   └── proxy.rs              # Browser stream proxy
│
├── tools/                    # Built-in tools
│   ├── mod.rs                # Tool catalog, feature gates
│   ├── types.rs              # AgentTool, AgentToolResult types
│   ├── shell_exec.rs         # Shell command execution
│   ├── read_file.rs, write.rs, grep.rs, ...
│   ├── web/                  # Web fetch + search tools
│   ├── mcp/                  # MCP client tools
│   │   ├── mod.rs
│   │   ├── client.rs         # MCP client connection
│   │   ├── config.rs         # MCP config schema
│   │   ├── sse.rs            # SSE transport
│   │   └── ...
│   └── collaboration/        # Collaboration tools
│
├── session/                  # Session management
│   ├── mod.rs
│   ├── types.rs
│   ├── storage.rs            # InMemorySessionStorage, SessionDirStorage, TursoSessionStorage
│   └── dir.rs
│
├── compaction/               # History compaction
│   ├── mod.rs
│   ├── estimation.rs         # Token estimation
│   ├── branch.rs             # Branch management
│   └── summarization.rs      # Branch summarization
│
├── goals/                    # Goal/todo system
│   ├── mod.rs
│   ├── types.rs
│   ├── runtime.rs
│   └── store.rs
│
├── messages/                 # Message types
│   ├── mod.rs
│   ├── types.rs
│   └── format.rs
│
├── prompt/                   # Prompt templates (MiniJinja)
├── skills/                   # Skill loading/formatting
├── collaboration/            # Collaboration protocols
├── plugins/                  # WASM extension host
├── datastore/                # Database specs
├── trace/                    # Distributed tracing
└── types/                    # Shared types
```

## `elph-ai` — `/crates/elph-ai/`

Provider-agnostic LLM API layer.

```
crates/elph-ai/src/
├── lib.rs                # Provider resolution, auth helpers, model lookup
├── trace.rs              # Tracing integration
├── session_resources.rs  # Session resource cleanup
│
├── api/                  # Provider API implementations
│   ├── mod.rs
│   ├── anthropic.rs      # Anthropic Messages API
│   ├── bedrock.rs        # AWS Bedrock (Converse API)
│   ├── google.rs         # Google Gemini/Vertex AI
│   ├── openai_compat.rs  # OpenAI-compatible APIs
│   ├── openai_responses  # OpenAI Responses API
│   ├── azure.rs          # Azure OpenAI
│   ├── copilot.rs        # GitHub Copilot
│   ├── mistral.rs        # Mistral AI
│   ├── cloudflare.rs     # Cloudflare Workers AI
│   ├── openrouter.rs     # OpenRouter
│   └── codex.rs          # OpenAI Codex (WebSocket transport)
│
├── auth/                 # Authentication
│   ├── mod.rs
│   ├── api_key.rs
│   ├── env.rs
│   ├── oauth.rs          # OAuth 2.1 + PKCE
│   └── store.rs          # Credential store
│
├── models/               # Model catalog
│   ├── mod.rs
│   └── builtin.json      # Built-in model definitions (JSON)
│
├── providers/            # Provider definitions
│   ├── mod.rs
│   ├── definitions.rs
│   └── faux.rs           # Mock provider for testing
│
├── images/               # Image generation
├── types/                # Core types
└── utils/                # Deferred tools, diagnostics, streaming, retry
```

## `elph-core` — `/crates/elph-core/`

Shared primitives and utilities used across the workspace.

```
crates/elph-core/src/
├── lib.rs                # Re-exports
├── fs.rs                 # File system helpers
│
├── floppy/               # Agent memory system (ported from memelord)
│   ├── mod.rs
│   ├── builder.rs        # FloppyBuilder
│   ├── embed.rs          # ONNX embedding
│   ├── migrations.rs     # DB schema migrations
│   ├── paths.rs          # Storage paths
│   ├── scoring.rs        # Welford scoring, EMA weight updates
│   ├── report.rs         # Memory reporting
│   ├── util.rs           # Utilities
│   ├── types/            # Memory, config, task, report types
│   ├── store/            # Turso DB operations (read/write/tasks/embed)
│   └── query/            # Memory query (search, memories, status, tasks, timeline)
│
├── logger/               # Logging configuration
│   ├── mod.rs
│   ├── crash.rs          # Crash handler
│   └── options.rs        # Log rotation, level options
│
├── trace/                # Distributed tracing
│   ├── mod.rs
│   ├── imp.rs            # fastrace implementation
│   ├── reporter.rs       # HTTP trace reporter
│   └── stub.rs           # No-op stub
│
├── scaffold/             # Project scaffolding
│   ├── mod.rs
│   ├── bundled.rs        # Bundled manifest
│   ├── trust.rs          # Trust store
│   └── version.rs        # Version file
│
└── utils/                # General utilities
    ├── mod.rs
    ├── git.rs            # Git integration (git2)
    ├── lines.rs          # Line counting/processing
    ├── project_key.rs    # Project key generation
    └── path/             # Path resolution
        ├── mod.rs
        ├── app_paths.rs  # Application path definitions
        └── resolver.rs   # Path resolver
```

## `elph-tui` — `/crates/elph-tui/`

Reusable terminal UI widgets built on `iocraft`.

```
crates/elph-tui/src/
├── lib.rs                # Public API
├── color.rs              # Color parsing (hex, CSS, CSV, named)
├── theme_config.rs       # Theme system (auto/dark/light palette tokens)
├── transcript_layout.rs  # Chat transcript layout
├── text_input_layout.rs  # Text input layout
├── input_prefix.rs       # Prompt prefix detection (> / $ #)
├── cli_progress.rs       # CLI progress spinners
├── loader.rs             # Loading animations
├── paste.rs              # Paste handler
├── types.rs              # Shared types
├── utils.rs              # Utilities
│
├── components/           # Reusable UI components
│   ├── mod.rs
│   ├── markdown/         # Markdown rendering
│   ├── textarea/         # Text area component
│   ├── dialog_shell/     # Dialog shell
│   ├── progress_indicator.rs
│   ├── status_indicator.rs
│   ├── select.rs
│   └── ...
│
└── slash_palette/        # Slash command palette
    ├── mod.rs
    ├── completer.rs      # Fuzzy completion
    └── floating.rs       # Floating palette widget
```

## `elph-exec` — `/crates/elph-exec/`

Shell and PTY execution.

```
crates/elph-exec/src/
├── lib.rs
├── shell.rs              # Shell execution
├── pty/                  # Unix PTY support (via rustix)
├── error.rs
├── output.rs
└── types.rs
```

## Additional crates (placeholder status)

| Crate          | Path                    | Status | Notes                                                          |
| -------------- | ----------------------- | ------ | -------------------------------------------------------------- |
| `elph-cron`    | `/crates/elph-cron/`    | Empty  | `src/lib.rs` has no implementation                             |
| `elph-sandbox` | `/crates/elph-sandbox/` | Empty  | `src/lib.rs` has no implementation                             |
| `elph-swarm`   | `/crates/elph-swarm/`   | Empty  | `src/lib.rs` has no implementation                             |
| `floppy`       | `/crates/floppy/`       | Empty  | Standalone crate; implementation is in `elph-core/src/floppy/` |
