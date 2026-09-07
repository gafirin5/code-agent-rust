<div align="center">

# 🤖 ctrl-cli

**Ultra-lightweight AI Coding Agent CLI — Built with Rust**

[![Rust](https://img.shields.io/badge/Built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Binary Size](https://img.shields.io/badge/Binary-~1.8%20MB-brightgreen)]()
[![OpenAI Compatible](https://img.shields.io/badge/API-OpenAI%20Compatible-412991?logo=openai)]()

*Read in: [🇮🇩 Bahasa Indonesia](#-bahasa-indonesia) · [🇬🇧 English](#-english) · [🇨🇳 中文](#-中文)*

</div>

---

## 🇮🇩 Bahasa Indonesia

### Apa itu ctrl-cli?

**ctrl-cli** adalah agen coding AI yang sangat ringan, dibangun dengan Rust murni. Kamu bisa langsung chat dengan AI dari terminal, ganti model, aktifkan skill spesialis, semuanya tanpa perlu browser!

Biner hanya **~1.8 MB** dengan penggunaan RAM yang minimal.

### ✨ Fitur Utama

| 🤖 Autonomous Agent | ReAct execution loop otonom untuk inspeksi & eksekusi kode |
| ⚡ Real-Time Streaming | Streaming respons SSE (`stream: true`) langsung ke terminal |
| 🩺 Self-Healing Code Loop | Feedback diagnosa compiler otomatis (`cargo check`, python, tsc) untuk perbaikan mandiri |
| 🔄 Git Checkpoint & Undo | Shadow snapshot file otomatis, inspect `/diff`, & rollback instan via `/undo` |
| 🧹 Context Compaction | Kompaksi & ringkasan otomatis riwayat sesi untuk cegah token blowout |
| 🌐 Web Search & Fetch | Built-in `web_fetch` (HTML to Markdown) & `web_search` (DuckDuckGo) |
| 🔌 MCP Protocol Support | Klien Model Context Protocol (`.ctrl/mcp.json`) untuk memuat tool eksternal |
| 👥 Subagent Delegation | Delegasi tugas/riset terisolasi ke background subagent via `subagent` |
| 🛠️ 14 Built-in Tools | Tools lengkap ala `fx` (`read`, `write`, `edit`, `glob`, `grep`, `shell`, `web`, `subagent`, dll.) |
| 🛡️ Permission Gate | Kebijakan keamanan interaktif (`Ask`, `AutoApprove`, `ReadOnly`) |
| 🖥️ Mode REPL | Chat interaktif langsung di terminal dengan autocompletion `/` |
| ⚡ Mode Generate | Eksekusi tugas & generate kode dari satu baris perintah |
| 📊 Token & Context | Pantau penggunaan token & kapasitas context window |
| 🎯 Skill Spesialis | 8 skill built-in + support muat `SKILL.md` lokal |
| 🔄 Ganti Model | Beralih antar model AI kapan saja di REPL |
| 🔌 Multi-Provider | Support OpenAI, DeepSeek, Groq, OpenRouter, dll. |
| 👤 User Profile | Personalisasi nama, tech stack, bahasa respons |
| 📦 Biner Kecil | ~2.0 MB, LTO optimized, siap jalan tanpa install |

### 🎯 Skill yang Tersedia

| Skill ID | Nama | Kegunaan |
|----------|------|----------|
| `rust-expert` | 🦀 Rust Expert | Kode Rust idiomatik & memory-safe |
| `code-reviewer` | 🔍 Code Reviewer | Audit bug, keamanan & code smell |
| `web-frontend` | 🎨 Web Frontend UI/UX | HTML/CSS/Tailwind modern & responsif |
| `api-architect` | 🏗️ API & Backend Architect | Desain REST/GraphQL, database, auth |
| `security-auditor` | 🛡️ Security Auditor | OWASP Top 10, mitigasi exploit |
| `debugger` | 🐞 Debugger & Trace Doctor | Analisis error & stack trace |
| `test-engineer` | 🧪 Test Engineer & TDD | Unit test, integration test, mock |
| `refactor` | 🧹 Clean Code & Refactoring | SOLID, DRY, arsitektur modular |

### 🚀 Cara Memulai

#### 1. Prasyarat

- [Rust](https://rustup.rs/) (edisi 2021+)
- API key dari salah satu provider AI berikut:
  - [OpenAI](https://platform.openai.com/)
  - [DeepSeek](https://platform.deepseek.com/)
  - [Groq](https://console.groq.com/)
  - [OpenRouter](https://openrouter.ai/)

#### 2. Clone & Setup

```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust

# Salin file konfigurasi
copy .env.example .env
```

#### 3. Isi File `.env`

Buka file `.env` dan isi konfigurasi:

```env
# API Key dari provider pilihanmu (wajib diisi)
AI_API_KEY=sk-xxxxxxxxxxxxxxxx

# Base URL provider (opsional, default: OpenAI)
AI_BASE_URL=https://api.openai.com/v1

# Nama model (opsional, default: gpt-4o-mini)
AI_MODEL=gpt-4o-mini
```

**Contoh provider lain:**

```env
# DeepSeek
AI_BASE_URL=https://api.deepseek.com/v1
AI_MODEL=deepseek-chat

# Groq (gratis & cepat)
AI_BASE_URL=https://api.groq.com/openai/v1
AI_MODEL=llama-3.3-70b-versatile

# OpenRouter
AI_BASE_URL=https://openrouter.ai/api/v1
AI_MODEL=anthropic/claude-3.5-sonnet
```

#### 4. Build & Jalankan

```bash
# Build release (sekali saja)
cargo build --release

# Jalankan REPL interaktif
./target/release/ctrl-cli

# Atau langsung generate kode
./target/release/ctrl-cli generate "buat fungsi quicksort di Rust"

# Generate kode dengan metrik token & context window
./target/release/ctrl-cli generate -t "buat fungsi quicksort di Rust"
```

### 💬 Cara Pakai REPL

Setelah dijalankan, kamu akan masuk ke mode REPL:

```
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent)
 Active Model: gpt-4o-mini
 Type your prompt and press Enter.
 Ketik `/` untuk rekomendasi interaktif (geser panah ↑ / ↓).
 Commands: /tokens, /model, /skill, /dev, /profile, /info, /clear, /help, /exit
══════════════════════════════════════════════════════════════

[gpt-4o-mini] ➜ 
```

Cukup ketik pertanyaan atau permintaan kode, lalu tekan Enter! Di bawah setiap respon, badge token akan muncul otomatis (misal: `📊 [Tokens: 120 in + 350 out = 470 total | Context: 0.37% of 128k]`).

### ⌨️ Slash Commands

| Perintah | Fungsi |
|----------|--------|
| `/` | Buka menu interaktif (pilih perintah dengan panah ↑↓) |
| `/tools` | Lihat daftar 14 built-in agent tools, status eksekusi & MCP bridge |
| `/undo` | Batalkan (*rollback*) modifikasi file terakhir dari shadow checkpoint |
| `/diff [file]` | Tampilkan unified diff perubahan berkas terkini atau git diff |
| `/check [file]` | Jalankan pemeriksaan compiler / linter (*self-healing loop*) |
| `/compact` | Ringkas (*compact*) riwayat percakapan lama untuk hemat context window |
| `/mcp` | Lihat status server & tool Model Context Protocol (`.ctrl/mcp.json`) |
| `/stream` | Toggle output streaming real-time SSE (`on` / `off`) |
| `/checkpoints` | Tampilkan riwayat snapshot berkas yang tersimpan |
| `/permissions` | Atur kebijakan izin tool (`Ask`, `AutoApprove`, `ReadOnly`) |
| `/memory` | Lihat catatan memori jangka panjang proyek (`.ctrl/MEMORY.md`) |
| `/reset` | Kosongkan riwayat percakapan & memori sesi (mulai konteks baru) |
| `/save [file]` | Simpan kode respon terakhir langsung ke file (auto-detect nama file) |
| `/tokens` | Cek statistik token (in/out/total) & batas context window |
| `/tokens toggle` | Aktifkan/nonaktifkan badge token otomatis setelah respon |
| `/model` | Pilih model AI dari daftar interaktif |
| `/model <nama>` | Ganti model langsung (contoh: `/model deepseek-chat`) |
| `/skill` | Pilih skill spesialis dari daftar |
| `/skill <id>` | Aktifkan skill tertentu (contoh: `/skill rust-expert`) |
| `/skill reset` | Nonaktifkan skill, kembali ke General Assistant |
| `/profile` | Lihat profil developer aktif |
| `/dev` | Informasi pembuat & pengembang aplikasi |
| `/info` | Cek endpoint, model aktif, context window & total token sesi |
| `/clear` | Bersihkan layar terminal |
| `/help` | Tampilkan bantuan |
| `/exit` | Keluar dari REPL |

### 🛠️ Built-in Agent Tools & Keamanan

AI Agent di `ctrl-cli` dapat menginspeksi, menjelajah, dan memodifikasi proyek secara mandiri melalui 14 perkakas bawaan + MCP:

- **`read_file`**: Membaca file dengan dukungan penomoran baris dan offset.
- **`write_file`**: Menulis file baru atau menimpa file yang sudah ada (dilengkapi auto checkpoint & self-heal).
- **`edit_file`**: Modifikasi kode secara presisi dan bedah (*surgical replacement* dengan auto checkpoint & self-heal).
- **`code_check`**: Menjalankan pengecekan compiler atau sintaks (`cargo check`, `py_compile`, `tsc`).
- **`web_fetch`**: Mengambil konten web dari URL HTTP(S) dan mengubah HTML menjadi Markdown bersih.
- **`web_search`**: Mencari solusi pemrograman dan dokumentasi via mesin pencari DuckDuckGo.
- **`subagent`**: Mendelegasikan tugas atau riset terisolasi ke agen anak (*subagent*) tanpa membebani sesi utama.
- **`glob_files`**: Menemukan pola file dalam direktori proyek (contoh: `**/*.rs`).
- **`grep_files`**: Mencari kata kunci/teks di seluruh file dalam workspace.
- **`shell`**: Menjalankan perintah terminal/shell secara aman.
- **`read_tool_result`**: Membaca output tool yang terpotong jika terlalu panjang.
- **`ask_user_question`**: Bertanya dan meminta konfirmasi interaktif ke pengguna.
- **`skill`**: Memuat instruksi khusus dari berkas `SKILL.md` lokal.
- **`manage_memory`**: Membaca atau menambahkan memori kerja jangka panjang ke `.ctrl/MEMORY.md`.
- **`mcp__<server>__<tool>`**: Tool dinamis eksternal yang dimuat otomatis dari Model Context Protocol.

#### 🛡️ Kebijakan Izin (*Permission Modes*)
Gunakan `/permissions` di REPL untuk memilih mode keamanan:
1. **`Ask`** *(Default)*: Agent akan meminta izin Anda sebelum menjalankan tool yang mengubah file atau mengeksekusi shell.
2. **`AutoApprove`**: Mengizinkan semua tool secara otonom tanpa henti (cocok untuk otomasi penuh).
3. **`ReadOnly`**: Memblokir seluruh eksekusi shell dan operasi mutasi file.

### 🧬 Fitur Canggih ala `fx`
1. **Self-Healing Code Loop**: Saat file ditulis atau diedit, `ctrl-cli` otomatis memeriksa diagnosa kompilasi. Jika terjadi eror (misal `cargo check`), feedback kesalahan kompilasi langsung diteruskan ke agen agar segera diperbaiki pada giliran berikutnya.
2. **Git Checkpoint & `/undo` / `/diff`**: Sebelum berkas dimutasi, snapshot bayangan dibuat di `.ctrl/snapshots/`. Kamu bisa mengetik `/diff` untuk melihat perbedaan baris berwarna atau `/undo` untuk mengembalikan berkas ke kondisi semula dengan aman.
3. **Real-Time Streaming Output**: Teks dan indikator pemikiran (`reasoning`) dialirkan langsung baris demi baris via Server-Sent Events (SSE), sehingga terminal terasa sangat responsif dan bebas jeda tunggu.
4. **Smart Context Compaction**: Ketika percakapan mendekati 65-70% batas context window atau melebihi 16 putaran, sistem otomatis meringkas giliran lama menjadi satu ringkasan padat tanpa menghilangkan instruksi utama dan file yang aktif.
5. **Built-in Web Fetch & Search**: Agent dapat menelusuri halaman dokumentasi (`web_fetch`) dan mencari solusi bug via web (`web_search`).
6. **Model Context Protocol (MCP)**: Konfigurasikan `.ctrl/mcp.json` untuk menghubungkan tool pihak ketiga seperti SQLite, GitHub, browser automation, atau Postgres.
7. **Background Subagent Delegation**: Tugas riset yang berat atau investigasi dependensi dapat didelegasikan ke `subagent` yang berjalan terisolasi dan hanya mengembalikan kesimpulan akhir ke sesi utama.

### 🧠 Memori & Auto-Writer
- **Sesi Otomatis**: Riwayat percakapan tersimpan otomatis di `.ctrl/session.json` dan dipulihkan saat REPL dibuka kembali. Gunakan `/reset` untuk memulai sesi baru.
- **Long-Term Memory**: Catatan proyek persisten disimpan di `.ctrl/MEMORY.md` dan dimuat otomatis ke *system prompt*.
- **Auto-Writer Safety Net**: Jika AI menghasilkan blok kode lengkap saat diminta membuat halaman/file tetapi lupa memanggil `write_file`, sistem akan otomatis mendeteksi nama file dan menyimpannya langsung ke direktori Anda.

### 🔧 Cara Pakai Mode Generate

```bash
# Generate kode biasa
ctrl-cli generate "buat struct linked list di Rust"

# Simpan langsung ke file (-o atau --output) tanpa membanjiri chat terminal
ctrl-cli generate -o index.html "buat website html landing page responsif"

# Menampilkan metrik token & context window (-t atau --tokens)
ctrl-cli generate -t "buat struct linked list di Rust"

# Dengan skill spesifik
ctrl-cli generate --skill rust-expert "implementasi binary search tree"

# Dengan model spesifik
ctrl-cli generate --model deepseek-chat "refactor kode ini agar lebih clean"
```

### 👤 Personalisasi Profil

Profil disimpan di `~/.ctrl-cli/profile.json`. Edit file tersebut untuk menyesuaikan:

```json
{
  "name": "namamu",
  "tech_stack": ["Python", "TypeScript", "Rust"],
  "response_language": "Bahasa Indonesia",
  "coding_style": "Tulis kode yang bersih, modern, dan efisien."
}
```

---

## 🇬🇧 English

### What is ctrl-cli?

**ctrl-cli** is an ultra-lightweight AI coding agent CLI built with pure Rust. Chat with AI directly from your terminal, switch models on the fly, activate specialist skills — all without a browser!

Binary size is only **~1.8 MB** with minimal RAM usage.

### ✨ Key Features

| Feature | Description |
|---------|-------------|
| 🤖 Autonomous Agent | ReAct loop engine for autonomous code inspection & execution |
| ⚡ Real-Time Streaming | Live SSE streaming (`stream: true`) directly into terminal |
| 🩺 Self-Healing Code Loop | Automated compiler diagnostics feedback (`cargo check`, python, tsc) for self-correction |
| 🔄 Git Checkpoint & Undo | Shadow file snapshots, unified `/diff` review, & instant rollback via `/undo` |
| 🧹 Context Compaction | Automatic & manual context compaction to prevent context window blowouts |
| 🌐 Web Search & Fetch | Built-in `web_fetch` (HTML to clean Markdown) & `web_search` (DuckDuckGo) |
| 🔌 MCP Protocol Support | Model Context Protocol client (`.ctrl/mcp.json`) for dynamic external tools |
| 👥 Subagent Delegation | Delegate isolated research and heavy sub-tasks to child agents via `subagent` |
| 🛠️ 14 Built-in Tools | Full fx-style tool suite (`read`, `write`, `edit`, `glob`, `grep`, `shell`, `web`, `subagent`, etc.) |
| 🛡️ Permission Gate | Interactive safety policy (`Ask`, `AutoApprove`, `ReadOnly`) |
| 🖥️ REPL Mode | Interactive chat directly in terminal with `/` autocompletion |
| ⚡ Generate Mode | Execute tasks & generate code from a single CLI command |
| 📊 Token & Context | Real-time token consumption & context window monitoring |
| 🎯 Specialist Skills | 8 built-in skills + support for loading local `SKILL.md` |
| 🔄 Model Switching | Switch AI models anytime within REPL |
| 🔌 Multi-Provider | Supports OpenAI, DeepSeek, Groq, OpenRouter, etc. |
| 👤 User Profile | Personalize name, tech stack, response language |
| 📦 Tiny Binary | ~2.0 MB, LTO optimized, runs without installation |

### 🎯 Available Skills

| Skill ID | Name | Purpose |
|----------|------|---------|
| `rust-expert` | 🦀 Rust Expert | Idiomatic & memory-safe Rust code |
| `code-reviewer` | 🔍 Code Reviewer | Audit bugs, security & code smells |
| `web-frontend` | 🎨 Web Frontend UI/UX | Modern responsive HTML/CSS/Tailwind |
| `api-architect` | 🏗️ API & Backend Architect | REST/GraphQL design, database, auth |
| `security-auditor` | 🛡️ Security Auditor | OWASP Top 10, exploit mitigation |
| `debugger` | 🐞 Debugger & Trace Doctor | Error analysis & stack trace |
| `test-engineer` | 🧪 Test Engineer & TDD | Unit tests, integration tests, mocks |
| `refactor` | 🧹 Clean Code & Refactoring | SOLID, DRY, modular architecture |

### 🚀 Getting Started

#### 1. Prerequisites

- [Rust](https://rustup.rs/) (edition 2021+)
- An API key from one of these AI providers:
  - [OpenAI](https://platform.openai.com/)
  - [DeepSeek](https://platform.deepseek.com/)
  - [Groq](https://console.groq.com/) *(free tier available)*
  - [OpenRouter](https://openrouter.ai/)

#### 2. Clone & Setup

```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust

# Copy the configuration file
cp .env.example .env
```

#### 3. Configure `.env`

Open `.env` and fill in your settings:

```env
# Your AI provider API key (required)
AI_API_KEY=sk-xxxxxxxxxxxxxxxx

# Provider base URL (optional, defaults to OpenAI)
AI_BASE_URL=https://api.openai.com/v1

# Model name (optional, defaults to gpt-4o-mini)
AI_MODEL=gpt-4o-mini
```

**Other provider examples:**

```env
# DeepSeek
AI_BASE_URL=https://api.deepseek.com/v1
AI_MODEL=deepseek-chat

# Groq (fast & free tier)
AI_BASE_URL=https://api.groq.com/openai/v1
AI_MODEL=llama-3.3-70b-versatile

# OpenRouter
AI_BASE_URL=https://openrouter.ai/api/v1
AI_MODEL=anthropic/claude-3.5-sonnet
```

#### 4. Build & Run

```bash
# Build release binary (one time only)
cargo build --release

# Start interactive REPL
./target/release/ctrl-cli

# Or generate code directly
./target/release/ctrl-cli generate "write a quicksort in Rust"

# Generate code with token & context window metrics
./target/release/ctrl-cli generate -t "write a quicksort in Rust"
```

### 💬 Using the REPL

After running, you will enter REPL mode:

```
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent)
 Active Model: gpt-4o-mini
 Type your prompt and press Enter.
 Press `/` for interactive autocomplete (navigate with ↑ / ↓).
 Commands: /tokens, /model, /skill, /dev, /profile, /info, /clear, /help, /exit
══════════════════════════════════════════════════════════════

[gpt-4o-mini] ➜ 
```

Just type your question or code request and press Enter! A token usage badge will automatically display below each response (e.g. `📊 [Tokens: 120 in + 350 out = 470 total | Context: 0.37% of 128k]`).

### ⌨️ Slash Commands

| Command | Function |
|---------|----------|
| `/` | Open interactive menu (navigate with ↑↓) |
| `/tools` | List the 14 built-in agent tools, execution status & MCP bridge |
| `/undo` | Rollback last file modification from shadow checkpoint |
| `/diff [file]` | Display colorized unified diff of recent changes or git diff |
| `/check [file]` | Run compiler / syntax check (*self-healing loop*) |
| `/compact` | Compact old conversation history to conserve context window |
| `/mcp` | View Model Context Protocol (`.ctrl/mcp.json`) server & tool status |
| `/stream` | Toggle real-time SSE output streaming (`on` / `off`) |
| `/checkpoints` | List recent saved file shadow snapshots |
| `/permissions` | Configure tool security policy (`Ask`, `AutoApprove`, `ReadOnly`) |
| `/memory` | Inspect long-term project memory (`.ctrl/MEMORY.md`) |
| `/reset` | Clear conversation history & session memory (fresh context) |
| `/save [file]` | Save last generated code snippet directly to file (auto-detects filename) |
| `/tokens` | Check token statistics (in/out/total) & context window capacity |
| `/tokens toggle` | Enable or disable the automatic token badge below responses |
| `/model` | Select AI model from an interactive list |
| `/model <name>` | Switch model directly (e.g. `/model deepseek-chat`) |
| `/skill` | Pick a specialist skill from a list |
| `/skill <id>` | Activate a specific skill (e.g. `/skill rust-expert`) |
| `/skill reset` | Deactivate skill, return to General Assistant |
| `/profile` | View active developer profile |
| `/dev` | About developer / author information |
| `/info` | Check active endpoint, model, context window & session token total |
| `/clear` | Clear terminal screen |
| `/help` | Show help |
| `/exit` | Exit REPL |

### 🛠️ Built-in Agent Tools & Security

The AI agent in `ctrl-cli` can autonomously inspect and modify projects using 14 built-in tools + MCP:

- **`read_file`**: Read file content with line numbers and offset support.
- **`write_file`**: Create new files or overwrite existing files (with auto-checkpoint & self-heal).
- **`edit_file`**: Precise surgical code replacements (with auto-checkpoint & self-heal).
- **`code_check`**: Run compiler or syntax validation (`cargo check`, `py_compile`, `tsc`).
- **`web_fetch`**: Fetch web pages from HTTP(S) URLs and convert HTML to clean Markdown.
- **`web_search`**: Search programming documentation and solutions via DuckDuckGo.
- **`subagent`**: Delegate isolated tasks or research to a child agent loop.
- **`glob_files`**: Discover files matching wildcard patterns (e.g., `**/*.rs`).
- **`grep_files`**: Search code and text across the workspace with line numbers.
- **`shell`**: Safely execute shell commands with stdout/stderr capture.
- **`read_tool_result`**: Paginate and read long truncated tool results.
- **`ask_user_question`**: Prompt user interactively for clarification or decisions.
- **`skill`**: Load specialized guidance from local `SKILL.md` files.
- **`manage_memory`**: View or append to persistent project memory in `.ctrl/MEMORY.md`.
- **`mcp__<server>__<tool>`**: Dynamically loaded external tools from Model Context Protocol servers.

#### 🛡️ Permission Modes
Use `/permissions` in REPL to configure the security level:
1. **`Ask`** *(Default)*: The agent prompts for your approval before modifying files or executing shell commands.
2. **`AutoApprove`**: Automatically approves all tool executions (ideal for unattended workflows).
3. **`ReadOnly`**: Blocks all shell execution and mutating file operations.

### 🧬 Advanced Features (Inspired by `fx`)
1. **Self-Healing Code Loop**: When files are written or edited, `ctrl-cli` automatically inspects compilation diagnostics. If an error occurs (e.g., `cargo check`), compiler diagnostic feedback is seamlessly returned to the agent to fix immediately in the next turn.
2. **Git Checkpoint & `/undo` / `/diff`**: Before any file mutation, a shadow snapshot is preserved in `.ctrl/snapshots/`. You can inspect changes with colorized unified diffs via `/diff` or safely roll back any file to its previous state with `/undo`.
3. **Real-Time Streaming Output**: Text and reasoning deltas are streamed in real-time via Server-Sent Events (SSE), making terminal interactions fluid and zero-latency.
4. **Smart Context Compaction**: When context nears 65-70% limit or exceeds 16 turns, the agent intelligently condenses older conversation turns into a compact summary preserving key instructions and active files.
5. **Built-in Web Fetch & Search**: Agent can browse live web documentation (`web_fetch`) and search for bug fixes online (`web_search`).
6. **Model Context Protocol (MCP)**: Configure `.ctrl/mcp.json` to link external tools such as SQLite, GitHub, browser automation, or PostgreSQL via stdio JSON-RPC.
7. **Background Subagent Delegation**: Heavy investigation, dependency audits, or auxiliary research can be delegated to isolated child subagents via `subagent`.

### 🧠 Persistent Memory & Auto-Writer
- **Automatic Sessions**: Conversation history is persisted in `.ctrl/session.json` and restored across REPL launches. Use `/reset` to start clean.
- **Long-Term Memory**: Persistent project notes are stored in `.ctrl/MEMORY.md` and fed into the agent's system prompt.
- **Auto-Writer Safety Net**: If the model outputs code blocks when asked to build or write a file without invoking `write_file`, the CLI automatically detects the intended filename and writes the file for you.

### 🔧 Using Generate Mode

```bash
# Basic code generation
ctrl-cli generate "create a linked list struct in Rust"

# Write directly to file (-o or --output) without flooding chat output
ctrl-cli generate -o index.html "create a modern responsive HTML website landing page"

# Display token usage and context window metrics (-t or --tokens)
ctrl-cli generate -t "create a linked list struct in Rust"

# With a specific skill
ctrl-cli generate --skill rust-expert "implement a binary search tree"

# With a specific model
ctrl-cli generate --model deepseek-chat "refactor this code to be cleaner"
```

### 👤 Profile Customization

Your profile is stored at `~/.ctrl-cli/profile.json`. Edit it to personalize:

```json
{
  "name": "yourname",
  "tech_stack": ["Python", "TypeScript", "Rust"],
  "response_language": "English",
  "coding_style": "Write clean, modern, idiomatic, and efficient code."
}
```

---

## 🇨🇳 中文

### 什么是 ctrl-cli？

**ctrl-cli** 是一个用纯 Rust 构建的超轻量级 AI 编程助手 CLI。你可以直接在终端与 AI 对话、随时切换模型、激活专业技能 —— 无需浏览器！

二进制文件仅约 **~1.8 MB**，内存占用极少。

### ✨ 主要功能

| 功能 | 说明 |
|------|------|
| 🤖 自主智能体 | ReAct 循环执行引擎，自主审查和修改代码 |
| 🛠️ 9 个内置工具 | 类 Unix 工具（`read`, `write`, `edit`, `glob`, `grep`, `shell` 等） |
| 🛡️ 权限安全门 | 交互式安全策略（`Ask`, `AutoApprove`, `ReadOnly`） |
| 🖥️ REPL 模式 | 直接在终端进行带自动补全的交互式对话 |
| ⚡ 生成模式 | 单行命令行直接执行任务并生成代码 |
| 📊 Token 与上下文 | 实时监控 Token 消耗与模型上下文窗口容量 |
| 🎯 专业技能 | 8 种内置技能 + 支持加载本地 `SKILL.md` |
| 🔄 切换模型 | 在 REPL 中随时切换 AI 模型 |
| 🔌 多提供商 | 支持 OpenAI、DeepSeek、Groq、OpenRouter 等 |
| 👤 用户配置 | 个性化名称、技术栈、响应语言 |
| 📦 体积极小 | ~2.0 MB，LTO 极致优化，开箱即用 |

### 🎯 可用技能

| 技能 ID | 名称 | 用途 |
|---------|------|------|
| `rust-expert` | 🦀 Rust 专家 | 惯用且内存安全的 Rust 代码 |
| `code-reviewer` | 🔍 代码审查员 | 审查 Bug、安全漏洞和代码异味 |
| `web-frontend` | 🎨 Web 前端 UI/UX | 现代响应式 HTML/CSS/Tailwind |
| `api-architect` | 🏗️ API & 后端架构师 | REST/GraphQL 设计、数据库、认证 |
| `security-auditor` | 🛡️ 安全审计员 | OWASP Top 10、漏洞缓解 |
| `debugger` | 🐞 调试器 & 追踪专家 | 错误分析 & 堆栈跟踪 |
| `test-engineer` | 🧪 测试工程师 & TDD | 单元测试、集成测试、Mock |
| `refactor` | 🧹 整洁代码 & 重构 | SOLID、DRY、模块化架构 |

### 🚀 快速开始

#### 1. 前置条件

- [Rust](https://rustup.rs/)（2021 版或更高）
- 来自以下 AI 提供商之一的 API 密钥：
  - [OpenAI](https://platform.openai.com/)
  - [DeepSeek](https://platform.deepseek.com/)
  - [Groq](https://console.groq.com/) *(有免费额度)*
  - [OpenRouter](https://openrouter.ai/)

#### 2. 克隆并设置

```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust

# 复制配置文件
cp .env.example .env
```

#### 3. 配置 `.env`

打开 `.env` 文件并填写你的设置：

```env
# AI 提供商的 API 密钥（必填）
AI_API_KEY=sk-xxxxxxxxxxxxxxxx

# 提供商基础 URL（可选，默认为 OpenAI）
AI_BASE_URL=https://api.openai.com/v1

# 模型名称（可选，默认为 gpt-4o-mini）
AI_MODEL=gpt-4o-mini
```

**其他提供商示例：**

```env
# DeepSeek
AI_BASE_URL=https://api.deepseek.com/v1
AI_MODEL=deepseek-chat

# Groq（快速且有免费额度）
AI_BASE_URL=https://api.groq.com/openai/v1
AI_MODEL=llama-3.3-70b-versatile

# OpenRouter
AI_BASE_URL=https://openrouter.ai/api/v1
AI_MODEL=anthropic/claude-3.5-sonnet
```

#### 4. 构建并运行

```bash
# 构建发布版（仅需一次）
cargo build --release

# 启动交互式 REPL
./target/release/ctrl-cli

# 或直接生成代码
./target/release/ctrl-cli generate "用 Rust 写一个快速排序"
```

### 💬 使用 REPL

运行后，你将进入 REPL 模式：

```
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent)
 Active Model: gpt-4o-mini
 输入你的提示并按 Enter。
══════════════════════════════════════════════════════════════

[gpt-4o-mini] ➜ 
```

直接输入问题或代码请求，然后按 Enter！

### ⌨️ 斜杠命令

| 命令 | 功能 |
|------|------|
| `/` | 打开交互菜单（用 ↑↓ 导航选择） |
| `/tools` | 查看 9 个内置智能体工具列表及其执行策略 |
| `/permissions` | 设置工具安全权限策略（`Ask`, `AutoApprove`, `ReadOnly`） |
| `/memory` | 查看项目长期记忆记录（`.ctrl/MEMORY.md`） |
| `/reset` | 清空对话历史和会话记忆（开启全新上下文） |
| `/save [文件]` | 将最近生成的代码直接保存到文件（自动检测文件名） |
| `/tokens` | 查看 Token 使用统计与模型上下文窗口上限 |
| `/tokens toggle` | 开启/关闭响应后自动显示的 Token 状态栏 |
| `/model` | 从交互列表中选择 AI 模型 |
| `/model <name>` | 直接切换模型（例如 `/model deepseek-chat`） |
| `/skill` | 从列表中选择专业技能 |
| `/skill <id>` | 激活特定技能（例如 `/skill rust-expert`） |
| `/skill reset` | 停用技能，返回通用助手 |
| `/profile` | 查看当前开发者配置 |
| `/dev` | 查看开发者与作者信息 |
| `/info` | 检查活跃端点、模型、上下文窗口和会话总 Token |
| `/clear` | 清除终端屏幕 |
| `/help` | 显示帮助 |
| `/exit` | 退出 REPL |

### 🛠️ 内置智能体工具与安全策略

`ctrl-cli` 中的 AI 智能体可以通过 9 种内置工具自主检查和修改工作区：

- **`read_file`**: 读取文件内容，支持行号和偏移。
- **`write_file`**: 创建新文件或覆盖已有文件。
- **`edit_file`**: 精确替换代码片段（精准手术式修改）。
- **`glob_files`**: 模式匹配检索工作区文件路径（如 `**/*.rs`）。
- **`grep_files`**: 全局检索代码与文本关键词。
- **`shell`**: 安全执行终端命令并捕获标准输出与错误。
- **`read_tool_result`**: 分页读取过长截断的工具执行结果。
- **`ask_user_question`**: 在需要澄清或确认时向用户交互式提问。
- **`skill`**: 加载本地 `SKILL.md` 的专精规则。

#### 🛡️ 权限安全门（Permission Modes）
在 REPL 中输入 `/permissions` 切换安全策略：
1. **`Ask`**（默认）：执行修改文件或运行 Shell 脚本前，智能体会先征询您的批准。
2. **`AutoApprove`**：自动允许所有工具调用（适用于全自动场景）。
3. **`ReadOnly`**：只读模式，拦截所有 Shell 命令和文件写入。

### 🧠 持久化记忆与自动保存
- **会话持久化**：对话历史自动保存在 `.ctrl/session.json`，重启 REPL 时自动恢复。使用 `/reset` 即可重置。
- **长期记忆**：工作区关键信息存放在 `.ctrl/MEMORY.md` 并自动作为 System Prompt 的一部分。
- **Auto-Writer 安全兜底**：当您要求创建网页或脚本而模型忘记调用 `write_file` 时，CLI 会自动检测代码块中的文件名并为您写入磁盘。

### 🔧 使用生成模式

```bash
# 基本代码生成
ctrl-cli generate "用 Rust 创建一个链表结构"

# 指定技能
ctrl-cli generate --skill rust-expert "实现二叉搜索树"

# 指定模型
ctrl-cli generate --model deepseek-chat "重构这段代码使其更简洁"
```

### 👤 个性化配置

你的配置存储在 `~/.ctrl-cli/profile.json`，可以编辑它进行个性化：

```json
{
  "name": "你的名字",
  "tech_stack": ["Python", "TypeScript", "Rust"],
  "response_language": "中文",
  "coding_style": "编写简洁、现代、高效的代码。"
}
```

---

<div align="center">

## 🏗️ Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (edition 2021) |
| CLI Parsing | [clap](https://github.com/clap-rs/clap) v4 |
| Interactive Input | [inquire](https://github.com/mikaelmello/inquire) v0.7 |
| HTTP Client | [ureq](https://github.com/algesten/ureq) v2 |
| Serialization | [serde](https://serde.rs/) + serde_json |
| Error Handling | [anyhow](https://github.com/dtolnay/anyhow) |

## 📄 License

MIT License — Free to use and modify.

---

*Made with ❤️ and 🦀 by [galangfjr](https://github.com/gafirin5)*

</div>
