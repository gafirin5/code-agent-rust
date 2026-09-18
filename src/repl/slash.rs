use std::io::Write;

use inquire::{Confirm, Select, Text};

use crate::agent::memory::MemoryManager;
use crate::agent::permissions::{PermissionGate, PermissionMode};
use crate::agent::probe::{probe_provider_and_model, resolve_model_limits};
use crate::agent::provider::{ApiProtocol, ProviderConfig, ProvidersRegistry};
use crate::context::{get_model_context_info, SessionTokenTracker};
use crate::formatter::{
    detect_filename, format_compact_num, format_number, print_token_stats, save_code_to_file,
    textwrap_simple,
};
use crate::profile::{save_user_profile, SupportedLanguage, UserProfile};
use crate::skill::{get_available_skills, Skill};
use crate::tui;
use crate::types::ChatMessage;

#[allow(clippy::too_many_arguments)]
pub fn handle_slash_command(
    input: &str,
    current_model: &mut String,
    active_skill: &mut Option<Skill>,
    user_profile: &mut UserProfile,
    providers_reg: &mut ProvidersRegistry,
    tracker: &SessionTokenTracker,
    permission_gate: &mut PermissionGate,
    conversation: &mut Vec<ChatMessage>,
    streaming: &mut bool,
) -> bool {
    let trimmed = input.trim();
    if trimmed == "/" {
        let options = vec![
            "─── 🤖 Model & Agent Persona ─────────────────────────────────",
            "🤖 /model        • Pilih model AI aktif (GLM-5.3-Flash, Claude, DeepSeek)",
            "🧠 /skill        • Pilih peran spesialis (Rust Expert, Reviewer, Web UI)",
            "🌐 /lang         • Ganti bahasa respon (English, Bahasa Indonesia, 中文)",
            "🔌 /provider     • Kelola multi-provider (OpenAI, Anthropic, Ollama, Costum)",
            "🩺 /probe        • Auto-test koneksi, API key & deteksi context limits",
            "🛡️  /permissions  • Pengaturan izin keamanan (Ask / AutoApprove / ReadOnly)",
            "⚡ /stream       • Toggle streaming respons real-time SSE (on / off)",
            "─── 🛠️  Workspace & Checkpoint Engine ──────────────────────────",
            "🛠️  /tools        • Lihat 14 built-in agent tools (read, write, web, shell)",
            "🔄 /undo         • Batalkan (rollback) modifikasi berkas dari checkpoint",
            "🔍 /diff         • Lihat unified diff perubahan berkas / status git",
            "🩺 /check        • Self-healing diagnosa compiler & syntax error",
            "📦 /compact      • Kompaksi riwayat percakapan lama hemat context",
            "🔌 /mcp          • Status server Model Context Protocol (.ctrl/mcp.json)",
            "💾 /memory       • Catatan memori proyek (.ctrl/MEMORY.md)",
            "🕒 /checkpoints  • Riwayat snapshot modifikasi berkas",
            "─── 📊 Session & Utilities ───────────────────────────────────",
            "📊 /stats        • Pantau pemakaian resource real-time (RAM, CPU, storage)",
            "📊 /tokens       • Cek statistik token & progress bar context window",
            "📝 /save         • Simpan kode respon terakhir langsung ke file",
            "🧹 /reset        • Kosongkan riwayat percakapan (mulai sesi baru)",
            "👤 /profile      • Developer Profile & preferensi bahasa",
            "💻 /dev          • Info pengembang & arsitektur proyek",
            "ℹ️  /info         • Ringkasan konfigurasi endpoint & model",
            "✨ /clear        • Bersihkan layar terminal",
            "🖥️  /tui          • Beralih ke antarmuka grafis terminal (Ratatui TUI)",
            "❓ /help         • Bantuan lengkap semua perintah",
            "🚪 /exit         • Keluar dari ctrl-cli",
        ];
        match Select::new("Pilih perintah [/] — Menu Navigasi & Penjelasan Fitur (Gunakan panah ↑/↓, Enter untuk memilih):", options).prompt() {
            Ok(choice) => {
                if choice.starts_with("───") {
                    return true;
                }
                let cmd = choice.split_whitespace().find(|p| p.starts_with('/')).unwrap_or("");
                if !cmd.is_empty() {
                    return handle_slash_command(cmd, current_model, active_skill, user_profile, providers_reg, tracker, permission_gate, conversation, streaming);
                }
                return true;
            }
            _ => return true,
        }
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let cmd = parts[0].to_lowercase();

    match cmd.as_str() {
        "/help" | "?" => {
            println!("\n\x1B[1;36m╭─ ❓ ctrl-cli Available Slash Commands ─────────────────────────────────╮\x1B[0m");
            println!("  \x1B[1;33mModel & Persona\x1B[0m");
            println!("    \x1B[1;36m/model\x1B[0m \x1B[90m[nama]\x1B[0m       Pilih model AI aktif (GLM-5.3-Flash, Claude, dll)");
            println!("    \x1B[1;36m/skill\x1B[0m \x1B[90m[id|reset]\x1B[0m   Aktifkan peran spesialis (rust-expert, web, dll)");
            println!("    \x1B[1;36m/lang\x1B[0m  \x1B[90m[en|id|zh]\x1B[0m   Ganti bahasa respon (English, Indonesia, 中文)");
            println!("    \x1B[1;36m/provider\x1B[0m           Kelola provider API (list, switch, add, delete)");
            println!("    \x1B[1;36m/probe\x1B[0m              Uji koneksi, API key & batas context model");
            println!("    \x1B[1;36m/stream\x1B[0m \x1B[90m[on|off]\x1B[0m    Toggle streaming respons real-time (SSE)");
            println!("    \x1B[1;36m/permissions\x1B[0m        Atur mode izin tool (Ask / AutoApprove / ReadOnly)");
            println!("  \x1B[1;33mWorkspace & Tools\x1B[0m");
            println!("    \x1B[1;36m/tools\x1B[0m              Lihat 14 built-in agent tools");
            println!("    \x1B[1;36m/undo\x1B[0m               Rollback perubahan file terakhir dari checkpoint");
            println!("    \x1B[1;36m/diff\x1B[0m \x1B[90m[file]\x1B[0m        Lihat perbedaan (diff) perubahan file terkini");
            println!("    \x1B[1;36m/check\x1B[0m \x1B[90m[file]\x1B[0m       Jalankan compiler check (self-healing loop)");
            println!("    \x1B[1;36m/compact\x1B[0m            Ringkas riwayat sesi agar hemat context window");
            println!(
                "    \x1B[1;36m/mcp\x1B[0m                Status koneksi Model Context Protocol"
            );
            println!("    \x1B[1;36m/memory\x1B[0m             Lihat isi memori proyek (.ctrl/MEMORY.md)");
            println!(
                "    \x1B[1;36m/checkpoints\x1B[0m        Lihat riwayat snapshot modifikasi file"
            );
            println!("    \x1B[1;36m/tasks\x1B[0m \x1B[90m[sub]\x1B[0m        Kelola background tasks (list/view/cancel/wait/logs/clear)");
            println!("  \x1B[1;33mSession & Utilities\x1B[0m");
            println!("    \x1B[1;36m/\x1B[0m                   Buka menu interaktif (panah ↑/↓)");
            println!("    \x1B[1;36m/stats\x1B[0m \x1B[90m[json|table]\x1B[0m Pantau pemakaian resource real-time (RAM, CPU, storage)");
            println!("    \x1B[1;36m/tokens\x1B[0m \x1B[90m[toggle]\x1B[0m    Lihat statistik token & grafik context");
            println!("    \x1B[1;36m/save\x1B[0m \x1B[90m[nama_file]\x1B[0m   Simpan hasil kode terakhir langsung ke file");
            println!(
                "    \x1B[1;36m/reset\x1B[0m              Kosongkan riwayat percakapan (sesi baru)"
            );
            println!("    \x1B[1;36m/profile\x1B[0m            Lihat & edit profil developer / preferensi");
            println!("    \x1B[1;36m/info\x1B[0m               Cek konfigurasi endpoint, provider & model");
            println!("    \x1B[1;36m/clear\x1B[0m              Bersihkan layar terminal");
            println!("    \x1B[1;36m/tui\x1B[0m \x1B[90m[/gui]\x1B[0m         Beralih ke antarmuka grafis terminal (Ratatui TUI)");
            println!("    \x1B[1;36m/exit\x1B[0m               Keluar dari ctrl-cli");
            println!("\x1B[1;36m╰────────────────────────────────────────────────────────────────────────╯\x1B[0m");
            println!(
                "  \x1B[90m💡 Tip: Mendukung shortcut cepat: /comp, /und, /prov, /tok\x1B[0m\n"
            );
            true
        }
        "/lang" | "/language" => {
            if parts.len() > 1 {
                let target = parts[1].to_lowercase();
                let chosen = SupportedLanguage::from_str(&target);
                user_profile.response_language = chosen.display_name().to_string();
                save_user_profile(user_profile);
                match chosen {
                    SupportedLanguage::English => {
                        println!("\n✔ Active response language set to: English (System prompt & assistant reasoning updated).\n");
                    }
                    SupportedLanguage::Indonesian => {
                        println!("\n✔ Bahasa respon aktif disetel ke: Bahasa Indonesia (System prompt & penalaran asisten diperbarui).\n");
                    }
                    SupportedLanguage::Chinese => {
                        println!("\n✔ 当前回复语言已设置为：中文（简体中文）（系统提示词与助手思考逻辑已更新）。\n");
                    }
                }
            } else {
                let options = vec!["🇬🇧 English", "🇮🇩 Bahasa Indonesia", "🇨🇳 中文 (Chinese)"];
                match Select::new(
                    "Pilih bahasa respon AI agent (Select response language):",
                    options,
                )
                .prompt()
                {
                    Ok(selected) => {
                        let chosen = SupportedLanguage::from_str(selected);
                        user_profile.response_language = chosen.display_name().to_string();
                        save_user_profile(user_profile);
                        match chosen {
                            SupportedLanguage::English => {
                                println!("\n✔ Active response language set to: English.\n");
                            }
                            SupportedLanguage::Indonesian => {
                                println!("\n✔ Bahasa respon aktif disetel ke: Bahasa Indonesia.\n");
                            }
                            SupportedLanguage::Chinese => {
                                println!("\n✔ 当前回复语言已设置为：中文（简体中文）。\n");
                            }
                        }
                    }
                    _ => println!("Bahasa tidak berubah.\n"),
                }
            }
            true
        }
        "/provider" | "/providers" => {
            let sub = if parts.len() > 1 {
                parts[1].to_lowercase()
            } else {
                String::new()
            };
            match sub.as_str() {
                "list" => {
                    print_providers_list(providers_reg);
                }
                "switch" | "use" | "select" => {
                    if parts.len() > 2 {
                        let target_id = parts[2];
                        match providers_reg.switch_active(target_id) {
                            Ok(p) => {
                                *current_model = p.default_model.clone();
                                println!(
                                    "\n✔ Beralih ke provider: {} [{}] (Default Model: {})\n",
                                    p.name, p.protocol, current_model
                                );
                                probe_and_update_switched_provider(
                                    providers_reg,
                                    &p.id,
                                    current_model,
                                );
                            }
                            Err(e) => println!("\n❌ {}", e),
                        }
                    } else {
                        let options: Vec<String> = providers_reg
                            .providers
                            .iter()
                            .map(|p| {
                                let mark = if p.id == providers_reg.active_provider_id {
                                    "★ [ACTIVE] "
                                } else {
                                    "  "
                                };
                                format!("{}{:<18} │ {:<18} │ {}", mark, p.id, p.protocol, p.name)
                            })
                            .collect();

                        match Select::new("Pilih AI Provider aktif:", options).prompt() {
                            Ok(selected) => {
                                if let Some(id_part) = selected.split('│').next() {
                                    let clean_id =
                                        id_part.replace("★ [ACTIVE]", "").trim().to_string();
                                    if let Ok(p) = providers_reg.switch_active(&clean_id) {
                                        *current_model = p.default_model.clone();
                                        println!("\n✔ Beralih ke provider: {} [{}] (Default Model: {})\n", p.name, p.protocol, current_model);
                                        probe_and_update_switched_provider(
                                            providers_reg,
                                            &p.id,
                                            current_model,
                                        );
                                    }
                                }
                            }
                            _ => println!("Provider tetap.\n"),
                        }
                    }
                }
                "add" | "new" => {
                    if parts.len() >= 4 {
                        let id = parts[2].to_string();
                        let base_url = parts[3].to_string();
                        let api_key = parts.get(4).unwrap_or(&"").to_string();
                        let protocol = if let Some(&proto_str) = parts.get(6) {
                            ApiProtocol::from_str(proto_str)
                        } else if let Some(&proto_str) = parts.get(5) {
                            if proto_str.contains("anthropic")
                                || proto_str.contains("claude")
                                || proto_str.contains("openai")
                            {
                                ApiProtocol::from_str(proto_str)
                            } else {
                                ApiProtocol::OpenAi
                            }
                        } else {
                            ApiProtocol::from_str(&base_url)
                        };
                        let default_model = if let Some(&m) = parts.get(5) {
                            if m != "openai" && m != "anthropic" {
                                m.to_string()
                            } else {
                                match protocol {
                                    ApiProtocol::OpenAi => "gpt-4o-mini".to_string(),
                                    ApiProtocol::Anthropic => {
                                        "claude-3-5-sonnet-20241022".to_string()
                                    }
                                    ApiProtocol::Gemini => "gemini-2.5-flash".to_string(),
                                    ApiProtocol::Ollama => "qwen2.5-coder:7b".to_string(),
                                }
                            }
                        } else {
                            match protocol {
                                ApiProtocol::OpenAi => "gpt-4o-mini".to_string(),
                                ApiProtocol::Anthropic => "claude-3-5-sonnet-20241022".to_string(),
                                ApiProtocol::Gemini => "gemini-2.5-flash".to_string(),
                                ApiProtocol::Ollama => "qwen2.5-coder:7b".to_string(),
                            }
                        };
                        add_provider_direct(
                            providers_reg,
                            current_model,
                            id,
                            base_url,
                            api_key,
                            default_model,
                            protocol,
                        );
                    } else {
                        interactive_add_provider(providers_reg, current_model);
                    }
                }
                "delete" | "remove" | "rm" => {
                    if parts.len() > 2 {
                        let target_id = parts[2];
                        match providers_reg.remove(target_id) {
                            Ok(_) => println!("\n✔ Provider '{}' berhasil dihapus.\n", target_id),
                            Err(e) => println!("\n❌ {}", e),
                        }
                    } else {
                        let deletable: Vec<String> = providers_reg
                            .providers
                            .iter()
                            .filter(|p| p.id != providers_reg.active_provider_id)
                            .map(|p| format!("{:<16} │ {}", p.id, p.name))
                            .collect();

                        if deletable.is_empty() {
                            println!("\nTidak ada provider lain yang dapat dihapus.\n");
                        } else {
                            match Select::new("Pilih provider yang ingin dihapus:", deletable)
                                .prompt()
                            {
                                Ok(selected) => {
                                    if let Some(id) = selected.split('│').next() {
                                        let target_id = id.trim();
                                        match providers_reg.remove(target_id) {
                                            Ok(_) => println!(
                                                "\n✔ Provider '{}' berhasil dihapus.\n",
                                                target_id
                                            ),
                                            Err(e) => println!("\n❌ {}", e),
                                        }
                                    }
                                }
                                _ => println!("Batal menghapus.\n"),
                            }
                        }
                    }
                }
                "probe" | "check" | "test" => {
                    run_and_print_probe(providers_reg, current_model);
                }
                _ => {
                    println!("\n╭─────────────────────────────────────────────────────────────╮");
                    println!("│ 🔌 AI Provider Management Subcommands                       │");
                    println!("├─────────────────────────────────────────────────────────────┤");
                    println!("│  • /provider list         Daftar semua provider terkonfigurasi│");
                    println!("│  • /provider switch [id]  Ganti provider AI aktif          │");
                    println!("│  • /provider add          Tambah provider custom/resmi baru│");
                    println!("│  • /provider probe        Live test endpoint & deteksi limit│");
                    println!("│  • /provider delete [id]  Hapus konfigurasi provider       │");
                    println!("╰─────────────────────────────────────────────────────────────╯\n");
                    print_providers_list(providers_reg);
                }
            }
            true
        }
        "/probe" | "/check-model" => {
            run_and_print_probe(providers_reg, current_model);
            true
        }
        "/tools" => {
            println!("\n╭──────────────────┬────────────────────┬────────────────────────────────────────────────────────────╮");
            println!("│ Tool Name        │ Policy             │ Capabilities & Description                                 │");
            println!("├──────────────────┼────────────────────┼────────────────────────────────────────────────────────────┤");
            let tool_list = [
                (
                    "read_file",
                    "Safe (Auto)",
                    "Membaca konten berkas dengan range baris spesifik",
                ),
                (
                    "write_file",
                    "Mutating (Prompt)",
                    "Menulis berkas baru / menimpa (auto snapshot + self-heal)",
                ),
                (
                    "edit_file",
                    "Mutating (Prompt)",
                    "Mengganti blok kode eksak (auto snapshot + self-heal)",
                ),
                (
                    "glob_files",
                    "Safe (Auto)",
                    "Mencari path berkas pola wildcard (*.rs, *.py, dll.)",
                ),
                (
                    "grep_files",
                    "Safe (Auto)",
                    "Mencari pola teks literal di seluruh berkas proyek",
                ),
                (
                    "shell",
                    "Mutating (Prompt)",
                    "Mengeksekusi command terminal (stdout, stderr, exit code)",
                ),
                (
                    "read_tool_result",
                    "Safe (Auto)",
                    "Membaca chunk lanjutan hasil tool berukuran panjang",
                ),
                (
                    "ask_user_question",
                    "Interactive",
                    "Mengajukan pertanyaan klarifikasi interaktif ke user",
                ),
                (
                    "skill",
                    "Safe (Auto)",
                    "Memuat instruksi spesialis dari SKILL.md lokal",
                ),
                (
                    "manage_memory",
                    "Safe (Auto)",
                    "Membaca atau menulis memori proyek (.ctrl/MEMORY.md)",
                ),
                (
                    "code_check",
                    "Safe (Auto)",
                    "Cek diagnostik compiler (cargo check, python, tsc)",
                ),
                (
                    "web_fetch",
                    "Safe (Auto)",
                    "Fetch webpage dari URL & konversi ke markdown bersih",
                ),
                (
                    "web_search",
                    "Safe (Auto)",
                    "Pencarian DuckDuckGo untuk dokumentasi & solusi issue",
                ),
                (
                    "subagent",
                    "Autonomous",
                    "Delegasikan task cabang ke autonomous subagent terisolasi",
                ),
                (
                    "knowledge_search",
                    "Safe (Auto)",
                    "Pencarian BM25 in-process dokumen data/knowledge/*.md",
                ),
                (
                    "mcp__*",
                    "MCP Bridge",
                    "Tools eksternal dinamis dari server .ctrl/mcp.json",
                ),
            ];
            for (t_name, t_pol, t_desc) in tool_list {
                println!("│ {:<16} │ {:<18} │ {:<58} │", t_name, t_pol, t_desc);
            }
            println!("╰──────────────────┴────────────────────┴────────────────────────────────────────────────────────────╯\n");
            true
        }
        "/undo" => {
            if parts.len() > 1 && parts[1].eq_ignore_ascii_case("list") {
                let manifests = crate::agent::checkpoint::CheckpointManager::list_multi_checkpoints()
                    .unwrap_or_default();
                let records = crate::agent::checkpoint::CheckpointManager::list_checkpoints();

                if manifests.is_empty() && records.is_empty() {
                    println!("\nBelum ada snapshot checkpoint yang tersimpan.\n");
                } else {
                    println!("\n╭─────────────────────────────────────────────────────────────╮");
                    println!("│ 🕒 Checkpoint History (/undo list)                          │");
                    println!("├─────────────────────────────────────────────────────────────┤");
                    for m in &manifests {
                        println!(
                            "│ • [{}] {:<18} -> {} file(s) [{}]",
                            m.timestamp,
                            m.id,
                            m.files.len(),
                            m.files.join(", ")
                        );
                    }
                    for r in records.iter().rev().take(10) {
                        println!(
                            "│ • #{:<3} [{}] {:<8} -> {:<26}│",
                            r.id, r.timestamp, r.action, r.file_path
                        );
                    }
                    println!("╰─────────────────────────────────────────────────────────────╯\n");
                }
            } else {
                match crate::agent::checkpoint::CheckpointManager::undo_last() {
                    Ok(msg) => println!("\n{}\n", msg),
                    Err(e) => println!("\n❌ Undo error: {}\n", e),
                }
            }
            true
        }
        "/diff" => {
            let filter = if parts.len() > 1 {
                Some(parts[1])
            } else {
                None
            };
            match crate::agent::checkpoint::CheckpointManager::get_diff(filter) {
                Ok(diff) => {
                    println!("\n╭─────────────────────────────────────────────────────────────╮");
                    println!("│ 🔍 Unified Diff (Git & File Mutation Snapshots)             │");
                    println!("╰─────────────────────────────────────────────────────────────╯");
                    println!("{}", diff);
                    println!("───────────────────────────────────────────────────────────────\n");
                }
                Err(e) => println!("\n❌ Diff error: {}\n", e),
            }
            true
        }
        "/check" => {
            let target = if parts.len() > 1 {
                Some(parts[1])
            } else {
                None
            };
            match crate::tools::self_heal::run_code_check(target) {
                Ok(res) => println!("\n{}\n", res),
                Err(e) => println!("\n❌ Code check error: {}\n", e),
            }
            true
        }
        "/compact" => {
            let active_p = providers_reg.get_active_provider();
            match crate::agent::compaction::force_compact_context(
                conversation,
                current_model,
                &active_p.api_key,
                &active_p.base_url,
                active_p.protocol,
            ) {
                Ok(res) => println!("\n{}\n", res),
                Err(e) => println!("\n❌ Compaction error: {}\n", e),
            }
            true
        }
        "/mcp" => {
            let summary = crate::tools::mcp::McpManager::get_status_summary();
            println!("\n{}\n", summary);
            true
        }
        "/stream" => {
            if parts.len() > 1 {
                match parts[1].to_lowercase().as_str() {
                    "on" | "enable" | "true" => *streaming = true,
                    "off" | "disable" | "false" => *streaming = false,
                    "toggle" => *streaming = !*streaming,
                    _ => println!("Pilihan: /stream on | /stream off | /stream toggle"),
                }
            } else {
                *streaming = !*streaming;
            }
            println!(
                "\n✔ Real-time SSE streaming: {}\n",
                if *streaming { "AKTIF" } else { "NONAKTIF" }
            );
            true
        }
        "/checkpoints" => {
            let records = crate::agent::checkpoint::CheckpointManager::list_checkpoints();
            if records.is_empty() {
                println!("\nBelum ada snapshot checkpoint yang tersimpan.\n");
            } else {
                println!("\n╭─────────────────────────────────────────────────────────────╮");
                println!("│ 🕒 File Mutation Checkpoints History                        │");
                println!("├─────────────────────────────────────────────────────────────┤");
                for r in records.iter().rev().take(10) {
                    println!(
                        "│ • #{:<3} [{}] {:<8} -> {:<26}│",
                        r.id, r.timestamp, r.action, r.file_path
                    );
                }
                println!("╰─────────────────────────────────────────────────────────────╯\n");
            }
            true
        }
        "/permissions" | "/security" => {
            let current_str = match permission_gate.mode {
                PermissionMode::Ask => "Ask (Minta persetujuan sebelum modifikasi berkas/shell)",
                PermissionMode::AutoApprove => "AutoApprove (Otomatis izinkan semua tool)",
                PermissionMode::ReadOnly => "ReadOnly (Tolak semua aksi mutasi/shell)",
            };
            println!("\nStatus Permission Mode saat ini: {}\n", current_str);

            let options = vec![
                "🛡️  Ask          │ Minta konfirmasi untuk shell / write / edit (Recommended)",
                "⚡ AutoApprove  │ Eksekusi otomatis tanpa konfirmasi (Unattended / Fast mode)",
                "🔒 ReadOnly     │ Blokir semua eksekusi shell dan modifikasi berkas",
                "❌ Batal        │ Tidak berubah",
            ];

            match Select::new("Pilih Permission Mode baru:", options).prompt() {
                Ok(selected) => {
                    if selected.contains("Ask") {
                        permission_gate.mode = PermissionMode::Ask;
                        println!("✔ Permission Mode disetel ke: Ask\n");
                    } else if selected.contains("AutoApprove") {
                        permission_gate.mode = PermissionMode::AutoApprove;
                        println!("✔ Permission Mode disetel ke: AutoApprove\n");
                    } else if selected.contains("ReadOnly") {
                        permission_gate.mode = PermissionMode::ReadOnly;
                        println!("✔ Permission Mode disetel ke: ReadOnly\n");
                    }
                }
                _ => println!("Mode tetap.\n"),
            }
            true
        }
        "/memory" => {
            match MemoryManager::load_long_term_memory() {
                Some(mem) => {
                    println!("\n╭─────────────────────────────────────────────────────────────╮");
                    println!("│ 🧠 Workspace Long-Term Memory (.ctrl/MEMORY.md)             │");
                    println!("╰─────────────────────────────────────────────────────────────╯");
                    println!("{}", mem);
                    println!("───────────────────────────────────────────────────────────────\n");
                }
                None => {
                    println!("\nBelum ada catatan memori yang tersimpan di .ctrl/MEMORY.md.\nAI Agent akan otomatis mencatat file dan keputusan penting ke sini.\n");
                }
            }
            true
        }
        "/reset" | "/clear-history" => {
            conversation.clear();
            let _ = MemoryManager::clear_session_history();
            println!("\n✔ Riwayat percakapan & memori sesi berhasil dikosongkan. Konteks baru dimulai!\n");
            true
        }
        "/save" | "/write" => {
            if let Some(content) = &tracker.last_content {
                let target_file = if parts.len() > 1 {
                    parts[1..].join(" ")
                } else if let Some(detected) = detect_filename(content) {
                    println!("\nNama file terdeteksi dari respon: {}", detected);
                    detected
                } else {
                    let default_name = "output.txt".to_string();
                    match Text::new("Masukkan nama file tujuan:")
                        .with_default(&default_name)
                        .prompt()
                    {
                        Ok(val) => val,
                        Err(_) => return true,
                    }
                };

                match save_code_to_file(&target_file, content) {
                    Ok(summary) => println!("\n💾 Berhasil disimpan ke: {}\n", summary),
                    Err(e) => println!("\n❌ Gagal menyimpan file: {}\n", e),
                }
            } else {
                println!("\nBelum ada respon kode yang bisa disimpan di sesi ini.\n");
            }
            true
        }
        "/tokens" | "/usage" | "/context" => {
            let active_p = providers_reg.get_active_provider();
            if parts.len() > 1 {
                let sub = parts[1].to_lowercase();
                match sub.as_str() {
                    "toggle" => {
                        user_profile.show_token_usage = !user_profile.show_token_usage;
                        save_user_profile(user_profile);
                        println!(
                            "\n✔ Ringkasan token otomatis: {}\n",
                            if user_profile.show_token_usage {
                                "AKTIF (ditampilkan setelah setiap respon)"
                            } else {
                                "NONAKTIF (disembunyikan)"
                            }
                        );
                    }
                    "on" | "enable" | "true" => {
                        user_profile.show_token_usage = true;
                        save_user_profile(user_profile);
                        println!("\n✔ Ringkasan token otomatis: AKTIF\n");
                    }
                    "off" | "disable" | "false" => {
                        user_profile.show_token_usage = false;
                        save_user_profile(user_profile);
                        println!("\n✔ Ringkasan token otomatis: NONAKTIF\n");
                    }
                    _ => {
                        println!("\nSubcommand token tidak dikenal. Opsi: `/tokens`, `/tokens toggle`, `/tokens on`, `/tokens off`\n");
                    }
                }
            } else {
                print_token_stats(
                    tracker,
                    current_model,
                    Some(active_p),
                    user_profile.show_token_usage,
                );
            }
            true
        }
        "/model" => {
            if parts.len() > 1 {
                let new_model = parts[1..].join(" ");
                *current_model = new_model;
                println!("\n✔ Model AI aktif: {}\n", current_model);
            } else {
                let options = vec![
                    "⚡ glm-5.3-flash              │ 1M Ctx   │ 128k Out │ OpenAgentic / Zhipu (Default)",
                    "🧠 deepseek-reasoner          │ 64k Ctx  │ 8k Out   │ DeepSeek R1 Reasoning",
                    "💬 deepseek-chat              │ 64k Ctx  │ 8k Out   │ DeepSeek V3 General",
                    "🚀 glm-4-plus                 │ 128k Ctx │ 4k Out   │ Zhipu GLM-4 Flagship",
                    "🎭 claude-3-5-sonnet-20241022 │ 200k Ctx │ 8k Out   │ Anthropic Claude 3.5 Sonnet",
                    "✨ claude-3-7-sonnet          │ 200k Ctx │ 8k Out   │ Anthropic Claude 3.7 Sonnet",
                    "⚡ llama-3.3-70b-versatile    │ 128k Ctx │ 8k Out   │ Meta Llama 3.3 (Groq)",
                    "💻 qwen-2.5-coder-32b         │ 128k Ctx │ 8k Out   │ Qwen 2.5 Coder",
                    "🎯 gpt-4o-mini                │ 128k Ctx │ 16k Out  │ OpenAI Fast & Smart",
                    "🌟 gpt-4o                     │ 128k Ctx │ 16k Out  │ OpenAI Flagship",
                    "❌ Batal / Cancel",
                ];
                match Select::new("Pilih model AI (gunakan panah ↑/↓):", options).prompt() {
                    Ok(selected) if !selected.starts_with("❌") && !selected.contains("Batal") => {
                        if let Some(first_col) = selected.split('│').next() {
                            let model_name = first_col
                                .split_whitespace()
                                .last()
                                .unwrap_or("")
                                .to_string();
                            if !model_name.is_empty() {
                                *current_model = model_name;
                                println!("\n✔ Model AI aktif: {}\n", current_model);
                            }
                        }
                    }
                    _ => {
                        println!("\nModel tetap: {}\n", current_model);
                    }
                }
            }
            true
        }
        "/models" => handle_slash_command(
            "/model",
            current_model,
            active_skill,
            user_profile,
            providers_reg,
            tracker,
            permission_gate,
            conversation,
            streaming,
        ),
        "/skills" | "/skill-list" => {
            let sub = parts.get(1).map(|s| s.to_lowercase());
            match sub.as_deref() {
                Some("list") => {
                    let root = crate::tools::filesystem::get_workspace_root();
                    let discovered = crate::tools::skills::discover_skills(&root);
                    if discovered.is_empty() {
                        println!("\nTidak ada skill yang ditemukan di workspace (skills/, .ctrl/skills/, prompts/).\n");
                    } else {
                        println!("\n╭─────────────────────────────────────────────────────────────────────────────╮");
                        println!("│ 🧠 Discovered Agent Skills ({:<2} found)                                      │", discovered.len());
                        println!("├───────────────────────┬─────────────────────────────────────────────────────┤");
                        println!("│ {:<21} │ {:<51} │", "Skill Name", "Description");
                        println!("├───────────────────────┼─────────────────────────────────────────────────────┤");
                        for s in &discovered {
                            let desc = if s.description.chars().count() > 51 {
                                format!("{}...", s.description.chars().take(48).collect::<String>())
                            } else {
                                s.description.clone()
                            };
                            println!("│ {:<21} │ {:<51} │", s.name, desc);
                        }
                        println!("╰───────────────────────┴─────────────────────────────────────────────────────╯");
                        println!("Ketik `/skills info <name>` untuk melihat detail dan instruksi lengkap.\n");
                    }
                }
                Some("info") => {
                    if let Some(name) = parts.get(2) {
                        let root = crate::tools::filesystem::get_workspace_root();
                        if let Some(skill) = crate::tools::skills::get_skill_by_name(name, &root) {
                            println!("\n╭─────────────────────────────────────────────────────────────────────────────╮");
                            println!("│ ℹ️  Skill Details: {:<56}│", skill.name);
                            println!("├─────────────────────────────────────────────────────────────────────────────┤");
                            println!("│  • Name        : {:<58}│", skill.name);
                            println!("│  • Path        : {:<58}│", skill.path.display());
                            let tools_str = if skill.tools.is_empty() {
                                "All default tools allowed".to_string()
                            } else {
                                skill.tools.join(", ")
                            };
                            println!("│  • Tools       : {:<58}│", tools_str);
                            println!("│  • Description : {:<58}│", skill.description);
                            println!("├─────────────────────────────────────────────────────────────────────────────┤");
                            println!("│ 📝 Prompt Template / Instructions Snippet:                                  │");
                            println!("├─────────────────────────────────────────────────────────────────────────────┤");
                            for (i, line) in skill.prompt_template.lines().take(20).enumerate() {
                                println!("│ {:>2}: {:<71}│", i + 1, line.chars().take(71).collect::<String>());
                            }
                            if skill.prompt_template.lines().count() > 20 {
                                println!("│ ... ({} lines total)                                                        │", skill.prompt_template.lines().count());
                            }
                            println!("╰─────────────────────────────────────────────────────────────────────────────╯\n");
                        } else {
                            println!("\n❌ Skill '{}' tidak ditemukan di workspace.", name);
                            println!("   Gunakan `/skills list` untuk melihat semua skill yang tersedia.\n");
                        }
                    } else {
                        println!("\nPenggunaan: `/skills info <name>`");
                        println!("Contoh: `/skills info researcher`\n");
                    }
                }
                _ => {
                    println!("\n╭─────────────────────────────────────────────────────────────────────────────╮");
                    println!("│ 🧭 Skills Command Usage                                                     │");
                    println!("├─────────────────────────────────────────────────────────────────────────────┤");
                    println!("│  • /skills list         - Tampilkan semua skill yang ditemukan di workspace │");
                    println!("│  • /skills info <name>  - Tampilkan detail & prompt template suatu skill    │");
                    println!("╰─────────────────────────────────────────────────────────────────────────────╯");
                    let root = crate::tools::filesystem::get_workspace_root();
                    let discovered = crate::tools::skills::discover_skills(&root);
                    if !discovered.is_empty() {
                        println!("\nDiscovered Skills ({}) :", discovered.len());
                        for s in &discovered {
                            println!("  • {:<16} - {}", s.name, s.description);
                        }
                        println!("\nKetik `/skills info <name>` untuk detail atau `/skill` untuk memilih persona.\n");
                    }
                }
            }
            true
        }
        "/skill" => {
            if parts.len() > 1 && cmd == "/skill" {
                let target = parts[1].to_lowercase();
                if target == "reset" || target == "none" || target == "default" {
                    *active_skill = None;
                    println!("\n✔ Skill dinonaktifkan (kembali ke General Assistant).\n");
                } else if let Some(found) =
                    get_available_skills().into_iter().find(|s| s.id == target)
                {
                    println!("\n✔ Skill aktif: {} ({})\n", found.name, found.id);
                    *active_skill = Some(found);
                } else {
                    let root = crate::tools::filesystem::get_workspace_root();
                    if let Some(dyn_skill) = crate::tools::skills::get_skill_by_name(&target, &root) {
                        println!("\n✔ Dynamic Skill aktif: {} ({})\n", dyn_skill.name, dyn_skill.path.display());
                        *active_skill = Some(Skill::from(dyn_skill));
                    } else {
                        println!(
                            "\nSkill '{}' tidak ditemukan. Ketik `/skills list` untuk melihat daftar.\n",
                            parts[1]
                        );
                    }
                }
            } else {
                let mut options: Vec<String> = get_available_skills()
                    .iter()
                    .map(|s| format!("{:<28} │ {:<16} │ {}", s.name, s.id, s.description))
                    .collect();
                options.push("🔄 Reset Skill             │ default          │ Kembali ke mode General Assistant".to_string());
                options.push(
                    "❌ Batal / Cancel          │ cancel           │ Tidak berubah".to_string(),
                );

                match Select::new("Pilih Skill untuk AI Agent (gunakan panah ↑/↓):", options)
                    .prompt()
                {
                    Ok(selected) if !selected.starts_with("❌") && !selected.contains("Batal") => {
                        if selected.contains("Reset") || selected.contains("default") {
                            *active_skill = None;
                            println!("\n✔ Skill dinonaktifkan (kembali ke General Assistant).\n");
                        } else if let Some(id_col) = selected.split('│').nth(1) {
                            let id = id_col.trim();
                            if let Some(found) =
                                get_available_skills().into_iter().find(|s| s.id == id)
                            {
                                println!("\n✔ Skill aktif: {} ({})\n", found.name, found.id);
                                *active_skill = Some(found);
                            }
                        }
                    }
                    _ => {
                        if let Some(s) = active_skill {
                            println!("\nSkill tetap: {}\n", s.name);
                        } else {
                            println!("\nTetap di mode General Assistant.\n");
                        }
                    }
                }
            }
            true
        }
        "/dev" | "/developer" | "/author" | "/about" => {
            println!("\n╭─────────────────────────────────────────────────────────────╮");
            println!("│ 💻 About Developer & Architecture                           │");
            println!("├─────────────────────────────────────────────────────────────┤");
            println!("│  • Creator / Dev  : {:<40}│", user_profile.name);
            println!(
                "│  • Tech Stack     : {:<40}│",
                user_profile.tech_stack.join(", ")
            );
            println!(
                "│  • Project        : {:<40}│",
                "ctrl-cli (Autonomous Coding Agent)"
            );
            println!(
                "│  • Binary Size    : {:<40}│",
                "~2.1 MB (LTO & Strip Optimized)"
            );
            println!(
                "│  • Architecture   : {:<40}│",
                "Pure Rust ReAct loop + 14 Unix Tools"
            );
            println!(
                "│  • Multi-Provider : {:<40}│",
                "OpenAI-Compatible + Anthropic Native"
            );
            println!(
                "│  • Multilingual   : {:<40}│",
                "English, Bahasa Indonesia, 中文"
            );
            println!("╰─────────────────────────────────────────────────────────────╯\n");
            true
        }
        "/profile" => {
            println!("\n╭─────────────────────────────────────────────────────────────╮");
            println!("│ 👤 Developer Profile                                        │");
            println!("├─────────────────────────────────────────────────────────────┤");
            println!("│  • Nama Panggilan : {:<40}│", user_profile.name);
            println!(
                "│  • Tech Stack     : {:<40}│",
                user_profile.tech_stack.join(", ")
            );
            println!(
                "│  • Bahasa Respon  : {:<40}│",
                user_profile.response_language
            );
            println!(
                "│  • Token Badge    : {:<40}│",
                if user_profile.show_token_usage {
                    "Aktif"
                } else {
                    "Nonaktif"
                }
            );
            println!(
                "│  • Mode Default   : {:<40}│",
                user_profile
                    .default_ui
                    .as_deref()
                    .map(|s| if s.eq_ignore_ascii_case("tui") { "TUI (Modern Terminal UI)" } else { "CLI / REPL (Classic)" })
                    .unwrap_or("CLI / REPL (Auto)")
            );
            println!("├─────────────────────────────────────────────────────────────┤");
            println!("│  • Coding Style Guidelines:                                 │");
            for line in textwrap_simple(&user_profile.coding_style, 54) {
                println!("│    {:<57}│", line);
            }
            println!("╰─────────────────────────────────────────────────────────────╯");

            let edit_opts = vec![
                "🌐 Ganti Bahasa Respon (English / Indonesia / 中文)",
                "🖥️  Pilih Mode Default (TUI vs CLI/REPL)",
                "👤 Ubah Nama & Coding Style",
                "⬅️  Kembali",
            ];
            if let Ok(choice) = Select::new("Opsi profil:", edit_opts).prompt() {
                if choice.contains("Bahasa") {
                    handle_slash_command(
                        "/lang",
                        current_model,
                        active_skill,
                        user_profile,
                        providers_reg,
                        tracker,
                        permission_gate,
                        conversation,
                        streaming,
                    );
                } else if choice.contains("Pilih Mode Default") {
                    let ui_opts = vec![
                        "🖥️  TUI (Modern Terminal UI - Otomatis terbuka saat menjalankan ctrl-cli)",
                        "⌨️  CLI / REPL (Classic line-based prompt)",
                    ];
                    if let Ok(ui_choice) = Select::new("Pilih antarmuka default:", ui_opts).prompt() {
                        if ui_choice.contains("TUI") {
                            user_profile.default_ui = Some("tui".to_string());
                            println!("✔ Antarmuka default diatur ke TUI!");
                        } else {
                            user_profile.default_ui = Some("cli".to_string());
                            println!("✔ Antarmuka default diatur ke CLI / REPL!");
                        }
                        save_user_profile(user_profile);
                        println!("✔ Profil berhasil disimpan!\n");
                    }
                } else if choice.contains("Ubah Nama") {
                    if let Ok(new_name) = Text::new("Nama panggilan:")
                        .with_default(&user_profile.name)
                        .prompt()
                    {
                        user_profile.name = new_name;
                        save_user_profile(user_profile);
                        println!("✔ Profil berhasil disimpan!\n");
                    }
                }
            }
            true
        }
        "/info" | "/config" => {
            let active_p = providers_reg.get_active_provider();
            let ctx_info = get_model_context_info(current_model, Some(active_p));
            let perm_str = match permission_gate.mode {
                PermissionMode::Ask => "Ask (Prompt on mutations)",
                PermissionMode::AutoApprove => "AutoApprove (Unattended / Fast)",
                PermissionMode::ReadOnly => "ReadOnly (Safe guard)",
            };
            println!("\n\x1B[1;36m╭─ ℹ️  Active Session Configuration ──────────────────────────────╮\x1B[0m");
            println!(
                "  \x1B[90mProvider       :\x1B[0m \x1B[1;37m{}\x1B[0m \x1B[90m[{}]\x1B[0m",
                active_p.name, active_p.protocol
            );
            println!(
                "  \x1B[90mEndpoint       :\x1B[0m \x1B[36m{}\x1B[0m",
                active_p.base_url
            );
            println!(
                "  \x1B[90mActive Model   :\x1B[0m \x1B[1;37m{}\x1B[0m",
                current_model
            );
            println!(
                "  \x1B[90mFamily / Note  :\x1B[0m \x1B[37m{}\x1B[0m",
                ctx_info.note
            );
            println!(
                "  \x1B[90mContext Window :\x1B[0m \x1B[36m{} tokens\x1B[0m",
                format_number(ctx_info.context_window)
            );
            if let Some(mo) = ctx_info.max_output {
                println!(
                    "  \x1B[90mMax Output     :\x1B[0m \x1B[36m{} tokens\x1B[0m",
                    format_number(mo)
                );
            }
            println!(
                "  \x1B[90mResponse Lang  :\x1B[0m \x1B[32m{}\x1B[0m",
                user_profile.response_language
            );
            println!(
                "  \x1B[90mActive Skill   :\x1B[0m \x1B[35m{}\x1B[0m",
                active_skill
                    .as_ref()
                    .map(|s| s.name.as_ref())
                    .unwrap_or("General Assistant")
            );
            println!(
                "  \x1B[90mPermission Mode:\x1B[0m \x1B[33m{}\x1B[0m",
                perm_str
            );
            println!(
                "  \x1B[90mHistory Turns  :\x1B[0m \x1B[37m{} messages in context\x1B[0m",
                conversation.len()
            );
            println!(
                "  \x1B[90mReal-Time SSE  :\x1B[0m {}",
                if *streaming {
                    "\x1B[32mAktif\x1B[0m"
                } else {
                    "\x1B[90mNonaktif\x1B[0m"
                }
            );
            println!(
                "  \x1B[90mSession Tokens :\x1B[0m \x1B[37m{} ({} queries)\x1B[0m",
                format_number(tracker.total_tokens),
                tracker.query_count
            );
            println!("\x1B[1;36m╰─────────────────────────────────────────────────────────────────╯\x1B[0m\n");
            true
        }
        "/clear" => {
            print!("\x1B[2J\x1B[1;1H");
            let _ = std::io::stdout().flush();
            true
        }
        "/tasks" => {
            use crate::agent::tasks::TaskManager;
            let tm = TaskManager::global();
            let sub = if parts.len() > 1 {
                parts[1].to_lowercase()
            } else {
                "list".to_string()
            };

            match sub.as_str() {
                "list" | "ls" => {
                    let snapshots = tm.list_tasks();
                    if snapshots.is_empty() {
                        println!("\nTidak ada background task yang terdaftar.\n");
                    } else {
                        println!(
                            "\n╭─────────────────────────────────────────────────────────────╮"
                        );
                        println!(
                            "│ 🚀 Background Tasks ({} total){:>34}│",
                            snapshots.len(),
                            ""
                        );
                        println!("├─────────────────────────────────────────────────────────────┤");
                        for snap in &snapshots {
                            let desc_short = if snap.description.len() > 35 {
                                format!("{}...", &snap.description[..32])
                            } else {
                                snap.description.clone()
                            };
                            println!(
                                "│ {} {:<10} {:<37} {:>6} │",
                                snap.status.badge(),
                                snap.id,
                                desc_short,
                                snap.elapsed_human
                            );
                        }
                        println!(
                            "╰─────────────────────────────────────────────────────────────╯\n"
                        );
                    }
                }
                "view" | "status" => {
                    if parts.len() < 3 {
                        println!("\nGunakan: /tasks view <task_id>\n");
                    } else {
                        let task_id = parts[2];
                        match tm.get_task(task_id) {
                            Some(snap) => {
                                println!("\n╭───── Task Detail ─────────────────────────────────────────╮");
                                println!("  ID:          {}", snap.id);
                                println!("  Name:        {}", snap.name);
                                println!("  Status:      {}", snap.status.badge());
                                println!("  Description: {}", snap.description);
                                if !snap.dependencies.is_empty() {
                                    println!("  Depends On:  {}", snap.dependencies.join(", "));
                                }
                                println!("  Created:     {}", snap.created_at);
                                if let Some(ref s) = snap.started_at {
                                    println!("  Started:     {}", s);
                                }
                                if let Some(ref f) = snap.finished_at {
                                    println!("  Finished:    {}", f);
                                }
                                println!("  Elapsed:     {}", snap.elapsed_human);
                                if let Some(ref r) = snap.result {
                                    let preview = if r.len() > 300 { &r[..300] } else { r };
                                    println!(
                                        "  Result:      {}{}",
                                        preview,
                                        if r.len() > 300 { "..." } else { "" }
                                    );
                                }
                                if let Some(ref e) = snap.error {
                                    println!("  Error:       {}", e);
                                }
                                println!("╰──────────────────────────────────────────────────────────╯\n");
                            }
                            None => println!("\nTask '{}' tidak ditemukan.\n", task_id),
                        }
                    }
                }
                "cancel" => {
                    if parts.len() < 3 {
                        println!("\nGunakan: /tasks cancel <task_id>\n");
                    } else {
                        let task_id = parts[2];
                        match tm.cancel_task(task_id) {
                            Ok(()) => {
                                tm.mark_task_notified(task_id);
                                println!("\n✔ Task '{}' berhasil dibatalkan.\n", task_id);
                            }
                            Err(e) => println!("\n❌ Gagal membatalkan task: {}\n", e),
                        }
                    }
                }
                "wait" => {
                    if parts.len() < 3 {
                        println!("\nGunakan: /tasks wait <task_id> [timeout_secs]\n");
                    } else {
                        let task_id = parts[2];
                        let timeout = parts
                            .get(3)
                            .and_then(|s| s.parse::<u64>().ok())
                            .map(std::time::Duration::from_secs);
                        println!("\n⏳ Menunggu task '{}' selesai...\n", task_id);
                        match tm.await_task(task_id, timeout) {
                            Ok(snap) => {
                                tm.mark_task_notified(task_id);
                                println!(
                                    "✔ Task '{}' selesai: {} ({})\n",
                                    snap.id,
                                    snap.status.as_str(),
                                    snap.elapsed_human
                                );
                                if let Some(ref r) = snap.result {
                                    let preview = if r.len() > 500 { &r[..500] } else { r };
                                    println!(
                                        "{}{}\n",
                                        preview,
                                        if r.len() > 500 { "..." } else { "" }
                                    );
                                }
                                if let Some(ref e) = snap.error {
                                    println!("Error: {}\n", e);
                                }
                            }
                            Err(e) => println!("❌ Gagal menunggu task: {}\n", e),
                        }
                    }
                }
                "logs" | "log" => {
                    if parts.len() < 3 {
                        println!("\nGunakan: /tasks logs <task_id>\n");
                    } else {
                        let task_id = parts[2];
                        match tm.get_task_logs(task_id) {
                            Some(log_lines) => {
                                if log_lines.is_empty() {
                                    println!("\nBelum ada log untuk task '{}'.\n", task_id);
                                } else {
                                    println!(
                                        "\n╭───── Task Logs: {} ({} lines) ─────╮",
                                        task_id,
                                        log_lines.len()
                                    );
                                    let start = if log_lines.len() > 50 {
                                        log_lines.len() - 50
                                    } else {
                                        0
                                    };
                                    for line in &log_lines[start..] {
                                        println!("  {}", line);
                                    }
                                    if start > 0 {
                                        println!("  ... ({} earlier lines omitted)", start);
                                    }
                                    println!("╰──────────────────────────────────────────────────────────╯\n");
                                }
                            }
                            None => println!("\nTask '{}' tidak ditemukan.\n", task_id),
                        }
                    }
                }
                "clear" => {
                    let removed = tm.clear_completed();
                    println!("\n✔ {} completed task(s) dihapus dari registry.\n", removed);
                }
                _ => {
                    println!("\nSub-perintah tidak dikenal: '{}'\n", sub);
                    println!(
                        "Gunakan: /tasks [list|view <id>|cancel <id>|wait <id>|logs <id>|clear]\n"
                    );
                }
            }
            true
        }
        "/stats" | "/metrics" | "/telemetry" | "/resources" => {
            let sub = if parts.len() > 1 {
                parts[1].to_lowercase()
            } else {
                String::new()
            };

            let ctrl_path = std::path::Path::new(".ctrl");
            let ctrl_dir = if ctrl_path.exists() {
                Some(ctrl_path)
            } else {
                None
            };
            let metrics = crate::telemetry::capture_metrics(ctrl_dir);

            match sub.as_str() {
                "" | "table" => {
                    println!("\n{}\n", crate::telemetry::format_metrics_table(&metrics));
                }
                "json" | "--json" => match serde_json::to_string_pretty(&metrics) {
                    Ok(json_str) => println!("{}", json_str),
                    Err(e) => eprintln!("Error serializing metrics to JSON: {}", e),
                },
                "reset" => {
                    let _ = crate::telemetry::capture_metrics(ctrl_dir);
                    println!("\n✔ Telemetry baselines reset.\n");
                }
                "help" | "--help" | "-h" => {
                    println!("\nGunakan: /stats [table|json|--json|reset]\n");
                }
                _ => {
                    println!("\nSub-perintah tidak dikenal: '{}'\n", sub);
                    println!("Gunakan: /stats [table|json|--json|reset]\n");
                }
            }
            true
        }
        "/tui" | "/gui" => {
            println!("\nBeralih ke mode TUI (Ratatui)...");
            let res = tui::run_tui(user_profile, providers_reg);
            if let Err(e) = res {
                eprintln!("\n❌ Gagal menjalankan TUI: {}\n", e);
                return true;
            }
            if tui::app::take_return_to_repl() {
                println!("\nKembali ke REPL. Ketik /help untuk bantuan atau /tui untuk kembali ke TUI.\n");
                true
            } else {
                false
            }
        }
        "/exit" | "/quit" => false,
        _ => {
            println!(
                "\nUnknown command '{}'. Type /help for available commands.\n",
                parts[0]
            );
            true
        }
    }
}

fn print_providers_list(registry: &ProvidersRegistry) {
    println!("\n\x1B[1;36m╭─ 🔌 Configured AI Providers ───────────────────────────────────────────────────────────╮\x1B[0m");
    println!(
        "  \x1B[90m{:<18} {:<10} {:<30} {:<14} {:<16}\x1B[0m",
        "PROVIDER ID", "PROTOCOL", "BASE URL", "DEF. MODEL", "CONTEXT / OUT"
    );
    println!("  \x1B[90m────────────────────────────────────────────────────────────────────────────────────────\x1B[0m");
    for p in &registry.providers {
        let is_active = p.id == registry.active_provider_id;
        let star = if is_active {
            "\x1B[1;32m★\x1B[0m"
        } else {
            " "
        };
        let id_colored = if is_active {
            format!("{} \x1B[1;32m{:<16}\x1B[0m", star, p.id)
        } else {
            format!("{} \x1B[37m{:<16}\x1B[0m", star, p.id)
        };
        let proto_str = match p.protocol {
            ApiProtocol::OpenAi => "OpenAI",
            ApiProtocol::Anthropic => "Anthropic",
            ApiProtocol::Gemini => "Gemini",
            ApiProtocol::Ollama => "Ollama",
        };
        let ctx_str = match (p.context_window, p.max_output_tokens) {
            (Some(c), Some(o)) => format!("{}/{}", format_compact_num(c), format_compact_num(o)),
            (Some(c), None) => format_compact_num(c),
            _ => "Auto".to_string(),
        };
        let short_url = if p.base_url.len() > 28 {
            format!("{}...", &p.base_url[..25])
        } else {
            p.base_url.clone()
        };
        let short_model = if p.default_model.len() > 13 {
            format!("{}...", &p.default_model[..10])
        } else {
            p.default_model.clone()
        };
        println!(
            "  {} {:<10} {:<30} {:<14} {:<16}",
            id_colored, proto_str, short_url, short_model, ctx_str
        );
    }
    println!("  \x1B[90m────────────────────────────────────────────────────────────────────────────────────────\x1B[0m");
    println!("  \x1B[90mActive Provider: \x1B[1;32m★ {}\x1B[0m \x1B[90m(/provider switch [id] to switch)\x1B[0m\n", registry.active_provider_id);
}

fn interactive_add_provider(registry: &mut ProvidersRegistry, current_model: &mut String) {
    println!("\n➕ Tambah Provider AI Baru (OpenAI-Compatible atau Anthropic Messages API)");

    let id = match Text::new("Provider ID (slug unik, contoh: my-vllm, deepseek-local):").prompt() {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => return,
    };

    let name = match Text::new("Display Name (contoh: My Custom vLLM, DeepSeek Official):").prompt()
    {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => id.clone(),
    };

    let proto_opts = vec![
        "OpenAI-Compatible (Endpoint: /chat/completions, Bearer Token)",
        "Anthropic Messages API (Endpoint: /messages, x-api-key)",
        "Google Gemini AI Studio (Endpoint: /v1beta/models, x-goog-api-key)",
        "Ollama Native API (Endpoint: /api/chat, local)",
    ];
    let protocol = match Select::new("Pilih protokol API:", proto_opts).prompt() {
        Ok(choice) => {
            if choice.starts_with("Anthropic") {
                ApiProtocol::Anthropic
            } else if choice.starts_with("Google") || choice.contains("Gemini") {
                ApiProtocol::Gemini
            } else if choice.starts_with("Ollama") {
                ApiProtocol::Ollama
            } else {
                ApiProtocol::OpenAi
            }
        }
        _ => return,
    };

    let default_url = match protocol {
        ApiProtocol::OpenAi => "https://api.openai.com/v1",
        ApiProtocol::Anthropic => "https://api.anthropic.com/v1",
        ApiProtocol::Gemini => "https://generativelanguage.googleapis.com",
        ApiProtocol::Ollama => "http://localhost:11434",
    };
    let base_url = match Text::new("Base URL endpoint:")
        .with_default(default_url)
        .prompt()
    {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => return,
    };

    let api_key = match Text::new("API Key (kosongkan jika lokal/ollama tanpa auth):").prompt() {
        Ok(v) => v.trim().to_string(),
        _ => String::new(),
    };

    let default_model_hint = match protocol {
        ApiProtocol::OpenAi => "gpt-4o-mini",
        ApiProtocol::Anthropic => "claude-3-5-sonnet-20241022",
        ApiProtocol::Gemini => "gemini-2.5-flash",
        ApiProtocol::Ollama => "qwen2.5-coder:7b",
    };
    let default_model = match Text::new("Default Model:")
        .with_default(default_model_hint)
        .prompt()
    {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => default_model_hint.to_string(),
    };

    let mut new_provider = ProviderConfig {
        id: id.clone(),
        name,
        protocol,
        base_url,
        api_key,
        default_model: default_model.clone(),
        context_window: None,
        max_output_tokens: None,
    };

    println!("\n🔎 Melakukan live probe untuk mengecek koneksi & auto-detect context limits...");
    let report = probe_provider_and_model(&new_provider, Some(&default_model));
    if report.success {
        println!(
            "✔ {} (Latency: {}ms)",
            report.status_message, report.latency_ms
        );
        println!(
            "✔ Batas Terdeteksi: Context Window: {} tokens | Max Output: {}",
            format_number(report.context_window),
            report
                .max_output_tokens
                .map(format_number)
                .unwrap_or_else(|| "N/A".to_string())
        );
        if !report.models_found.is_empty() {
            let sample = report
                .models_found
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            println!("✔ Model ditemukan via /models: {}", sample);
        }
        new_provider.context_window = Some(report.context_window);
        new_provider.max_output_tokens = report.max_output_tokens;
    } else {
        println!("⚠️  Probe warning: {}", report.status_message);
        let (cat_ctx, cat_out, _) = resolve_model_limits(&default_model);
        new_provider.context_window = Some(cat_ctx);
        new_provider.max_output_tokens = cat_out;
        println!(
            "   Menggunakan estimasi default: Context {} tokens",
            format_number(cat_ctx)
        );
    }

    registry.add_or_update(new_provider);
    println!(
        "\n✔ Provider '{}' berhasil ditambahkan ke ~/.ctrl-cli/providers.json!",
        id
    );

    if let Ok(true) = Confirm::new("Aktifkan provider ini sekarang?")
        .with_default(true)
        .prompt()
    {
        if let Ok(p) = registry.switch_active(&id) {
            *current_model = p.default_model;
            println!(
                "✔ Provider aktif beralih ke: {} (Model: {})\n",
                p.name, current_model
            );
        }
    }
}

fn add_provider_direct(
    registry: &mut ProvidersRegistry,
    current_model: &mut String,
    id: String,
    base_url: String,
    api_key: String,
    default_model: String,
    protocol: ApiProtocol,
) {
    let name = id.clone();
    let mut new_provider = ProviderConfig {
        id: id.clone(),
        name,
        protocol,
        base_url,
        api_key,
        default_model: default_model.clone(),
        context_window: None,
        max_output_tokens: None,
    };

    println!("\n🔎 Melakukan live probe untuk mengecek koneksi & auto-detect context limits...");
    let report = probe_provider_and_model(&new_provider, Some(&default_model));
    if report.success {
        println!(
            "✔ {} (Latency: {}ms)",
            report.status_message, report.latency_ms
        );
        println!(
            "✔ Batas Terdeteksi: Context Window: {} tokens | Max Output: {}",
            format_number(report.context_window),
            report
                .max_output_tokens
                .map(format_number)
                .unwrap_or_else(|| "N/A".to_string())
        );
        if !report.models_found.is_empty() {
            let sample = report
                .models_found
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            println!("✔ Model ditemukan via /models: {}", sample);
        }
        new_provider.context_window = Some(report.context_window);
        new_provider.max_output_tokens = report.max_output_tokens;
    } else {
        println!("⚠️  Probe warning: {}", report.status_message);
        let (cat_ctx, cat_out, _) = resolve_model_limits(&default_model);
        new_provider.context_window = Some(cat_ctx);
        new_provider.max_output_tokens = cat_out;
        println!(
            "   Menggunakan estimasi default: Context {} tokens",
            format_number(cat_ctx)
        );
    }

    registry.add_or_update(new_provider);
    println!(
        "\n✔ Provider '{}' berhasil ditambahkan ke ~/.ctrl-cli/providers.json!",
        id
    );
    if let Ok(p) = registry.switch_active(&id) {
        *current_model = p.default_model;
        println!(
            "✔ Provider aktif beralih ke: {} (Model: {})\n",
            p.name, current_model
        );
    }
}

fn probe_and_update_switched_provider(
    registry: &mut ProvidersRegistry,
    provider_id: &str,
    current_model: &str,
) {
    let provider = registry.get_active_provider().clone();
    println!(
        "🔎 Memeriksa konektivitas & context limits untuk provider '{}'...",
        provider.name
    );
    let report = probe_provider_and_model(&provider, Some(current_model));
    if report.success {
        registry.update_limits(provider_id, report.context_window, report.max_output_tokens);
        println!(
            "✔ Probe OK (Latency: {}ms) | Context: {} tokens | Max Out: {}\n",
            report.latency_ms,
            format_number(report.context_window),
            report
                .max_output_tokens
                .map(format_number)
                .unwrap_or_else(|| "N/A".to_string())
        );
    } else {
        println!(
            "⚠️  Probe note: {} (menggunakan limit bawaan)\n",
            report.status_message
        );
    }
}

fn run_and_print_probe(registry: &mut ProvidersRegistry, current_model: &str) {
    let provider = registry.get_active_provider().clone();
    println!("\n╭─────────────────────────────────────────────────────────────╮");
    println!("│ 🩺 AI Provider & Model Capabilities Live Probe              │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!(
        "│  • Provider ID      : {:<38}│",
        format!("{} ({})", provider.id, provider.protocol)
    );
    println!("│  • Endpoint URL     : {:<38}│", provider.base_url);
    println!("│  • Target Model     : {:<38}│", current_model);
    println!("╰─────────────────────────────────────────────────────────────╯");
    print!("⏳ Connecting & probing endpoint capabilities... ");
    let _ = std::io::stdout().flush();

    let report = probe_provider_and_model(&provider, Some(current_model));
    print!("\r\x1B[K");
    let _ = std::io::stdout().flush();

    println!("╭─────────────────────────────────────────────────────────────╮");
    println!("│ 📊 Probe Diagnostic Report                                  │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!(
        "│  • Connectivity     : {:<38}│",
        if report.endpoint_reachable {
            "✔ Reachable"
        } else {
            "❌ Unreachable"
        }
    );
    println!(
        "│  • Authentication   : {:<38}│",
        if report.auth_valid {
            "✔ Valid"
        } else {
            "❌ Auth Failed"
        }
    );
    println!(
        "│  • Latency (RTT)    : {:<38}│",
        format!("{} ms", report.latency_ms)
    );
    println!(
        "│  • Context Window   : {:<38}│",
        format!("{} tokens", format_number(report.context_window))
    );
    if let Some(mo) = report.max_output_tokens {
        println!(
            "│  • Max Output Limit : {:<38}│",
            format!("{} tokens", format_number(mo))
        );
    }
    println!("│  • Model Family     : {:<38}│", report.model_note);
    if !report.models_found.is_empty() {
        let sample = if report.models_found.len() > 3 {
            format!(
                "{} models (e.g. {}, {})",
                report.models_found.len(),
                report.models_found[0],
                report.models_found[1]
            )
        } else {
            report.models_found.join(", ")
        };
        println!("│  • Available Models : {:<38}│", sample);
    }
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│  • Status           : {:<38}│", report.status_message);
    if let Some(err) = report.error_detail {
        println!("│  • Error Detail     : {:<38}│", err);
    }
    println!("╰─────────────────────────────────────────────────────────────╯\n");

    if report.success {
        registry.update_limits(
            &provider.id,
            report.context_window,
            report.max_output_tokens,
        );
        println!("✔ Batas Context Window ({} tokens) otomatis diintegrasikan ke session tracker & progress bar.\n", format_number(report.context_window));
    }
}
