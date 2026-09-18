# 📚 Dokumentasi ctrl-cli

Selamat datang di pusat dokumentasi resmi **ctrl-cli** — Ultra-lightweight AI Coding Agent CLI yang dibangun secara modular dengan Rust murni.

Direktori ini dirancang khusus sebagai panduan menyeluruh bagi pengguna, pengembang, dan **AI Agent** yang berkontribusi atau beroperasi di dalam repositori ini.

---

## 🗺️ Master Index & Hub Navigasi Dokumentasi

Dokumentasi proyek dikelompokkan secara terstruktur ke dalam empat kategori utama berikut:

### 1. 🤖 AI Agent Operational Rules & Handbook
Aturan kerja mendasar, batas arsitektur, dan panduan teknis operasional bagi asisten AI yang beroperasi dalam repositori ini.

| Dokumen | Deskripsi | Target Pembaca |
|---------|-----------|----------------|
| [**Workspace Rules (`../../AGENTS.md`)**](../../AGENTS.md) | **Aturan Otoritatif Workspace**: Pedoman Web UI Zero-CDN, kontrak data telemetri, dual-tier README, dan batas ukuran biner (< 10 MB). | AI Agent & Maintainer |
| [**Buku Panduan AI Agent (`AI_AGENT_GUIDE.md`)**](./AI_AGENT_GUIDE.md) | Mental model arsitektur, pola orkestrasi subagent, cara menambah tool/skill, protokol verifikasi 3-tahap, dan mitigasi dead-ends. | **AI Coding Agent** & Kontributor |

---

### 2. 🏗️ Architecture & Technical Specs
Spesifikasi teknis, arsitektur konkurensi, metrik benchmark, referensi tools, dan konfigurasi sistem.

| Dokumen | Deskripsi | Target Pembaca |
|---------|-----------|----------------|
| [**Arsitektur Sistem (`ARCHITECTURE.md`)**](./ARCHITECTURE.md) | Desain teknis: pure Rust threading, lifecycle task engine, isolasi log (`TaskLogBuffer`), dan notifikasi inter-turn. | Kontributor, Arsitek, AI Agent |
| [**Referensi Tool Lengkap (`TOOLS_REFERENCE.md`)**](./TOOLS_REFERENCE.md) | Spesifikasi parameter, skema output, mode izin keamanan, dan contoh eksekusi dari 15 built-in tools. | AI Agent & Developer |
| [**Hasil Uji Performa (`PERFORMANCE.md`)**](./PERFORMANCE.md) | Metrik efisiensi Zero-GC, footprint RAM (~9 MB idle), waktu startup (< 15 ms), throughput eksekusi, dan telemetri OS. | Pengembang, Arsitek, DevOps |
| [**Catatan Update & Rilis (`UPDATE_NOTES.md`)**](./UPDATE_NOTES.md) | Catatan rilis, riwayat pembaruan, status milestone M1-M4, perbaikan bug, dan log evolusi sistem. | Pengguna, Pengembang, AI Agent |
| [**Panduan Konfigurasi (`CONFIGURATION.md`)**](./CONFIGURATION.md) | Panduan setup `.env`, model provider (Groq, Ollama, OpenAI, DeepSeek), MCP server (`.ctrl/mcp.json`), dan permission mode. | Pengguna, DevOps, AI Agent |
| [**Roadmap Fitur Masa Depan (`ROADMAP.md`)**](./ROADMAP.md) | Rencana pengembangan fitur selanjutnya, estimasi dampak/kompleksitas, dan backlog arsitektur. | Pengguna, Pengembang, AI Agent |

---

### 3. 📋 Maintenance & Backlog
Pelacakan utang teknis, dekomposisi modularitas, dan pemeliharaan kesehatan repositori.

| Dokumen | Deskripsi | Target Pembaca |
|---------|-----------|----------------|
| [**Backlog Pembersihan (`CLEANUP_BACKLOG.md`)**](./CLEANUP_BACKLOG.md) | Rencana dan status pembersihan repositori: dekomposisi `main.rs` (Opsi 1), pembersihan direktori (Opsi 2), disk & git hygiene. | Maintainer & AI Agent |

---

### 4. 🏛️ Historical Reports & Milestone Logs (`archive/`)
Dokumen arsip historis perancangan awal, analisis kegagalan masa lalu, dan laporan kesiapan pengujian milestone terdahulu.

| Dokumen | Deskripsi | Target Pembaca |
|---------|-----------|----------------|
| [**Arsitektur Milestone M1-M4 (`archive/PROJECT.md`)**](./archive/PROJECT.md) | Catatan arsitektur awal dan inventarisasi 22 fitur milestone M1-M4 modernization. | Pengembang & AI Agent |
| [**Catatan Kegagalan Konkurensi (`archive/DEAD_ENDS.md`)**](./archive/DEAD_ENDS.md) | Log investigasi jebakan thread sleep dan narrow wall-clock assertions di Windows. | Arsitek & AI Agent |
| [**Spesifikasi Test 4-Tier (`archive/TEST_INFRA.md`)**](./archive/TEST_INFRA.md) | Spesifikasi arsitektur pengujian 4-tier hermetik 100% offline. | Test Engineers & AI Agent |
| [**Laporan Kesiapan Tes R1-R6 (`archive/TEST_READY.md`)**](./archive/TEST_READY.md) | Laporan verifikasi kesiapan suite pengujian fitur generasi berikutnya (R1-R6). | Test Engineers & AI Agent |
| [**Spesifikasi Kebutuhan Awal (`archive/ORIGINAL_REQUEST.md`)**](./archive/ORIGINAL_REQUEST.md) | Dokumen requirements awal pengembangan coding agent Rust. | Arsitek & Pengembang |

---

## 🚀 Ringkasan Singkat Proyek

- **Bahasa & Runtime**: Rust murni (edisi 2021) tanpa runtime async (`tokio`), memanfaatkan `std::thread`, sinkronisasi atomik (`Arc`, `RwLock`, `Condvar`, `AtomicBool`), dan HTTP blocking `ureq 2.10`.
- **Mode Eksekusi**:
  - **REPL CLI**: Terminal interaktif dengan autocompletion `/`, syntax coloring, dan inter-turn completion notifications.
  - **TUI Mode**: Terminal User Interface berbasis Ratatui & Crossterm dengan panel responsif.
  - **Web Dashboard**: Dashboard SPA modern 6-tab berbasis Web REST + SSE tersemat (Zero-CDN, offline 100%).
  - **Generate Mode**: Eksekusi instan satu perintah dari shell terminal (`ctrl-cli generate "buat script..."`).
- **Kapabilitas Multi-Subagent**: Delegasi sub-tugas secara paralel dan non-blocking di background thread dengan isolasi log penuh (`OutputSink::Buffered`).

---

## 💡 Petunjuk untuk AI Agent
Jika Anda adalah AI Coding Assistant yang sedang mengoperasikan atau memperbarui repositori ini:
1. Baca terlebih dahulu [AGENTS.md](../../AGENTS.md) dan [AI_AGENT_GUIDE.md](./AI_AGENT_GUIDE.md) untuk memahami batasan konkurensi dan aturan workspace.
2. Hindari dependensi `async`/`tokio` baru kecuali diminta secara eksplisit.
3. Selalu validasi perubahan dengan `cargo clippy --bin ctrl-cli -- -D warnings` dan `cargo test --bin ctrl-cli`.
