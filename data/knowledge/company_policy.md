# Company Policy & Engineering Guidelines

Dokumen ini merupakan panduan dan kebijakan teknis yang harus dipatuhi oleh seluruh developer dan AI Agent dalam proyek.

## 1. Keamanan & Akses Sistem
- **Kerahasiaan API Key**: Dilarang keras melakukan commit atau mempublikasikan API Key (`AI_API_KEY`, credential cloud, token OAuth) ke dalam repositori publik. Seluruh kredensial harus dikelola via file `.env` atau *secret manager*.
- **Izin Mutasi File**: Setiap tool yang melakukan mutasi destruktif (`overwrite: true` pada file kritis atau perintah `shell` seperti `rm -rf`, `format`) harus melalui konfirmasi keamanan (*Permission Gate*).
- **Eksekusi Perintah Eksternal**: Jangan menjalankan perintah shell yang mengunduh dan mengeksekusi biner dari sumber yang tidak tepercaya (`curl | sh`).

## 2. Standar Kualitas Kode (Rust)
- **Zero Warnings**: Kode harus lolos `cargo check` dan `cargo clippy` tanpa warning.
- **Memory Safety & Idiom**: Utamakan penggunaan pola idiomatik Rust (RAII, `Result<T, E>`, ownership model). Hindari blok `unsafe` kecuali terdapat justifikasi performa kritis yang teruji.
- **Cakupan Pengujian**: Setiap penambahan modul atau fitur baru wajib disertai unit test atau integration test.
- **Konkurensi Aman**: Hindari *deadlock* dan *race condition*. Gunakan sinkronisasi standar (`Arc`, `Mutex`, `Condvar`, `AtomicBool`) dengan timeout atau mekanisme *cooperative cancellation*.

## 3. Etika Subagent & Delegasi
- Subagent yang didelegasikan tugas latar belakang (*background subagents*) harus selalu mengisolasi output diagnostik ke sink terpisah (`TaskLogBuffer`) agar tidak mengganggu antarmuka REPL pengguna utama.
- Subagent dilarang memblokir thread secara tak terbatas; gunakan batasan putaran (*max turns*) dan token pembatalan (*cancellation token*).
