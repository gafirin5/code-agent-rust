# Product FAQs: ctrl-cli & Multi-Agent Architecture

Pertanyaan umum mengenai arsitektur, cara kerja, dan fitur dari engine `ctrl-cli`.

### Q1: Apa keunggulan ctrl-cli dibandingkan framework agen Python?
**Jawaban:**
`ctrl-cli` dibangun menggunakan bahasa Rust murni (edisi 2021) tanpa runtime Python atau Node.js. Keunggulannya:
- **Ukuran Biner Sangat Ringan**: Hanya ~1.8 MB.
- **Konsumsi Memori Minimal**: Tanpa overhead garbage collection runtime besar, hemat RAM.
- **Konkurensi OS Native**: Subagent berjalan pada native OS worker threads dengan sinkronisasi thread-safe murni (`std::sync`).
- **Instan Startup**: Siap pakai tanpa `pip install` atau `virtualenv`.

### Q2: Bagaimana subagent mengelola output dan logging?
**Jawaban:**
Subagent yang berjalan di latar belakang (*background mode*) diarahkan ke `TaskLogBuffer` dan disimpan ke `.ctrl/tasks/<id>.log`. Terminal stdout utama tetap bersih untuk percakapan REPL interaktif pengguna, dan notifikasi status penyelesaian dikirim via badge notifikasi yang non-intrusif.

### Q3: Bagaimana cara agen mengakses dokumen di data/knowledge/?
**Jawaban:**
Agen menggunakan tool bawaan seperti:
- `read_file`: Untuk membaca seluruh atau sebagian baris file referensi.
- `grep_files`: Untuk mencari kata kunci/frasa spesifik di seluruh file `.md`.
- `glob_files`: Untuk mendaftar dokumen knowledge yang tersedia (`data/knowledge/*.md`).

### Q4: Di mana tempat menyimpan persona baru?
**Jawaban:**
Persona baru dapat disimpan di direktori `skills/<nama_skill>/SKILL.md` atau `prompts/<nama_persona>_persona.md`. Engine `ctrl-cli` dapat langsung memuatnya menggunakan tool `skill` atau slash command terkait.
