/// Canonical command definition and its recognized aliases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub primary: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

pub const COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        primary: "/help",
        aliases: &["?", "/?", "help"],
        description: "Bantuan & panduan lengkap semua perintah",
    },
    CommandSpec {
        primary: "/model",
        aliases: &["/models"],
        description: "Pilih / ganti model AI aktif (GLM, Claude, GPT, dll.)",
    },
    CommandSpec {
        primary: "/skill",
        aliases: &[],
        description: "Pilih peran spesialis AI (Rust Expert, Reviewer, Web UI)",
    },
    CommandSpec {
        primary: "/skills",
        aliases: &["/skill-list"],
        description: "Daftar dan info skill / persona agen (dynamic discovery)",
    },
    CommandSpec {
        primary: "/lang",
        aliases: &["/language"],
        description: "Ganti bahasa respon (English, Bahasa Indonesia, 中文)",
    },
    CommandSpec {
        primary: "/provider",
        aliases: &["/providers"],
        description: "Kelola endpoint & API key provider AI (list, switch, add)",
    },
    CommandSpec {
        primary: "/probe",
        aliases: &["/check-model"],
        description: "Auto-test koneksi, API key & deteksi context limits model",
    },
    CommandSpec {
        primary: "/permissions",
        aliases: &["/security"],
        description: "Atur izin eksekusi tool (Ask / AutoApprove / ReadOnly)",
    },
    CommandSpec {
        primary: "/stream",
        aliases: &[],
        description: "Toggle streaming respons real-time SSE (on / off)",
    },
    CommandSpec {
        primary: "/tools",
        aliases: &[],
        description: "Lihat daftar 14 built-in agent tools",
    },
    CommandSpec {
        primary: "/undo",
        aliases: &[],
        description: "Batalkan (rollback) modifikasi berkas dari checkpoint",
    },
    CommandSpec {
        primary: "/diff",
        aliases: &[],
        description: "Lihat unified diff perubahan berkas / status git terkini",
    },
    CommandSpec {
        primary: "/check",
        aliases: &[],
        description: "Jalankan diagnosa syntax / compiler (self-healing loop)",
    },
    CommandSpec {
        primary: "/compact",
        aliases: &[],
        description: "Ringkas riwayat percakapan lama hemat context window",
    },
    CommandSpec {
        primary: "/mcp",
        aliases: &[],
        description: "Info server Model Context Protocol (.ctrl/mcp.json)",
    },
    CommandSpec {
        primary: "/memory",
        aliases: &[],
        description: "Lihat catatan memori proyek (.ctrl/MEMORY.md)",
    },
    CommandSpec {
        primary: "/checkpoints",
        aliases: &[],
        description: "Lihat riwayat snapshot modifikasi berkas",
    },
    CommandSpec {
        primary: "/tokens",
        aliases: &["/usage", "/context"],
        description: "Cek statistik token & progress bar context window",
    },
    CommandSpec {
        primary: "/save",
        aliases: &["/write"],
        description: "Simpan kode respon terakhir langsung ke file",
    },
    CommandSpec {
        primary: "/reset",
        aliases: &["/clear-history"],
        description: "Kosongkan riwayat percakapan (mulai sesi baru)",
    },
    CommandSpec {
        primary: "/profile",
        aliases: &[],
        description: "Lihat Developer Profile & preferensi bahasa",
    },
    CommandSpec {
        primary: "/dev",
        aliases: &["/developer", "/author", "/about"],
        description: "Info pengembang & arsitektur proyek (galangfjr)",
    },
    CommandSpec {
        primary: "/info",
        aliases: &["/config"],
        description: "Ringkasan konfigurasi endpoint, provider & model",
    },
    CommandSpec {
        primary: "/clear",
        aliases: &[],
        description: "Bersihkan layar terminal",
    },
    CommandSpec {
        primary: "/tasks",
        aliases: &[],
        description: "Kelola background tasks (list, view, cancel, wait, logs, clear)",
    },
    CommandSpec {
        primary: "/stats",
        aliases: &["/metrics", "/telemetry", "/resources"],
        description: "Pantau pemakaian resource real-time (RAM, CPU, thread, storage)",
    },
    CommandSpec {
        primary: "/tui",
        aliases: &["/gui"],
        description: "Beralih ke antarmuka grafis terminal (Ratatui TUI)",
    },
    CommandSpec {
        primary: "/theme",
        aliases: &["/themes"],
        description: "Ganti tema warna tampilan TUI (Tokyo Night, Catppuccin, Gruvbox, Cyberpunk, Monokai)",
    },
    CommandSpec {
        primary: "/zen",
        aliases: &["/sidebar"],
        description: "Toggle Zen Mode (tampilkan / sembunyikan sidebar di TUI)",
    },
    CommandSpec {
        primary: "/exit",
        aliases: &["/quit"],
        description: "Keluar dari aplikasi ctrl-cli",
    },
];

/// Resolves user slash input with prefix auto-expansion & autocorrect (e.g. /comp -> /compact, ? -> /help).
pub fn resolve_slash_command(input: &str) -> (String, Option<String>) {
    let raw_trimmed = input.trim();
    if raw_trimmed.is_empty() {
        return (String::new(), None);
    }

    // Strip trailing descriptions if input was autocompleted with description (e.g. "/compact — ...")
    let trimmed = if let Some((cmd_part, _)) = raw_trimmed.split_once(" — ") {
        cmd_part.trim()
    } else if let Some((cmd_part, _)) = raw_trimmed.split_once(" - ") {
        cmd_part.trim()
    } else {
        raw_trimmed
    };

    // Direct help shortcuts: ?, /?, help
    if trimmed == "?" || trimmed == "/?" || trimmed.eq_ignore_ascii_case("help") {
        return ("/help".to_string(), None);
    }

    // If query starts with "? " or "/? "
    if trimmed.starts_with("? ") || trimmed.starts_with("/? ") {
        let after = if let Some(stripped) = trimmed.strip_prefix("? ") {
            stripped
        } else {
            &trimmed[3..]
        }
        .trim();
        let target_cmd = if after.starts_with('/') {
            after.to_string()
        } else {
            format!("/{}", after)
        };
        let (resolved, _) = resolve_slash_command(&target_cmd);
        return (format!("/help {}", resolved), None);
    }

    if !trimmed.starts_with('/') {
        return (trimmed.to_string(), None);
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return (String::new(), None);
    }
    let cmd = parts[0].to_lowercase();
    let rest = if parts.len() > 1 {
        format!(" {}", parts[1..].join(" "))
    } else {
        String::new()
    };

    // 1. Exact match check against all primary commands and aliases
    for spec in COMMAND_SPECS {
        if spec.primary == cmd || spec.aliases.iter().any(|&a| a == cmd) {
            return (trimmed.to_string(), None);
        }
    }

    // 2. Prefix match: find all unique primary commands whose primary or aliases start with `cmd`
    let mut matching_primaries: Vec<&'static str> = Vec::new();
    for spec in COMMAND_SPECS {
        let matches_primary = spec.primary.starts_with(&cmd);
        let matches_alias = spec.aliases.iter().any(|&a| a.starts_with(&cmd));
        if (matches_primary || matches_alias) && !matching_primaries.contains(&spec.primary) {
            matching_primaries.push(spec.primary);
        }
    }

    // If both /skill and /skills match, prefer the base command /skill for sub-prefixes
    if matching_primaries.contains(&"/skill") && matching_primaries.contains(&"/skills") {
        matching_primaries.retain(|&p| p != "/skills");
    }

    if matching_primaries.len() == 1 {
        let expanded = matching_primaries[0];
        let note = format!("⚡ Auto-corrected '{}' -> '{}'", cmd, expanded);
        (format!("{}{}", expanded, rest), Some(note))
    } else if matching_primaries.is_empty() {
        (trimmed.to_string(), None)
    } else {
        // Ambiguous prefix across distinct primary commands
        let note = format!(
            "⚡ Ambiguous command prefix '{}'. Suggestions: {}",
            cmd,
            matching_primaries.join(", ")
        );
        (trimmed.to_string(), Some(note))
    }
}
