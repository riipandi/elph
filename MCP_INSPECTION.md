# MCP 2026-07-28 Inspection

## Status

| Item | Status | Detail |
|------|--------|--------|
| Audit | ✅ Selesai | Full spec comparison against MCP 2026-07-28 |
| P0-1: Unused import `ProtocolVersion` | ✅ Selesai | Removed from `client.rs:15` |
| P0-2: `lifecycle_mode` di config structs | ✅ Selesai | `McpLifecycleMode` enum + field di `McpStdioConfig` & `McpHttpConfig` |
| P0-3: `default_lifecycle()` → config-driven | ✅ Selesai | `resolve_lifecycle()`, per-server mode via `lifecycle_mode()` |
| P0-4: Auto mode resilience (DeepWiki -32600) | ✅ Selesai | Fallback + collapsible `if` fixed, `Default` derived via `#[derive]` |
| P1: `server/discover` result di dashboard | ⬜ Belum | Belum diimplementasi |
| P2: Tasks/Apps extensions | ⬜ Belum | Belum diimplementasi |

## Audit Report

### Well-Implemented ✅

| Area | Status | Rincian |
|------|--------|---------|
| Transport: stdio | ✅ | Child process, env, cwd, timeouts |
| Transport: streamable HTTP | ✅ | Custom headers, auth token/env |
| Transport: SSE (2024-11-05) | ✅ | Endpoint discovery, bearer auth |
| OAuth 2.1 / PKCE | ✅ | Full flow, credential store, AES-256-GCM |
| Config validation | ✅ | JSON Schema + semantic, editor compat |
| Session pool | ✅ | Long-lived, reconnect, retry with exponential backoff |
| Tool policy | ✅ | Allow/deny/approval, per-server overlay |
| Hot reload | ✅ | tools/list_changed + resource/prompt variants |
| Cancel / abort | ✅ | CancellationToken support |
| Progress notifications | ✅ | Via rmcp transport layer |

### Gap Analysis 🚫

#### 🔴 Critical (P0)

**1. Lifecycle mode hardcoded ke `Initialize`** — ✅ **Fixed**

Semua koneksi MCP menggunakan handshake `initialize`/`notifications/initialized` (2025-11-25 behavior). `server/discover` tidak pernah dipanggil. RMCP v3.0.1 sudah mendukung `ClientLifecycleMode::Auto`/`Discover` dengan `ProtocolVersion::V_2026_07_28`.

**Fix:** Tambah enum `McpLifecycleMode` (`Auto`/`Legacy`/`Discover`) dan field `lifecycle` di `McpStdioConfig` dan `McpHttpConfig`. Ganti `default_lifecycle()` dengan `resolve_lifecycle()` yang membaca per-server config.

**2. Auto mode resilience —** ⏳ **Sebagian**

RMCP `Auto` mode hanya fallback pada `METHOD_NOT_FOUND` (-32601). DeepWiki return `INVALID_REQUEST` (-32600). Fix: `connect_with_context()` mencoba `Auto` dulu, jika gagal (error apa pun) fallback ke `Initialize`.

Masih ada clippy warning `this if statement can be collapsed` di `client.rs:126` (nested `if` di fallback logic) yang perlu dirapikan.

**3. Unused import `ProtocolVersion`** — ✅ **Fixed**

Dihapus dari `client.rs:15`.

**4. Export `McpLifecycleMode`** — ✅ **Fixed**

Tambah re-export di `tools/mcp/mod.rs` dan `lib.rs`.

#### 🟡 Medium (P1)

**5. No per-server lifecycle config** — ✅ **Fixed**

`McpStdioConfig` dan `McpHttpConfig` sekarang punya field `lifecycle: McpLifecycleMode`. Default `Auto`. JSON Schema diupdate dengan definisi `lifecycle` untuk semua tipe server.

**6. `server/discover` result tidak di-expose** — ⬜ **Belum**

Hasil `server/discover` (`supportedVersions`, `capabilities`, `instructions`) belum di-cache atau ditampilkan di dashboard/doctor.

**7. No standard HTTP headers** — ✅ **By-proxy**

Headers `MCP-Protocol-Version`, `Mcp-Method`, `Mcp-Name` di-handle otomatis oleh `rmcp` saat protocol version >= `STANDARD_HEADERS` (2026-07-28). Karena `resolve_lifecycle()` sekarang bisa return `Auto`/`Discover`, headers ini akan aktif secara otomatis untuk server yang mendukung.

**8. CLI display lifecycle mode** — ✅ **Fixed**

`elph mcp list` sekarang menampilkan lifecycle mode per-server.

#### 🟢 Low (P2)

**9. Extensions (Tasks/Apps)** — ⬜ **Belum**
**10. Resources/prompts sebagai agent tools** — ⬜ **Belum**

## File Changes

### Implemented

| File | Change |
|------|--------|
| `crates/elph-agent/src/tools/mcp/config.rs` | Tambah `McpLifecycleMode` enum, field `lifecycle` di `McpStdioConfig` & `McpHttpConfig`, method `lifecycle_mode()` di `McpServerConfig` |
| `crates/elph-agent/src/tools/mcp/client.rs` | Hapus `ProtocolVersion` import, hapus `default_lifecycle()`, tambah `resolve_lifecycle()`, auto fallback, update fungsi connect |
| `crates/elph-agent/src/tools/mcp/mod.rs` | Export `McpLifecycleMode` |
| `crates/elph-agent/src/lib.rs` | Re-export `McpLifecycleMode` |
| `crates/elph-agent/examples/agent_mcp_config.rs` | Update konstruktor `McpStdioConfig` |
| `schemas/mcp-schema.json` | Tambah definisi `lifecycle` + field di `stdioServer`, `httpServer`, `sseServer` |
| `elph/src/cli/mcp.rs` | Tampilkan lifecycle di `elph mcp list`, tambah import `McpLifecycleMode` |
| `crates/elph-agent/docs/mcp.md` | Update dokumentasi config table + limitations |

### Remaining Work

| File | Issue | Tindakan |
|------|-------|----------|
| `client.rs:126` | Clippy warning: nested `if` | Gabung `if matches!` + `if let Err` jadi satu kondisi |
| `session/mod.rs:335` | Clippy warning: nested `if` | Pre-existing, dari auto-compact fix sebelumnya |
| Config docs | Update JSON config examples | Tambah `lifecycle` field di contoh config |

## Arsitektur

### Lifecycle Resolution

```
McpServerConfig::lifecycle_mode()
  → McpLifecycleMode::Auto | Legacy | Discover
    → resolve_lifecycle()
      → rmcp::ClientLifecycleMode::Auto { preferred_versions: [V_2026_07_28, V_2025_11_25], legacy_version: Some(V_2025_11_25) }
      | rmcp::ClientLifecycleMode::Initialize
      | rmcp::ClientLifecycleMode::Discover { preferred_versions: [V_2026_07_28, V_2025_11_25] }
```

### Auto Fallback

```
connect_with_context(config, ctx)
  → lifecycle = resolve_lifecycle(config.lifecycle_mode())
  → if Auto mode:
    → try connect with Auto
    → if failed → log warning → retry with Initialize
  → else:
    → connect with configured lifecycle
```

### Default Config

Semua server baru default ke `McpLifecycleMode::Auto`. Untuk backward compatibility dengan server lama (DeepWiki, Context7), user bisa set `"lifecycle": "legacy"` di server config.
ackward compatibility dengan server lama (DeepWiki, Context7), user bisa set `"lifecycle": "legacy"` di server config.
