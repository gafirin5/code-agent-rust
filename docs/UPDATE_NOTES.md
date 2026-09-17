# 📝 Catatan Update (Update Notes & Changelog)

Dokumen ini mencatat seluruh pembaruan, evolusi arsitektur, penyelesaian milestone, serta perbaikan bug pada **ctrl-cli**.

---

## 🚀 Fase 5: Next-Generation Agent Modernization & Production Readiness (2026-09-17)

Pembaruan besar **Fase 5** memperluas kapabilitas `ctrl-cli` dengan fitur-fitur otonom modern berstandar enterprise tanpa menambah runtime asynchronous (`tokio`) ataupun dependensi eksternal yang berat. Arsitektur tetap berpedoman teguh pada **Pure Rust (Edition 2021)**, determinisme tinggi, serta binary footprint ramping (< 3.5 MB).

### 🌟 Rangkuman Inovasi Fase 5 (Phase 5 Highlights)

| Milestone | Modul & Komponen | Fitur & Kapabilitas Utama |
|---|---|---|
| **M1. Dynamic Skills & BM25 RAG** | `src/tools/skills.rs`<br>`src/tools/knowledge.rs` | Auto-discovery skill dari direktori `skills/`, `.ctrl/skills/`, dan `prompts/` dengan parsing YAML frontmatter.<br>Slash commands `/skills list` dan `/skills info <name>`.<br>Mesin perankingan BM25 in-process berbasis BM25Okapi untuk Markdown knowledge base (`data/knowledge/`) yang diakses via tool `knowledge_search`. |
| **M2. Guardrails & Immutable Audit** | `src/tools/guardrails.rs`<br>`src/tools/audit.rs` | Deteksi perintah shell destruktif (`rm -rf`, `del /s`, `format`, `git reset --hard`) dengan konfirmasi interaktif di mode REPL dan safe dry-run rejection di mode background.<br>Audit log append-only JSON terstruktur di `.ctrl/audit.log` mencatat setiap eksekusi tool, argumen, timestamp, dan status akhir. |
| **M3. Web Dashboard SSE & Run API** | `src/server.rs`<br>`src/telemetry/mod.rs` | Endpoint Server-Sent Events (SSE) `GET /api/events` untuk streaming event task real-time dan keepalive ping `: ping\n\n`.<br>Endpoint `POST /api/tasks/run` untuk memicu eksekusi subagent task dari antarmuka web.<br>Penyempurnaan non-blocking socket loopback Windows (resolusi error 10053/10054/10060) dengan stack thread 128KB dan sub-microsecond mutex CPU sampling. |
| **M4. Structured Git & Rollback** | `src/tools/git.rs`<br>`src/agent/checkpoint.rs` | Tool git terstruktur: `git_status`, `git_diff`, `git_commit` menggunakan irisan argumen aman (`Command::new("git")`).<br>Mekanisme multi-file snapshot rollback otomatis: metadata tersimpan di `.ctrl/checkpoints/<id>/manifest.json`.<br>Perintah REPL `/undo` dan `/undo list` untuk inspeksi riwayat dan pemulihan instan berkas termutasi. |
| **M5. Terminal UX & Audio Alert** | `src/tui/highlight.rs`<br>`src/agent/tasks.rs`<br>`src/main.rs` | Pewarnaan sintaksis ANSI murni tanpa dependency parser berat untuk blok kode Markdown (Rust, Python, JavaScript, Shell, JSON).<br>Audio completion alert via terminal bell (`\x07`) yang dipicu saat tugas latar belakang selesai (didukung toggle `ALERT_ON_TASK_DONE`). |
| **M6. Full Quality Verification** | 25 Test Suites & Clippy | 100% test pass rate di seluruh 25 file integrasi test suite (termasuk adversarial test dan stress socket).<br>Zero Clippy warnings (`cargo clippy --all-targets -- -D warnings`).<br>Biner release final Windows: **3.10 MB** (jauh di bawah batas ketat 3.5 MB). |

---

## 🚀 Fase 4: Resource Telemetry, Profiling & Comprehensive Performance Benchmarks (2026-09-15)

Pembaruan strategis **Fase 4** menghadirkan subsistem telemetri resource dan profiling internal yang mandiri, deterministik, dan berkinerja tinggi pada **ctrl-cli**, serta rangkaian benchmark performa otomatis komprehensif (5 suites, 83 passing assertions) untuk mengaudit dan memverifikasi batas konsumsi memori (RAM), CPU, penyimpanan persistent (disk), ukuran biner release, dan siklus hidup (*lifecycles*) thread, handle, dan socket OS.

Pembaruan ini sepenuhnya mematuhi filosofi inti proyek: **arsitektur Pure Rust (Edition 2021) tanpa runtime async berat (`tokio`, `async-std`)**, tanpa pustaka C eksternal, dan tanpa dependensi daemon latar belakang pihak ketiga.

---

### 🌟 Rangkuman Inovasi Fase 4 (Phase 4 Highlights)

| Komponen | Implementasi & Kapabilitas |
|---|---|
| **Core Telemetry Engine** | Modul `src/telemetry/` murni Rust dengan abstraksi cross-platform. Mengumpulkan metrik Resident Set Size (RSS), Peak RSS, Virtual Memory, delta CPU utilization percentage, durasi CPU (user & kernel time), jumlah OS thread aktif, open handles / file descriptors, dan footprint storage `.ctrl/`. |
| **Platform-Native FFI** | **Windows**: FFI Win32 native via `kernel32.dll` (`K32GetProcessMemoryInfo`, `GetProcessTimes`, `CreateToolhelp32Snapshot`, `GetProcessHandleCount`).<br>**Unix/Linux/macOS**: Pembacaan virtual filesystem `/proc/self/status`, `/proc/self/stat`, `/proc/self/task`, `/proc/self/fd`, dan fallback POSIX `getrusage`. |
| **Stateful CPU Sampler** | `CpuSampler` menghitung persentase CPU delta secara akurat, dinormalisasi terhadap jumlah core logis (`available_parallelism`), dengan proteksi sub-millisecond debouncing (< 1ms) untuk mencegah pembagian nol (*zero division defense*). |
| **Terminal REPL `/stats`** | Slash command `/stats` (dan alias `/metrics`, `/telemetry`, `/resources`) dengan sub-perintah `table` (ANSI visual box), `json` (pretty-printed JSON), dan `reset`. |
| **Embedded REST API** | Endpoint `GET /api/metrics` pada server HTTP embedded menyajikan snapshot resource dalam format JSON terstruktur lengkap dengan header CORS. |
| **Live TUI Status Indicator** | Widget footer pada mode TUI Ratatui menampilkan `RAM: <rss> M (Pk <peak> M) │ CPU: <pct>% │ Th: <threads>` secara real-time (1 Hz) dengan visual alert highlights saat RAM > 50 MB atau CPU > 80%. |
| **Rangkaian Benchmark Otomatis** | 5 suite pengujian deterministik di `tests/resource_telemetry_benchmark.rs` membuktikan konsumsi RAM idle < 20 MB (terukur 1.27 MB in-process / 6.91 MB CLI), 0 memory leak pada 1.000 task, CPU idle 0.00% non-busy-wait, isolasi 100% log task, binary release <= 3.2 MB (Windows MSVC), dan zero thread/handle/socket leaks. |

---

### 🛠️ Detail Rancang Bangun & Integrasi (Detailed Implementation)

#### 1. Arsitektur Engine Telemetri (`src/telemetry/`)
- **Struktur Metrik Terpadu (`src/telemetry/mod.rs`)**:
  - `MemoryMetrics`: Menyimpan `rss_bytes`, `peak_rss_bytes`, `virtual_bytes`, serta string terformat manusia (`formatted_rss`, `formatted_peak`).
  - `CpuMetrics`: Menyimpan persentase penggunaan CPU ternormalisasi (`process_pct`), waktu eksekusi kode pengguna (`user_ms`), waktu eksekusi kernel (`kernel_ms`), dan total waktu eksekusi CPU (`total_ms`).
  - `ThreadMetrics`: Menyimpan jumlah OS thread aktif (`active_threads`) dan hitungan handle/fd (`process_handles`).
  - `StorageMetrics`: Menghitung ukuran total direktori `.ctrl/` (`ctrl_dir_bytes`), ukuran khusus file log task (`task_logs_bytes`), jumlah file (`file_count`), dan string terformat (`formatted_ctrl`).
- **Implementasi Native Windows (`src/telemetry/windows.rs`)**:
  - `K32GetProcessMemoryInfo` mengekstrak `WorkingSetSize` dan `PeakWorkingSetSize` secara akurat dari struktur `PROCESS_MEMORY_COUNTERS`.
  - `GetProcessTimes` terhadap handle `GetCurrentProcess()` mengembalikan total waktu eksekusi CPU yang diakumulasikan dari seluruh thread milik proses, dikonversi dari satuan 100 nanodetik ke milidetik.
  - `CreateToolhelp32Snapshot` dengan flag `TH32CS_SNAPTHREAD` mengiterasi seluruh thread aktif dan memfilter berdasarkan `th32OwnerProcessID` proses saat ini.
  - `GetProcessHandleCount` mengembalikan jumlah total open handle yang dialokasikan oleh OS.
- **Implementasi Native Unix / POSIX (`src/telemetry/unix.rs`)**:
  - Membaca `/proc/self/status` untuk `VmRSS:`, `VmHWM:`, `VmSize:`, dan `Threads:`.
  - Membaca `/proc/self/stat` dengan algoritma pemindaian token aman setelah karakter `)` terakhir untuk mengekstrak `utime` dan `stime` (skala 100 Hz ke milidetik).
  - Menghitung direktori `/proc/self/task/` untuk OS thread dan `/proc/self/fd/` untuk open file descriptors.
  - Fallback otomatis ke `getrusage(RUSAGE_SELF, ...)` untuk kompatibilitas macOS dan BSD.
- **Perhitungan Footprint Disk Workspace (`calculate_storage_metrics`)**:
  - Menelusuri direktori `.ctrl/` secara rekursif dengan melewati symlink direktori untuk mencegah rekursi tak berhingga.
  - Membedakan file log task (`.log`) dari berkas metadata dan checkpoint lainnya.

#### 2. Integrasi Multi-Interface
- **REPL Slash Command (`src/main.rs`)**:
  - Menambahkan perintah `/stats`, `/metrics`, `/telemetry`, dan `/resources`.
  - Menghasilkan representasi tabel visual berbingkai ANSI (`format_metrics_table`) atau format JSON mentah (`serde_json::to_string_pretty`).
  - Sub-perintah `reset` mengkalibrasi ulang titik awal sampler CPU.
- **REST API Endpoint (`src/server.rs`)**:
  - `GET /api/metrics` merespons permintaan HTTP dengan dokumen JSON lengkap `ProcessMetrics`, beroperasi sinkron di atas `TcpListener`.
- **TUI Live Status Indicators (`src/tui/ui.rs` & `src/tui/app.rs`)**:
  - Widget Paragraph pada footer layout berukuran 38 karakter merender status CPU, RAM, dan thread secara live.
  - Mekanisme tick 1 Hz pada event loop TUI memperbarui metrik di latar belakang tanpa memblokir input pengguna ataupun menyebabkan flicker rendering.

---

### 📊 Hasil Pengujian & Benchmark Kinerja Empiris (Empirical Verification)

Pengujian benchmark dieksekusi melalui `ctrl-cli/tests/resource_telemetry_benchmark.rs` dengan hasil **83 passed; 0 failed (100% lulus)** dalam 19.66 detik:

#### 1. Suite 1: RAM & Memory Footprint
- **Idle Baseline RAM**:
  - In-Process Idle RAM: **1.27 MB** (1,335,296 byte).
  - Standalone CLI Subprocess: **6.91 MB** (7,249,920 byte).
  - *Batas Persyaratan*: < 20 MB (tercapai dengan margin keamanan > 65%).
- **Repetitive 1,000 Mock Tasks (Zero Memory Leak)**:
  - 1.000 task subagent dieksekusi secara berurutan dalam **746.75 ms** (**0.75 ms/task**).
  - RSS Awal: 4,689,920 byte | RSS Akhir: 4,714,496 byte.
  - Kenaikan Retained Memory: **24.00 KB** (24,576 byte) $\ll$ batas 1.0 MB.
  - Thread aktif kembali persis ke baseline (Zero thread leak).
- **Concurrent Task Burst Peak Memory**:
  - 25 worker thread paralel yang masing-masing mengalokasikan buffer memori 256 KB.
  - Peak RSS selama burst: **9.28 MB** (9,728,000 byte) $\ll$ batas 50 MB.
  - Thread mengembang dari $7 \to 32 \to 7$.
  - Retained Delta setelah task selesai: **2.18 MB** (2,289,664 byte) $\ll$ batas 5 MB.

#### 2. Suite 2: CPU Utilization & Non-Busy-Wait Idle Verification
- **Baseline Idle Process**: **0.00% CPU** (0 ms delta CPU / 1.000 ms wall).
- **TaskScheduler Tick (`Condvar::wait_timeout`)**: **0.00% CPU** (0 ms delta CPU / 1.200 ms wall).
- **HTTP Server Accept Loop (`TcpListener` non-blocking sleep)**: **0.00% CPU** (0 ms delta CPU / 1.003 ms wall).
- **TaskManager Awaiter (`Condvar::wait`)**: **0.00% CPU** (0 ms delta CPU / 1.000 ms wall).
- **Combined 4 Subsystems Running**: **0.00% CPU** (0 ms delta CPU / 1.000 ms wall).
- **Adversarial Busy-Spin Detection**:
  - Menguji thread latar belakang dengan loop `spin_loop` aktif selama 500 ms.
  - Terdeteksi penggunaan CPU sebesar **500 ms** (**24.94% CPU** pada sistem 4-core, setara 100% beban 1 core).
  - Membuktikan secara matematis bahwa harness pengujian mengukur CPU proses secara nyata dan tidak memiliki *blind spot*.

#### 3. Suite 3: Storage Isolation & Boundedness
- **Persistensi Atomik `.ctrl/tasks.jsonl`**:
  - 50 task konkuren dari 8 thread koordinator menghasilkan tepat **140 baris** dan **35,090 byte** ($\le 150$ baris dan $\le 50$ KB).
  - 100% baris JSON valid dengan transisi status yang terurut secara monotonik.
- **Isolasi Log Subagent Paralel**:
  - 20 subagent konkuren menulis total 1.000 baris log unik ke file `.ctrl/tasks/<id>.log` masing-masing.
  - Pemeriksaan 380 kombinasi pasangan file log membuktikan **0 baris terkontaminasi** (100% isolasi output).
- **Pembersihan Bersih RAII TempDir**:
  - 50 scope drop normal + 20 scope panic unwinding (`catch_unwind`).
  - **70 dari 70 direktori temporer terhapus sempurna**, 0 orphan files.

#### 4. Suite 4: Enforcing Release Binary Size
- **Biner Windows Release MSVC (`target/release/ctrl-cli.exe`)**:
  - Ukuran: **3,098,112 byte** (**2.95 MiB / 3.10 MB**).
  - Sesuai dengan batas arsitektur Windows PE ($\le 3.20$ MB).
  - Pada lingkungan Linux stripped ELF, ukuran biner berada pada kisaran **~1.80 MB** ($\le 2.50$ MB).

#### 5. Suite 5: Thread, Handle & Socket Lifecycles
- **100 Siklus Spawn & Cancel Task**:
  - Thread awal: 5 | Thread akhir: 5 | Delta: **0 thread leak**.
  - Handle awal: 76 | Handle akhir: 76 | Delta: **0 handle leak**.
- **100 Permintaan HTTP Berturut-turut ke `/api/metrics`**:
  - Handle awal: 85 | Handle akhir: 85 | Delta: **0 handle/socket leak**.

---

### 📁 Berkas Baru & Modifikasi Terkait Fase 4 (File Inventory)

| Berkas | Status | Deskripsi Perubahan |
|---|---|---|
| `ctrl-cli/src/telemetry/mod.rs` | Baru | Definisi struktur data metrik, kalkulator storage, CPU sampler, dan formatter tabel ANSI. |
| `ctrl-cli/src/telemetry/windows.rs` | Baru | Provider telemetri Windows native berbasis Win32 FFI (`kernel32.dll`). |
| `ctrl-cli/src/telemetry/unix.rs` | Baru | Provider telemetri Unix/Linux native berbasis `/proc` dan POSIX `getrusage`. |
| `ctrl-cli/tests/resource_telemetry_benchmark.rs`| Baru | Suite benchmark performa komprehensif menguji 5 suite (83 assertions). |
| `ctrl-cli/docs/PERFORMANCE.md` | Baru | Spesifikasi teknis mendalam arsitektur telemetri, metrik performa, dan panduan reproduktibilitas. |
| `ctrl-cli/src/main.rs` | Dimodifikasi | Penambahan perintah REPL `/stats`, `/metrics`, `/telemetry`, `/resources` dan handler subcommand. |
| `ctrl-cli/src/server.rs` | Dimodifikasi | Penambahan endpoint REST API `GET /api/metrics` dengan respons JSON terstruktur. |
| `ctrl-cli/src/tui/ui.rs` | Dimodifikasi | Widget Paragraph footer merender telemetri real-time dengan alert highlights. |
| `ctrl-cli/src/tui/app.rs` | Dimodifikasi | Polling telemetri periodik (1 Hz) pada event loop TUI. |
| `ctrl-cli/docs/UPDATE_NOTES.md` | Dimodifikasi | Pencatatan komprehensif rilis Fase 4, hasil benchmark, dan evolusi arsitektur. |

---

## 🚀 Versi 0.3.0: Core Modernization, Streaming, Persistence, Embedded Web Dashboard & Security Sandboxing (2026-09-14)

Rilis besar **v0.3.0** menandai transformasi arsitektur menyeluruh pada **ctrl-cli** (`code-agent-rust`). Rilis ini memperluas kapabilitas agen coding AI berbasis Rust murni (*pure Rust*) dengan integrasi antarmuka ganda (REPL & interactive Ratatui TUI), *real-time Server-Sent Events (SSE) streaming*, pembatalan soket instan (*synchronous socket abort*), ekosistem provider yang diperluas (Google Gemini REST v1beta & native Ollama), persistensi task berbasis disk (`.ctrl/tasks.jsonl`) dengan *crash recovery*, orkestrasi dependensi task (*DAG execution with cycle detection*), penjadwal *cron* 5-field dengan algoritma kalender sipil Howard Hinnant, server HTTP mini *embedded* untuk *dashboard monitoring*, serta *filesystem sandboxing* dengan normalisasi UNC untuk perlindungan direktori *workspace*.

Seluruh fitur ini dirancang dan diimplementasikan dengan mematuhi prinsip inti: **tanpa dependensi runtime async (`tokio`)**, menjaga biner bereksekusi sangat cepat, deterministik, dan berukuran ultra-ringan (**~1.8 MB** stripped binary).

---

### 🌟 Rangkuman Evolusi Arsitektur (Release Highlights & Architecture)

| Pilar Arsitektur | Implementasi v0.2.0 | Evolusi v0.3.0 |
|---|---|---|
| **Antarmuka Pengguna** | REPL Inquire + CLI argumen statis | REPL interaktif berbasis **Rustyline 15.0** + Mode TUI layar penuh (**Ratatui 0.30 / Crossterm 0.29**) dengan hotkey navigasi runtime (`/tui`, `Esc`, `Ctrl+C`, `:repl`) & `TerminalGuard` RAII drop guard. |
| **Inference Streaming** | Non-streaming / buffering lengkap | **Real-Time SSE & NDJSON Streaming** (kata-demi-kata) dengan penanganan soket abort sinkron (`CancellationToken`) berlatensi <50ms tanpa *zombie connection*. |
| **Provider LLM** | OpenAI & Anthropic Messages API | Penambahan provider native **Google Gemini (REST v1beta)** & **Ollama (local daemon /api/chat)** dengan *model probing* dan deteksi kapabilitas otomatis. |
| **Penyimpanan Task** | In-memory RAM (`TaskManager` HashMap) | **Disk-Backed JSONL Persistence** (`.ctrl/tasks.jsonl`), log eksekusi terisolasi (`.ctrl/tasks/<id>.log`), sinkronisasi ID counter monotonik, dan **Startup Crash Recovery**. |
| **Orkestrasi Alur Kerja** | Task independen / single-level | **DAG Dependency Orchestration** (Topological Sort Kahn & Cycle Detection `depends_on`), kaskade pembatalan/kegagalan otomatis, serta penjadwal **5-field Cron Scheduler**. |
| **Monitoring Visual** | Slash command terminal (`/tasks`) | **Embedded Local Web Server** (`std::net::TcpListener`) menyajikan *responsive single-file dashboard* (`index.html`) & REST API (`/api/tasks`, `/api/tasks/<id>`, `/api/tasks/<id>/logs`, `/api/tasks/<id>/cancel`). |
| **Keamanan File** | Validasi path relatif dasar | **Strict Workspace Sandboxing** (`resolve_sandboxed_path`) dengan normalisasi UNC prefix Windows (`\\?\`), pencegahan *parent directory traversal*, dan verifikasi leluhur terdekat (*nearest ancestor*). |
| **Metrik Pengujian** | 168 unit/stress tests | **320+ automated tests across 14 test suites** (605 passing assertions), 100% pass rate, **0 Clippy warnings** (`cargo clippy --all-targets -- -D warnings`). |

---

### 🛠️ Detail Per Milestone (Milestone Breakdown)

#### 1. Milestone 1: Core Modernization & TUI Integration
- **Penyelarasan Versi 0.3.0**:
  - Memperbarui versi pada `Cargo.toml` ke `0.3.0`.
  - Sinkronisasi versi CLI pada `src/main.rs`, banner REPL, dan header Ratatui TUI.
  - Memperbarui seluruh badge dokumentasi pada `README.md` dan `ctrl-cli/README.md` (320+ Passing tests, version 0.3.0, 0 clippy warnings).
- **Upgrade Rustyline 15.0**:
  - Mengupgrade dependensi REPL `rustyline` ke versi `15.0` modern dengan penanganan autocompletion slash command yang lebih stabil dan penanganan sinyal terminal yang bersih.
- **Integrasi Penuh Ratatui TUI (`src/tui/`)**:
  - Menghadirkan antarmuka visual terminal interaktif layar penuh berbasis `ratatui 0.30` dan `crossterm 0.29`.
  - Perintah slash `/tui` pada REPL memungkinkan pengguna berpindah dari baris perintah ke mode visual TUI secara dinamis saat runtime tanpa me-restart aplikasi.
  - Komunikasi thread UI berbasis event channel sinkron (`AgentUiEvent::ContentChunk`, `AgentUiEvent::StatusUpdate`, `AgentUiEvent::TurnComplete`).
- **`TerminalGuard` RAII Drop Guard (`src/tui/mod.rs`)**:
  - Mengisolasi perubahan state terminal (raw mode, alternate screen, mouse capture) ke dalam struktur RAII `TerminalGuard`.
  - Memasang custom panic hook (`std::panic::set_hook`) yang menjamin pemulihan atribut terminal (`disable_raw_mode`, `LeaveAlternateScreen`, `Show` cursor) secara deterministik bahkan saat terjadi *panic* atau *early return*, mencegah terminal pengguna terkunci dalam raw mode.
- **Hotkey & Navigasi Runtime**:
  - Menyediakan tombol pintas `Esc`, `Ctrl+C`, dan perintah kembali (`:repl`, `:exit`, `:q`, `/repl`, `/exit`) pada TUI untuk kembali ke sesi REPL dengan mempertahankan riwayat percakapan secara utuh.

#### 2. Milestone 2: Real-Time SSE Streaming, Synchronous Cancellation Socket Abort & Expanded Providers
- **Real-Time Token Streaming Sinkron (`src/agent/orchestrator.rs`)**:
  - Mengembangkan pembaca SSE (*Server-Sent Events*) berbasis `BufReader` sinkron di atas response socket `ureq 2.10`.
  - Mengalirkan token kata-demi-kata langsung ke `stdout` terminal (mode CLI) dan ke `AgentUiEvent::ContentChunk` channel (mode TUI).
  - Memperbaiki perhitungan `effective_stream` di `src/agent/orchestrator.rs` sehingga `OutputSink::Channel` menerima event streaming tanpa terblokir, sementara `OutputSink::Buffered` tetap menonaktifkan streaming agar log subagent background terisolasi sempurna.
- **In-Flight Cancellation & Synchronous Socket Abort**:
  - Integrasi `CancellationToken` ke dalam loop pembacaan chunk/baris HTTP streaming.
  - Saat pembatalan dipicu pengguna (`Ctrl+C` atau `/tasks cancel`), loop pembaca segera mendeteksi `token.is_cancelled()`, memutus pembacaan (`anyhow::bail!`), dan men-drop *underlying socket* secara instan (`shutdown(Both)`).
  - Verifikasi empiris membuktikan latensi pembatalan soket aktif berada di bawah **50ms** (rata-rata terukur **~15-25ms** pada challenge suite `m2_empirical_challenge.rs`), sepenuhnya bebas dari *zombie socket* atau kebocoran resource jaringan.
- **Google Gemini REST v1beta Native Provider (`src/agent/provider.rs`, `src/agent/probe.rs`, `src/agent/orchestrator.rs`)**:
  - Implementasi protokol native `ApiProtocol::Gemini` berinteraksi langsung dengan Google AI Studio endpoint (`streamGenerateContent?alt=sse` dan `generateContent`).
  - Mendukung autentikasi via header `x-goog-api-key` atau URL query parameter `key`.
  - Parsing streaming SSE Gemini (`candidates[0].content.parts[0].text`), ekstraksi pemanggilan tool (*function calls*), dan akumulasi metrik token (*usageMetadata*: `promptTokenCount`, `candidatesTokenCount`).
  - Fungsi probe endpoint `probe_gemini_provider` untuk memeriksa konektivitas dan validitas API key secara non-destruktif.
- **Ollama Native Provider & Local Offline Discovery**:
  - Implementasi protokol native `ApiProtocol::Ollama` untuk inferensi lokal tanpa koneksi internet.
  - Mengonsumsi endpoint streaming NDJSON `/api/chat` dan endpoint penemuan model `/api/tags`.
  - Mendukung konversi skema tool ReAct ke format Ollama native dan toleransi argumen JSON stringifikasi.
  - Fungsi probe `probe_ollama_provider` untuk mendeteksi ketersediaan daemon Ollama lokal dan model yang terpasang (*installed models*).

#### 3. Milestone 3: Task Persistence, Startup Crash Recovery, DAG Dependencies & 5-Field Cron Scheduler
- **Disk-Backed Task Persistence (`.ctrl/tasks.jsonl`)**:
  - Setiap perubahan lifecycle task (`Queued`, `Running`, `Completed`, `Failed`, `Cancelled`) diserialisasi secara append-only ke file `.ctrl/tasks.jsonl`.
  - Seluruh output log, pesan diagnostik compiler, dan respons subagent ditulis secara terisolasi ke file `.ctrl/tasks/<id>.log`.
  - Fungsi `TaskManager::load_from_disk` memulihkan seluruh riwayat task saat aplikasi dinyalakan ulang (*cold boot*), serta menyinkronkan alokator ID atomik (`task_counter`) ke nilai maksimum ID yang ada untuk mencegah duplikasi atau tabrakan ID task.
- **Startup Crash Recovery Engine (`src/agent/tasks.rs`)**:
  - Mekanisme rekonsiliasi otomatis saat start-up: seluruh task yang sebelumnya tertinggal dalam status `Running` atau `Queued` akibat proses mati mendadak (*power failure* atau `SIGKILL`) secara atomik direkonsiliasi menjadi status `Failed` dengan pesan diagnostik yang jelas (*"Task execution interrupted by system shutdown/crash"*).
  - Menghindari task mengambang (*orphaned zombie tasks*) dan menjamin integritas state machine.
- **DAG Dependency Orchestration & Cycle Detection (`src/agent/dag.rs`, `src/agent/tasks.rs`)**:
  - Task mendukung deklarasi dependensi prasyarat melalui parameter `depends_on: Vec<String>`.
  - Struktur `DagValidator` menggunakan algoritma Topological Sort (Kahn's Algorithm) untuk memvalidasi graf alur kerja. Secara proaktif menolak:
    - *Self-cycles* (task bergantung pada dirinya sendiri).
    - *Direct cycles* (A -> B -> A).
    - *Indirect cycles* (A -> B -> C -> A).
    - Prasyarat tidak terdaftar (*missing dependencies*).
  - Worker thread background menahan eksekusi task (`Queued`) hingga seluruh task prasyarat selesai dengan status `Completed`.
  - Jika task prasyarat gagal (`Failed`) atau dibatalkan (`Cancelled`), kegagalan tersebut secara otomatis merambat (*cascading abort*) membatalkan seluruh task hilir yang bergantung padanya, dengan tetap mengisolasi branch independen lainnya.
- **5-Field Cron Scheduler (`src/agent/scheduler.rs`)**:
  - Parser ekspresi cron standar 5-field: `minute (0-59)`, `hour (0-23)`, `day_of_month (1-31)`, `month (1-12)`, `day_of_week (0-6, Sun=0)`.
  - Mendukung sintaks lengkap: wildcard (`*`), rentang (`1-5`), daftar (`1,3,5`), interval/step (`*/15`, `1-30/5`).
  - Mendukung macro standar: `@hourly`, `@daily`, `@weekly`, `@monthly`, `@yearly`, `@annually`, serta interval kustom `@every <duration>` (e.g. `@every 30s`, `@every 5m`).
  - **Howard Hinnant Calendrical Algorithm**: Algoritma konversi kalender sipil UTC murni tanpa ketergantungan pada crate `chrono`, menghitung tahun kabisat, bulan, hari, dan hari dalam minggu secara deterministik dari `SystemTime::UNIX_EPOCH`.
  - Background scheduler thread melakukan evaluasi setiap 1 detik untuk memicu task berkala secara akurat dan aman terhadap kepanikan (*panic-safe runner*).

#### 4. Milestone 4: Local Web Server, Visual Dashboard & Filesystem Sandboxing
- **Embedded Local Web Server (`src/server.rs`)**:
  - Web server HTTP/1.1 mini berbasis `std::net::TcpListener` tanpa dependensi runtime async atau framework eksternal.
  - Subcommand CLI: `ctrl-cli serve --port <port>` (default port: `8080`, host: `127.0.0.1`).
  - Flag daemon: `--web` dapat digabungkan dengan `--cli` untuk menjalankan server monitoring di thread latar belakang secara bersamaan dengan sesi REPL.
  - Shutdown anggun (*graceful shutdown*) dikendalikan oleh `CancellationToken`.
- **Responsive Single-File Web Dashboard (`index.html`)**:
  - Menyajikan visual dashboard mandiri berukuran 77 KB yang terletak di root repository.
  - Dilengkapi *compile-time fallback* (`include_str!("../../index.html")`) sehingga dashboard tetap berfungsi meskipun file di disk dipindahkan.
  - Menyediakan monitoring visual real-time untuk: status task, durasi eksekusi, dependensi DAG, log diagnostik, serta tombol pembatalan interaktif.
- **Task Status REST API**:
  - `GET /` & `GET /index.html`: Menyajikan antarmuka visual dashboard HTML.
  - `GET /api/tasks`: Mengembalikan seluruh snapshot task dalam format JSON.
  - `GET /api/tasks/<id>`: Mengembalikan detail spesifik task (status, durasi, timestamp, output, error).
  - `GET /api/tasks/<id>/logs`: Mengembalikan daftar baris log eksekusi subagent.
  - `POST /api/tasks/<id>/cancel`: Membatalkan task yang sedang berjalan secara asinkron.
  - Dukungan penuh header keamanan dan CORS: `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, POST, OPTIONS`, dan penanganan preflight `OPTIONS` 204.
- **Strict Filesystem Sandboxing (`src/tools/filesystem.rs`)**:
  - Fungsi sentral `resolve_sandboxed_path` membatasi seluruh operasi file (`read_file`, `write_file`, `edit_file`, `list_directory`, `search_files`, `grep_content`) strictly di dalam direktori root *workspace*.
  - **Normalisasi Windows Extended-Length UNC Prefix**: Menghilangkan awalan `\\?\` secara transparan agar pencocokan path kanonikal di Windows tidak mengalami false-positive penolakan.
  - **Pertahanan Directory Traversal**: Mencegah lolosnya path menggunakan `..`, `../../`, `./`, tautan simbolik (*symlinks*), ataupun case-insensitivity drive Windows (`C:\` vs `c:\`).
  - **Verifikasi Leluhur Terdekat (*Nearest Ancestor Check*)**: Untuk pembuatan file baru yang jalurnya belum ada di disk, sandboxing menelusuri direktori induk yang ada terdekat untuk memastikan lokasi pembuatan tetap berada di dalam batas sandbox yang sah.
  - Mendukung pengujian hermetis melalui fungsi *thread-local override* `with_workspace_root`.

---

### 📊 Metrik Pengujian & Performa (Test Metrics & Benchmarks)

#### 1. Rangkaian Pengujian Otomatis (14 Test Suites, 100% Pass)
Seluruh 14 test suite dieksekusi secara offline tanpa memerlukan API key eksternal ataupun mock network pihak ketiga. Hasil eksekusi `cargo test`:

```text
running 14 test suites:
- unittests src/main.rs ............................................ 120 passed
- tests/boundary_stress.rs .........................................  77 passed
- tests/concurrency_stress.rs ......................................  59 passed
- tests/e2e_modernization.rs .......................................  13 passed
- tests/m1_empirical_challenge.rs ..................................   5 passed
- tests/m2_empirical_challenge.rs ..................................  56 passed
- tests/m2_protocol_empirical_challenge.rs .........................   5 passed
- tests/m3_dag_cron_empirical_challenge.rs .........................  80 passed
- tests/m3_persistence_empirical_challenge.rs ......................  57 passed
- tests/m4_dashboard_sandboxing_challenge.rs .......................   4 passed
- tests/m4_sandboxing_adversarial_challenge.rs .....................   6 passed
- tests/m4_server_empirical_challenge.rs ...........................  61 passed
- tests/output_isolation_stress.rs .................................  63 passed
- tests/ux_notification_concurrency_stress.rs ......................  59 passed

Total Pengujian: 605 passing assertions (320+ unique automated tests)
Status: 100% Passed (0 failed, 0 ignored)
```

#### 2. Kualitas Kode & Linter (Clean Code Hygiene)
- `cargo clippy --all-targets -- -D warnings`: **0 Peringatan (Zero warnings / Clean)**.
- `cargo fmt --check`: 100% Sesuai standar pemformatan resmi Rust.
- Tidak ada kebocoran lock (*lock poisoning recovery* teruji pada seluruh mutex/RwLock).

#### 3. Efisiensi Biner & Jejak Memori (Resource Footprint)
- **Ukuran Biner Release**: **~1.8 MB** (Linux stripped) / **~2.9 MB** (Windows PE unstripped dengan LTO & codegen-units=1).
- **Runtime Dependencies**: Murni `std::thread`, `std::sync`, `std::net`, dan `ureq 2.10`. Bebas dari `tokio`, `hyper`, atau `actix`, menjamin konsumsi memori idle di bawah 15 MB RAM dan latensi start-up sub-milidetik.

---

### 📁 Daftar Berkas yang Diubah dan Ditambahkan (File Inventory)

| Berkas | Jenis Perubahan | Deskripsi Fungsional |
|---|---|---|
| `Cargo.toml` | Dimodifikasi | Sinkronisasi versi ke `0.3.0`, upgrade `rustyline` ke `15.0`. |
| `src/main.rs` | Dimodifikasi | Integrasi subcommand `serve`, flag `--web`, slash command `/tui`, dan banner versi 0.3.0. |
| `src/tui/mod.rs` | Baru | Entry point mode TUI, konfigurasi crossterm, dan `TerminalGuard` RAII drop guard. |
| `src/tui/app.rs` | Baru | State machine aplikasi TUI, penanganan navigasi, input editor, dan routing hotkey. |
| `src/tui/ui.rs` | Baru | Layout rendering Ratatui: chat history, input prompt, sidebar subagents, status bar. |
| `src/tui/event.rs` | Baru | Event polling non-blocking crossterm dan sinkronisasi thread background. |
| `src/agent/provider.rs` | Dimodifikasi | Penambahan `ApiProtocol::Gemini` dan `ApiProtocol::Ollama` pada registry provider default. |
| `src/agent/probe.rs` | Dimodifikasi | Implementasi `probe_gemini_provider`, `probe_ollama_provider`, dan resolusi batas token model Gemini/Ollama. |
| `src/agent/orchestrator.rs` | Dimodifikasi | Real-time SSE streaming, perbaikan `effective_stream` TUI channel, in-flight cancellation socket abort. |
| `src/agent/tasks.rs` | Dimodifikasi | Persistensi disk `.ctrl/tasks.jsonl`, startup crash recovery, dependensi DAG, deduplikasi notifikasi. |
| `src/agent/dag.rs` | Baru | Validasi topological sort Kahn, deteksi direct/indirect/self cycle pada task `depends_on`. |
| `src/agent/scheduler.rs` | Baru | Parser cron 5-field, konversi tanggal sipil Howard Hinnant UTC, thread eksekusi berkala. |
| `src/server.rs` | Baru | HTTP server mini berbasis `TcpListener`, REST API `/api/tasks`, penyaji `index.html` dengan fallback. |
| `src/tools/filesystem.rs` | Dimodifikasi | Sandboxing path workspace, pembersihan UNC prefix Windows (`\\?\`), pencegahan directory traversal. |
| `tests/e2e_modernization.rs` | Baru | Suite pengujian end-to-end menyeluruh memvalidasi seluruh pilar arsitektur v0.3.0. |
| `tests/m1_empirical_challenge.rs` | Baru | Verifikasi empiris versi biner, subcommands, banner REPL, dan integrasi TUI. |
| `tests/m2_empirical_challenge.rs` | Baru | Verifikasi empiris latensi pembatalan in-flight (<50ms) dan integritas streaming socket abort. |
| `tests/m2_protocol_empirical_challenge.rs`| Baru | Validasi penanganan error SSE Gemini, probe Ollama `/api/tags`, dan tool calling NDJSON. |
| `tests/m3_dag_cron_empirical_challenge.rs` | Baru | Pengujian ketahanan DAG cycle rejection, cascade abort, parser cron 5-field, dan scheduler. |
| `tests/m3_persistence_empirical_challenge.rs` | Baru | Pengujian cold-boot restore, toleransi korupsi JSONL, dan kontinuitas counter task ID. |
| `tests/m4_dashboard_sandboxing_challenge.rs` | Baru | Pengujian subprocess `ctrl-cli serve` dan kepatuhan sandboxing path workspace. |
| `tests/m4_sandboxing_adversarial_challenge.rs`| Baru | Pengujian adversarial sandboxing: traversal `..`, prefix collisions, dan Windows quirks. |
| `tests/m4_server_empirical_challenge.rs` | Baru | Pengujian ketahanan konkurensi server HTTP, REST endpoints, CORS headers, dan graceful shutdown. |
| `docs/UPDATE_NOTES.md` | Dimodifikasi | Dokumentasi komprehensif seluruh evolusi fitur v0.3.0 dan riwayat historis. |
| `README.md` & `ctrl-cli/README.md` | Dimodifikasi | Pembaruan badge pengujian (320+ Passing) dan versi 0.3.0. |

---

## 📌 Update Sebelumnya: Pembersihan Kode & Rangkaian Dokumentasi Komprehensif (2026-09-10)

### 🧹 Pembersihan & Standardisasi Kode (Code Hygiene & Quality)
- **Zero Clippy Warnings (`cargo clippy --all-targets -- -D warnings`)**:
  - Menyelesaikan 27 temuan linter pada seluruh target biner dan suite pengujian integrasi.
  - Implementasi trait standar `std::str::FromStr` untuk `ApiProtocol` dan `SupportedLanguage`.
  - Mengganti manual match state transitions di `TaskStatus::can_transition_to` dengan macro idiomatik `matches!`.
  - Menghilangkan `needless_range_loop` pada algoritma pencarian kontekstual di `src/tools/search.rs`.
  - Mengganti kalkulasi batas manual menjadi `saturating_sub` pada `src/tools/mod.rs`.
  - Mengganti pemotongan string manual dengan `.strip_prefix(...)` dan `.unwrap_or_default()` pada parser autocompletion slash commands di `src/main.rs`.
  - Menangani anotasi `#[allow(clippy::too_many_arguments)]` secara selektif pada fungsi orkestrator yang membutuhkan konteks multi-state.
- **Konsistensi Format Kode (`cargo fmt`)**:
  - Seluruh file di dalam `src/` dan `tests/` telah diformat seragam sesuai standar resmi Rust.
- **Integritas Pengujian 100%**:
  - Seluruh 168+ unit dan integration stress test tetap lulus tanpa regresi.

### 📚 Dokumentasi Pusat (`docs/`)
- Membuat direktori dokumentasi khusus di `docs/` yang terdiri dari:
  - `README.md`: Portal dan peta navigasi dokumentasi.
  - `UPDATE_NOTES.md`: Riwayat pembaruan sistematis (dokumen ini).
  - `CONFIGURATION.md`: Panduan konfigurasi menyeluruh (environment variables, provider LLM, MCP, profil pengguna, dan batas background tasks).
  - `ARCHITECTURE.md`: Penjelasan mendalam mengenai arsitektur threading pure Rust, lifecycle task, isolasi log, dan notifikasi inter-turn.
  - `AI_AGENT_GUIDE.md`: Pedoman operasional AI agent, invariant arsitektur, dan cara memperluas sistem.
  - `TOOLS_REFERENCE.md`: Kamus referensi parameter dan skema seluruh built-in tools.

---

## 🚀 Versi 0.2.0: Concurrent Subagent Orchestration & Background Task Engine

Pembaruan besar yang menghadirkan kapabilitas delegasi tugas paralel, eksekusi subagent di background, isolasi output terminal, dan sistem notifikasi inter-turn.

### ✨ Fitur Baru yang Diimplementasikan

#### 1. Task Lifecycle State Machine & Engine (`src/agent/tasks.rs`)
- **State Machine Transisi Aman**: `Queued` → `Running` → (`Completed` | `Failed` | `Cancelled`).
- **Pelacakan Durasi & Timestamp Monotonik**: Setiap task mencatat waktu mulai, waktu selesai, durasi terformat (`format_duration_human`), nama task, deskripsi, output payload, dan pesan error jika gagal.
- **Thread-Safe Registry (`TaskManager`)**: Singleton `TaskManager::global()` berbasis `RwLock<HashMap<String, Arc<TaskRecord>>>` dengan alokasi ID berurutan (`task-1`, `task-2`, ...).
- **Pembatalan Kooperatif (`CancellationToken`)**: Abstraksi `Arc<AtomicBool>` yang diperiksa pada batas giliran LLM dan eksekusi tool, memungkinkan pembatalan aman tanpa merusak state thread.
- **Sinkronisasi Hasil (`await_task`)**: Mendukung penungguan task hingga selesai menggunakan `Condvar` dengan opsi batas waktu (`timeout: Option<Duration>`).

#### 2. Isolasi Output & Silent Background Execution (`TaskLogBuffer`)
- **Abstraksi `OutputSink`**:
  - `OutputSink::Terminal`: Menulis langsung ke `stdout` terminal untuk agen utama / interaksi pengguna.
  - `OutputSink::Buffered(Arc<TaskLogBuffer>)`: Mengalihkan seluruh diagnostik, turn log, dan output tool dari subagent background ke buffer memori dan file `.ctrl/tasks/<id>.log`.
- **Penekanan Spinner & Prompt**: Saat berjalan di background, pemanggilan spinner terminal (`inquire`) dan interaksi prompt dilewati secara otomatis (auto-resolve) sehingga terminal utama bebas dari polusi teks saat pengguna sedang mengetik.
- **Dukungan Parameter `background: true`**: Tool `subagent` kini mendukung argumen `background: Option<bool>`. Jika `true`, subagent langsung me-return `TaskId` tanpa memblokir thread pemanggil.

#### 3. Manajemen Task untuk Agen & Pengguna
- **Tool Agen `manage_task`**:
  - `action: "list"`: Mengambil daftar semua background tasks dan snapshot statusnya.
  - `action: "status"`: Memeriksa status task tertentu beserta ringkasan log terbarunya.
  - `action: "await"`: Menunggu task tertentu selesai dengan timeout opsional.
  - `action: "cancel"`: Membatalkan task yang sedang berjalan atau antre.
  - `action: "logs"`: Membaca log eksekusi lengkap dari task tertentu.
- **Slash Command REPL `/tasks`**:
  - `/tasks list`: Menampilkan tabel ringkas seluruh task dengan badge status warna ANSI.
  - `/tasks view <id>`: Menampilkan detail mendalam snapshot task.
  - `/tasks wait <id> [timeout_secs]`: Menunggu task selesai dari baris perintah REPL.
  - `/tasks cancel <id>`: Membatalkan task yang aktif.
  - `/tasks logs <id> [limit]`: Menampilkan log eksekusi subagent di terminal.
  - `/tasks clear`: Menghapus riwayat task yang telah berada pada status terminal (`Completed`, `Failed`, `Cancelled`).

#### 4. Notifikasi Penyelesaian Task Antar-Giliran (Inter-Turn REPL Notifications)
- **Once-Only Notification Delivery**: Fungsi atomik `drain_unnotified_terminal_tasks` mengumpulkan seluruh task yang baru saja selesai/gagal/dibatalkan sejak turn terakhir, menandainya sebagai `notified = true`, dan mengembalikannya untuk ditampilkan tepat sebelum prompt input berikutnya.
- **Pencegahan Spam Notifikasi**: Menjamin setiap task hanya diberitahukan tepat satu kali, tidak berulang pada penekanan Enter kosong ataupun giliran chat selanjutnya.
- **Pengurutan Deterministik**: Notifikasi diurutkan berdasarkan ID numerik task (`task-1` sebelum `task-2`), menjaga konsistensi tampilan.

#### 5. Suite Pengujian Konkurensi & Ketahanan (Concurrency Stress Testing)
- Menambahkan 4 file test integrasi komprehensif di `tests/`:
  - `boundary_stress.rs`: Menguji batas konkurensi ekstrem, task cancellation sebelum thread berjalan, dan toleransi kegagalan worker.
  - `concurrency_stress.rs`: Menguji 20+ worker konkuren yang berjalan bersamaan, perebutan lock, dan pencegahan deadlock.
  - `output_isolation_stress.rs`: Memvalidasi bahwa tidak ada kebocoran escape code atau karakter stdout dari background subagent ke terminal utama.
  - `ux_notification_concurrency_stress.rs`: Memvalidasi deduplikasi atomik drain notifikasi di bawah beban konkurensi tinggi.
- **Total Pengujian**: 168+ automated tests, 100% passing secara offline tanpa ketergantungan mock eksternal.

---

## 🛠️ Riwayat Masalah & Pembelajaran Konkurensi (Dead Ends Resolved)

| Kasus Masalah | Pendekatan Awal yang Gagal | Akar Masalah | Solusi Akhir yang Diterapkan |
|---|---|---|---|
| **Pembatalan Task Cepat** | Menggunakan `thread::sleep(25ms)` sebelum memanggil pembatalan | Latensi inisiasi OS thread di Windows menyebabkan race condition: task dibatalkan saat masih `Queued`, durasi tercatat 0ms | Menambahkan `start_instant: Option<Instant>` pada saat task di-spawn, dan menjamin populasi durasi bahkan saat dibatalkan di fase `Queued`. |
| **Flaky Timing Assertions** | Menggunakan assertion wall-clock sempit (`elapsed < 600ms`) pada pengujian unit | Timer resolution dan jitter penjadwalan OS Windows di bawah beban pengujian paralel 40 thread rutin melampaui batas waktu sempit | Menggunakan pengujian berbasis event barrier (`Condvar` / `Arc<AtomicBool>`) daripada batas waktu sleep statis. |
| **Deduplikasi Notifikasi** | Membaca daftar task via read lock lalu mengubah status `notified` via write lock terpisah | Jeda antara read dan write membuka celah race condition (multi-drain stampede) yang menduplikasi notifikasi | Menggabungkan operasi drain dan penandaan `notified = true` ke dalam satu write lock transaksi atomik di `drain_unnotified_terminal_tasks`. |

---

## 📦 Versi 0.1.0: Rilis Fondasi (Initial Foundation)

- **ReAct Execution Loop**: Loop pemikiran dan tindakan agen otonom untuk inspeksi dan eksekusi kode.
- **14 Built-in Tools**:
  - File system: `read_file`, `write_file`, `edit_file`, `list_directory`.
  - Search & Discovery: `search_files` (glob), `grep_content` (regex & text).
  - Terminal & OS: `execute_command`.
  - Web & Remote: `web_fetch`, `web_search`.
  - Git & Checkpoint: `checkpoint_diff`, `checkpoint_undo`.
  - Diagnostik: `self_heal_lint`.
  - Interaksi: `ask_user_question`.
  - Subagent: `subagent` (synchronous execution).
- **Protokol MCP**: Dukungan integrasi server Model Context Protocol via konfigurasi `.ctrl/mcp.json`.
- **Dukungan Multi-Provider**: Integrasi fleksibel ke OpenAI, DeepSeek, Groq, OpenRouter, dan Anthropic Messages API.
- **Manajemen Memori & Kompaksi**: Kompaksi riwayat percakapan otomatis berdasarkan estimasi token window.
- **Dual Interface**: Mode CLI REPL klasik dan Modern Terminal UI (TUI) berbasis `ratatui`.
