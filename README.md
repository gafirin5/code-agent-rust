<div align="center">

# 🤖 ctrl-cli

**Asisten Coding AI Ringan di Terminal & Orkestrator Otonom — 100% Rust Murni**

[![Version](https://img.shields.io/badge/Version-0.3.0-blue)]()
[![Rust](https://img.shields.io/badge/Built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![Binary Size](https://img.shields.io/badge/Binary-~3.2%20MB-brightgreen)]()
[![RAM Idle](https://img.shields.io/badge/RAM%20Idle-<30%20MB-brightgreen)]()
[![Tests](https://img.shields.io/badge/Tests-340%2B%20Passing-success)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![OpenAI Compatible](https://img.shields.io/badge/API-OpenAI%20Compatible-412991?logo=openai)]()

*Pilih bahasa / Choose language: [🇮🇩 Bahasa Indonesia](#-bahasa-indonesia) · [🇬🇧 English](#-english) · [🇨🇳 中文](#-中文)*

</div>

---

## 📑 Daftar Isi (Table of Contents)
- [🇮🇩 Bahasa Indonesia](#-bahasa-indonesia)
  - [💡 1. Apa itu ctrl-cli?](#-1-apa-itu-ctrl-cli)
  - [⚡ 2. Panduan Cepat Memulai (3 Menit)](#-2-panduan-cepat-memulai-3-menit)
  - [🎬 3. Contoh Pemakaian Sehari-hari](#-3-contoh-pemakaian-sehari-hari)
  - [🎮 4. Pilihan 4 Mode Tampilan](#-4-pilihan-4-mode-tampilan)
  - [⌨️ 5. Menu Perintah Cepat (Slash Commands)](#️-5-menu-perintah-cepat-slash-commands)
  - [🎯 6. Persona Spesialis AI (/skill)](#-6-persona-spesialis-ai-skill)
  - [🧠 7. Penjelasan Lengkap Cara Kerja Agent (ReAct Loop)](#-7-penjelasan-lengkap-cara-kerja-agent-react-loop)
  - [🛠️ 8. Daftar Lengkap 15 Built-in Tools](#️-8-daftar-lengkap-15-built-in-tools)
  - [👥 9. Konkurensi & Subagent Background Tasks](#-9-konkurensi--subagent-background-tasks)
  - [🛡️ 10. Sistem Keamanan, Izin & Auto-Snapshot Sandbox](#️-10-sistem-keamanan-izin--auto-snapshot-sandbox)
  - [🖥️ 11. Panduan Lengkap TUI Fullscreen & Keybindings](#-11-panduan-lengkap-tui-fullscreen--keybindings)
  - [🌐 12. Mode Web Dashboard & REST API](#-12-mode-web-dashboard--rest-api)
  - [⚙️ 13. Konfigurasi Lanjutan & Profil Pengguna](#️-13-konfigurasi-lanjutan--profil-pengguna)
  - [📊 14. Metrik Kinerja & Efisiensi Sistem](#-14-metrik-kinerja--efisiensi-sistem)
  - [📂 15. Struktur Arsitektur Kode](#-15-struktur-arsitektur-kode)
  - [📚 16. Pusat Dokumentasi Lengkap](#-16-pusat-dokumentasi-lengkap)
- [🇬🇧 English Overview](#-english)
- [🇨🇳 中文概述](#-中文)

---

## 🇮🇩 Bahasa Indonesia

### 💡 1. Apa itu ctrl-cli?

Bayangkan kamu memiliki **rekan programmer senior di terminal kamu**. Kamu cukup mengetik apa yang ingin dibuat atau diperbaiki menggunakan bahasa sehari-hari, dan `ctrl-cli` akan membaca file proyek, menulis kode, mengecek apakah ada error kompilasi, dan langsung memperbaikinya sendiri!

Berbeda dengan aplikasi AI lain yang berat dan lambat (seperti aplikasi GUI berbasis Electron yang memakan ratusan MB RAM), `ctrl-cli` dirancang **sangat hemat (~1.8 MB binary, < 25 MB RAM)** dan **super responsif** karena dibangun 100% menggunakan bahasa pemrograman **Rust murni**.

#### ✨ Keunggulan Utama:
* 💬 **Chat Interaktif di Terminal**: Berdiskusi dan meminta bantuan kode langsung dari konsol favoritmu.
* 🛠️ **Bisa Bekerja Mandiri**: Membaca susunan folder, mengedit file secara presisi, dan menjalankan terminal.
* 🩺 **Otomatis Benerin Kode (Self-Healing)**: Jika kode mengalami error saat dikompilasi (`cargo check`, python, tsc), agent menangkap pesan error tersebut dan langsung memperbaikinya seketika!
* 🛡️ **Anti-Panik (`/undo`)**: Setiap sebelum file diubah, sistem otomatis menyimpan salinan cadangan (*checkpoint*). Jika kamu tidak puas dengan hasilnya, cukup ketik `/undo` untuk kembali ke kondisi semula!
* 🆓 **Bisa Gratis & Offline**: Mendukung provider gratisan super cepat (Groq), model lokal offline tanpa kuota internet (Ollama), maupun model komersial (OpenAI, DeepSeek, Gemini).

---

### ⚡ 2. Panduan Cepat Memulai (3 Menit)

#### Langkah 1: Download & Masuk Folder Proyek
Pastikan di komputermu sudah terpasang [Rust](https://rustup.rs/) (versi 2021 atau lebih baru):
```bash
git clone https://github.com/gafirin5/code-agent-rust.git
cd code-agent-rust
```

#### Langkah 2: Buat File Konfigurasi `.env`
Salin template konfigurasi:
```bash
# Windows (CMD / PowerShell):
copy .env.example .env

# Linux / macOS:
cp .env.example .env
```

Buka `.env` dan pilih salah satu konfigurasi penyedia AI berikut:

> 💡 **Mau coba gratis tanpa ribet?**
> * **Opsi A: Groq (Rekomendasi Gratis & Super Cepat — Tanpa Kartu Kredit)**
>   1. Buka [console.groq.com/keys](https://console.groq.com/keys) lalu buat API Key gratis.
>   2. Isi di `.env`:
>   ```env
>   AI_API_KEY=gsk_xxxxxxxxxxxxxxxxxxxxxx
>   AI_BASE_URL=https://api.groq.com/openai/v1
>   AI_MODEL=llama-3.3-70b-versatile
>   ```
>
> * **Opsi B: Ollama (100% Offline, Privat & Gratis Tanpa Internet)**
>   1. Pasang [Ollama](https://ollama.com/) dan unduh model coding: `ollama run qwen2.5-coder:7b`
>   2. Isi di `.env`:
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

#### Langkah 3: Jalankan!

Pilih mode yang kamu sukai:

- **Mode Modern TUI (Visual, Rapi & Elegan — Sangat Direkomendasikan)**:
  ```powershell
  # Dari root workspace:
  .\tui        # Windows PowerShell (instan)
  tui          # Windows Command Prompt (instan)
  ./tui.sh     # Linux / macOS / Git Bash

  # Atau dari folder ctrl-cli:
  cargo tui
  # atau
  ctrl-cli -t
  ```

- **Mode Classic Chat (REPL teks konsol)**:
  ```bash
  cargo run
  # atau
  ctrl-cli --cli
  ```
*Selesai! Terminal siap diajak ngobrol dan coding bersama.* 🎉

> 💡 **Tips Pasang Permanen**: Ingin bisa mengetik `ctrl-cli` dari folder mana saja tanpa `cargo run`?
> ```bash
> cargo install --path . --force
> ```
> Sekarang cukup ketik `ctrl-cli -t` (untuk TUI) atau `ctrl-cli` di terminal mana pun!

---

### 🎬 3. Contoh Pemakaian Sehari-hari

Berikut gambaran alur saat kamu menggunakan `ctrl-cli`:

```text
══════════════════════════════════════════════════════════════
 🤖 ctrl-cli REPL (AI Coding Agent v0.3.0)
 Active Model: llama-3.3-70b-versatile
 Ketik pesanmu dan tekan Enter.
 Ketik 'tui' untuk grafis TUI, '/' untuk menu cepat.
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

[llama-3.3-70b] ➜ tui
🖥️ Beralih ke antarmuka grafis Ratatui TUI modern...
```

---

### 🎮 4. Pilihan 4 Mode Tampilan

`ctrl-cli` menyediakan 4 cara penggunaan yang fleksibel sesuai kebutuhanmu:

| Mode | Cara Menjalankan | Kapan Cocok Digunakan? |
| :--- | :--- | :--- |
| **1. 🖥️ Visual Terminal (TUI)** | `.\tui` / `cargo tui` / `ctrl-cli -t` / ketik `tui` di REPL | **Paling Direkomendasikan & Modern.** Tampilan fullscreen dengan 5 tema warna (F6), Zen Mode layar penuh (F9), Command Palette (F8/Ctrl+P), pesan berbingkai kartu, syntax highlighting native + ikon bahasa + diff highlighting, live animated spinner, telemetry & context window gauge, foldable reasoning (z), dan sidebar interaktif (F1–F4). |
| **2. 💬 Chat Terminal (REPL)** | `cargo run` (atau `ctrl-cli --cli`) | **Classic & Ringan.** Mengobrol santai baris-per-baris dengan auto-completion `/` dan hot-switch ke TUI kapan saja tanpa memutus sesi. |
| **3. 🌐 Web Browser** | `cargo run -- serve` | **Tampilan Grafis Browser.** Buka `http://127.0.0.1:3000` di Chrome/Firefox untuk memantau proses secara visual via REST & SSE. |
| **4. ⚡ Sekali Jalan (One-Shot)** | `cargo run -- generate "..."` | **Scripting / Automasi.** Menghasilkan kode atau jawaban instan langsung ke file tanpa masuk sesi chat interaktif. |

#### Pintasan Keyboard & Fitur Canggih TUI:
- **`F6`** atau **`/theme`**: Ganti tema warna secara live (Tokyo Night, Catppuccin Mocha, Gruvbox Dark, Cyberpunk Matrix, Monokai Pro).
- **`F8`** atau **`Ctrl+P`**: Buka **Command Palette** modal terapung untuk mencari dan mengeksekusi aksi instan.
- **`F9`** atau **`Ctrl+B`** atau **`/zen`**: Toggle **Zen Mode** (sembunyikan / tampilkan sidebar untuk fokus penuh).
- **`z`** atau **`Space`** (di panel Chat): **Fold / Unfold** reasoning *Thought Process* agar chat rapi dan ringkas.
- **`F1`–`F4`**: Akses cepat tab Sidebar (Help, Tasks, Skills, Provider).
- **`F5`** atau **`:cli`**: Kembali ke mode CLI / REPL biasa kapan saja.
- **`Tab`**: Pindah fokus navigasi antar panel (Input ⇄ Chat ⇄ Sidebar).
- **Statusline Cerdas**: Visual context window gauge `Ctx [████░░░░] %` (hijau/kuning/merah), Git branch indicator (` 🌿 master `), dan live RAM/CPU metrics.

---

### ⌨️ 5. Menu Perintah Cepat (Slash Commands)

Saat berada di dalam mode chat (REPL) atau TUI, ketik `/` lalu gunakan tombol panah keyboard `↑` / `↓` untuk memilih perintah:

| Perintah | Fungsi & Kegunaan |
| :--- | :--- |
| **/help** | ❓ Menampilkan panduan bantuan lengkap seluruh perintah. |
| **/theme** | 🎨 Ganti tema warna TUI (Tokyo Night, Catppuccin, Gruvbox, Matrix, Monokai). |
| **/zen** | 🪟 Toggle Zen Mode layar penuh (sembunyikan / tampilkan sidebar). |
| **/undo** | ⏪ **Batalkan perubahan file terakhir** (kembali ke snapshot sebelum diedit). |
| **/diff** | 🔍 Tampilkan perbandingan baris kode yang baru saja dimodifikasi. |
| **/check** | 🩺 Jalankan compiler / linter untuk memastikan tidak ada syntax error. |
| **/tui** | 🖥️ Pindah seketika ke tampilan TUI visual (atau cukup ketik `tui` / `:tui`). |
| **/model** | 🔄 Ganti model AI yang sedang aktif (misal beralih ke DeepSeek atau GPT-4o). |
| **/skill** | 🎭 Aktifkan persona spesialis (Rust Expert, Reviewer, Debugger, dll.). |
| **/tokens** | 📊 Cek kapasitas memori percakapan dan konsumsi token saat ini. |
| **/compact** | 🧹 Ringkas riwayat obrolan panjang untuk menghemat pemakaian token. |
| **/profile** | 👤 Pengaturan profil, bahasa komunikasi, dan antarmuka default (TUI vs CLI). |
| **/clear** | 🧼 Bersihkan tampilan layar terminal. |
| **/exit** | 🚪 Keluar dari aplikasi. |

---

### 🎯 6. Persona Spesialis AI (`/skill`)

Kamu bisa mengarahkan fokus keahlian AI dengan mengetik `/skill <nama>`:

| Skill | Fokus & Keahlian Khusus |
| :--- | :--- |
| `rust-expert` | 🦀 Kode Rust idiomatik, zero-cost abstractions, konkurensi thread-safe, dan memori aman. |
| `code-reviewer` | 🔍 Audit kode untuk mencari celah bug, keamanan (OWASP), dan saran arsitektur bersih. |
| `web-frontend` | 🎨 Desain antarmuka web modern, responsif, dan aksesibel (HTML, Tailwind CSS, JS). |
| `api-architect` | 🏗️ Perancangan REST/GraphQL API, skema basis data, dan arsitektur otentikasi JWT/OAuth. |
| `debugger` | 🐞 Analisis mendalam terhadap stack trace error, memory leak, dan pelacakan akar masalah bug. |
| `test-engineer` | 🧪 Pembuatan unit test, integration test komprehensif, dan skenario pengujian TDD. |
| `refactor` | 🧹 Pembersihan kode kusut (*code smell*), penerapan SOLID & DRY tanpa merusak fungsionalitas. |

---

### 🧠 7. Penjelasan Lengkap Cara Kerja Agent (ReAct Loop)

Bagaimana `ctrl-cli` bisa berpikir dan bekerja secara otonom? Sistem menggunakan arsitektur **ReAct (Reasoning + Acting)** yang diperkaya dengan verifikasi kompilasi otomatis (*Self-Healing*).

```text
  ┌────────────────────────────────────────────────────────┐
  │                 Instruksi Pengguna                     │
  └───────────────────────────┬────────────────────────────┘
                              │
                              ▼
  ┌────────────────────────────────────────────────────────┐
  │ 1. OBSERVASI & KONTEKS (Read Files, History, Memory)   │
  └───────────────────────────┬────────────────────────────┘
                              │
                              ▼
  ┌────────────────────────────────────────────────────────┐
  │ 2. REASONING (Model menganalisis & merencanakan tugas) │
  └───────────────────────────┬────────────────────────────┘
                              │
                              ▼
  ┌────────────────────────────────────────────────────────┐
  │ 3. TOOL EXECUTION (Menulis/Mengedit file, Shell, Web)  │
  └───────────────────────────┬────────────────────────────┘
                              │
                              ▼
  ┌────────────────────────────────────────────────────────┐
  │ 4. SELF-HEALING CHECK (cargo check / compiler test)    │
  └─────────────┬────────────────────────────┬─────────────┘
                │ Ada Error Kompilasi        │ Sukses 100%
                ▼                            ▼
  ┌───────────────────────────┐  ┌─────────────────────────┐
  │ Perbaiki Kode Otomatis    │  │ Berikan Hasil Akhir     │
  │ (Ulangi ke Langkah 2)     │  │ ke Pengguna             │
  └───────────────────────────┘  └─────────────────────────┘
```

1. **Observasi**: Agent memeriksa lingkungan kerja (membaca file, struktur folder, atau catatan memori di `.ctrl/MEMORY.md`).
2. **Penalaran (Thought)**: Model menyusun rencana langkah demi langkah untuk menyelesaikan tugas.
3. **Aksi (Tool Call)**: Agent memanggil salah satu dari 15 perkakas bawaan (misal mengedit fungsi tertentu pada file).
4. **Validasi Mandiri (Self-Healing)**: Setelah memodifikasi kode, sistem secara otomatis menjalankan verifikasi compiler. Jika ada error kompilasi, error tersebut dikirim kembali ke agen untuk segera diperbaiki sebelum melapor selesai kepada pengguna!

---

### 🛠️ 8. Daftar Lengkap 15 Built-in Tools

`ctrl-cli` dibekali 15 perkakas (*tools*) otonom yang dapat dipanggil oleh AI sesuai kebutuhan:

| Perkakas (*Tool*) | Kegunaan & Spesifikasi | Tingkat Izin |
| :--- | :--- | :--- |
| **`read_file`** | Membaca isi file dengan rentang baris (*offset & limit*) agar hemat token. | Aman (Auto) |
| **`write_file`** | Membuat file baru secara atomik lengkap dengan pembuatan folder induk otomatis. | Menulis (Checkpoint) |
| **`edit_file`** | Mengganti blok teks secara presisi tinggi (*surgical replacement*) tanpa merusak baris lain. | Menulis (Checkpoint) |
| **`glob_files`** | Mencari lokasi file di seluruh proyek menggunakan pola pencocokan glob (misal `**/*.rs`). | Aman (Auto) |
| **`grep_files`** | Mencari kata kunci atau ekspresi reguler (regex) di seluruh isi file proyek. | Aman (Auto) |
| **`shell`** | Menjalankan perintah terminal sistem (PowerShell / Bash) dengan batas waktu (*timeout*). | Eksekusi (Konfirmasi) |
| **`web_search`** | Mencari solusi, dokumentasi, atau pustaka terbaru menggunakan DuckDuckGo. | Aman (Auto) |
| **`web_fetch`** | Mengambil isi halaman web dan mengubahnya menjadi format Markdown yang bersih. | Aman (Auto) |
| **`subagent`** | Meluncurkan subagent pekerja di latar belakang untuk menyelesaikan riset/tugas berat. | Eksekusi (Isolasi) |
| **`task_manage`** | Memantau status, membaca log, atau membatalkan subagent task yang berjalan. | Aman (Auto) |
| **`manage_memory`** | Membaca atau menyimpan catatan permanen proyek di `.ctrl/MEMORY.md`. | Menulis |
| **`mcp`** | Berkomunikasi dengan server eksternal melalui protokol *Model Context Protocol* (MCP). | Tergantung Server |
| **`self_heal`** | Menjalankan compiler/linter untuk menguji integritas sintaksis kode secara mandiri. | Aman (Auto) |
| **`create_checkpoint`** | Membuat cadangan snapshot manual dari file kerja. | Aman (Internal) |
| **`restore_checkpoint`**| Mengembalikan kondisi file ke snapshot cadangan tertentu (`/undo`). | Menulis (Rollback) |

---

### 👥 9. Konkurensi & Subagent Background Tasks

Saat kamu meminta tugas besar (seperti *"Riset dokumentasi library X dan buatkan arsitektur modulnya"*), `ctrl-cli` tidak membekukan terminal. Sistem memanfaatkan konkurensi murni Rust:

1. **Background Worker Threads**: Setiap subagent berjalan pada `std::thread` mandiri dengan sinkronisasi atomik aman (`Arc<RwLock<...>>`).
2. **Isolasi Log Penuh (`OutputSink::Buffered`)**: Output dari subagent disimpan ke dalam buffer memori terpisah, sehingga layar percakapan utamamu tidak akan tertimpa atau berantakan saat subagent sedang bekerja.
3. **Notifikasi Antar-Giliran (*Inter-turn Notifications*)**: Saat kamu sedang mengetik atau menyelesaikan satu prompt, sistem secara otomatis memberi notifikasi jika subagent di latar belakang telah menyelesaikan tugasnya.
4. **Monitoring Real-Time**: Status subagent dapat dipantau langsung lewat tombol **`F2`** pada mode TUI atau melalui halaman Web Dashboard.

---

### 🛡️ 10. Sistem Keamanan, Izin & Auto-Snapshot Sandbox

Keamanan kode sumber kamu adalah prioritas utama di `ctrl-cli`:

#### 1. Shadow Snapshot & Rollback Atomik (`/undo`)
* Setiap kali agent hendak memodifikasi file (melalui `write_file` atau `edit_file`), sistem terlebih dahulu membuat salinan snapshot di folder cadangan `.ctrl/snapshots/`.
* Snapshot mencakup hash checksum dan stempel waktu (*timestamp*).
* Jika kamu mengetik `/undo`, file target akan dipulihkan seketika ke keadaan sebelum diedit.
* Ketik `/diff` untuk melihat perbedaan baris berwarna secara presisi (*unified diff*).

#### 2. Tiga Tingkat Izin Keamanan (`/permissions`)
* **`Ask` (Mode Bawaan)**: Agent akan selalu meminta konfirmasi `[Y/n]` sebelum mengeksekusi perintah shell atau mengubah berkas penting.
* **`AutoApprove`**: Untuk developer yang ingin agent bekerja cepat tanpa henti (misal dalam skrip automasi CI/CD).
* **`ReadOnly`**: Mode audit aman 100%, agent hanya diizinkan membaca file dan dilarang mengubah apa pun.

---

### 🖥️ 11. Panduan Lengkap TUI Modern & Cara Mudah Menjalankan

Mode **TUI (*Terminal User Interface*)** pada `ctrl-cli` menghadirkan pengalaman visual modern berbasis pustaka [Ratatui](https://ratatui.rs/) dan [Crossterm](https://github.com/crossterm-rs/crossterm) dengan tema **Nord / Tokyo Night Dark** yang elegan, ringan, dan responsif.

#### 🚀 6 Cara Mudah Menjalankan Mode TUI:
1. **Script Satu Kata di Root Workspace (Paling Cepat & Instan)**:
   - **PowerShell**: `.\tui` atau `.\tui.ps1`
   - **Command Prompt (CMD)**: `tui`
   - **Linux / macOS / Git Bash**: `./tui.sh`
   *(Otomatis mengeksekusi biner release terkompilasi tanpa jeda compile).*
2. **Cargo Alias**: Cukup ketik `cargo tui` dari dalam folder `ctrl-cli/`.
3. **Short Flag CLI**: Jalankan `ctrl-cli -t` (atau `cargo run -- -t`).
4. **Jadikan Default Permanen**: Buka `/profile` di REPL -> pilih `🖥️ Pilih Mode Default` -> set ke `TUI`. Selanjutnya cukup jalankan `ctrl-cli` polos tanpa argumen apa pun!
5. **Environment Variable**: Set `$env:CTRL_TUI="1"` (PowerShell) atau `export CTRL_TUI=1` (Bash).
6. **Quick-Switch dari REPL**: Cukup ketik `tui` atau `:tui` di prompt percakapan REPL tanpa awalan slash.

#### 🎨 Fitur Visual Unggulan TUI:
* **Header Bar Pill Modern**: Inverted badge `⚡ CTRL-CLI`, pill status provider, model aktif, ikon spesialisasi persona, dan live animated thinking spinner (`⠋ ⠙ ⠹...`).
* **Card-Based Message Framing**: Pesan berbingkai rapi (`╭─ 👤 You`, `╭─ 🤖 Assistant`, `╭─ 💭 Thought Process`).
* **Native Syntax Highlighting**: Blok kode Markdown otomatis diberi warna kata kunci, penomoran baris, dan badge ikon bahasa (`🦀 Rust`, `🐍 Python`, `📘 TypeScript`, `🐚 Shell`, dll.) tanpa dependensi parser berat eksternal.
* **Multi-Line Tool Previews**: Cuplikan hasil eksekusi tool terstruktur (hingga 6 baris) dengan badge status `✔ SUKSES` / `✖ GAGAL` dan indikator lipatan baris `... (+N baris disembunyikan)`.
* **Scrollbar Visual**: Widget scrollbar vertikal Ratatui di sisi kanan jendela obrolan saat riwayat percakapan panjang.
* **Sidebar Tabbed (F1–F4)**:
  * **`F1` Help**: Panduan pintasan keyboard & perintah slash populer.
  * **`F2` Tasks**: Pemantauan background task dengan status badge (`● RUNNING`, `✔ DONE`, `✖ GAGAL`) dan log output bergaris vertikal.
  * **`F3` Skills**: Pilihan peran AI dengan penanda `★ [ACTIVE]`.
  * **`F4` Provider**: Ringkasan konfigurasi model dan endpoint URL.
* **Focus Glow & Placeholder**: Border fokus menyala saat input aktif, placeholder miring dinamis, prompt `❯ `, dan penghitung karakter di kanan bawah.

#### 🎮 Daftar Pintasan Keyboard (Keybindings):
* **`Tab` / `Shift + Tab`**: Berpindah fokus antar panel (**Input Teks** ↔ **Chat** ↔ **Sidebar**).
* **`F1` / `F2` / `F3` / `F4`**: Beralih langsung antar tab Sidebar (Help, Tasks, Skills, Provider).
* **`F5`**: 🔄 **Beralih kembali ke mode REPL konsol biasa** seketika tanpa memutus sesi.
* **`PageUp` / `PageDown`**: Menggulir riwayat pesan ke atas dan ke bawah secara halus.
* **`Esc`**: Membatalkan eksekusi yang sedang berlangsung / menutup popup autokomplet.
* **`Ctrl + L`**: Membersihkan layar obrolan.
* **`Ctrl + Q`** atau **`Ctrl + C`**: Keluar dari aplikasi.

---

### 🌐 12. Mode Web Dashboard & REST API

Selain mode terminal teks dan TUI fullscreen, `ctrl-cli` dilengkapi **Web Dashboard modern berbasis Single Page Application (SPA)** yang kaya fitur, responsif, dan **100% mandiri (zero external CDN, offline-ready)**. Dashboard ini didefinisikan pada file tunggal [`ctrl-cli/src/dashboard.html`](src/dashboard.html).

#### 🚀 Cara Menjalankan Web Dashboard:

1. **Jalankan Server Lokal via CLI:**
   ```bash
   ctrl-cli serve --port 3000
   ```
   Buka browser di **`http://127.0.0.1:3000`**.

2. **Akses Langsung via Berkas Lokal (`file:///`):**
   Kamu juga dapat membuka berkas [`ctrl-cli/src/dashboard.html`](src/dashboard.html) langsung dengan klik ganda atau menyeretnya ke peramban (Chrome, Edge, Firefox). Antarmuka secara otomatis mendeteksi protokol berkas dan berkomunikasi dengan backend via CORS di port 3000.

3. **Dynamic Hot-Reloading:**
   Server HTTP secara dinamis membaca `ctrl-cli/src/dashboard.html` langsung dari disk saat request masuk. Setiap kali kamu memodifikasi file HTML/CSS/JS, kamu cukup menekan `F5` di browser untuk melihat hasilnya seketika tanpa perlu kompilasi ulang biner Rust!

---

#### 🎨 6 Tab Utama Web Dashboard:

1. **📊 Ringkasan (Overview & Telemetri Real-Time):**
   * **Live Sparkline Canvas**: Grafik visual mini interaktif pemantauan memori (RAM RSS) dan utilisasi CPU yang dirender langsung via HTML5 Canvas murni tanpa library grafik eksternal.
   * **Kartu Telemetri Akurat**:
     * **Memory (RSS / Working Set)**: Penggunaan RAM fisik proses saat ini dan nilai puncak (*peak memory*).
     * **CPU Utilization**: Persentase utilisasi proses beserta rincian waktu komputasi Windows Kernel vs User mode (*user_ms* & *kernel_ms*).
     * **Active Worker Threads**: Jumlah worker threads aktif dan jumlah OS handles yang dikelola oleh proses.
     * **Active Tasks**: Jumlah subagent yang sedang berjalan vs total tugas selesai.
     * **Storage (.ctrl/ Footprint)**: Jejak ukuran penyimpanan direktori kerja internal `.ctrl/` (task logs, cache BM25, checkpoints) beserta jumlah file.
     * **Live Uptime & Jam Server**: Durasi waktu aktif server secara real-time (`Xs`, `Xm Ys`) dan timestamp sinkron.
   * **⚡ Peluncur Cepat (Quick Action)**: 4 tombol template tugas sekali klik (Analisis Arsitektur Proyek, Diagnostik Self-Healing, Pencarian Knowledge Base RAG, Inspeksi Status Git).
   * **🖥️ Info Sistem & Lingkungan**: Menampilkan path workspace aktif, sistem operasi & arsitektur CPU, serta model default yang digunakan.

2. **🚀 Task Runner & Subagent Manager:**
   * **Eksekusi Prompt Interaktif**: Input multi-line dengan pintasan `Ctrl+Enter` untuk submit.
   * **Spesialisasi Persona (/skill)**: Dropdown pilihan peran agen (misal: `general`, `researcher`, `security-auditor`, dll.) yang ditarik dinamis dari API.
   * **Model AI Override**: Kemudahan beralih model AI per tugas (Groq Llama 3.3, Ollama Qwen 2.5 Coder, OpenAI GPT-4o, Gemini Flash).
   * **Filter & Pencarian Antrean**: Memfilter daftar tugas berdasarkan status (`Semua`, `Running`, `Completed`, `Failed`) serta kotak pencarian teks ID / prompt.
   * **Terminal Log Inspector**: Tampilan log gelap bergaya terminal konsol dengan penyorotan warna otomatis (`✔ SUKSES`, `✖ GAGAL`, `INFO`). Dilengkapi tombol **Salin Log**, **Unduh Log (.txt)**, **Kunci Auto-scroll**, dan tombol **Batalkan Tugas (Cancel)** untuk tugas yang masih berjalan.

3. **🧠 Skills & Knowledge Base (BM25 RAG):**
   * **BM25 Search Playground**: Pengujian langsung mesin pencari relevansi Okapi BM25 terhadap dokumen internal proyek di `data/knowledge/` (seperti `company_policy.md`, `product_faqs.md`).
   * Dilengkapi slider `top_k` (1–10) dan tombol kueri instan (`security`, `architecture`, `self-healing`, `policy`).
   * Menampilkan kartu hasil terurut peringkat lengkap dengan nama file, judul sub-seksi, badge skor presisi BM25, dan potongan teks (snippet).
   * **Katalog Persona Skills**: Eksplorasi skill yang terpasang dengan deskripsi peran, daftar tools yang diizinkan, dan tombol peluncur instan.

4. **🛡️ Git & Checkpoints (/undo):**
   * **Monitor Status Git**: Menampilkan branch aktif serta 3 panel pemantauan perubahan: File Staged (`+`), File Unstaged (`~`), dan File Untracked (`?`).
   * **Riwayat Multi-File Checkpoint**: Daftar riwayat cadangan snapshot atomik dari direktori `.ctrl/checkpoints/` dengan timestamp dan berkas yang terdampak.
   * **Tombol Rollback (/undo)**: Tombol pemulihan per-checkpoint serta tombol master **Undo Perubahan Terakhir** lengkap dengan dialog konfirmasi aman.

5. **📋 Audit Trail:**
   * Membaca dan menampilkan rekaman append-only dari `.ctrl/audit.log` ke dalam tabel terstruktur.
   * Rincian kolom: Waktu kejadian, Nama Tool yang dipanggil, Status (`success` / `failed`), Durasi waktu eksekusi (ms), dan Parameter input JSON.
   * Filter pencarian cepat berdasarkan kata kunci nama perkakas atau status.

6. **⚡ Live Event Stream (SSE):**
   * Pemantau aliran event real-time berbasis Server-Sent Events (`/api/events`).
   * Menampilkan event `task_status`, `task_log`, dan `message` seketika saat subagent beroperasi di latar belakang.
   * Kontrol aliran: Tombol **Jeda / Lanjutkan Aliran** dan **Bersihkan Layar**.

---

#### 💡 Catatan Penting Mengenai Metrik Telemetri (`/api/metrics`):

* **Karakteristik Pure Rust (Zero-GC Efficiency):**
  Berbeda dengan aplikasi berbasis Node.js, Python, atau JVM yang terus-menerus melakukan alokasi berkala dan siklus *garbage collection* (yang membuat grafik RAM naik-turun berkala seperti gergaji bahkan saat idle), `ctrl-cli` ditulis dalam **Rust murni tanpa runtime berat**.
  - Saat dalam kondisi **Idle** (menunggu instruksi), penggunaan CPU berada di bawah `0.4%` dan memori stabil di kisaran `~9.2 MB` (RSS).
  - Nilai ini **100% akurat dan dibaca langsung dari Windows OS Process API** (`GetProcessMemoryInfo` & `GetProcessTimes`), bukan simulasi data acak.
  - Begitu tugas kompilasi atau LLM dijalankan, metrik dan grafik sparkline akan langsung meningkat dan merekam aktivitas secara proporsional.
* **Storage Melacak Direktori `.ctrl/`:**
  Kartu storage mengukur total ukuran data kerja internal pada folder `.ctrl/` (task logs, cache, snapshot), bukan sisa ruang kosong keseluruhan harddisk komputer.

---

#### 🔌 Daftar Lengkap REST & SSE Endpoints:

| Metode | Endpoint URL | Fungsi & Deskripsi |
| :--- | :--- | :--- |
| `GET` | `/` | Menyajikan antarmuka visual Web Dashboard ([`dashboard.html`](src/dashboard.html)). |
| `GET` | `/api/health` | Status kesehatan server HTTP dan timestamp terkini. |
| `GET` | `/api/metrics` | Data telemetri real-time: Memory (RSS/Peak/Virt), CPU (pct/ms), Threads, OS Handles, Storage. |
| `GET` | `/api/system/info` | Informasi sistem: Workspace root path, OS & arsitektur, dan default model. |
| `GET` | `/api/skills` | Mengambil daftar seluruh persona skill yang tersedia di proyek. |
| `POST` | `/api/knowledge/search` | Mencari dokumen relevan pada knowledge base menggunakan algoritma Okapi BM25. |
| `GET` | `/api/git/status` | Mengambil status repositori Git (branch aktif, staged, unstaged, untracked). |
| `GET` | `/api/checkpoints` | Mengambil riwayat snapshot cadangan multi-file dari direktori `.ctrl/checkpoints/`. |
| `POST` | `/api/checkpoints/rollback` | Melakukan pemulihan file kerja ke kondisi snapshot checkpoint tertentu (`/undo`). |
| `GET` | `/api/audit` | Membaca riwayat catatan audit keamanan dari `.ctrl/audit.log`. |
| `GET` | `/api/tasks` | Mengambil daftar seluruh tugas subagent beserta status dan log-nya. |
| `POST` | `/api/tasks` | Meluncurkan tugas subagent baru di latar belakang (menerima prompt, skill, model). |
| `DELETE` | `/api/tasks/:id` | Membatalkan / menghentikan eksekusi subagent yang sedang berjalan. |
| `GET` | `/api/events` | Aliran event Server-Sent Events (SSE) real-time untuk log dan status tugas. |

---

### ⚙️ 13. Konfigurasi Lanjutan & Profil Pengguna

#### 1. Profil Personalisasi (`~/.ctrl-cli/profile.json`)
Agar AI selalu menyesuaikan diri dengan gaya coding dan bahasa yang kamu inginkan, buat atau edit file konfigurasi profil di `~/.ctrl-cli/profile.json`:
```json
{
  "name": "Budi",
  "tech_stack": ["Rust", "Python", "TypeScript"],
  "response_language": "Bahasa Indonesia",
  "coding_style": "Tulis kode yang bersih, idiomatik, beri penjelasan singkat, dan sertakan unit test.",
  "default_ui": "tui"
}
```

#### 2. Memori Jangka Panjang Proyek (`.ctrl/MEMORY.md`)
Kamu bisa menuliskan aturan khusus proyek pada file `.ctrl/MEMORY.md` di folder proyekmu (misal: aturan arsitektur, panduan database, atau konvensi penamaan). Agent akan membaca berkas ini secara otomatis di setiap sesi percakapan.

#### 3. Integrasi MCP (Model Context Protocol)
`ctrl-cli` mendukung integrasi perkakas eksternal via MCP. Cukup definisikan server MCP pada berkas `.ctrl/mcp.json`:
```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_xxxx" }
    }
  }
}
```

---

### 📊 14. Metrik Kinerja & Efisiensi Sistem

Berdasarkan pengujian empiris performa (lihat dokumen lengkap [docs/PERFORMANCE.md](./docs/PERFORMANCE.md)):

| Parameter Metrik | `ctrl-cli` (Rust Murni) | Tool AI Berbasis Electron/Node |
| :--- | :--- | :--- |
| **Ukuran File Biner** | **~3.2 MB** (dengan TUI & Server) | ~150 MB – 300 MB |
| **Konsumsi RAM (Idle)** | **< 30 MB** | 200 MB – 800 MB |
| **Konsumsi RAM (Puncak)** | **< 60 MB** | 600 MB – 1.5 GB |
| **Waktu Startup Terminal** | **< 15 ms** | 1.2 s – 3.5 s |
| **Keamanan Memori** | **100% Memory-Safe** (Rust ownership) | Rawan garbage collection spikes |

---

### 📂 15. Struktur Arsitektur Kode

```text
code-agent-rust/
├── Cargo.toml                <-- Metadata paket & dependensi Rust (edisi 2021)
├── src/
│   ├── main.rs               <-- Titik masuk aplikasi, CLI routing, & arg parser
│   ├── types.rs              <-- Definisi tipe data pesan, tool call, & konfigurasi
│   ├── server.rs             <-- Server HTTP REST & SSE untuk Web Dashboard
│   ├── agent/                <-- Inti orkestrasi ReAct loop
│   │   ├── orchestrator.rs   <-- Manajemen siklus eksekusi agen utama
│   │   ├── provider.rs       <-- Klien HTTP ureq kompatibel OpenAI API
│   │   ├── subagent.rs       <-- Manajemen worker thread subagent
│   │   ├── checkpoint.rs     <-- Sistem auto-snapshot file & /undo
│   │   ├── permissions.rs    <-- Pintu izin akses (Ask/AutoApprove/ReadOnly)
│   │   └── memory.rs         <-- Manajemen memori persisten .ctrl/MEMORY.md
│   ├── tools/                <-- Implementasi 15 perkakas bawaan
│   │   ├── filesystem.rs     <-- Operasi berkas (read, write, edit, glob, grep)
│   │   ├── shell.rs          <-- Eksekutor terminal aman dengan timeout
│   │   ├── web.rs            <-- Integrasi DuckDuckGo & web fetcher
│   │   ├── self_heal.rs      <-- Evaluator compiler (cargo check, py, ts)
│   │   └── mcp.rs            <-- Klien Model Context Protocol
│   ├── tui/                  <-- Antarmuka terminal fullscreen Ratatui
│   │   ├── app.rs            <-- State machine dan state loop TUI
│   │   ├── highlight.rs      <-- Native syntax highlighter & language icons
│   │   └── ui.rs             <-- Render tata letak panel visual Nord/Tokyo Night
│   ├── telemetry/            <-- Pemantauan CPU, RAM, & resource sistem
│   └── bin/
│       └── ctrl-cli-tui.rs   <-- File biner mandiri mode TUI
├── tests/                    <-- 340+ unit test, integration test, & stress test
└── docs/                     <-- Dokumentasi teknis mendalam
```

---

### 📚 16. Pusat Dokumentasi Lengkap

Untuk panduan teknis mendalam, arsitektur, dan referensi tools lengkap, silakan kunjungi [**Master Documentation Hub (`docs/README.md`)**](./docs/README.md):

* 🗺️ [**Pusat Indeks Dokumentasi (`docs/README.md`)**](./docs/README.md)
* 🤖 [**Buku Panduan AI Agent (`docs/AI_AGENT_GUIDE.md`)**](./docs/AI_AGENT_GUIDE.md)
* 🏗️ [**Arsitektur Sistem & Concurrency (`docs/ARCHITECTURE.md`)**](./docs/ARCHITECTURE.md)
* 🛠️ [**Referensi 15 Built-in Tools (`docs/TOOLS_REFERENCE.md`)**](./docs/TOOLS_REFERENCE.md)
* 📊 [**Hasil Uji Performa Komprehensif (`docs/PERFORMANCE.md`)**](./docs/PERFORMANCE.md)
* ⚙️ [**Panduan Konfigurasi Lengkap (`docs/CONFIGURATION.md`)**](./docs/CONFIGURATION.md)
* 📝 [**Catatan Update & Rilis (`docs/UPDATE_NOTES.md`)**](./docs/UPDATE_NOTES.md)
* 📋 [**Backlog Pembersihan & Maintenance (`docs/CLEANUP_BACKLOG.md`)**](./docs/CLEANUP_BACKLOG.md)
* 🗺️ [**Roadmap Pengembangan Fitur (`docs/ROADMAP.md`)**](./docs/ROADMAP.md)
* 🏛️ [**Arsip Catatan Historis & Milestone (`docs/archive/`)**](./docs/archive/)

---

## 🇬🇧 English

### 💡 What is ctrl-cli?

`ctrl-cli` is an **ultra-lightweight (~3.2 MB binary, < 30 MB RAM)** autonomous AI coding assistant in your terminal, built with **100% pure Rust**. It can explore workspaces, surgically edit code, verify compiler errors (`cargo check`, python, tsc), and self-heal issues automatically.

#### 🚀 Quick Start (3 Minutes):
1. **Clone & Enter**:
   ```bash
   git clone https://github.com/gafirin5/code-agent-rust.git
   cd code-agent-rust
   ```
2. **Configure `.env`**:
   ```bash
   cp .env.example .env
   ```
   Choose either free **Groq** (`AI_BASE_URL=https://api.groq.com/openai/v1`), offline **Ollama** (`AI_BASE_URL=http://localhost:11434/v1`), or **OpenAI / DeepSeek**.
3. **Run**:
   ```bash
   # Modern visual TUI:
   .\tui       # PowerShell
   tui         # CMD
   ./tui.sh    # Bash
   # or inside ctrl-cli: cargo tui

   # Classic line REPL:
   cargo run
   ```

#### 🎮 4 Operational Modes:
* **Fullscreen Modern TUI**: `.\tui` / `cargo tui` / `ctrl-cli -t` (or type `tui` in REPL).
* **Interactive REPL**: `cargo run` (chat with `/help`, `/undo`, `/diff`, `/model`, `/skill`).
* **Web Dashboard**: `ctrl-cli serve` or `cargo run -- serve` (visit `http://127.0.0.1:3000` or open `src/dashboard.html` directly in browser).
* **One-Shot Command**: `cargo run -- generate "prompt..."`.

For comprehensive technical architecture, tools reference, and benchmarks, check the [Technical Docs](./docs/README.md).

---

## 🇨🇳 中文

### 💡 什么是 ctrl-cli？

**`ctrl-cli`** 是完全采用 **纯 Rust 编写** 的极轻量终端 AI 编程助手与自主智能体（体积仅 ~3.2 MB，内存占用 < 30 MB）。具备文件精准手术式编辑、编译器错误自愈修复（Self-Healing）、自动快照撤销（`/undo`）以及多子任务并发处理能力。

#### ⚡ 3 步极速上手：
1. **克隆代码**：
   ```bash
   git clone https://github.com/gafirin5/code-agent-rust.git
   cd code-agent-rust
   ```
2. **配置密钥**：复制 `.env.example` 为 `.env`，填入 Groq（免费极速）、本地 Ollama（100% 离线隐私）或 OpenAI/DeepSeek 密钥。
3. **启动运行**：
   ```bash
   # 启动现代 TUI 界面：
   .\tui        # PowerShell
   tui          # CMD
   cargo tui    # 在 ctrl-cli 目录

   # 启动经典命令行交互：
   cargo run
   ```

详细技术架构设计与 15+ 工具参数规范请参阅 [完整文档目录](./docs/README.md)。

---

<div align="center">

## 📄 Lisensi / License

MIT License — Bebas digunakan, dipelajari, dan dimodifikasi.

*Dibuat dengan ❤️ dan 🦀 oleh [galangfjr](https://github.com/gafirin5)*

</div>
