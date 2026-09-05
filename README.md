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
| 🛠️ 9 Built-in Tools | Tools ala `fx` (`read`, `write`, `edit`, `glob`, `grep`, `shell`, dll.) |
| 🛡️ Permission Gate | Kebijakan keamanan interaktif (`Ask`, `AutoApprove`, `ReadOnly`) |
| 🖥️ Mode REPL | Chat interaktif langsung di terminal |
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
| `/` | Buka menu interaktif (pilih dengan ↑↓) |
| `/save [file]` | Simpan kode respon terakhir langsung ke file (auto-detect nama file) |
| `/tokens` | Cek statistik token (in/out/total) & batas context window |
| `/tokens toggle` | Aktifkan/nonaktifkan badge token otomatis setelah respon |
| `/model` | Pilih model AI dari daftar |
| `/model <nama>` | Ganti model langsung (contoh: `/model deepseek-chat`) |
| `/skill` | Pilih skill spesialis dari daftar |
| `/skill <id>` | Aktifkan skill tertentu (contoh: `/skill rust-expert`) |
| `/skill reset` | Nonaktifkan skill, kembali ke General Assistant |
| `/profile` | Lihat profil developer aktif |
| `/info` | Cek endpoint, model aktif, context window & total token sesi |
| `/clear` | Bersihkan layar terminal |
| `/help` | Tampilkan bantuan |
| `/exit` | Keluar dari REPL |

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
| 🖥️ REPL Mode | Interactive chat directly in terminal |
| ⚡ Generate Mode | Generate code from a single command |
| 📊 Token & Context | Monitor token consumption & model context window limit |
| 🎯 Specialist Skills | 8 built-in skills (Rust Expert, Debugger, etc.) |
| 🔄 Model Switching | Switch AI models anytime within REPL |
| 🔌 Multi-Provider | Supports OpenAI, DeepSeek, Groq, OpenRouter, etc. |
| 👤 User Profile | Personalize name, tech stack, response language |
| 📦 Tiny Binary | ~1.8 MB, LTO optimized, runs without installation |

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
 Commands: /tokens, /model, /skill, /dev, /profile, /info, /clear, /help, /exit
══════════════════════════════════════════════════════════════

[gpt-4o-mini] ➜ 
```

Just type your question or code request and press Enter! A token usage badge will automatically display below each response (e.g. `📊 [Tokens: 120 in + 350 out = 470 total | Context: 0.37% of 128k]`).

### ⌨️ Slash Commands

| Command | Function |
|---------|----------|
| `/` | Open interactive menu (navigate with ↑↓) |
| `/save [file]` | Save last generated code snippet directly to file (auto-detects filename) |
| `/tokens` | Check token statistics (in/out/total) & context window capacity |
| `/tokens toggle` | Enable or disable the automatic token badge below responses |
| `/model` | Select AI model from a list |
| `/model <name>` | Switch model directly (e.g. `/model deepseek-chat`) |
| `/skill` | Pick a specialist skill from a list |
| `/skill <id>` | Activate a specific skill (e.g. `/skill rust-expert`) |
| `/skill reset` | Deactivate skill, return to General Assistant |
| `/profile` | View active developer profile |
| `/info` | Check active endpoint, model, context window & session token total |
| `/clear` | Clear terminal screen |
| `/help` | Show help |
| `/exit` | Exit REPL |

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
| 🖥️ REPL 模式 | 直接在终端进行交互式对话 |
| ⚡ 生成模式 | 单行命令生成代码 |
| 🎯 专业技能 | 8 种内置技能（Rust 专家、调试器等） |
| 🔄 切换模型 | 在 REPL 中随时切换 AI 模型 |
| 🔌 多提供商 | 支持 OpenAI、DeepSeek、Groq、OpenRouter 等 |
| 👤 用户配置 | 个性化名称、技术栈、响应语言 |
| 📦 体积极小 | ~1.8 MB，LTO 优化，无需安装即可运行 |

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
| `/` | 打开交互菜单（用 ↑↓ 导航） |
| `/model` | 从列表中选择 AI 模型 |
| `/model <name>` | 直接切换模型（例如 `/model deepseek-chat`） |
| `/skill` | 从列表中选择专业技能 |
| `/skill <id>` | 激活特定技能（例如 `/skill rust-expert`） |
| `/skill reset` | 停用技能，返回通用助手 |
| `/profile` | 查看当前开发者配置 |
| `/info` | 检查活跃端点和模型配置 |
| `/clear` | 清除终端屏幕 |
| `/help` | 显示帮助 |
| `/exit` | 退出 REPL |

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
