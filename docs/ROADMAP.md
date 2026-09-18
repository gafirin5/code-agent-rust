# 🗺️ Roadmap & Kemungkinan Pembaruan Fitur (Future Feature Updates)

Dokumen ini memetakan rencana pengembangan strategis, potensi fitur baru, peningkatan arsitektur, dan backlog ide untuk **`ctrl-cli`** (`code-agent-rust`).

---

## 🎯 Prinsip Pengembangan (Core Principles)

Setiap pembaruan fitur pada `ctrl-cli` harus berpegang teguh pada prinsip arsitektur inti:
1. **Pure Rust & Zero-Bloat**: Menjaga efisiensi biner (~1.8 MB) dan performa tinggi tanpa memperkenalkan runtime asinkron yang berat (`tokio`) kecuali benar-benar diperlukan.
2. **Thread-Safe & Deadlock-Free**: Mengutamakan primitif konkurensi standar (`std::sync`, `Condvar`, `RwLock`, `AtomicBool`) dengan alur komunikasi berbasis pesan (*message passing*).
3. **Isolasi Output**: Menjaga kebersihan terminal utama agar proses latar belakang (*background subagents*) tidak pernah mencemari interaksi pengguna.
4. **Offline First & High Testability**: Fitur baru harus dapat diuji secara mandiri (*offline mock test harness*) dengan standar nol peringatan linter (`clippy -D warnings`).

---

## 🧭 Matriks Prioritas & Kategori Pembaruan

```
Tinggi ▲
       │  [Fase 1: Versioning & TUI]       [Fase 2: Streaming LLM]
       │  [Fase 1: Rustyline 15.0]         [Fase 4: Task Persistence]
Dampak │
       │  [Fase 3: Native Gemini/Ollama]   [Fase 4: DAG Task Dependencies]
       │  [Fase 5: Skills & RAG BM25]      [Fase 6: Embedded Web Dashboard]
Rendah │─────────────────────────────────────────────────────────────►
       Rendah                          Kompleksitas                  Tinggi
```

---

## 📌 Rincian Rencana Pembaruan Berdasarkan Fase

### 🚀 Fase 1: Fondasi, Dependensi & Pemolesan TUI *(Prioritas Cepat)*
Fokus pada kebersihan repositori, pembaruan versi pustaka pihak ketiga, dan aktivasi penuh antarmuka terminal visual.

- [ ] **Sinkronisasi Metadata Versi (`v0.3.0`)**:
  - Memperbarui `version` di `Cargo.toml` menjadi `0.3.0`.
  - Memperbarui badge pengujian di `README.md` dari `168+` menjadi `200+ Passing`.
  - Melakukan staging dan commit rapi untuk 28 file termodifikasi serta modul baru `src/tui/`.
- [ ] **Modernisasi Dependensi REPL (`rustyline` 11.0 ➔ 15.0)**:
  - Mengadopsi parser escape sequence modern untuk Windows Terminal (ConPTY).
  - Peningkatan penanganan prompt multiline dan autocompletion slash commands yang lebih mulus.
- [ ] **Aktivasi Penuh Mode TUI Ratatui**:
  - Mengintegrasikan switch runtime antara mode REPL dan mode TUI secara dinamis.
  - Menghubungkan visualisasi real-time status background task ke tab `Tasks (F2)` di TUI.

---

### ⚡ Fase 2: Responsivitas & Streaming LLM
Fokus pada peningkatan *User Experience* (UX) agar pengguna tidak perlu menunggu seluruh balasan JSON selesai di-generate.

- [ ] **Streaming Token Response (SSE / Chunked Output)**:
  - Membaca stream respons token kata-demi-kata dari endpoint `chat/completions` menggunakan `ureq::Response::into_reader()`.
  - Menampilkan teks token secara real-time di terminal REPL dan panel TUI secara simultan.
  - Memastikan pembatalan kooperatif (`CancellationToken`) dapat menghentikan streaming di tengah jalan tanpa menggantung koneksi soket.
- [ ] **Terminal Syntax Highlighting**:
  - Menambahkan pewarnaan syntax untuk blok kode (Rust, Python, JS, Markdown, JSON) langsung di terminal REPL saat agen menampilkan solusi.
- [ ] **Terminal Completion Bell / Sound Notification (Opsional)**:
  - Opsi konfigurasi `.env` (`ALERT_ON_TASK_DONE=true`) untuk membunyikan bell terminal (`\x07`) saat background task berdurasi panjang selesai.

---

### 🌐 Fase 3: Multi-Provider & Model Lokal Ekstensif
Fokus pada kebebasan pengguna memilih model AI tanpa bergantung pada proxy perantara.

- [ ] **Native Google Gemini Provider**:
  - Integrasi langsung ke Google AI Studio (`generativelanguage.googleapis.com`) via REST API protokol v1beta.
  - Dukungan otomatis untuk model `gemini-2.5-pro`, `gemini-2.5-flash`, dan varian *thinking*.
- [ ] **Native Ollama / Local Model Integration**:
  - Dukungan out-of-the-box untuk Ollama di `http://localhost:11434` tanpa konfigurasi API key.
  - Probing model lokal otomatis (`/api/tags`) untuk mengenali daftar model yang sudah terpasang di mesin pengguna.
- [ ] **Anthropic Prompt Caching Support**:
  - Implementasi header `anthropic-beta: prompt-caching-2024-07-31` untuk menghemat latensi dan biaya token pada konteks codebase besar.

---

### 🧩 Fase 4: Orkestrasi Subagent Lanjutan & Persistensi
Fokus pada peningkatan kapabilitas manajemen tugas latar belakang untuk alur kerja yang kompleks.

- [ ] **Persistensi State Task (`.ctrl/tasks.jsonl` / SQLite)**:
  - Menyimpan snapshot `TaskRecord` dan ringkasan log ke penyimpanan lokal.
  - Riwayat eksekusi subagent tidak hilang meskipun terminal ditutup atau komputer di-restart.
  - Perintah baru: `/tasks history` dan `/tasks resume <id>`.
- [ ] **Task Dependencies & Workflow Chaining (DAG Execution)**:
  - Memungkinkan subagent atau user menjadwalkan task berurutan: `Task B` hanya berjalan otomatis setelah `Task A` berstatus `Completed`.
  - Mendukung eksekusi paralel berbasis dependensi (misal: Task A & B paralel, lalu Task C menggabungkan hasil keduanya).
- [ ] **Scheduled / Periodic Background Tasks**:
  - Penjadwalan tugas berulang (*cron-like*) untuk memantau status build, memeriksa error log secara berkala, atau menjalankan unit test otomatis saat file berubah (*watch mode*).
- [ ] **Export Hasil Task**:
  - Perintah `/tasks export <id> --format [markdown|json|html]` untuk mengekspor laporan investigasi subagent menjadi berkas laporan siap baca.

---

### 🧠 Fase 5: Ekosistem Skill, Persona & Knowledge Base (RAG)
Fokus pada kecerdasan domain spesifik dan basis pengetahuan proyek.

- [ ] **Dynamic Skill Auto-Discovery**:
  - Membaca dan mendaftarkan skill baru secara otomatis dari folder `skills/*/SKILL.md` tanpa perlu mendefinisikannya secara manual di kode biner.
- [ ] **Penambahan Persona Bawaan (Built-in Skills)**:
  - `qa-tester`: Spesialis membuat unit test, integration test, dan edge-case stress tests.
  - `code-reviewer`: Spesialis memeriksa performa, memory leaks, dan standardisasi kode.
  - `devops-architect`: Spesialis Dockerfile, CI/CD GitHub Actions, dan cross-compilation.
- [ ] **Lightweight Vector / BM25 Knowledge Retrieval**:
  - Implementasi pencarian teks kontekstual (BM25 / fuzzy index) pada berkas di `data/knowledge/` agar agen dapat merujuk SOP perusahaan atau FAQ produk secara relevan dan cepat.

---

### 🖥️ Fase 6: Antarmuka Web & Visual Dashboard
Fokus pada integrasi visual antara CLI dan antarmuka browser.

- [ ] **Embedded Web Server untuk Visual Dashboard**:
  - Memanfaatkan berkas visual dashboard mandiri di [`src/dashboard.html`](../src/dashboard.html) (Zero-CDN).
  - Menjalankan mini HTTP server lokal (port misal `127.0.0.1:3030`) melalui perintah `ctrl-cli serve` atau flag `--web`.
- [ ] **Live Task Monitoring via SSE / WebSocket**:
  - Web dashboard menampilkan grafik status subagent, pemakaian memori, token counter, dan output log secara real-time langsung dari browser pengguna.

---

### 🛡️ Fase 7: Keamanan, Sandboxing & Observabilitas
Fokus pada proteksi lingkungan lokal saat agen menjalankan kode atau perintah terminal.

- [ ] **Workspace Path Sandboxing**:
  - Membatasi tool `read_file`, `write_file`, dan `edit_file` agar secara ketat terkurung di dalam direktori kerja proyek (mencegah akses file sensitif sistem seperti `C:\Windows\` atau `/etc/`).
- [ ] **Safe Dry-Run & Destructive Command Guard**:
  - Peringatan interaktif otomatis saat agen hendak menjalankan perintah terminal yang berpotensi merusak (`rm -rf`, `del /s`, `format`, `git reset --hard`).
- [ ] **Audit Trail Telemetry**:
  - Pencatatan seluruh aksi tool dan penggunaan token ke `.ctrl/audit.log` yang terenkripsi atau terproteksi.

---

## 📊 Matriks Dampak vs. Upaya (Impact vs. Effort)

| Fitur | Dampak | Perkiraan Upaya | Komponen yang Terkena |
|---|---|---|---|
| **Sync Versi & Commit Working Tree** | Sedang | Sangat Rendah | `Cargo.toml`, `README.md`, Git |
| **Upgrade Rustyline 15.0** | Sedang | Rendah | `Cargo.toml`, `src/main.rs` |
| **Streaming Output (SSE)** | Tinggi | Sedang | `src/agent/provider.rs`, `orchestrator.rs` |
| **Native Google Gemini Provider** | Tinggi | Sedang | `src/agent/provider.rs`, `probe.rs` |
| **Persistensi Task ke Disk** | Tinggi | Sedang | `src/agent/tasks.rs` |
| **DAG Task Dependencies** | Sangat Tinggi | Tinggi | `src/agent/tasks.rs`, `src/agent/subagent.rs` |
| **Embedded Web Dashboard** | Sedang | Sedang | Mini HTTP Server, `index.html` |
| **Path Sandboxing** | Sangat Tinggi | Rendah | `src/tools/filesystem.rs` |

---

## 📝 Catatan Kontribusi
Bagi developer atau AI Agent yang ingin mulai mengimplementasikan fitur dari roadmap ini:
1. Buat branch baru dari `master`.
2. Pastikan setiap penambahan fitur dilengkapi dengan unit test mandiri di bawah modul terkait atau di `tests/`.
3. Jalankan `cargo test` dan `cargo clippy --all-targets -- -D warnings` sebelum mengajukan pull request atau memfinalisasi perubahan.
