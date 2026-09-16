# ⚙️ Panduan Konfigurasi (Configuration Guide)

Dokumen ini menjelaskan secara lengkap seluruh konfigurasi sistem pada **ctrl-cli**, meliputi variabel lingkungan (`.env`), profil pengguna, model provider, integrasi MCP server, mode izin keamanan, serta parameter background task.

---

## 1. Variabel Lingkungan (`.env`)

File `.env` digunakan untuk mengatur kredensial API dan endpoint utama LLM. Salin `.env.example` menjadi `.env` di direktori proyek:

```bash
# Windows PowerShell
Copy-Item .env.example .env
```

### Parameter Utama

| Variabel | Wajib? | Default | Deskripsi |
|----------|--------|---------|-----------|
| `AI_API_KEY` | **Ya** | *(kosong)* | Kunci API resmi dari provider LLM yang digunakan. |
| `AI_BASE_URL` | Opsional | `https://api.openai.com/v1` | URL basis endpoint API provider. |
| `AI_MODEL` | Opsional | `gpt-4o-mini` | Model default yang akan digunakan saat memulai agen. |

### Contoh Konfigurasi Provider Populer

#### OpenAI
```env
AI_API_KEY=sk-proj-xxxxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://api.openai.com/v1
AI_MODEL=gpt-4o-mini
```

#### DeepSeek
```env
AI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://api.deepseek.com/v1
AI_MODEL=deepseek-chat
```

#### Groq Cloud (Ultra-Fast Inference)
```env
AI_API_KEY=gsk_xxxxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://api.groq.com/openai/v1
AI_MODEL=llama-3.3-70b-versatile
```

#### OpenRouter (Multi-Model Gateway)
```env
AI_API_KEY=sk-or-v1-xxxxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://openrouter.ai/api/v1
AI_MODEL=anthropic/claude-3.5-sonnet
```

#### Anthropic (Native Messages API)
```env
AI_API_KEY=sk-ant-xxxxxxxxxxxxxxxxxxxxxxxx
AI_BASE_URL=https://api.anthropic.com/v1
AI_MODEL=claude-3-5-sonnet-20241022
```

> [!NOTE]
> **Prioritas Pemuatan `.env`**: Sistem akan memeriksa keberadaan file `.env` pada direktori kerja saat ini (`std::env::current_dir()`). Jika tidak ditemukan, sistem akan beralih memeriksa path fallback di dalam direktori `ctrl-cli/.env`.

---

## 2. Profil Pengguna (`profile.json`)

Profil pengguna disimpan secara terpusat di folder home sistem operasi:
- **Windows**: `C:\Users\<Username>\.ctrl-cli\profile.json`
- **Linux/macOS**: `~/.ctrl-cli/profile.json`

File ini dibuat secara otomatis dengan nilai bawaan saat `ctrl-cli` pertama kali dijalankan.

### Struktur Skema JSON
```json
{
  "name": "galangfjr",
  "tech_stack": [
    "Python",
    "TypeScript",
    "Rust",
    "Zig"
  ],
  "response_language": "Bahasa Indonesia",
  "coding_style": "Tulis kode yang bersih (clean code), modern, idiomatik, efisien, dan minim dependensi tidak perlu. Berikan penjelasan ringkas dan solutif.",
  "show_token_usage": true
}
```

### Penjelasan Properti

1. **`name`** *(string)*: Nama panggilan pengguna yang akan diintegrasikan ke dalam system prompt agen.
2. **`tech_stack`** *(array of strings)*: Daftar bahasa pemrograman atau framework prioritas yang akan diprioritaskan agen saat memberikan solusi kode.
3. **`response_language`** *(string)*: Bahasa komunikasi utama.
   - Pilihan standar: `"Bahasa Indonesia"`, `"English"`, `"中文"`.
   - Dapat diubah cepat dari REPL menggunakan slash command `/lang id`, `/lang en`, atau `/lang zh`.
4. **`coding_style`** *(string)*: Preferensi instruksi arsitektur kode yang akan disisipkan ke agen.
5. **`show_token_usage`** *(boolean)*: Mengontrol tampilan badge visual metrik token (prompt tokens, completion tokens, dan persentase context window) di bawah respon chat.

---

## 3. Manajemen Provider Dinamis (`providers.json`)

`ctrl-cli` mendukung perpindahan provider secara dinamis tanpa perlu mengubah file `.env` secara manual setiap saat. Registry disimpan di `~/.ctrl-cli/providers.json`.

### Perintah Slash Command REPL untuk Provider
- `/provider list`: Menampilkan tabel seluruh provider yang terdaftar, status aktif, protocol, dan default model.
- `/provider switch <id>`: Beralih provider aktif (misal `/provider switch deepseek`).
- `/provider add`: Menambahkan provider baru secara interaktif (meminta nama, base URL, API key, protocol, dan batas context window).
- `/provider delete <id>`: Menghapus provider dari registry.
- `/provider probe`: Mengirimkan request uji konektivitas dan mengukur latensi serta mendeteksi batas context window secara otomatis.

---

## 4. Protokol MCP (Model Context Protocol)

`ctrl-cli` memiliki klien MCP bawaan yang memungkinkan agen terhubung ke server tool pihak ketiga (misalnya PostgreSQL inspector, Git context server, dsb).

### File Konfigurasi: `.ctrl/mcp.json`
Buat file konfigurasi `.ctrl/mcp.json` pada direktori root proyek:

```json
{
  "mcpServers": {
    "filesystem-extra": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "./target"],
      "env": {
        "DEBUG": "mcp:*"
      }
    },
    "custom-python-tool": {
      "command": "python",
      "args": ["scripts/mcp_server.py"],
      "env": {}
    }
  }
}
```

### Alur Kerja MCP
1. Saat startup, `ctrl-cli` membaca file `.ctrl/mcp.json`.
2. Meluncurkan proses server melalui transport `stdio`.
3. Mengirim payload RPC `tools/list` untuk membaca tool tambahan yang diekspos server.
4. Mendaftarkan skema tool tersebut secara otomatis ke dalam katalog tool LLM.

---

## 5. Kebijakan Keamanan & Izin (`PermissionMode`)

`ctrl-cli` menyediakan sistem kontrol akses berbasis peran (`PermissionGate`) untuk mencegah eksekusi instruksi yang merusak atau tidak diinginkan:

| Mode | Perilaku Keamanan | Cocok Digunakan Untuk |
|------|-------------------|-----------------------|
| `Ask` *(Default REPL)* | Memunculkan dialog prompt konfirmasi interaktif di terminal sebelum mengeksekusi operasi mutating (`write_file`, `edit_file`, `execute_command`). | Sesi interaktif harian bersama developer |
| `AutoApprove` | Memberikan izin eksekusi otomatis tanpa meminta konfirmasi pengguna di terminal. | Background Subagents, CI/CD pipeline, Mode Generate non-interaktif |
| `ReadOnly` | Memblokir sepenuhnya operasi yang mengubah state (write, edit, shell). Hanya mengizinkan inspeksi baca (`read_file`, `grep_content`, `list_directory`, `search_files`). | Audit keamanan kode, analisis read-only repositori asing |

---

## 6. Konfigurasi Background Task & Output Isolation

Sistem konkurensi subagent memiliki konfigurasi default berikut:

- **Direktori Log Task**: `.ctrl/tasks/<task-id>.log`
  - Setiap background task yang dijalankan mencatat riwayat diagnostik lengkapnya ke file ini.
- **Kapasitas Buffer In-Memory (`TaskLogBuffer`)**:
  - Menyimpan hingga **5.000 baris log terbaru** per task secara thread-safe menggunakan lock internal `RwLock<VecDeque<String>>`.
- **Batas Default Paginasi Log**:
  - Pada tool `manage_task(action: "logs")`, batas default adalah 100 baris (dapat diubah via parameter `limit`).
  - Pada `/tasks logs <id> [limit]`, baris paling akhir akan ditampilkan dengan ringkasan jumlah baris sebelumnya yang dipotong.
- **Batas Maksimal Turn Subagent**:
  - Nilai default adalah `15` turns (dapat disesuaikan hingga `50` turns melalui parameter `max_turns` pada tool `subagent`).
