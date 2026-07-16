# Riset Topik 2026: Rust, Terminal UI, & AI Coding Agent Architecture

> Dikumpulkan pada: 2026-07-16
> Sumber: Web search & artikel daring

---

## Daftar Isi

1. [Rust Programming Language Best Practices 2026](#1-rust-programming-language-best-practices-2026)
2. [Modern Terminal UI Trends 2026](#2-modern-terminal-ui-trends-2026)
3. [AI Coding Agent Architecture 2026](#3-ai-coding-agent-architecture-2026)
4. [Kesimpulan & Keterkaitan](#4-kesimpulan--keterkaitan)

---

## 1. Rust Programming Language Best Practices 2026

### 1.1 Ringkasan Eksekutif

Rust di 2026 telah melalui transformasi besar. Dengan stabilisasi **Rust Edition 2024** (Rust 1.85, Feb 2025), standar penulisan Rust "production-grade" telah bergeser. Sekadar memuaskan borrow checker tidak lagi cukup — arsitektur dan desain sistem menjadi fokus utama.

### 1.2 Fitur-Fitur Kunci Rust 2024

| Fitur | Deskripsi | Dampak |
|-------|-----------|--------|
| **Async Closures** (`async \|\| {}`) | Closure asinkron yang stabil | API middleware & event-driven lebih bersih, tanpa `Box<dyn Future>` |
| **Precise Capturing** (`use<'a, T>`) | Eksplisit tentang lifetime yang di-capture di `impl Trait` | Menghilangkan "hidden lifetime" bugs di kode async kompleks |
| **`IntoFuture` trait** | Dukungan lebih luas | Ergonomi API yang lebih baik |

### 1.3 Best Practices Utama

#### A. Struktur Proyek: Module-First, Crate-Last

- **Mulai dengan module** (`mod.rs`) daripada langsung bikin crate terpisah
- Setiap batas crate adalah "dinding kompilasi" — ada biaya kompilasi
- Pisahkan ke crate hanya jika:
  1. **Butuh Procedural Macro** (harus crate sendiri)
  2. **Ingin enforce strict boundaries** (compiler-level encapsulation)
  3. **Compile-time parallelism** (dua crate besar bisa di-build paralel)

#### B. Error Handling: Standar Emas 2026

| Lapisan | Tools | Prinsip |
|---------|-------|---------|
| **Library** | `thiserror` | Error terstruktur, machine-readable, bisa di-match |
| **Application** | `anyhow` + `.context()` | Error chain yang naratif, mudah di-debug |

**Golden rule:** Library selalu pakai `thiserror`, aplikasi selalu pakai `anyhow`.

```rust
// Library — structured, matchable
#[derive(thiserror::Error, Debug)]
pub enum DataError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parser error at line {line}")]
    Parse { line: usize },
}

// Application — contextual chains
db.fetch_user(id).await
    .context("failed to fetch user for profile update")?;
```

#### C. Async Best Practices

- **Tokio** dominan (95% use cases), tapi penggunaannya lebih disiplin
- **Jangan blocking di async:** selalu pakai `tokio::task::spawn_blocking` untuk operasi berat
- **Cancellation safety:** `tokio::select!` bisa drop branch yang pending — pastikan method cancellation-safe
- **Prefer channels over shared state:** `tokio::sync::mpsc` daripada `Arc<Mutex<T>>`

#### D. Zero-Copy & Memory Layout

- **rkyv** (zero-copy deserialization) menggantikan bincode untuk high-throughput
- Mapping byte buffer langsung ke Rust struct tanpa alokasi
- **Data-Oriented Design:** Struct of Arrays (SoA) untuk hot paths — mengurangi cache misses
- `serde` masih populer, tapi `rkyv` untuk performa maksimal

#### E. Testing Modern

| Tools | Fungsi |
|-------|--------|
| **proptest** | Property-based testing — generate ratusan input random |
| **insta** | Snapshot testing untuk output kompleks (API, CLI) |
| **cargo-nextest** | Lebih cepat dari `cargo test`, isolasi per-process |
| **cargo-expand** | Lihat hasil ekspansi macro untuk debugging |

#### F. Tooling

- **cargo-nextest** → standar baru untuk test runner
- **cargo-expand** → debug macro-generated code
- **cargo fmt/clippy** → wajib di CI

### 1.4 Tren Senior Engineer 2026

Menurut artikel "What Senior Engineers Are Quietly Unlearning" (Medium, Jun 2026):

- **Over-enginerring di crate splitting** — terlalu banyak crate kecil bikin compile time membengkak
- **Premature optimization** — jangan optimasi sebelum diukur
- **"Sync in async" sin** — blocking di async runtime adalah anti-pattern paling mahal
- **Tidak semua kode butuh `unsafe`** — compiler Rust sudah sangat pintar

### 1.5 Sumber Daya

- [Modern Rust Best Practices in 2026: Beyond the Borrow Checker](https://onehorizon.ai/blog/modern-rust-best-practices-in-2026-beyond-the-borrow-checker)
- [Idiomatic Rust: Writing Safe and Performant Code](https://www.sabaoon.dev/blog/rust-best-practices)
- [Rust in 2026: What Senior Engineers Are Quietly Unlearning](https://medium.com/rustaceans/rust-in-2026-what-senior-engineers-are-quietly-unlearning-b11b736778a4)
- [Comprehensive Guide: Rust 2024 Edition, Tokio, Axum, etc.](https://www.youngju.dev/blog/culture/2026-05-16-modern-rust-2026-rust-1-85-edition-2024-tokio-axum-loco-leptos-bevy-tauri-deep-dive.en)

---

## 2. Modern Terminal UI Trends 2026

### 2.1 Ringkasan Eksekutif

Terminal UI (TUI) mengalami **renaisans** di 2026. Tiga kekuatan mendorongnya:
1. **AI coding agents memilih terminal** — Claude Code, Codex CLI, Gemini CLI, OpenCode semuanya berbasis terminal
2. **Modern tooling** — ripgrep, bat, eza, fd, yazi, lazygit, dll. menggantikan tool Unix lawas
3. **Terminal emulator premium** — Ghostty (GPU-accelerated, 500fps), Kitty, WezTerm, Rio (WebGPU)

### 2.2 Framework TUI Utama 2026

| Framework | Bahasa | Stars | Ideal Untuk |
|-----------|--------|-------|-------------|
| **Ratatui** | Rust | 18.7K ⭐ | Dashboard, monitor, data-heavy (Netflix, AWS, OpenAI pakai ini) |
| **Ink** | TypeScript/React | ~25K ⭐ | Conversational, agent-driven, chat UI (Claude Code pakai ini) |
| **Bubble Tea** | Go | 40K ⭐ | Elm-architecture, general purpose |
| **Textual** | Python | ~25K ⭐ | CSS-like styling, rapid prototyping |

### 2.3 Design Principles untuk TUI (dari Hyperbliss)

#### Tujuh Pola Layout

1. **Persistent Multi-Panel** — Semua terlihat simultan (lazygit, btop)
2. **Miller Columns** — 3 kolom (parent, current, preview) — yazi, ranger
3. **Drill-Down Stack** — Navigasi browser-like ke view spesifik (k9s)
4. **Widget Dashboard** — Widget independen dalam grid (btop, bottom)
5. **IDE Three-Panel** — Sidebar, konten, detail/output
6. **Overlay/Popup** — Muncul sekilas, hilang (atuin, fzf)
7. **Header + Scrollable List** — Paling klasik (htop, tig)

#### Tujuh Prinsip Desain

| Prinsip | Deskripsi |
|---------|-----------|
| **Spatial consistency** | Panel tidak pernah pindah — user navigasi pakai memori lokasi |
| **Keyboard-first, mouse-optional** | Semua fitur reachable tanpa mouse |
| **Progressive disclosure** | 3 tier: footer (3-5 keys), `?` overlay, dokumentasi lengkap |
| **Semantic color** | Warna = makna, bukan dekorasi. Usable at 16 colors, beautiful at true color |
| **Async everything** | Jangan pernah freeze UI. Progress indicators untuk semua background task |
| **Contextual intelligence** | UI adaptif terhadap konteks user saat ini |
| **Design in layers** | Mulai monochrome → 16 ANSI → true color. Setiap tier independen |

#### Vim Keybindings sebagai Lingua Franca

```
L0 (Universal): Arrow keys, Enter, Escape, q
L1 (Vim motions): j/k/h/l, /, ?, :
L2 (Actions): d=delete, s=stage, r=refresh
L3 (Power): Composed commands, macros, config
```

### 2.4 Color Architecture untuk TUI

**Three-Tier Model:**
1. **16 ANSI colors** — Foundation. Warna sesuai tema terminal user
2. **256 colors** — Extended palette. Hati-hati dengan clash dengan tema user
3. **True color (24-bit)** — Enhancement layer. Cek `$COLORTERM`

**Semantic Color Tokens:**
```
text.primary      → Main body text
text.muted        → Metadata, timestamps
text.emphasis     → Headers, focused items
bg.base/surface/overlay → 3 background layers
accent.primary    → Brand color, interactive elements
status.success/warning/error/info → Status colors
git.staged/modified/untracked → Git-specific
diff.added/removed → Diff-specific
```

### 2.5 Tools dan Projek TUI Kunci 2026

| Projek | Deskripsi |
|--------|-----------|
| **Opaline** | Token-based theme engine untuk Ratatui (20 builtin themes) |
| **SilkCircuit** | Terminal design language (Electric Purple, Neon Cyan, Coral, dll.) |
| **ChromaCat** | Terminal colorization dengan animated gradient & 40+ themes |
| **ghostty-automator** | Playwright untuk terminal — IPC layer untuk Ghostty |
| **Unifly** | Network dashboard real-time (Ratatui) |
| **Iris Studio** | AI-powered git workflow (Ratatui, 6 modes) |
| **Vigil** | AI-powered PR lifecycle manager |
| **q** | Minimal Claude Code CLI (Ink/TypeScript) |

### 2.6 AI + TUI Integration

ghostty-automator memungkinkan AI agent untuk:
- **Melihat** output terminal (bukan text scraped — structured cell data)
- **Berinteraksi** dengan TUI (keystrokes, mouse events)
- **Screenshot** terminal
- **Assert** content (Playwright-style)

Ini menutup loop: **design → build → run → see → fix → repeat**

### 2.7 Sumber Daya

- [The Terminal Renaissance: Designing Beautiful TUIs in the Age of AI](https://hyperbliss.tech/blog/2026.04.04_terminal-renaissance/)
- [The TUI Renaissance 2026 — Ratatui, Bubble Tea, Textual, Ink](https://www.youngju.dev/blog/culture/2026-05-14-tui-development-ratatui-bubbletea-ink-textual-terminal-ui-renaissance-deep-dive-2026.en)
- [Terminal Renaissance: Modern TUI Tools Reshaping Developer Workflows](https://1337skills.com/blog/2026-03-09-terminal-renaissance-modern-tui-tools-reshaping-developer-workflows/)
- [TUI Renaissance 2026: Why Terminal UIs Are Back](https://byteiota.com/tui-renaissance-2026-why-terminal-uis-are-back/)

---

## 3. AI Coding Agent Architecture 2026

### 3.1 Ringkasan Eksekutif

AI coding agents mengalami pergeseran fundamental dari **IDE plugins** ke **terminal-native agents**. Hampir setiap major AI lab (Anthropic, Google, OpenAI) mengeluarkan CLI agent mereka sendiri antara Feb 2025 - 2026. Arsitektur compound AI systems menjadi standar.

### 3.2 Pergeseran Paradigma: CLI Agent > IDE Plugin

**CLI Thesis** (Blake Crosley, 2026):
- CLI-first architecture **94% lebih murah token overhead** dibanding MCP
- Lebih cepat dan komposabel dengan Unix tool standard
- Agents butuh **composability dan scriptability**, bukan visual interfaces
- Planning-execution split bekerja karena CLI artifacts adalah text

### 3.3 Arsitektur Referensi: Dhi (Open-Source AI Coding IDE)

```
IDE Frontend (Monaco Editor)
    ↓  JSON-RPC / WebSocket
Orchestration Core (LangGraph · Tool Router · Context Assembler)
    ↓        ↓            ↓                ↓
FIM Engine  Chat Engine  Agent Engine  Execution Sandbox
    ↓        ↓            ↓                ↓
        Model Layer (Ollama · vLLM · StarCoder2 · DeepSeek)
                         ↓
        Repo Intelligence Layer (Tree-sitter · Chroma · LSP)
```

**Enam Layer:**

#### Layer 1 — Repo Understanding
- **Tree-sitter** → Parse 40+ language dalam <5ms per file
- **Semantic Chunks** → Split berdasarkan fungsi/class, bukan character count
- **Call Graph Layer** → LSP-based symbol reference graph (SQLite adjacency list)
- **Vector Store** → nomic-embed-text-v1.5 + Chroma/Qdrant

#### Layer 2 — Autocomplete (Fill-in-the-Middle)
- **FIM Prefix + Suffix + Middle** → Model generate yang di tengah
- **Model options:** StarCoder2-3B (laptop), DeepSeek-Coder-V2-Lite (24GB GPU)
- **Speculative Decoding:** Draft model kecil + verifier besar = 3-5× throughput
- **Target latency:** <150ms P50, <400ms P95

#### Layer 3 — Chat-in-Editor
- **Context Assembler** dengan slot yang dialokasikan:
  - System prompt: ~500 tokens
  - File aktif + selection: ~2000 tokens
  - LSP diagnostics: ~500 tokens
  - RAG chunks: ~4000 tokens
  - Conversation history: ~2000 tokens
- **UX critical:** Stream tokens ke chat real-time, buffer code blocks, apply setelah lengkap

#### Layer 4 — Multi-File Agent Editing
- **Plan-Act-Observe Loop:**
  ```
  Planner → Think → Act (Tools) → Observe → Think → ... → Done → Diff Preview
  ```
- **Tool Set:** read_file, write_file, search_codebase, run_command, list_directory, get_diagnostics, get_references, create_file, delete_file
- **Orchestration:** LangGraph — directed graph dengan checkpointing
- **Checkpointing:** Pause, serialize state, resume — untuk refactor panjang

#### Layer 5 — System Design & Reasoning
- **Repo Summary Builder:** 1-paragraph LLM summary per direktori
- **~8k token project map** untuk konteks keseluruhan kodebase
- **Reasoning model:** DeepSeek-R1 / QwQ-32B
- **Output:** Mermaid / PlantUML diagrams

#### Layer 6 — Safe Code Execution Loop
- **Docker container ephemeral:** read-only bind mount, no network, 512MB RAM, 30s timeout
- **Self-healing loop:** Write → Run tests → Fail? → Observe → Re-plan → Write
- **gVisor/Firecracker** untuk sandbox lebih ketat

### 3.4 Open-Source Stack Lengkap

| Capability | Component | License |
|------------|-----------|---------|
| Editor | Monaco Editor | MIT |
| Syntax parsing | Tree-sitter | MIT |
| Code intelligence | LSP servers | Per-language |
| Embeddings | nomic-embed-text-v1.5 | Apache 2.0 |
| Vector store | Chroma (dev) / Qdrant (prod) | Open-source |
| FIM autocomplete | StarCoder2-3B / Qwen2.5-Coder-7B | BigCode / Qwen |
| Chat model | Qwen2.5-Coder-32B-Instruct | Apache 2.0 |
| Reasoning model | QwQ-32B / DeepSeek-R1-32B | MIT |
| Model serving | Ollama (local) / vLLM (prod) | MIT / Apache 2.0 |
| Agent orchestration | LangGraph | MIT |
| Execution sandbox | Docker + seccomp / gVisor | Apache 2.0 |
| Backend API | FastAPI | MIT |
| Frontend | Next.js + Tailwind | MIT |

### 3.5 Microskill Architecture

**Paper:** [Microskill Architecture: A Modular Skill-Driven Framework for AI-Native Code Generation](https://arxiv.org/abs/2606.05720) (Jun 2026)

**Konsep:** Terinspirasi microservices, tapi untuk **knowledge encapsulation** bukan service decomposition.

**Cara kerja:**
1. Pengetahuan di-partisi menjadi **atomic skill capsules** yang sharply scoped
2. **Dynamic router** memilih hanya capsules yang relevan secara semantik
3. Context allocation diformulasikan sebagai **constrained optimization** dengan token budget

**Hasil:**
- Token consumption turun **>90%**
- **Hampir 2×** first-try compilation success rate
- **Eliminasi architectural violations** entirely
- Self-learning: agent bisa extract & register skill capsules baru

### 3.6 Tren Multi-Agent Systems

- **SEMAG:** Self-Evolutionary Multi-Agent Code Generation — adaptif terhadap task complexity
- **AgentConductor:** Topology evolution for multi-agent competition-level code
- **KAT-Coder-V2:** Specialize-then-Unify — 5 expert domains (SWE, WebCoding, Terminal, WebSearch, General)
- **CODESTRUCT:** Code agents over structured action spaces — codebase sebagai structured action space, bukan unstructured text
- **Sema Code:** Decoupling AI coding agents menjadi programmable, embeddable infrastructure

### 3.7 Key Challenges

| Challenge | Deskripsi | Solusi |
|-----------|-----------|--------|
| **Latency at low VRAM** | 32B model di 24GB GPU = 15-20 tok/s | Speculative decoding, quantization (GGUF Q4), cloud GPU |
| **Prompt cache invalidation** | Cache management tanpa inference provider | KV-cache management di vLLM |
| **Index freshness** | Sinkronisasi vector store dengan active edits | Debounced incremental re-indexing |
| **Security surface** | Agent bisa write file anywhere | Path allowlist, konfirmasi user untuk writes di luar cwd |
| **Context window management** | Inject full docs → model lose info, token cost spiral | Microskill Architecture, semantic chunking |

### 3.8 Sumber Daya

- [Building Effective AI Coding Agents for the Terminal](https://arxiv.org/pdf/2603.05344v3) — arXiv paper
- [Building an AI Coding IDE from Scratch: Full Open-Source Architecture](https://dev.to/sochaty/building-an-ai-coding-ide-from-scratch-a-full-open-source-architecture-2ap1)
- [Microskill Architecture: A Modular Skill-Driven Framework](https://arxiv.org/abs/2606.05720v1)
- [The CLI Thesis: Why Agent Architecture Beats IDE Plugins](https://blakecrosley.com/blog/cli-thesis)
- [AI Agent CLI Frameworks: Terminal-Native Agent Runtimes](https://zylos.ai/research/2026-02-21-ai-agent-cli-frameworks/)
- [Sema Code: Decoupling AI Coding Agents](https://arxiv.org/pdf/2604.11045)
- [CODESTRUCT: Code Agents over Structured Action Spaces](https://aclanthology.org/2026.acl-long.607.pdf)

---

## 4. Kesimpulan & Keterkaitan

### Tema Umum

1. **Terminal sebagai platform utama** — Baik untuk development tools (Rust) maupun AI agents
2. **Rust mendominasi infrastruktur** — Ratatui, tools replacement, dan AI agent infrastructure
3. **Compound AI systems** — Agent architecture bukan monolith, tapi orchestration dari specialized components
4. **Context management adalah kunci** — Baik untuk AI agents (token budget) maupun TUI (screen real estate)
5. **Design makin penting** — TUI punya design principles sendiri, Rust mementingkan arsitektur

### Relevansi untuk Projek Elph

Sebagai AI coding agent yang sedang dibangun di proyek Elph:
- **Arsitektur agent** di Elph bisa mengadopsi pola Microskill Architecture
- **TUI** menggunakan Ratatui (Rust) — sesuai dengan trend 2026
- **Best practices Rust** (edition 2024, async, error handling) langsung aplikatif
- **Ghostty-automator** pattern bisa diadopsi untuk testing TUI secara visual
- **Plan-Act-Observe loop** adalah pola yang sudah diadopsi oleh banyak agent termasuk potensi untuk Elph

---

*Dibuat oleh Elph — AI coding agent*
