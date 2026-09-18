use anyhow::Result;
use inquire::ui::{Color, RenderConfig, Styled};

use crate::agent::memory::MemoryManager;
use crate::agent::provider::ProviderConfig;
use crate::context::{get_model_context_info, SessionTokenTracker};
use crate::profile::{SupportedLanguage, UserProfile};
use crate::skill::Skill;
use crate::types::Usage;

pub fn extract_code_block(text: &str) -> String {
    if let Some(start_idx) = text.find("```") {
        let after_fence = &text[start_idx + 3..];
        let code_start = after_fence.find('\n').map(|n| n + 1).unwrap_or(0);
        let code_body = &after_fence[code_start..];
        if let Some(end_idx) = code_body.find("```") {
            return code_body[..end_idx].to_string();
        }
    }
    text.to_string()
}

pub fn detect_filename(text: &str) -> Option<String> {
    let extensions = [
        ".html", ".htm", ".rs", ".py", ".ts", ".js", ".css", ".json", ".toml", ".yaml", ".yml",
        ".md", ".sh", ".sql", ".zig", ".go", ".c", ".cpp",
    ];

    for line in text.lines() {
        let trimmed = line.trim();
        let mut rest = trimmed;
        while let Some(start) = rest.find('`') {
            let after = &rest[start + 1..];
            if let Some(end) = after.find('`') {
                let candidate = after[..end].trim();
                if !candidate.contains(' ')
                    && (3..=50).contains(&candidate.len())
                    && extensions.iter().any(|ext| candidate.ends_with(ext))
                {
                    return Some(candidate.to_string());
                }
                rest = &after[end + 1..];
            } else {
                break;
            }
        }
        let mut rest_bold = trimmed;
        while let Some(start) = rest_bold.find("**") {
            let after = &rest_bold[start + 2..];
            if let Some(end) = after.find("**") {
                let candidate = after[..end].trim();
                if !candidate.contains(' ')
                    && (3..=50).contains(&candidate.len())
                    && extensions.iter().any(|ext| candidate.ends_with(ext))
                {
                    return Some(candidate.to_string());
                }
                rest_bold = &after[end + 2..];
            } else {
                break;
            }
        }
    }
    None
}

pub fn save_code_to_file(path_str: &str, content: &str) -> Result<String> {
    let path = std::path::Path::new(path_str);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let code = extract_code_block(content);
    std::fs::write(path, code.as_bytes())?;
    let line_count = code.lines().count();
    let bytes = code.len();
    Ok(format!(
        "{} ({} baris, {} byte)",
        path_str, line_count, bytes
    ))
}

pub fn auto_save_if_code_generated(
    prompt: &str,
    final_content: &str,
    tools_executed: usize,
) -> Option<String> {
    if tools_executed > 0 {
        return None;
    }
    let code = extract_code_block(final_content);
    if code.lines().count() >= 3 && code != final_content {
        let suggested = detect_filename(final_content)
            .or_else(|| detect_filename(prompt))
            .or_else(|| {
                let lower_p = prompt.to_lowercase();
                if lower_p.contains("html")
                    || lower_p.contains("web")
                    || code.contains("<!DOCTYPE")
                    || code.contains("<html")
                {
                    Some("index.html".to_string())
                } else if lower_p.contains("python")
                    || code.contains("def ")
                    || code.contains("import sys")
                {
                    Some("main.py".to_string())
                } else if lower_p.contains("rust") || code.contains("fn main") {
                    Some("main.rs".to_string())
                } else {
                    None
                }
            });

        if let Some(fname) = suggested {
            if let Ok(summary) = save_code_to_file(&fname, &code) {
                let _ = MemoryManager::save_long_term_memory(
                    "append",
                    &format!("Created file '{}'", fname),
                );
                return Some(format!("💾 [Auto-Writer Memory] Mendeteksi kode untuk '{}'. File otomatis disimpan: {}\n", fname, summary));
            }
        }
    }
    None
}

pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

pub fn format_compact_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        n.to_string()
    }
}

pub fn generate_progress_bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width.saturating_sub(filled);
    format!("{}{}", "▰".repeat(filled), "▱".repeat(empty))
}

pub fn textwrap_simple(text: &str, max_len: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= max_len {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub fn configure_inquire_theme() {
    let config = RenderConfig {
        prompt_prefix: Styled::new("❯ ").with_fg(Color::LightCyan),
        answered_prompt_prefix: Styled::new("✔ ").with_fg(Color::LightGreen),
        highlighted_option_prefix: Styled::new("● ").with_fg(Color::LightCyan),
        scroll_up_prefix: Styled::new("▲"),
        scroll_down_prefix: Styled::new("▼"),
        ..Default::default()
    };
    inquire::set_global_render_config(config);
}

pub fn format_token_badge(
    usage: Option<&Usage>,
    model: &str,
    provider: Option<&ProviderConfig>,
) -> String {
    let ctx_info = get_model_context_info(model, provider);
    if let Some(u) = usage {
        let p = u.prompt_tokens.unwrap_or(0);
        let c = u.completion_tokens.unwrap_or(0);
        let t = u.total_tokens.unwrap_or(p + c);
        let pct = if ctx_info.context_window > 0 {
            (p as f64 / ctx_info.context_window as f64) * 100.0
        } else {
            0.0
        };
        let bar = generate_progress_bar(pct, 10);
        let bar_color = if pct > 85.0 {
            "\x1B[31m"
        } else if pct > 60.0 {
            "\x1B[33m"
        } else {
            "\x1B[32m"
        };
        let ctx_label = if ctx_info.context_window >= 1_000_000 {
            format!("{}M", ctx_info.context_window / 1_000_000)
        } else if ctx_info.context_window >= 1_000 {
            format!("{}k", ctx_info.context_window / 1_000)
        } else {
            format!("{}", ctx_info.context_window)
        };
        format!(
            "\x1B[90m─── \x1B[36mTokens:\x1B[0m \x1B[37m{}\x1B[0m \x1B[90min\x1B[0m \x1B[90m+\x1B[0m \x1B[37m{}\x1B[0m \x1B[90mout\x1B[0m \x1B[90m(\x1B[1;36m{}\x1B[0m\x1B[90m)\x1B[0m \x1B[90m•\x1B[0m \x1B[36mCtx:\x1B[0m {bar_color}[{bar}]\x1B[0m \x1B[37m{:.1}%\x1B[0m \x1B[90mof {}\x1B[0m",
            format_number(p),
            format_number(c),
            format_number(t),
            pct,
            ctx_label
        )
    } else {
        format!(
            "\x1B[90m─── \x1B[36mModel:\x1B[0m \x1B[37m{}\x1B[0m \x1B[90m•\x1B[0m \x1B[36mContext Window:\x1B[0m \x1B[37m{}\x1B[0m",
            model, ctx_info.note
        )
    }
}

pub fn print_token_stats(
    tracker: &SessionTokenTracker,
    current_model: &str,
    provider: Option<&ProviderConfig>,
    show_badge: bool,
) {
    let ctx_info = get_model_context_info(current_model, provider);
    println!(
        "\n\x1B[1;36m╭─ 📊 Token Usage & Context Metrics ─────────────────────────────╮\x1B[0m"
    );
    println!("  \x1B[1;33mActive Model Configuration\x1B[0m");
    println!(
        "    \x1B[90mModel        :\x1B[0m \x1B[1;37m{}\x1B[0m",
        current_model
    );
    println!(
        "    \x1B[90mFamily / Note:\x1B[0m \x1B[37m{}\x1B[0m",
        ctx_info.note
    );
    println!(
        "    \x1B[90mContext Limit:\x1B[0m \x1B[36m{} tokens\x1B[0m",
        format_number(ctx_info.context_window)
    );
    if let Some(max_out) = ctx_info.max_output {
        println!(
            "    \x1B[90mMax Output   :\x1B[0m \x1B[36m{} tokens\x1B[0m",
            format_number(max_out)
        );
    }
    println!("  \x1B[1;33mLast Interaction\x1B[0m");
    if let Some(last_u) = &tracker.last_usage {
        let p = last_u.prompt_tokens.unwrap_or(0);
        let c = last_u.completion_tokens.unwrap_or(0);
        let t = last_u.total_tokens.unwrap_or(p + c);
        let pct = if ctx_info.context_window > 0 {
            (p as f64 / ctx_info.context_window as f64) * 100.0
        } else {
            0.0
        };
        let bar = generate_progress_bar(pct, 10);
        let bar_color = if pct > 85.0 {
            "\x1B[31m"
        } else if pct > 60.0 {
            "\x1B[33m"
        } else {
            "\x1B[32m"
        };
        let remaining = ctx_info.context_window.saturating_sub(p);
        println!(
            "    \x1B[90mPrompt In    :\x1B[0m \x1B[37m{} tokens\x1B[0m",
            format_number(p)
        );
        println!(
            "    \x1B[90mCompletion   :\x1B[0m \x1B[37m{} tokens\x1B[0m",
            format_number(c)
        );
        println!(
            "    \x1B[90mTotal Query  :\x1B[0m \x1B[1;36m{} tokens\x1B[0m",
            format_number(t)
        );
        println!("    \x1B[90mContext Bar  :\x1B[0m {bar_color}[{bar}]\x1B[0m \x1B[37m{:.2}%\x1B[0m \x1B[90m({} free)\x1B[0m", pct, format_number(remaining));
    } else {
        println!("    \x1B[90m(Belum ada query yang dieksekusi di sesi ini)\x1B[0m");
    }
    println!("  \x1B[1;33mSession Accumulation\x1B[0m");
    println!(
        "    \x1B[90mTotal Queries:\x1B[0m \x1B[37m{}\x1B[0m",
        tracker.query_count
    );
    println!(
        "    \x1B[90mTotal Prompt :\x1B[0m \x1B[37m{} tokens\x1B[0m",
        format_number(tracker.total_prompt_tokens)
    );
    println!(
        "    \x1B[90mTotal Output :\x1B[0m \x1B[37m{} tokens\x1B[0m",
        format_number(tracker.total_completion_tokens)
    );
    println!(
        "    \x1B[90mGrand Total  :\x1B[0m \x1B[1;32m{} tokens\x1B[0m",
        format_number(tracker.total_tokens)
    );
    println!(
        "    \x1B[90mAuto-badge   :\x1B[0m {}",
        if show_badge {
            "\x1B[32mAktif\x1B[0m \x1B[90m(/tokens toggle)\x1B[0m"
        } else {
            "\x1B[90mNonaktif (/tokens toggle)\x1B[0m"
        }
    );
    println!(
        "\x1B[1;36m╰─────────────────────────────────────────────────────────────────╯\x1B[0m\n"
    );
}

pub fn build_system_prompt(active_skill: Option<&Skill>, user_profile: &UserProfile) -> String {
    let lang = SupportedLanguage::from_str(&user_profile.response_language);
    let lang_directive = lang.directive();

    let base_identity = "\
Identity and context:
- You are ctrl-cli, an ultra-lightweight, high-performance autonomous coding agent CLI.
- Work inside the user's real local workspace and use it as the source of truth for code, configuration, and verification.
- You have built-in tool access: `read_file`, `write_file`, `edit_file`, `glob_files`, `grep_files`, `shell`, `read_tool_result`, `ask_user_question`, `manage_memory`, and `skill`.
- Before answering questions about the workspace, gather local evidence with tools (read_file, glob_files, grep_files). Do not guess or assume.
- When asked to build, edit, or fix something, inspect relevant files first, then use `write_file` or `edit_file`. Prefer `edit_file` for targeted surgical modifications.
- CRITICAL FILE-CREATION RULE: When the user asks to create, make, build, generate, or write a file, webpage, or script (e.g. 'create a file html', 'make me a simple html web', 'buat file'), you MUST invoke the `write_file` tool directly to save the code to the workspace. NEVER merely output markdown code blocks and instruct the user to save it themselves!
- If a tool or command fails, diagnose the error before retrying.
- Keep answers practical and concise. Do not narrate routine steps unnecessarily.";

    let skill_part = active_skill.map(|s| s.system_prompt.as_ref()).unwrap_or("");

    let profile_part = format!(
        "\n\n[User Profile: {}]\n- Preferred Tech Stack: {}\n- Response Language: {}\n- Coding Style Guidelines: {}",
        user_profile.name,
        user_profile.tech_stack.join(", "),
        user_profile.response_language,
        user_profile.coding_style
    );

    let memory_part = if let Some(mem) = MemoryManager::load_long_term_memory() {
        format!(
            "\n\n[Workspace Persistent Memory (.ctrl/MEMORY.md)]:\n{}",
            mem
        )
    } else {
        String::new()
    };

    format!(
        "{}\n\n{}\n\n{}{}{}",
        base_identity, lang_directive, skill_part, profile_part, memory_part
    )
}
