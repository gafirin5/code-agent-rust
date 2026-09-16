<div align="center">

# 🤖 ctrl-cli

**Asisten Coding AI Ringan di Terminal — 100% Rust Murni**

[![Version](https://img.shields.io/badge/Version-0.3.0-blue)]()
[![Rust](https://img.shields.io/badge/Built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![Binary Size](https://img.shields.io/badge/Binary-~1.8%20MB-brightgreen)]()
[![Tests](https://img.shields.io/badge/Tests-320%2B%20Passing-success)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![OpenAI Compatible](https://img.shields.io/badge/API-OpenAI%20Compatible-412991?logo=openai)]()

*Pilih bahasa / Choose language: [🇮🇩 Bahasa Indonesia](#-bahasa-indonesia) · [🇬🇧 English](#-english) · [🇨🇳 中文](#-中文)*

</div>

---

## 🇮🇩 Bahasa Indonesia

### 💡 Apa itu ctrl-cli?

Bayangkan kamu memiliki **rekan programmer senior di terminal kamu**. Kamu cukup mengetik apa yang ingin dibuat atau diperbaiki menggunakan bahasa sehari-hari, dan `ctrl-cli` akan membaca file proyek, menulis kode, mengecek apakah ada error kompilasi, dan langsung memperbaikinya sendiri!

Berbeda dengan aplikasi AI lain yang berat dan lambat, `ctrl-cli` dirancang **sangat ringan (~1.8 MB)** dan **super kencang** karena dibangun 100% menggunakan bahasa pemrograman **Rust murni** tanpa embel-embel framework web yang boros RAM.

#### ✨ Keunggulan Utama:
* 💬 **Chat Interaktif di Terminal**: Berdiskusi dan meminta bantuan kode langsung dari terminal/konsol favoritmu.
* 🛠️ **Bisa Bekerja Mandiri**: Mampu membaca isi folder, mengedit file secara presisi, dan menjalankan terminal.
* 🩺 **Otomatis Benerin Kode (Self-Healing)**: Jika kode yang dibuat mengalami error saat dikompilasi (`cargo check`, python, tsc), agent akan membaca pesan error tersebut dan otomatis memperbaikinya saat itu juga!
* 🛡️ **Anti-Panik (`/undo`)**: Setiap sebelum file diubah, sistem otomatis menyimpan salinan cadangan (*checkpoint*). Jika kamu tidak puas dengan hasilnya, cukup ketik `/undo` untuk kembali ke kondisi awal!
* 🆓 **Bisa Gratis & Offline**: Mendukung provider gratisan super cepat (Groq), model lokal offline tanpa kuota internet (Ollama), maupun model komersial (OpenAI, DeepSeek, Gemini).

---

### ⚡ 3 Langkah Cepat Memulai (Quick Start)

#### 1. Download & Masuk ke Folder Proyek
Pastikan di komputermu sudah terpasang [Rust](https://rustup.rs/) (versi 2021 atau lebih baru):
```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust
```

#### 2. Siapkan File Konfigurasi `.env`
Salin template konfigurasi:
```bash
# Untuk Windows (CMD / PowerShell):
copy .env.example .env

# Untuk Linux / macOS:
cp .env.example .env
```

Buka file `.env` dengan editor teks favoritmu, lalu pilih salah satu konfigurasi di bawah ini:

> 💡 **Mau coba gratis tanpa ribet?**
> * **Opsi A: Groq (Gratis & Super Cepat — Sangat Direkomendasikan)**
>   1. Buka [console.groq.com/keys](https://console.groq.com/keys) dan buat akun (gratis, tanpa kartu kredit).
>   2. Buat API Key baru, lalu isi file `.env`:
>   ```env
>   AI_API_KEY=gsk_xxxxxxxxxxxxxxxxxxxxxx
>   AI_BASE_URL=https://api.groq.com/openai/v1
>   AI_MODEL=llama-3.3-70b-versatile
>   ```
>
> * **Opsi B: Ollama (100% Offline, Privat & Gratis Tanpa Internet)**
>   1. Pasang [Ollama](https://ollama.com/) dan unduh model coding: `ollama run qwen2.5-coder:7b`
>   2. Isi file `.env`:
>   ```env
>   AI_API_KEY=ollama
>   AI_BASE_URL=http://localhost:11434/v1
>   AI_MODEL=qwen2.5-coder:7b
>   ```
>
> * **Opsi C: OpenAI / DeepSeek**
>   ```env
>   AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
>   AI_BASE_URL=https://api.openai.com/v1   # atau https://api.deepseek.com/v1
>   AI_MODEL=gpt-4o-mini                   # atau deepseek-chat
>   ```

#### 3. Jalankan!
```bash
cargo run
```
*Selesai! Terminal siap diajak ngobrol dan coding bersama.* 🎉

> 💡 **Tips Tambahan**: Ingin bisa memanggil `ctrl-cli` dari folder mana saja tanpa harus mengetik `cargo run`? Cukup jalankan perintah:
> ```bash
> cargo install --path . --force
> ```
> Setelah itu, kamu bisa cukup mengetik `ctrl-cli` di terminal mana pun!

---

### 🎬 Contoh Pemakaian Nyata

Berikut gambaran alur saat kamu menggunakan `ctrl-cli`:

```text
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent v0.3.0)
 Active Model: llama-3.3-70b-versatile
 Ketik pesanmu dan tekan Enter.
 Ketik `/` untuk melihat menu perintah cepat.
══════════════════════════════════════════════════════════════

[llama-3.3-70b] ➜ buatkan fungsi hitung diskon bertingkat di file src/discount.rs

🤖 [ctrl-cli]: Menyiapkan file src/discount.rs...
   ✍️ Menulis kode fungsi kalkulasi diskon...
   🩺 Menjalankan verifikasi compiler... [Lolos]
✅ File src/discount.rs berhasil dibuat beserta unit test-nya!

[llama-3.3-70b] ➜ /diff
🔍 Menampilkan perubahan baris terbaru (+35 baris pada src/discount.rs)

[llama-3.3-70b] ➜ /undo
⏪ Perubahan dibatalkan! File src/discount.rs dikembalikan ke versi sebelumnya.
```

---

### 🎮 4 Pilihan Mode Tampilan (Sesuai Gaya Kamu)

`ctrl-cli` menyediakan 4 cara penggunaan yang fleksibel:

| Mode | Cara Menjalankan | Kapan Cocok Digunakan? |
| :--- | :--- | :--- |
| **1. 💬 Chat Terminal (REPL)** | `cargo run` (atau `ctrl-cli`) | **Default & Paling Santai.** Mengobrol santai layaknya di ChatGPT, langsung di terminalmu. |
| **2. 🖥️ Visual Terminal (TUI)** | `ctrl-cli-tui` (atau ketik `/tui`) | **Penggemar Vim / Htop.** Tampilan layar penuh dengan panel chat, daftar task, dan skill selector. |
| **3. 🌐 Web Browser** | `cargo run -- serve` | **Tampilan Grafis Browser.** Buka `http://127.0.0.1:3000` di Chrome/Firefox untuk memantau proses secara visual. |
| **4. ⚡ Sekali Jalan (One-Shot)** | `cargo run -- generate "..."` | **Scripting / Automasi.** Menghasilkan kode atau jawaban instan langsung ke file tanpa masuk sesi chat. |

#### Contoh Perintah Sekali Jalan (One-Shot):
```bash
# Tanya cara koding cepat:
cargo run -- generate "bagaimana cara membaca file baris demi baris di Rust?"

# Buat script dan langsung simpan ke file tanpa perlu copy-paste manual:
cargo run -- generate -o salam.py "buat script python untuk menyapa pengguna sesuai waktu"
```

---

### ⌨️ Perintah Penting (Slash Commands)

Saat berada di dalam mode chat (REPL), ketik `/` lalu gunakan tombol panah keyboard `↑` / `↓` untuk memilih perintah:

| Perintah | Apa Fungsinya? |
| :--- | :--- |
| **/help** | ❓ Menampilkan daftar bantuan seluruh perintah. |
| **/undo** | ⏪ **Batalkan perubahan file terakhir** (kembali ke snapshot sebelum diedit). |
| **/diff** | 🔍 Lihat perbandingan baris kode yang baru saja dimodifikasi. |
| **/check** | 🩺 Jalankan compiler / linter untuk memastikan tidak ada syntax error. |
| **/tui** | 🖥️ Pindah seketika ke tampilan antarmuka visual layar penuh (*Fullscreen TUI*). |
| **/model** | 🔄 Ganti model AI yang sedang aktif (misal beralih ke DeepSeek atau GPT-4o). |
| **/skill** | 🎭 Aktifkan persona spesialis (Rust Expert, Reviewer, Debugger, dll.). |
| **/tokens** | 📊 Cek kapasitas memori obrolan dan konsumsi token saat ini. |
| **/compact** | 🧹 Ringkas riwayat obrolan panjang untuk menghemat pemakaian token. |
| **/clear** | 🧼 Bersihkan tampilan layar terminal. |
| **/exit** | 🚪 Keluar dari aplikasi. |

> 💡 **Pintasan Pindah Mode**:
> * Dari **Chat ➔ TUI**: Ketik `/tui` lalu Enter.
> * Dari **TUI ➔ Chat**: Tekan tombol **`F5`** di keyboard.

---

### 🎯 Pilihan Keahlian AI (`/skill`)

Kamu bisa mengarahkan fokus AI sesuai tugas yang sedang dikerjakan dengan mengetik `/skill <nama>`:

* 🦀 **`rust-expert`**: Ahli bahasa Rust, performa tinggi, zero-cost abstractions, dan memori aman.
* 🔍 **`code-reviewer`**: Mengaudit kode untuk mencari celah bug, keamanan, dan saran arsitektur bersih.
* 🎨 **`web-frontend`**: Ahli pembuatan tampilan antarmuka web modern (HTML, Tailwind CSS, JavaScript).
* 🏗️ **`api-architect`**: Ahli mendesain REST API, skema database, dan sistem otentikasi.
* 🐞 **`debugger`**: Menganalisis error stack trace dan menemukan akar penyebab masalah (*root cause*).
* 🧪 **`test-engineer`**: Menulis unit test komprehensif dan skenario pengujian TDD.
* 🧹 **`refactor`**: Merapikan kode yang berantakan agar mudah dibaca dan dirawat (*Clean Code*).

---

### 🛡️ Fitur Keamanan: Bebas Khawatir dari Kesalahan Kode!

Banyak developer khawatir AI akan merusak file proyek mereka. `ctrl-cli` memiliki sistem pengaman berlapis:
1. **Auto-Snapshot & Rollback**: Setiap kali agent akan mengedit file, salinan cadangan dibuat di `.ctrl/snapshots/`. Salah edit? Ketik `/undo`, dan file kembali seperti semula seketika.
2. **Konfirmasi Izin (`/permissions`)**:
   - `Ask` *(Bawaan)*: Agent akan selalu meminta izin `[Y/n]` sebelum mengubah file atau menjalankan perintah shell.
   - `AutoApprove`: Mode otomatis penuh jika kamu ingin agent bekerja mandiri tanpa sering bertanya.
   - `ReadOnly`: Mode 100% aman, agent hanya boleh membaca dan dilarang mengubah file apa pun.
3. **Self-Healing Loop**: Jika hasil kode menghasilkan error kompilasi, `ctrl-cli` akan membaca pesan error tersebut dan otomatis memperbaikinya sampai berhasil dikompilasi dengan baik.

---

### 📚 Dokumentasi Teknis Lengkap

Ingin mempelajari arsitektur internal atau konfigurasi lanjutan? Kunjungi folder [**`docs/`**](./docs/README.md):
* 📝 [Catatan Rilis & Pembaruan (`UPDATE_NOTES.md`)](./docs/UPDATE_NOTES.md)
* 📊 [Hasil Uji Performa & Benchmark (`PERFORMANCE.md`)](./docs/PERFORMANCE.md)
* ⚙️ [Panduan Konfigurasi Lengkap (`CONFIGURATION.md`)](./docs/CONFIGURATION.md)
* 🏗️ [Arsitektur Sistem & Threading (`ARCHITECTURE.md`)](./docs/ARCHITECTURE.md)
* 🤖 [Buku Panduan AI Agent (`AI_AGENT_GUIDE.md`)](./docs/AI_AGENT_GUIDE.md)
* 🛠️ [Referensi 15+ Built-in Tools (`TOOLS_REFERENCE.md`)](./docs/TOOLS_REFERENCE.md)
* 🗺️ [Roadmap Fitur Masa Depan (`ROADMAP.md`)](./docs/ROADMAP.md)

---

## 🇬🇧 English

### 💡 What is ctrl-cli?

Imagine having a **senior software engineer in your terminal**. You simply explain what you want to build or fix in plain English, and `ctrl-cli` reads your workspace, writes code, verifies compiler diagnostics, and automatically fixes errors on the spot!

Unlike heavy Electron-based AI editors, `ctrl-cli` is engineered to be **ultra-lightweight (~1.8 MB binary)** and blazingly fast using **100% pure Rust**.

#### ✨ Key Features:
* 💬 **Interactive Terminal Chat**: Pair-program directly inside your favorite terminal.
* 🛠️ **Autonomous Workspace Navigation**: Reads file trees, executes surgical edits, and runs commands safely.
* 🩺 **Self-Healing Loop**: Captures compiler errors (`cargo check`, python, tsc) and automatically self-corrects code.
* 🛡️ **Zero-Panic Safety (`/undo`)**: Checkpoints files before modifying them. Type `/undo` to instantly restore previous states.
* 🆓 **Free & Offline Friendly**: Out-of-the-box support for blazing-fast free tiers (Groq), 100% offline local models (Ollama), and commercial APIs (OpenAI, DeepSeek, Gemini).

---

### ⚡ 3-Step Quick Start

#### 1. Clone & Enter Directory
Ensure [Rust](https://rustup.rs/) (2021 edition or newer) is installed:
```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust
```

#### 2. Configure Your `.env`
Copy the environment template:
```bash
# Windows (CMD / PowerShell):
copy .env.example .env

# Linux / macOS:
cp .env.example .env
```

Edit `.env` and select your provider:

> 💡 **Want a free & instant setup?**
> * **Option A: Groq (Free & Extremely Fast — Recommended)**
>   1. Grab a free key at [console.groq.com/keys](https://console.groq.com/keys) (no credit card required).
>   2. Set in `.env`:
>   ```env
>   AI_API_KEY=gsk_xxxxxxxxxxxxxxxxxxxxxx
>   AI_BASE_URL=https://api.groq.com/openai/v1
>   AI_MODEL=llama-3.3-70b-versatile
>   ```
>
> * **Option B: Ollama (100% Offline & Private)**
>   1. Install [Ollama](https://ollama.com/) and run: `ollama run qwen2.5-coder:7b`
>   2. Set in `.env`:
>   ```env
>   AI_API_KEY=ollama
>   AI_BASE_URL=http://localhost:11434/v1
>   AI_MODEL=qwen2.5-coder:7b
>   ```
>
> * **Option C: OpenAI / DeepSeek**
>   ```env
>   AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxx
>   AI_BASE_URL=https://api.openai.com/v1   # or https://api.deepseek.com/v1
>   AI_MODEL=gpt-4o-mini                   # or deepseek-chat
>   ```

#### 3. Run It!
```bash
cargo run
```

> 💡 **Global Install**: To run `ctrl-cli` anywhere on your machine without typing `cargo run`:
> ```bash
> cargo install --path . --force
> ```

---

### 🎮 4 Ways to Run ctrl-cli

| Mode | Command | Best For |
| :--- | :--- | :--- |
| **1. 💬 Chat Mode (REPL)** | `cargo run` (or `ctrl-cli`) | Default conversational terminal workflow. |
| **2. 🖥️ Fullscreen TUI** | `ctrl-cli-tui` (or type `/tui`) | Vim/Htop style dashboard with chat feeds & task monitors. |
| **3. 🌐 Web Dashboard** | `cargo run -- serve` | Open `http://127.0.0.1:3000` in your browser for a graphical UI. |
| **4. ⚡ One-Shot CLI** | `cargo run -- generate "..."` | Quick scripting, pipe-friendly answers, and direct file output. |

#### One-Shot Examples:
```bash
# Quick explanation:
cargo run -- generate "how to read a file line by line in Rust?"

# Generate code directly into a file:
cargo run -- generate -o greet.py "write a python script that greets the user by local time"
```

---

### ⌨️ Essential Slash Commands

Type `/` in REPL mode and use `↑`/`↓` keys to navigate:

| Command | Description |
| :--- | :--- |
| **/help** | ❓ Show comprehensive command list and guidance. |
| **/undo** | ⏪ **Revert last modified file** to previous checkpoint snapshot. |
| **/diff** | 🔍 Show colorized unified diff of recent changes. |
| **/check** | 🩺 Run compiler/syntax self-healing check. |
| **/tui** | 🖥️ Switch to fullscreen TUI dashboard instantly. |
| **/model** | 🔄 Switch active AI model interactively. |
| **/skill** | 🎭 Activate specialized persona (Rust Expert, Reviewer, etc.). |
| **/tokens** | 📊 View token consumption and remaining context capacity. |
| **/compact** | 🧹 Compact chat history to save tokens. |
| **/clear** | 🧼 Clear terminal screen. |
| **/exit** | 🚪 Exit application. |

---

### 📚 Full Technical Documentation

* 📝 [Release Notes & Updates (`UPDATE_NOTES.md`)](./docs/UPDATE_NOTES.md)
* 📊 [Performance Benchmarks (`PERFORMANCE.md`)](./docs/PERFORMANCE.md)
* ⚙️ [Configuration Guide (`CONFIGURATION.md`)](./docs/CONFIGURATION.md)
* 🏗️ [System Architecture (`ARCHITECTURE.md`)](./docs/ARCHITECTURE.md)
* 🤖 [AI Agent Manual (`AI_AGENT_GUIDE.md`)](./docs/AI_AGENT_GUIDE.md)
* 🛠️ [Tools Reference Guide (`TOOLS_REFERENCE.md`)](./docs/TOOLS_REFERENCE.md)
* 🗺️ [Future Roadmap (`ROADMAP.md`)](./docs/ROADMAP.md)

---

## 🇨🇳 中文

### 💡 什么是 ctrl-cli？

**`ctrl-cli`** 是你终端里的超轻量级 AI 结对编程助手 —— 类似于 *Claude Code* 或 *Cursor CLI*，但体积**极小（仅 ~1.8 MB）**，完全采用**纯 Rust 编写**，极致轻巧且不占内存。

#### ✨ 核心亮点：
- 💬 **终端交互对话**：直接在控制台与 AI 讨论架构、编写函数与重构。
- 🛠️ **自主工作区探索**：自动检索文件、实施精准定点修改，并安全运行终端指令。
- 🩺 **代码自愈修复（Self-Healing）**：编辑后自动执行编译器诊断（`cargo check`、Python、tsc），遇到报错立即自动纠正。
- 🛡️ **安全防慌（`/undo`）**：每次修改前自动在 `.ctrl/snapshots/` 建立快照备份，随时输入 `/undo` 一键无损还原。
- 🆓 **支持免费与离线模型**：原生支持超高速免费 API（Groq）、100% 离线隐私模型（Ollama）以及商用模型（OpenAI、DeepSeek、Gemini）。

---

### ⚡ 3 步极速上手

#### 1. 克隆并进入目录
```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust
```

#### 2. 配置 `.env` 密钥文件
```bash
# Windows:
copy .env.example .env

# Linux / macOS:
cp .env.example .env
```

在 `.env` 中填入你的配置（支持 Groq 免费模型、本地 Ollama 或 OpenAI/DeepSeek）。

#### 3. 运行！
```bash
cargo run
```

> 💡 **全局安装**：若想在任意目录直接输入 `ctrl-cli` 运行，执行：
> ```bash
> cargo install --path . --force
> ```

---

### ⌨️ 常用斜杠命令速查

| 命令 | 功能说明 |
| :--- | :--- |
| **/help** | ❓ 显示完整帮助与命令列表 |
| **/undo** | ⏪ **撤销最近的文件修改**（恢复上一快照） |
| **/diff** | 🔍 查看最新文件改动的对比 |
| **/check** | 🩺 运行编译器诊断并自动修复错误 |
| **/tui** | 🖥️ 切换至全屏 TUI 可视化终端仪表盘 |
| **/model** | 🔄 交互式切换 AI 模型 |
| **/skill** | 🎭 激活特定领域专家技能 |
| **/tokens** | 📊 查看 Token 消耗与上下文余量 |
| **/compact** | 🧹 压缩长对话以节省 Token |
| **/exit** | 🚪 退出程序 |

---

<div align="center">

## 📄 Lisensi / License

MIT License — Bebas digunakan, dipelajari, dan dimodifikasi.

*Dibuat dengan ❤️ dan 🦀 oleh [galangfjr](https://github.com/gafirin5)*

</div>
