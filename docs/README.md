# 📚 Dokumentasi ctrl-cli

Selamat datang di pusat dokumentasi resmi **ctrl-cli** — Ultra-lightweight AI Coding Agent CLI yang dibangun secara modular dengan Rust murni.

Direktori ini dirancang khusus sebagai panduan menyeluruh bagi pengguna, pengembang, dan **AI Agent** yang berkontribusi atau beroperasi di dalam repositori ini.

---

## 🗺️ Peta Navigasi Dokumen

| Dokumen | Deskripsi | Target Pembaca |
|---------|-----------|----------------|
| [**Catatan Update (`UPDATE_NOTES.md`)**](./UPDATE_NOTES.md) | Catatan rilis, riwayat pembaruan, status milestone, perbaikan bug, dan log evolusi sistem. | Pengguna, Pengembang, AI Agent |
| [**Roadmap Fitur Masa Depan (`ROADMAP.md`)**](./ROADMAP.md) | Rencana pengembangan fitur selanjutnya, estimasi dampak/kompleksitas, dan backlog arsitektur. | Pengguna, Pengembang, AI Agent |
| [**Panduan Konfigurasi (`CONFIGURATION.md`)**](./CONFIGURATION.md) | Panduan setup `.env`, model provider, MCP server (`.ctrl/mcp.json`), profil pengguna (`profile.json`), dan permission mode. | Pengguna, DevOps, AI Agent |
| [**Arsitektur Sistem (`ARCHITECTURE.md`)**](./ARCHITECTURE.md) | Desain teknis: pure Rust threading, lifecycle task engine, isolasi log (`TaskLogBuffer`), dan notifikasi inter-turn. | Kontributor, Arsitek, AI Agent |
| [**Buku Panduan AI Agent (`AI_AGENT_GUIDE.md`)**](./AI_AGENT_GUIDE.md) | Invariant arsitektur, pola orkestrasi subagent, aturan modifikasi kode, cara menambah tool/skill, dan pencegahan dead ends. | **AI Coding Agent** & Maintainer |
| [**Referensi Tool Lengkap (`TOOLS_REFERENCE.md`)**](./TOOLS_REFERENCE.md) | Spesifikasi parameter, skema output, mode izin keamanan, dan contoh eksekusi dari 15+ built-in tools. | AI Agent & Developer |

---

## 🚀 Ringkasan Singkat Proyek

- **Bahasa & Runtime**: Rust murni (edisi 2021) tanpa runtime async (`tokio`), memanfaatkan `std::thread`, sinkronisasi atomik (`Arc`, `RwLock`, `Condvar`, `AtomicBool`), dan HTTP blocking `ureq 2.10`.
- **Mode Eksekusi**:
  - **REPL CLI**: Terminal interaktif dengan autocompletion `/`, syntax coloring, dan inter-turn completion notifications.
  - **TUI Mode**: Terminal User Interface berbasis Ratatui & Crossterm dengan panel responsif.
  - **Generate Mode**: Eksekusi instan satu perintah dari shell terminal (`ctrl-cli generate "buat script..."`).
- **Kapabilitas Multi-Subagent**: Delegasi sub-tugas secara paralel dan non-blocking di background thread dengan isolasi log penuh (`OutputSink::Buffered`).

---

## 💡 Petunjuk untuk AI Agent
Jika Anda adalah AI Coding Assistant yang sedang mengoperasikan atau memperbarui repositori ini:
1. Baca terlebih dahulu [AI_AGENT_GUIDE.md](./AI_AGENT_GUIDE.md) dan [ARCHITECTURE.md](./ARCHITECTURE.md) untuk memahami batasan konkurensi dan aturan thread safety.
2. Hindari dependensi `async`/`tokio` baru kecuali diminta secara eksplisit.
3. Selalu validasi perubahan dengan `cargo clippy --all-targets -- -D warnings` dan `cargo test`.
