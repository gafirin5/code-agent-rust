# 🛠️ Referensi Tool Lengkap (Tools Reference Manual)

Dokumen ini adalah kamus referensi resmi seluruh tool bawaan (*built-in tools*) dan ekstensi MCP pada **ctrl-cli**.

Setiap tool dilengkapi dengan spesifikasi parameter, tipe data, nilai bawaan, status keamanan izin (*permission level*), serta contoh payload JSON.

---

## 📋 Ringkasan Katalog Tool

| Nama Tool | Kategori | Tingkat Izin | Deskripsi Singkat |
|-----------|----------|--------------|-------------------|
| [`read_file`](#1-read_file) | Filesystem | `ReadOnly` | Membaca isi file teks dengan opsi rentang baris. |
| [`write_file`](#2-write_file) | Filesystem | `Mutating` | Menulis/menimpa file teks beserta pembuatan folder induk. |
| [`edit_file`](#3-edit_file) | Filesystem | `Mutating` | Mengganti chunk teks unik dalam file tanpa menulis ulang seluruh isi. |
| [`glob_files`](#4-glob_files) | Search | `ReadOnly` | Mencari pola file berdasarkan glob pattern. |
| [`grep_files`](#5-grep_files) | Search | `ReadOnly` | Pencarian teks literal di seluruh repositori dengan context line. |
| [`shell`](#6-shell) | Terminal | `Mutating` | Menjalankan perintah terminal lokal dengan batas waktu timeout. |
| [`read_tool_result`](#7-read_tool_result) | System | `ReadOnly` | Membaca baris lanjutan dari output tool yang terpotong (*truncated*). |
| [`ask_user_question`](#8-ask_user_question) | Interaksi | `ReadOnly` | Mengajukan pertanyaan interaktif kepada pengguna di terminal. |
| [`skill`](#9-skill) | Workflow | `ReadOnly` | Memuat instruksi panduan spesialis (misal: `rust-expert`). |
| [`manage_memory`](#10-manage_memory) | Memory | `Mutating` | Membaca atau menyimpan catatan arsitektur ke `.ctrl/MEMORY.md`. |
| [`code_check`](#11-code_check) | Diagnostik | `ReadOnly` | Menjalankan pemeriksaan compiler sintaksis (`cargo check`, `tsc`, `python`). |
| [`web_fetch`](#12-web_fetch) | Jaringan | `ReadOnly` | Mengunduh konten halaman web dan mengonversinya ke Markdown bersih. |
| [`web_search`](#13-web_search) | Jaringan | `ReadOnly` | Melakukan pencarian web teknis via DuckDuckGo. |
| [`subagent`](#14-subagent) | Orkestrasi | `ReadOnly` / `Mutating` | Mendelegasikan sub-tugas secara sinkron atau background non-blocking. |
| [`manage_task`](#15-manage_task) | Orkestrasi | `ReadOnly` / `Mutating` | Memeriksa status, menunggu (`await`), membatalkan, atau membaca log task. |

---

## 1. `read_file`
Membaca isi file teks dari sistem file lokal.

- **Parameters**:
  - `path` *(string, wajib)*: Jalur absolut atau relatif file yang ingin dibaca.
  - `start_line` *(integer, opsional)*: Nomor baris awal pembacaan (1-indexed, inklusif).
  - `end_line` *(integer, opsional)*: Nomor baris akhir pembacaan (1-indexed, inklusif).
- **Contoh Payload**:
  ```json
  {
    "path": "src/main.rs",
    "start_line": 1,
    "end_line": 50
  }
  ```

---

## 2. `write_file`
Menulis teks ke sebuah file. Jika direktori induk belum ada, direktori akan dibuat secara otomatis.

- **Parameters**:
  - `path` *(string, wajib)*: Path file tujuan.
  - `content` *(string, wajib)*: Konten teks lengkap yang akan ditulis.
  - `overwrite` *(boolean, opsional, default: `true`)*: Apakah menimpa file jika sudah ada.
- **Contoh Payload**:
  ```json
  {
    "path": "scripts/setup.sh",
    "content": "#!/bin/bash\necho 'Setup selesai.'\n",
    "overwrite": true
  }
  ```

---

## 3. `edit_file`
Melakukan pengeditan tepat sasaran pada file yang sudah ada dengan mengganti substring `target_content` menjadi `replacement_content`. Sangat hemat token dan mencegah penulisan ulang seluruh file.

- **Parameters**:
  - `path` *(string, wajib)*: Path file yang akan dimodifikasi.
  - `target_content` *(string, wajib)*: Potongan teks persis yang ada di file saat ini.
  - `replacement_content` *(string, wajib)*: Teks pengganti yang baru.
  - `allow_multiple` *(boolean, opsional, default: `false`)*: Mengizinkan penggantian jika ada lebih dari 1 kemunculan string target.
- **Contoh Payload**:
  ```json
  {
    "path": "src/lib.rs",
    "target_content": "pub fn old_function() -> bool { false }",
    "replacement_content": "pub fn new_function() -> bool { true }",
    "allow_multiple": false
  }
  ```

---

## 4. `glob_files`
Mencari daftar file berdasarkan pola glob.

- **Parameters**:
  - `pattern` *(string, wajib)*: Pola pencarian glob (misal: `*.rs`, `src/**/*.ts`).
  - `path` *(string, opsional, default: `"."`)*: Direktori root pencarian.
  - `mode` *(string, opsional, enum: `["list", "count"]`, default: `"list"`)*: Mode output berupa daftar file atau jumlah hitungan saja.
- **Contoh Payload**:
  ```json
  {
    "pattern": "**/*.rs",
    "path": "ctrl-cli",
    "mode": "list"
  }
  ```

---

## 5. `grep_files`
Pencarian teks mendalam melintasi file teks di workspace mirip dengan `ripgrep`.

- **Parameters**:
  - `query` *(string, wajib)*: Teks literal yang dicari.
  - `path` *(string, opsional, default: `"."`)*: Direktori pencarian.
  - `include` *(string, opsional)*: Pola filter ekstensi file (misal: `*.rs`).
  - `case_insensitive` *(boolean, opsional, default: `false`)*: Pencarian tidak sensitif huruf besar/kecil.
  - `head_limit` *(integer, opsional, default: `50`)*: Batas maksimal baris hasil temuan.
  - `offset` *(integer, opsional, default: `1`)*: Offset pagination temuan.
  - `context_lines` *(integer, opsional, default: `0`)*: Jumlah baris konteks sebelum dan sesudah baris yang cocok.
- **Contoh Payload**:
  ```json
  {
    "query": "TaskManager",
    "path": "src",
    "include": "*.rs",
    "context_lines": 2
  }
  ```

---

## 6. `shell`
Mengeksekusi perintah shell pada sistem operasi lokal dan mengembalikan kode keluar (*exit code*), stdout, dan stderr.

- **Parameters**:
  - `command` *(string, wajib)*: Perintah shell yang akan dijalankan.
  - `timeout_secs` *(integer, opsional, default: `60`)*: Batas waktu eksekusi dalam detik.
- **Contoh Payload**:
  ```json
  {
    "command": "cargo check",
    "timeout_secs": 120
  }
  ```

---

## 7. `read_tool_result`
Membaca baris lanjutan dari output tool sebelumnya yang terpotong karena melebihi batas token buffer.

- **Parameters**:
  - `result_id` *(string, wajib)*: ID hasil penyimpanan (misal: `"tr_1"`).
  - `offset` *(integer, wajib)*: Baris awal (1-indexed).
  - `limit` *(integer, wajib)*: Jumlah baris yang ingin diambil.
- **Contoh Payload**:
  ```json
  {
    "result_id": "tr_1",
    "offset": 51,
    "limit": 50
  }
  ```

---

## 8. `ask_user_question`
Mengajukan pertanyaan interaktif di terminal saat agen membutuhkan klarifikasi atau keputusan desain dari pengguna.

- **Parameters**:
  - `question` *(string, wajib)*: Pertanyaan yang diajukan ke pengguna.
  - `options` *(array of strings, opsional)*: Pilihan opsi jawaban jika berupa pilihan ganda.
- **Contoh Payload**:
  ```json
  {
    "question": "Apakah Anda ingin menggunakan SQLite atau PostgreSQL untuk database?",
    "options": ["SQLite (Default)", "PostgreSQL"]
  }
  ```

---

## 9. `skill`
Memuat instruksi kontekstual untuk skill spesialis tertentu.

- **Parameters**:
  - `name` *(string, wajib)*: ID skill yang ingin diaktifkan (misal: `"rust-expert"`, `"code-reviewer"`, `"security-auditor"`).
- **Contoh Payload**:
  ```json
  {
    "name": "rust-expert"
  }
  ```

---

## 10. `manage_memory`
Membaca atau memperbarui catatan memori jangka panjang proyek pada file `.ctrl/MEMORY.md`.

- **Parameters**:
  - `action` *(string, wajib, enum: `["read", "append", "set"]`)*:
    - `"read"`: Membaca isi memori saat ini.
    - `"append"`: Menambahkan baris catatan baru.
    - `"set"`: Menimpa seluruh isi memori.
  - `content` *(string, opsional)*: Teks informasi yang akan dicatat.
- **Contoh Payload**:
  ```json
  {
    "action": "append",
    "content": "Port server backend telah dipindahkan ke port 8080."
  }
  ```

---

## 11. `code_check`
Menjalankan diagnosa sintaksis dan kompilasi proyek secara mandiri.

- **Parameters**:
  - `target` *(string, opsional)*: File target atau subdirektori (default: seluruh proyek).
- **Contoh Payload**:
  ```json
  {
    "target": "src/main.rs"
  }
  ```

---

## 12. `web_fetch`
Mengunduh halaman web via protokol HTTP/HTTPS dan membersihkan tag HTML menjadi format Markdown yang mudah dibaca.

- **Parameters**:
  - `url` *(string, wajib)*: Alamat URL lengkap.
- **Contoh Payload**:
  ```json
  {
    "url": "https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html"
  }
  ```

---

## 13. `web_search`
Melakukan pencarian informasi atau dokumentasi teknis di web menggunakan DuckDuckGo.

- **Parameters**:
  - `query` *(string, wajib)*: Kata kunci pencarian.
  - `num_results` *(integer, opsional, default: `5`, max: `10`)*: Jumlah hasil pencarian yang dikembalikan.
- **Contoh Payload**:
  ```json
  {
    "query": "rust std sync condvar example",
    "num_results": 3
  }
  ```

---

## 14. `subagent`
Mendelegasikan tugas ke subagent otonom yang bekerja pada konteks percakapan terisolasi.

- **Parameters**:
  - `task` *(string, wajib)*: Deskripsi instruksi atau misi yang harus diselesaikan subagent.
  - `skill` *(string, opsional)*: ID spesialisasi skill yang ditugaskan (misal: `"debugger"`).
  - `model` *(string, opsional)*: Model AI alternatif untuk subagent.
  - `max_turns` *(integer, opsional, default: `8`)*: Batas maksimal giliran ReAct subagent.
  - `background` *(boolean, opsional, default: `false`)*:
    - Jika `false` (default): Berjalan secara sinkron dan langsung mengembalikan hasil saat selesai.
    - Jika `true`: Berjalan di thread background terisolasi dan segera mengembalikan ID task (`task-X`) tanpa memblokir.
- **Contoh Payload (Background)**:
  ```json
  {
    "task": "Jalankan cargo check dan audit seluruh file di folder tests/",
    "skill": "test-engineer",
    "background": true
  }
  ```

---

## 15. `manage_task`
Mengontrol dan memeriksa background task yang diluncurkan oleh `subagent(background=true)`.

- **Parameters**:
  - `action` *(string, wajib, enum: `["list", "status", "await", "cancel", "logs"]`)*:
    - `"list"`: Mengambil daftar ringkas semua background task dan statusnya.
    - `"status"`: Mengambil snapshot detail task tertentu beserta potongan log terbarunya.
    - `"await"`: Menunggu penyelesaian task hingga selesai.
    - `"cancel"`: Menghentikan eksekusi task yang sedang berjalan secara kooperatif.
    - `"logs"`: Mengambil riwayat log lengkap task dari buffer terisolasi.
  - `task_id` *(string, opsional)*: ID task target (wajib untuk action `status`, `await`, `cancel`, `logs`).
  - `timeout_secs` *(integer, opsional)*: Batas waktu tunggu dalam detik untuk action `await`.
  - `limit` *(integer, opsional, default: `100`)*: Batas maksimal baris log yang ditarik pada action `logs`.
- **Contoh Payload (Await)**:
  ```json
  {
    "action": "await",
    "task_id": "task-1",
    "timeout_secs": 60
  }
  ```
- **Contoh Payload (Logs)**:
  ```json
  {
    "action": "logs",
    "task_id": "task-1",
    "limit": 50
  }
  ```
