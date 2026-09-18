# 📋 Backlog & Rencana Pembersihan Proyek (Project Tidy-Up Backlog)

Dokumen ini mencatat ide, rekomendasi, dan rencana pembersihan/pemeliharaan (*tidy-up & technical debt cleanup*) untuk proyek **`ctrl-cli` (`code-agent-rust`)**.

---

## 📌 Status Ringkasan Opsi Pembersihan

| Opsi | Fokus Area | Status | Prioritas |
|---|---|:---:|:---:|
| **Opsi 1** | **Refaktorisasi `main.rs` (Dekomposisi "God File" 3.600+ baris)** |  **SELESAI (Completed)** | P0 |
| **Opsi 2** | **Directory Cleanup & Sinkronisasi Dokumentasi (Dual README & Top-Level Docs)** | ✅ **SELESAI (Completed)** | P1 |
| **Opsi 3** | **Disk Hygiene & Manajemen Artefak Sementara (Target, Cache, & `.ctrl/`)** | ⏳ **Backlog (Ready for Next Sprint)** | P2 |
| **Opsi 4** | **Git Hygiene & Version Control Setup (Inisialisasi Git & Audit `.gitignore`)** | ⏳ **Backlog (Ready for Next Sprint)** | P1 |

---

## ✅ Opsi 1: Refaktorisasi `main.rs` (SELESAI)

### 1.1 Masalah Sebelumnya
Berkas [`ctrl-cli/src/main.rs`](../src/main.rs) sebelumnya membengkak hingga **3.636 baris (~160 KB)** yang mencakup:
- Definisi parser CLI Clap
- Manajemen profil pengguna & preferensi bahasa
- Adaptor skill persona agen
- Model context tracking & session token tracker
- Formatting badge token, textwrap, auto-writer filename detection
- Slash commands specs & autocompleter
- Dispatcher slash commands (1.600+ baris)
- Loop interaktif REPL & runner kode
- Rangkaian unit test internal

### 1.2 Implementasi Pemecahan Modul
Struktur kini telah didekomposisi secara modular, bersih, dan idiomatik:
1. **[`ctrl-cli/src/cli.rs`](../src/cli.rs)**: Parser argumen CLI Clap (`Cli`, `Commands`).
2. **[`ctrl-cli/src/profile.rs`](../src/profile.rs)**: Manajemen profil pengembang (`UserProfile`, `SupportedLanguage`, load/save).
3. **[`ctrl-cli/src/skill.rs`](../src/skill.rs)**: Struktur persona agen AI (`Skill`, `get_available_skills`).
4. **[`ctrl-cli/src/context.rs`](../src/context.rs)**: Estimasi limit konteks & akumulasi token (`ModelContextInfo`, `SessionTokenTracker`).
5. **[`ctrl-cli/src/formatter.rs`](../src/formatter.rs)**: Formatting output terminal, badges token, ANSI helpers, auto-save detection, dan system prompt builder.
6. **[`ctrl-cli/src/repl/`](../src/repl/)**:
   - `commands.rs`: Definisi `CommandSpec`, tabel `COMMAND_SPECS`, dan fungsi autocorrect `resolve_slash_command`.
   - `completer.rs`: Autocomplete engine `SlashCompleter` berbasis `inquire`.
   - `slash.rs`: Dispatcher penanganan perintah slash `handle_slash_command` dan menu interaktif `/`.
   - `runner.rs`: Loop utama terminal `start_repl` dan eksekutor instan `handle_generate`.
   - `mod.rs`: Re-export publik modular.
7. **[`ctrl-cli/src/telemetry/`](../src/telemetry/)**: Didefinisikan secara mandiri di `src/server.rs` dan diekspos di root `src/main.rs` (`pub use server::telemetry;`) untuk menjaga kompatibilitas 100% dengan seluruh integrasi test suite dan bebas dari peringatan `clippy::duplicate_mod`.
8. **[`ctrl-cli/src/main.rs`](../src/main.rs)**: Berkas titik masuk ramping (~180 baris logika utama) yang bersih dan terisolasi.

---

## ✅ Opsi 2: Directory Cleanup & Sinkronisasi Dokumentasi (SELESAI)

### 2.1 Masalah Sebelumnya
- Terdapat aturan di [`AGENTS.md`](../../AGENTS.md) mengenai **Dual-Tier README** (root `README.md` untuk gambaran umum repositori, dan `ctrl-cli/README.md` untuk paket Crates.io). Keduanya perlu selalu diaudit dan disinkronkan terhadap fitur terbaru (seperti flag `-t` / `--tui`, subcommand `serve`, streaming SSE, `/undo`, dan 15 built-in tool).
- Di root repositori terdapat berkas-berkas catatan ad-hoc (`TEST_READY.md`, `TEST_INFRA.md`, `DEAD_ENDS.md`, `ORIGINAL_REQUEST.md`, dan `PROJECT.md`) yang mengacaukan kerapian root direktori dan menimbulkan duplikasi referensi.
- Dokumentasi antara direktori root `docs/` dan `ctrl-cli/docs/` belum tersinkronisasi penuh (`PERFORMANCE.md` hanya ada di `ctrl-cli/docs/`, sedangkan `CLEANUP_BACKLOG.md` hanya ada di `docs/`).

### 2.2 Implementasi & Hasil Pembersihan
1. **Konsolidasi Root Documents ke `docs/archive/`**:
   - Berkas ad-hoc (`PROJECT.md`, `DEAD_ENDS.md`, `TEST_INFRA.md`, `TEST_READY.md`, `ORIGINAL_REQUEST.md`) telah dipindahkan dari root direktori ke `docs/archive/` dan disinkronkan ke `ctrl-cli/docs/archive/`.
   - Root repositori kini bersih dan terstandarisasi, hanya menyisakan berkas otoritatif: `AGENTS.md`, `README.md`, `.gitignore`, dan skrip peluncur launcher (`tui.ps1`, `tui.cmd`, `tui.sh`).
2. **Master Index & Documentation Hub (`docs/README.md`)**:
   - Berkas [`docs/README.md`](../../docs/README.md) dan [`ctrl-cli/docs/README.md`](./README.md) diperbarui sebagai katalog navigasi terpadu yang memetakan seluruh dokumentasi ke dalam 4 kategori hierarkis:
     - *AI Agent Operational Rules & Handbook* (`AGENTS.md`, `AI_AGENT_GUIDE.md`)
     - *Architecture & Technical Specs* (`ARCHITECTURE.md`, `TOOLS_REFERENCE.md`, `PERFORMANCE.md`, `UPDATE_NOTES.md`, `CONFIGURATION.md`, `ROADMAP.md`)
     - *Maintenance & Backlog* (`CLEANUP_BACKLOG.md`)
     - *Historical Reports & Milestone Logs* (`archive/PROJECT.md`, `archive/DEAD_ENDS.md`, `archive/TEST_INFRA.md`, `archive/TEST_READY.md`, `archive/ORIGINAL_REQUEST.md`)
3. **Audit Tautan Markdown & Sinkronisasi Docs**:
   - Semua relative path di `AGENTS.md`, root `README.md`, `ctrl-cli/README.md`, `AI_AGENT_GUIDE.md`, dan pengujian telah diaudit untuk memastikan nol pranala rusak (*zero broken links*).
   - Seluruh berkas dokumentasi antara `docs/` dan `ctrl-cli/docs/` (termasuk `PERFORMANCE.md` dan `CLEANUP_BACKLOG.md`) telah disinkronkan 100%.
4. **Verifikasi Kualitas**:
   - 100% tes unit lulus (`cargo test --bin ctrl-cli` -> 188 passed).
   - Linter `cargo clippy --bin ctrl-cli -- -D warnings` bersih tanpa error maupun peringatan.

---

## ⏳ Opsi 3: Disk Hygiene & Manajemen Artefak Sementara

### 3.1 Latar Belakang & Masalah
- Proses kompilasi Rust (`cargo build`, `cargo test`) menghasilkan direktori `target/` yang dapat membengkak hingga puluhan gigabyte jika artefak debug dan incremental build tidak dikelola.
- Eksekusi background task agen menghasilkan direktori log dan snapshot di `.ctrl/` (`.ctrl/tasks/`, `.ctrl/checkpoints/`, `.ctrl/audit.log`).
- Direktori `.system_generated/` dan file temporer pengujian perlu mekanisme pembersihan berkala (*pruning*) agar tidak membebani kapasitas disk pengguna.
- Terkadang proses `ctrl-cli.exe` yang berjalan di background (misal daemon HTTP server) memegang file lock di Windows sehingga menyebabkan *Access Denied (OS error 5)* saat build berikutnya dijalankan.

### 3.2 Rencana Aksi
1. **Script Disk Hygiene Otomatis**:
   - Sediakan script pembersih (`scripts/clean_artifacts.ps1` atau `scripts/clean_artifacts.sh`) untuk membersihkan file lock zombie `ctrl-cli.exe` dan direktori temporer dengan aman.
   - Sediakan perintah pembersihan terarah (misal `cargo clean -p ctrl-cli` alih-alih `cargo clean` menyeluruh) untuk menghemat waktu kompilasi ulang dependensi eksternal.
2. **Pruning Otomatis pada `.ctrl/`**:
   - Implementasikan opsi atau flag retensi log (misalnya membatasi ukuran riwayat checkpoint hingga N snapshot terakhir atau rotasi log `.ctrl/audit.log` jika melebihi ukuran tertentu).

---

## ⏳ Opsi 4: Git Hygiene & Version Control Setup

### 4.1 Latar Belakang & Masalah
- Repositori lokal saat ini belum memiliki repositori Git aktif (`.git` belum diinisialisasi atau berada di luar folder kerja saat ini).
- Belum ada riwayat commit terstruktur untuk melacak evolusi milestone M1-M4 dan fitur next-gen R1-R6.
- Berkas `.gitignore` saat ini perlu diaudit untuk memastikan bahwa file kredensial sensitif (`.env`, `profile.json` dengan API keys), folder cache `.ctrl/`, serta artefak build biner `target/` tidak akan pernah terkomit secara tidak sengaja.

### 4.2 Rencana Aksi
1. **Audit & Penyempurnaan `.gitignore`**:
   - Pastikan entri berikut ada di `.gitignore`:
     ```gitignore
     /target/
     **/*.rs.bk
     .env
     .env.*
     !.env.example
     .ctrl/
     .system_generated/
     *.log
     *.exe
     *.pdb
     ```
2. **Inisialisasi Git & Baseline Commit**:
   - Jalankan `git init`.
   - Buat branch default `main`.
   - Buat initial commit yang bersih dan deskriptif untuk rilis `v0.3.0`.
3. **Dokumentasi Kontribusi & Branching Model**:
   - Tambahkan pedoman branch (`feature/...`, `bugfix/...`) dan konvensi commit (Conventional Commits) di `CONTRIBUTING.md` atau `AGENTS.md`.

---

*Dokumen ini diperbarui secara otomatis setelah penyelesaian Opsi 1 dan Opsi 2 pada 2026-09-18.*
