# Project Rules & Guidelines: `code-agent-rust` (`ctrl-cli`)

Dokumen ini mendefinisikan aturan kerja, batas arsitektur, dan pedoman pengembangan untuk asisten AI saat beroperasi di repositori ini.

---

## 1. Web UI & Dashboard (`dashboard.html`)
- **File Otoritatif**: Berkas visual dashboard web tunggal adalah [`ctrl-cli/src/dashboard.html`](ctrl-cli/src/dashboard.html).
- **Zero External CDN**: Semua styling, skrip logika, grafik (HTML5 Canvas sparklines), dan ikon harus mandiri secara lokal (100% offline, latensi 0 ms). Dilarang menyematkan tautan CDN eksternal (misal: Google Fonts, Tailwind CDN, CDNJS, FontAwesome CDN, dsb).
- **Dual Mode Protocol Compatibility**:
  - Saat diakses melalui server HTTP tersemat (`http://127.0.0.1:3000`), endpoint API menggunakan path relatif (`/api/...`).
  - Saat dibuka langsung via browser sebagai berkas lokal (`file:///.../dashboard.html`), skrip JavaScript harus mendeteksi `location.protocol === 'file:'` dan secara otomatis mengarahkan panggilan API ke `http://127.0.0.1:3000`.
- **Live Disk Reload**: Server di [`ctrl-cli/src/server.rs`](ctrl-cli/src/server.rs) memprioritaskan pembacaan berkas `dashboard.html` dari disk (fallback ke `include_str!`) agar perubahan pada file HTML dapat langsung diuji tanpa harus mengompilasi ulang biner.

---

## 2. Kontrak Data Telemetri & Endpoint API
- **Sinkronisasi Skema Rust <-> Frontend**:
  - Dilarang menebak nama atribut JSON di frontend. Selalu periksa struct Rust di [`ctrl-cli/src/telemetry/mod.rs`](ctrl-cli/src/telemetry/mod.rs) dan [`ctrl-cli/src/server.rs`](ctrl-cli/src/server.rs).
  - `threads`: Objek berstruktur `{ active_threads: u32, process_handles: u32 }` (bukan skalar / count biasa).
  - `storage`: Melacak jejak direktori internal `.ctrl/` (`ctrl_dir_bytes`, `formatted_ctrl`, `task_logs_bytes`, `file_count`), bukan kapasitas total partisi disk sistem operasi.
- **Karakteristik Pure Rust (Zero-GC)**:
  - Pada kondisi idle tanpa eksekusi agen/LLM yang aktif, proses `ctrl-cli` hanya memakan CPU < 0.4% dan RAM konstan ~9 MB. Ini adalah perilaku normal dan data yang akurat dari Windows OS API (`GetProcessMemoryInfo` & `GetProcessTimes`).

---

## 3. Manajemen Dokumentasi Ganda (Dual-Tier README)
Proyek ini memelihara dua berkas dokumentasi tingkat tinggi:
1. **Root [`README.md`](README.md)**: Halaman pengantar utama repositori GitHub (arsitektur makro, panduan penggunaan menyeluruh, setup cepat dari root).
2. **Crate [`ctrl-cli/README.md`](ctrl-cli/README.md)**: Dokumentasi resmi paket crate Rust untuk Cargo dan ekosistem crates.io.
- **Aturan**: Setiap ada pembaruan perintah CLI, sub-perintah baru, opsi flags, atau fitur utama (seperti TUI dan Web Dashboard), selalu periksa dan sinkronkan kedua berkas dokumentasi tersebut.

---

## 4. Batasan Biner & Kualitas Kode
- **Batas Ukuran Biner**: Biner rilis `ctrl-cli.exe` harus tetap ramping (**maksimal < 10 MB**, profil saat ini ~3.15 MB dengan `opt-level = "z"`, `lto = true`, `strip = true`). Memberikan kelonggaran ekspansi fitur tanpa membebani ukuran secara berlebihan.
- **Standar Pengujian**:
  - Pastikan seluruh pengujian unit lulus 100% (`cargo test`).
  - Pastikan linter `cargo clippy --bin ctrl-cli -- -D warnings` bersih tanpa error atau peringatan.

---

## 5. Struktur Dokumentasi & Master Index
- **Master Hub Navigasi**: Pusat navigasi dan katalog lengkap seluruh dokumen teknis, panduan operasional AI Agent, dan spesifikasi arsitektur berada di [`docs/README.md`](docs/README.md).
- **Arsip Historis**: Dokumen perencanaan masa lalu, catatan eksperimen, dan log kegagalan historis disimpan di [`docs/archive/`](docs/archive/).

