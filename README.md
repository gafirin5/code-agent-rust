<div align="center">

# 🤖 ctrl-cli

**Ultra-lightweight AI Coding Agent CLI — Built with Rust**

[![Version](https://img.shields.io/badge/Version-0.3.0-blue)]()
[![Rust](https://img.shields.io/badge/Built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/Tests-320%2B%20Passing-success)]()
[![Clippy](https://img.shields.io/badge/Clippy-0%20Warnings-brightgreen)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Binary Size](https://img.shields.io/badge/Binary-~1.8%20MB-brightgreen)]()
[![OpenAI Compatible](https://img.shields.io/badge/API-OpenAI%20Compatible-412991?logo=openai)]()

*Read in: [🇮🇩 Bahasa Indonesia](#-bahasa-indonesia) · [🇬🇧 English](#-english) · [🇨🇳 中文](#-中文)*

</div>

---

## 🇮🇩 Bahasa Indonesia

### 💡 Apa itu ctrl-cli?

**`ctrl-cli`** adalah asisten coding AI di terminal kamu — mirip seperti *Claude Code* atau *Cursor CLI*, tetapi dirancang **super ringan (~1.8 MB)** dengan performa tinggi menggunakan bahasa **Rust murni**.

Kamu tidak perlu membuka browser atau aplikasi berat:
- 💬 **Tanya Jawab & Diskusi Kode**: Ngobrol langsung dari terminal layaknya *pair programming*.
- 🛠️ **Bisa Baca & Edit Proyek Mandiri**: Agent bisa membaca struktur folder, mengedit file secara presisi, dan menjalankan terminal.
- 🩺 **Bisa Memperbaiki Error Sendiri (Self-Healing)**: Setelah menulis kode, agent mengecek hasil kompilasi compiler (`cargo check`, python, tsc) dan langsung membetulkan kodenya jika ada error.
- 🛡️ **Aman & Anti-Panik**: Sebelum mengubah file, sistem membuat salinan otomatis (*snapshot*). Jika kamu tidak suka perubahannya, cukup ketik `/undo` untuk kembali seperti semula!
- 🌐 **Cari Solusi di Web**: Terintegrasi langsung dengan mesin pencari DuckDuckGo untuk melihat dokumentasi atau solusi error terkini.
- 🔌 **Bebas Pilih Model AI**: Mau yang gratis dan super cepat (Groq), model lokal offline (Ollama), atau model canggih (OpenAI GPT-4o, DeepSeek, Claude 3.5 Sonnet).

---

### ⚡ 3 Langkah Cepat Memulai (Quick Start)

#### 1. Pasang & Buka Folder
Pastikan komputer kamu sudah terpasang [Rust](https://rustup.rs/) (versi 2021+), lalu:
```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust
```

#### 2. Buat File Konfigurasi `.env`
Salin template konfigurasi:
```bash
# Di Windows (CMD / PowerShell):
copy .env.example .env

# Di Linux / macOS:
cp .env.example .env
```

Buka file `.env` dan masukkan API Key dari provider pilihanmu:

```env
# Contoh 1: Pakai Groq (Gratis & Sangat Cepat!)
AI_API_KEY=gsk_xxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://api.groq.com/openai/v1
AI_MODEL=llama-3.3-70b-versatile

# Contoh 2: Pakai OpenAI
# AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
# AI_BASE_URL=https://api.openai.com/v1
# AI_MODEL=gpt-4o-mini

# Contoh 3: Pakai DeepSeek
# AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
# AI_BASE_URL=https://api.deepseek.com/v1
# AI_MODEL=deepseek-chat

# Contoh 4: Pakai Ollama Lokal (100% Offline & Gratis, tanpa API Key)
# AI_API_KEY=ollama
# AI_BASE_URL=http://localhost:11434/v1
# AI_MODEL=qwen2.5-coder:7b
```

#### 3. Jalankan Aplikasi!
```bash
# Jalankan langsung dalam mode interaktif (REPL):
cargo run
```
*Selesai! Terminal siap menerima perintah kode pertamamu.* 🎉

---

### 🎮 4 Cara Menggunakan ctrl-cli

Pilihlah gaya penggunaan yang paling nyaman untukmu:

#### 1. 💬 Mode Chat Interaktif (REPL) — *Paling Populer*
Ketik langsung pertanyaan atau perintah di terminal:
```bash
cargo run
```
Tampilan terminal:
```text
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent v0.3.0)
 Active Model: llama-3.3-70b-versatile
 Ketik pertanyaanmu lalu tekan Enter.
 Ketik `/` untuk melihat menu perintah cepat.
══════════════════════════════════════════════════════════════

[llama-3.3-70b] ➜ Buatkan fungsi validasi email di Rust lengkap dengan unit test-nya
```

#### 2. 🌐 Mode Web Dashboard (Tampilan Browser) — *Baru di v0.3.0!*
Lebih suka tampilan visual di browser? Cukup jalankan:
```bash
cargo run -- serve
```
Lalu buka browser di: **`http://127.0.0.1:3000`**. Kamu bisa chatting dengan AI melalui antarmuka web modern lengkap dengan editor kode dan task board!

#### 3. 🖥️ Mode TUI Fullscreen (Terminal Visual) — *Baru di v0.3.0!*
Untuk kamu penggemar terminal ala *Vim/Neovim/Htop*:
```bash
cargo run -- --tui
```
Menampilkan dashboard fullscreen di terminal dengan panel chat, daftar file, dan status task.

#### 4. ⚡ Mode Satu Baris (Generate One-Shot)
Jalankan tugas singkat langsung dari satu baris terminal tanpa masuk ke REPL:
```bash
# Tanya / minta kode singkat
cargo run -- generate "bagaimana cara membaca file baris demi baris di Rust?"

# Simpan langsung hasilnya ke file baru tanpa ribet copy-paste
cargo run -- generate -o salam.py "buat script python untuk menyapa pengguna sesuai waktu"

# Jalankan dengan persona spesialis
cargo run -- --skill rust-expert generate "buatkan arsitektur concurrency thread-safe"
```

---

### 🛡️ Fitur Keamanan: Bebas Khawatir dari Kesalahan Kode!

Banyak orang takut AI mengubah file sembarangan. `ctrl-cli` dilengkapi sistem proteksi berlapis:

1. **Auto-Snapshot & `/undo`**:
   Setiap kali agent akan mengedit file, sistem otomatis menyimpan salinan cadangan (*checkpoint*) di folder `.ctrl/snapshots/`.
   - Ketik `/diff` untuk melihat perbedaan baris berwarna yang diubah.
   - Ketik `/undo` jika kodenya salah, dan file akan **seketika kembali ke kondisi sebelum diedit**.
2. **Izin Akses Interaktif (`/permissions`)**:
   - **`Ask` (Bawaan)**: AI akan selalu minta persetujuanmu `[Y/n]` sebelum menyentuh file atau mengeksekusi shell.
   - **`AutoApprove`**: Untuk kamu yang ingin AI bekerja otomatis penuh tanpa henti.
   - **`ReadOnly`**: Mode aman 100%, AI hanya diizinkan membaca file dan dilarang mengubah apapun.
3. **Self-Healing Code Loop**:
   Jika AI membuat kode yang error kompilasi, `ctrl-cli` akan menangkap log error compiler (`cargo check`, python compiler, TypeScript), lalu memberikannya kembali ke AI agar langsung diperbaiki saat itu juga.

---

### ⌨️ Menu Perintah Cepat (Slash Commands)

Saat berada di dalam REPL, cukup ketik `/` lalu tekan tombol panah `↑`/`↓` pada keyboard untuk memilih perintah:

| Kategori | Perintah | Fungsi & Kegunaan |
|----------|----------|-------------------|
| **Paling Sering Digunakan** | `/help` | Menampilkan panduan bantuan lengkap |
| | `/clear` | Membersihkan layar terminal |
| | `/exit` | Keluar dari aplikasi |
| **Kontrol File & Kode** | `/undo` | ⏪ **Batalkan perubahan file terakhir** (kembali ke snapshot sebelumnya) |
| | `/diff [file]` | 🔍 Tampilkan perbandingan baris yang baru saja diubah |
| | `/check [file]` | 🩺 Jalankan compiler/linter untuk memastikan kode tidak error |
| | `/save [file]` | 💾 Simpan potongan kode terakhir langsung ke file |
| | `/tools` | 🛠️ Lihat daftar 14 perkakas bawaan yang bisa dipakai agent |
| **Pengaturan AI & Model** | `/model` | Ganti model AI (misal pindah ke GPT-4o atau DeepSeek) |
| | `/skill` | Aktifkan persona ahli (Rust, Reviewer, Frontend, Backend, dll.) |
| | `/tokens` | Cek sisa kuota context window & penggunaan token |
| | `/compact` | 🧹 Ringkas percakapan lama agar hemat biaya token |
| | `/stream` | Nyalakan/matikan efek ketikan real-time (*streaming*) |
| | `/permissions` | Ubah izin eksekusi (`Ask`, `AutoApprove`, `ReadOnly`) |
| | `/reset` | Hapus riwayat sesi lama dan mulai topik obrolan baru |

---

### 🎯 Skill Spesialis Bawaan

Kamu bisa mengaktifkan "topi keahlian" khusus untuk agent dengan perintah `/skill <nama>`:

| Skill | Fokus & Keahlian |
|-------|------------------|
| `rust-expert` | 🦀 Kode Rust idiomatik, zero-cost abstractions, dan memory-safe |
| `code-reviewer` | 🔍 Audit bug, celah keamanan, dan saran clean code |
| `web-frontend` | 🎨 Desain antarmuka HTML/CSS/Tailwind modern dan responsif |
| `api-architect` | 🏗️ Arsitektur REST/GraphQL API, skema database, dan autentikasi |
| `security-auditor` | 🛡️ Pemeriksaan celah keamanan OWASP Top 10 dan sanitasi input |
| `debugger` | 🐞 Analisis stack trace error dan pelacakan akar masalah bug |
| `test-engineer` | 🧪 Pembuatan unit test, integration test, dan skenario pengujian TDD |
| `refactor` | 🧹 Pembersihan kode kusut (Clean Code, SOLID, DRY) |

---

### 🛠️ Apa Saja yang Bisa Dilakukan Agent?

Agent di `ctrl-cli` dibekali 14 perkakas (*tools*) bawaan untuk bekerja seperti developer profesional:
- 📖 **`read_file`**: Membaca isi file dengan cepat dan terstruktur.
- ✍️ **`write_file` & `edit_file`**: Menulis dan mengubah file dengan presisi tinggi + auto backup.
- 🔎 **`glob_files` & `grep_files`**: Mencari nama file atau kata kunci tertentu di seluruh proyek.
- 💻 **`shell`**: Menjalankan perintah terminal (misal `cargo build`, `npm test`, dll.).
- 🌐 **`web_search` & `web_fetch`**: Mencari solusi di DuckDuckGo dan membaca dokumentasi web.
- 👥 **`subagent`**: Menugaskan agen anak di latar belakang untuk meriset hal rumit tanpa mengotori chat utama.
- 🧠 **`manage_memory`**: Menyimpan catatan penting proyek di `.ctrl/MEMORY.md` agar AI tidak lupa konteks.
- 🔌 **`mcp`**: Mendukung Model Context Protocol untuk integrasi tool eksternal (GitHub, Database, dll.).

---

### 👤 Profil Personal Kamu

Ingin AI memanggil namamu atau selalu membalas dengan gaya tertentu? Edit file `~/.ctrl-cli/profile.json`:
```json
{
  "name": "Budi",
  "tech_stack": ["Rust", "Python", "TypeScript"],
  "response_language": "Bahasa Indonesia",
  "coding_style": "Tulis kode yang bersih, mudah dibaca, dan berikan penjelasan singkat."
}
```

---

### 📚 Dokumentasi Lengkap
Ingin mempelajari arsitektur atau konfigurasi lebih dalam? Kunjungi folder [**`docs/`**](../docs/README.md):
- 📝 [Catatan Rilis & Update v0.3.0](../docs/UPDATE_NOTES.md)
- ⚙️ [Panduan Konfigurasi Lengkap](../docs/CONFIGURATION.md)
- 🏗️ [Arsitektur Sistem & Concurrency](../docs/ARCHITECTURE.md)
- 🤖 [Buku Panduan AI Agent](../docs/AI_AGENT_GUIDE.md)
- 🛠️ [Referensi Tool Lengkap](../docs/TOOLS_REFERENCE.md)


---

## 🇬🇧 English

### 💡 What is ctrl-cli?

**`ctrl-cli`** is an ultra-lightweight AI coding assistant in your terminal — similar to *Claude Code* or *Cursor CLI*, but engineered to be **extremely small (~1.8 MB)** and blazingly fast using **pure Rust**.

No heavy web browser or electronic bloat required:
- 💬 **Code Discussions & Pairing**: Chat directly from your terminal as if pair-programming with a senior engineer.
- 🛠️ **Autonomous Workspace Exploration & Edits**: The agent can inspect directory trees, surgical-edit code, and run terminal commands safely.
- 🩺 **Self-Healing Code Loop**: Automatically runs compiler checks (`cargo check`, python compiler, tsc) after edits and immediately self-corrects any compilation or syntax errors.
- 🛡️ **Panic-Free Safety (Snapshot & Undo)**: Before modifying any file, an automatic shadow checkpoint is saved. Don't like the AI's changes? Simply type `/undo` to instantly restore your file!
- 🌐 **Web Search & Documentation Fetch**: Integrated with DuckDuckGo to browse up-to-date documentation and troubleshoot errors online.
- 🔌 **Freedom of AI Providers**: Use free & ultra-fast cloud models (Groq), 100% offline local models (Ollama), or top-tier APIs (OpenAI GPT-4o, DeepSeek, Claude 3.5 Sonnet).

---

### ⚡ 3-Minute Quick Start

#### 1. Clone & Enter Directory
Ensure you have [Rust](https://rustup.rs/) (2021+ edition) installed, then run:
```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust
```

#### 2. Configure Your `.env`
Copy the environment template:
```bash
# On Windows (CMD / PowerShell):
copy .env.example .env

# On Linux / macOS:
cp .env.example .env
```

Open `.env` and configure your preferred provider:

```env
# Option 1: Groq (Free Tier & Super Fast!)
AI_API_KEY=gsk_xxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://api.groq.com/openai/v1
AI_MODEL=llama-3.3-70b-versatile

# Option 2: OpenAI
# AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
# AI_BASE_URL=https://api.openai.com/v1
# AI_MODEL=gpt-4o-mini

# Option 3: DeepSeek
# AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
# AI_BASE_URL=https://api.deepseek.com/v1
# AI_MODEL=deepseek-chat

# Option 4: Local Ollama (100% Offline & Free, No API Key needed)
# AI_API_KEY=ollama
# AI_BASE_URL=http://localhost:11434/v1
# AI_MODEL=qwen2.5-coder:7b
```

#### 3. Run It!
```bash
# Launch interactive REPL mode:
cargo run
```
*You're all set! Your terminal is ready for your first AI coding prompt.* 🎉

---

### 🎮 4 Ways to Use ctrl-cli

Choose whatever workflow fits you best:

#### 1. 💬 Interactive Chat Mode (REPL) — *Most Popular*
Start a conversation right in your terminal:
```bash
cargo run
```
Terminal prompt:
```text
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent v0.3.0)
 Active Model: llama-3.3-70b-versatile
 Type your prompt and press Enter.
 Type `/` to open interactive quick command menu.
══════════════════════════════════════════════════════════════

[llama-3.3-70b] ➜ Write a thread-safe LRU cache in Rust with unit tests
```

#### 2. 🌐 Web Dashboard UI (Browser Mode) — *New in v0.3.0!*
Prefer a visual browser experience?
```bash
cargo run -- serve
```
Then open your browser at: **`http://127.0.0.1:3000`**. You get an interactive chat interface, live task board, and code editor!

#### 3. 🖥️ Fullscreen TUI Mode (Terminal Visual) — *New in v0.3.0!*
For fans of terminal dashboards (*Vim / Htop* style):
```bash
cargo run -- --tui
```
Displays an interactive fullscreen terminal interface with chat feeds, file trees, and task statuses.

#### 4. ⚡ One-Shot Generate Mode
Execute tasks and generate code without launching the REPL:
```bash
# Quick explanation or question
cargo run -- generate "how to read a file line by line in Rust?"

# Directly save output to a file without manual copy-pasting
cargo run -- generate -o greet.py "write a python script that greets the user based on local time"

# Run with a specialist skill persona
cargo run -- --skill rust-expert generate "create a zero-allocation parsing pipeline"
```

---

### 🛡️ Safety Architecture: Never Worry About Broken Code

1. **Auto-Snapshot & `/undo`**:
   Before modifying any file, `ctrl-cli` creates a shadow checkpoint in `.ctrl/snapshots/`.
   - Type `/diff` to inspect colorized changes before committing.
   - Type `/undo` if the AI made a mistake, instantly reverting the file to its exact previous state.
2. **Permission Gate (`/permissions`)**:
   - **`Ask` (Default)**: The agent always prompts for confirmation `[Y/n]` before modifying files or executing shell scripts.
   - **`AutoApprove`**: For full hands-free autonomous workflows.
   - **`ReadOnly`**: 100% safe mode; forbids all file writes and shell execution.
3. **Self-Healing Code Loop**:
   If generated code fails to compile, `ctrl-cli` captures compiler diagnostic logs (`cargo check`, python compiler, TypeScript) and automatically hands them back to the agent for instant self-correction.

---

### ⌨️ Slash Commands Cheat Sheet

Inside the REPL, type `/` and press `↑`/`↓` arrow keys to autocomplete commands:

| Category | Command | Description |
|----------|---------|-------------|
| **General** | `/help` | Display full help instructions |
| | `/clear` | Clear terminal screen |
| | `/exit` | Exit the application |
| **Code & Files** | `/undo` | ⏪ **Rollback last file modification** to previous snapshot |
| | `/diff [file]` | 🔍 Review unified diff of recent changes |
| | `/check [file]` | 🩺 Run compiler/syntax check (*self-healing loop*) |
| | `/save [file]` | 💾 Save last generated code snippet directly to disk |
| | `/tools` | 🛠️ List 14 built-in agent tools and their statuses |
| **AI & Models** | `/model` | Switch AI models interactively (e.g., GPT-4o, DeepSeek) |
| | `/skill` | Activate specialist persona (Rust, Reviewer, Frontend, etc.) |
| | `/tokens` | Check token usage & remaining context window capacity |
| | `/compact` | 🧹 Compact long chat history to reduce token costs |
| | `/stream` | Toggle real-time SSE output streaming |
| | `/permissions` | Switch security modes (`Ask`, `AutoApprove`, `ReadOnly`) |
| | `/reset` | Clear session history and start fresh context |

---

### 🎯 Built-in Specialist Skills

Activate domain-specific agent intelligence with `/skill <name>`:

| Skill | Focus & Expertise |
|-------|-------------------|
| `rust-expert` | 🦀 Idiomatic, zero-cost, memory-safe Rust engineering |
| `code-reviewer` | 🔍 Bug detection, security audit, and clean code suggestions |
| `web-frontend` | 🎨 Modern responsive HTML/CSS/Tailwind UI/UX |
| `api-architect` | 🏗️ REST/GraphQL API design, database schemas, and auth |
| `security-auditor` | 🛡️ OWASP Top 10 vulnerability mitigation and sanitization |
| `debugger` | 🐞 Stack trace analysis and root cause investigation |
| `test-engineer` | 🧪 Unit tests, integration tests, and TDD scenarios |
| `refactor` | 🧹 Clean Code, SOLID principles, and modular architecture |

---

### 🛠️ Built-in Agent Tools

The autonomous agent is equipped with 14 tools to work just like a human developer:
- 📖 **`read_file`**: Read file content with line numbering and offsets.
- ✍️ **`write_file` & `edit_file`**: Precise file writes and surgical edits with automatic snapshots.
- 🔎 **`glob_files` & `grep_files`**: Search filenames and grep text across your workspace.
- 💻 **`shell`**: Execute terminal commands safely with stdout/stderr capture.
- 🌐 **`web_search` & `web_fetch`**: Search DuckDuckGo and convert web pages to readable Markdown.
- 👥 **`subagent`**: Delegate heavy investigations to isolated child agents in the background.
- 🧠 **`manage_memory`**: Read/write persistent project notes in `.ctrl/MEMORY.md`.
- 🔌 **`mcp`**: Model Context Protocol client for external tools (PostgreSQL, GitHub, etc.).

---

### 👤 User Profile Personalization

Personalize how the AI interacts with you by editing `~/.ctrl-cli/profile.json`:
```json
{
  "name": "Alex",
  "tech_stack": ["Rust", "Python", "TypeScript"],
  "response_language": "English",
  "coding_style": "Write clean, modern, idiomatic code with concise explanations."
}
```

---

### 📚 Full Documentation
Looking for deeper technical and architecture guides? Visit the [**`docs/`**](../docs/README.md) directory:
- 📝 [v0.3.0 Release Notes & Changelog](../docs/UPDATE_NOTES.md)
- ⚙️ [Configuration Guide](../docs/CONFIGURATION.md)
- 🏗️ [System Architecture & Concurrency](../docs/ARCHITECTURE.md)
- 🤖 [AI Agent Operations Guide](../docs/AI_AGENT_GUIDE.md)
- 🛠️ [Comprehensive Tools Reference](../docs/TOOLS_REFERENCE.md)

---

## 🇨🇳 中文

### 💡 什么是 ctrl-cli？

**`ctrl-cli`** 是你终端里的超轻量级 AI 编程助手 —— 类似于 *Claude Code* 或 *Cursor CLI*，但体积**极小（仅 ~1.8 MB）**，完全使用**纯 Rust** 编写，极致迅捷。

无需启动笨重的浏览器或大型客户端：
- 💬 **代码对话与结对编程**：直接在终端与 AI 讨论架构、编写函数与排查 Bug。
- 🛠️ **自主工作区探索与修改**：智能体可自动浏览目录、精确定点编辑文件，并安全执行终端命令。
- 🩺 **代码自愈修复（Self-Healing）**：写完代码后自动执行编译器诊断（`cargo check`、Python 编译、tsc），遇到报错立即自动纠错。
- 🛡️ **安全防慌（快照备份与 `/undo`）**：修改任何文件前自动创建快照。如果不满意 AI 的改动，只需输入 `/undo` 即可瞬间回退！
- 🌐 **实时网络检索**：集成 DuckDuckGo 搜索与网页抓取，随时查阅最新官方文档与报错解决方案。
- 🔌 **自由选择 AI 模型**：支持免费且极速的 Groq、100% 离线隐私的本地 Ollama，以及顶级商用模型（OpenAI GPT-4o、DeepSeek、Claude 3.5 Sonnet）。

---

### ⚡ 3 步快速上手

#### 1. 克隆并进入目录
确保电脑已安装 [Rust](https://rustup.rs/)（2021 版或更高版本）：
```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust
```

#### 2. 配置 `.env` 密钥文件
复制配置模板：
```bash
# Windows (CMD / PowerShell):
copy .env.example .env

# Linux / macOS:
cp .env.example .env
```

编辑 `.env` 文件，填入你偏好的 AI 提供商：

```env
# 示例 1: 使用 Groq（快速且提供免费额度！）
AI_API_KEY=gsk_xxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://api.groq.com/openai/v1
AI_MODEL=llama-3.3-70b-versatile

# 示例 2: 使用 OpenAI
# AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
# AI_BASE_URL=https://api.openai.com/v1
# AI_MODEL=gpt-4o-mini

# 示例 3: 使用 DeepSeek
# AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
# AI_BASE_URL=https://api.deepseek.com/v1
# AI_MODEL=deepseek-chat

# 示例 4: 使用本地 Ollama（100% 离线免密钥）
# AI_API_KEY=ollama
# AI_BASE_URL=http://localhost:11434/v1
# AI_MODEL=qwen2.5-coder:7b
```

#### 3. 运行！
```bash
# 启动交互式终端模式（REPL）：
cargo run
```
*大功告成！终端已准备就绪，输入你的第一条编程指令吧。* 🎉

---

### 🎮 4 种运行模式

#### 1. 💬 交互对话模式 (REPL) — *最常用*
```bash
cargo run
```
终端界面：
```text
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent v0.3.0)
 Active Model: llama-3.3-70b-versatile
 输入你的问题并按 Enter。
 输入 `/` 弹出交互式快捷命令列表。
══════════════════════════════════════════════════════════════

[llama-3.3-70b] ➜ 用 Rust 写一个线程安全的 LRU 缓存并包含测试用例
```

#### 2. 🌐 Web 仪表盘模式 (浏览器界面) — *v0.3.0 新特性!*
```bash
cargo run -- serve
```
然后在浏览器中打开：**`http://127.0.0.1:3000`**。享受包含对话界面、代码编辑器和任务看板的现代 Web UI！

#### 3. 🖥️ 全屏 TUI 终端模式 — *v0.3.0 新特性!*
适合终端极客（*Vim/Htop* 风格）：
```bash
cargo run -- --tui
```

#### 4. ⚡ 单行生成模式 (Generate)
无需进入交互界面，单行指令直接完成任务：
```bash
# 快速提问
cargo run -- generate "Rust 如何逐行读取大文件？"

# 直接将代码保存到文件，避免繁琐复制
cargo run -- generate -o greet.py "写一个根据当前时间向用户打招呼的 Python 脚本"

# 切换专家角色执行任务
cargo run -- --skill rust-expert generate "设计零分配数据解析流"
```

---

### 🛡️ 安全架构：告别代码被改坏的担忧

1. **自动快照与 `/undo`**：
   修改文件前，系统在 `.ctrl/snapshots/` 中自动备份。
   - 输入 `/diff` 查看高亮改动对比。
   - 输入 `/undo` 即可一键恢复原样。
2. **权限门控 (`/permissions`)**：
   - **`Ask`（默认）**：执行文件写入或 Shell 命令前会向你征询确认 `[Y/n]`。
   - **`AutoApprove`**：适合无人值守的全自动流。
   - **`ReadOnly`**：只读模式，完全杜绝文件修改与命令执行。
3. **自愈修复循环 (Self-Healing Loop)**：
   自动检测编译器报错（`cargo check`、Python、TypeScript），并将报错信息无缝提供给 AI 进行自动修复。

---

### ⌨️ 常用斜杠快捷命令速查

在 REPL 中输入 `/` 并按键盘 `↑`/`↓` 键即可唤出命令：

| 分类 | 命令 | 说明 |
|------|------|------|
| **常用** | `/help` | 显示完整帮助说明 |
| | `/clear` | 清理终端屏幕 |
| | `/exit` | 退出程序 |
| **文件与代码** | `/undo` | ⏪ **撤销最近的文件改动**（恢复上一快照） |
| | `/diff [文件]` | 🔍 查看最新文件改动的 Unified Diff 对比 |
| | `/check [文件]` | 🩺 运行编译器/语法检查（*自愈循环*） |
| | `/save [文件]` | 💾 将最近生成的代码直接保存到磁盘 |
| | `/tools` | 🛠️ 查看 14 种内置工具及其运行状态 |
| **模型与技能** | `/model` | 交互式切换 AI 模型（如 GPT-4o、DeepSeek 等） |
| | `/skill` | 激活专业角色技能（Rust、Reviewer、前端等） |
| | `/tokens` | 查看上下文窗口容量与 Token 消耗统计 |
| | `/compact` | 🧹 压缩长对话上下文以降低 Token 消耗 |
| | `/stream` | 开启/关闭打字机式流式输出 |
| | `/permissions` | 切换安全策略（`Ask`, `AutoApprove`, `ReadOnly`） |
| | `/reset` | 清空历史会话，开启新话题 |

---

### 🎯 内置专家技能

输入 `/skill <名称>` 即可为智能体分配专业领域特长：

| 技能 | 核心特长 |
|------|----------|
| `rust-expert` | 🦀 惯用、零成本抽象、内存安全的 Rust 高级开发 |
| `code-reviewer` | 🔍 Bug 审查、安全漏洞防范与 Clean Code 建议 |
| `web-frontend` | 🎨 现代化响应式 HTML/CSS/Tailwind 前端与交互设计 |
| `api-architect` | 🏗️ REST/GraphQL API 设计、数据库建模与认证 |
| `security-auditor` | 🛡️ OWASP Top 10 漏洞审计与输入清理 |
| `debugger` | 🐞 堆栈跟踪分析与深层 Bug 定位排查 |
| `test-engineer` | 🧪 单元测试、集成测试与 TDD 测试用例编写 |
| `refactor` | 🧹 代码异味重构与模块化架构设计 |

---

### 🛠️ 智能体 14 种内置工具

- 📖 **`read_file`**：带行号与偏移量的高效文件读取。
- ✍️ **`write_file` & `edit_file`**：高精度写入与局部定点手术式替换（带自动快照）。
- 🔎 **`glob_files` & `grep_files`**：文件名通配与全局文本关键词检索。
- 💻 **`shell`**：安全执行系统终端命令并捕获标准输出与错误。
- 🌐 **`web_search` & `web_fetch`**：DuckDuckGo 在线搜索与网页转 Markdown 解析。
- 👥 **`subagent`**：在后台启动隔离的子智能体执行重型检索任务。
- 🧠 **`manage_memory`**：在 `.ctrl/MEMORY.md` 读写长期记忆。
- 🔌 **`mcp`**：支持 Model Context Protocol 连接外部工具（Postgres、GitHub 等）。

---

### 👤 开发者个性化配置

在 `~/.ctrl-cli/profile.json` 中配置你的偏好：
```json
{
  "name": "你的名字",
  "tech_stack": ["Rust", "Python", "TypeScript"],
  "response_language": "中文",
  "coding_style": "编写简洁、现代、高效且带必要注释的代码。"
}
```

---

### 📚 完整文档
更多技术设计与架构细节，请参阅 [**`docs/`**](../docs/README.md) 目录：
- 📝 [v0.3.0 更新日志与说明](../docs/UPDATE_NOTES.md)
- ⚙️ [完整配置指南](../docs/CONFIGURATION.md)
- 🏗️ [系统架构与并发模型](../docs/ARCHITECTURE.md)
- 🤖 [AI 智能体操作指南](../docs/AI_AGENT_GUIDE.md)
- 🛠️ [完整工具参考手册](../docs/TOOLS_REFERENCE.md)

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
