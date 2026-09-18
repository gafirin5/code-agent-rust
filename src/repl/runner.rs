use std::io::{IsTerminal, Write};

use anyhow::Result;
use inquire::{InquireError, Text};

use crate::agent::memory::MemoryManager;
use crate::agent::orchestrator::run_agent_loop;
use crate::agent::permissions::{PermissionGate, PermissionMode};
use crate::agent::provider::ProvidersRegistry;
use crate::context::SessionTokenTracker;
use crate::formatter::{
    auto_save_if_code_generated, build_system_prompt, configure_inquire_theme, format_compact_num,
    format_token_badge, save_code_to_file,
};
use crate::profile::UserProfile;
use crate::repl::commands::resolve_slash_command;
use crate::repl::completer::SlashCompleter;
use crate::repl::slash::handle_slash_command;
use crate::skill::Skill;
use crate::types::ChatMessage;

pub fn handle_generate(
    prompt: &str,
    model_override: Option<&str>,
    active_skill: Option<&Skill>,
    user_profile: &UserProfile,
    providers_reg: &ProvidersRegistry,
    show_tokens: bool,
    output_file: Option<&str>,
) -> Result<()> {
    let active_prov = providers_reg.get_active_provider();
    let model = model_override.unwrap_or(&active_prov.default_model);
    let system_prompt = build_system_prompt(active_skill, user_profile);

    let mut conversation = Vec::new();
    let mut permission_gate = PermissionGate::new(PermissionMode::AutoApprove);

    let stream_output = output_file.is_none();
    let result = run_agent_loop(
        prompt,
        &mut conversation,
        model,
        &active_prov.api_key,
        &active_prov.base_url,
        active_prov.protocol,
        &mut permission_gate,
        &system_prompt,
        25,
        stream_output,
        active_prov.context_window,
        None,
        None,
    )?;

    if let Some(dest) = output_file {
        match save_code_to_file(dest, &result.final_content) {
            Ok(summary) => println!("✔ File berhasil dibuat: {}", summary),
            Err(e) => {
                eprintln!("Gagal menulis file {}: {}", dest, e);
                let highlighted = crate::tui::highlight_markdown_code_blocks_ansi(&result.final_content);
                println!("{}", highlighted);
            }
        }
    } else {
        if !stream_output {
            let highlighted = crate::tui::highlight_markdown_code_blocks_ansi(&result.final_content);
            println!("{}", highlighted);
        }
        if let Some(note) =
            auto_save_if_code_generated(prompt, &result.final_content, result.tools_executed)
        {
            println!("{}", note);
        }
    }

    if show_tokens {
        println!(
            "\n{}",
            format_token_badge(Some(&result.total_usage), model, Some(active_prov))
        );
    }
    Ok(())
}


/// Drains unnotified terminal background tasks (Completed, Failed, Cancelled)
/// and prints formatted completion notification banners.
/// Respects interactive TTY (ANSI color highlights) vs non-TTY (clean plain text).
fn print_task_completion_notifications(tm: &crate::agent::tasks::TaskManager, is_term: bool) {
    let unnotified = tm.drain_unnotified_terminal_tasks();
    if unnotified.is_empty() {
        return;
    }

    let alert_enabled = std::env::var("ALERT_ON_TASK_DONE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(is_term);
    let bell = crate::agent::tasks::emit_task_completion_alert(alert_enabled);
    if !bell.is_empty() {
        print!("{}", bell);
        let _ = std::io::stdout().flush();
    }

    println!();
    for snap in &unnotified {
        println!("{}", snap.format_notification(is_term));
    }
    println!();
}

pub fn start_repl(user_profile: &mut UserProfile, providers_reg: &mut ProvidersRegistry) -> Result<()> {
    configure_inquire_theme();

    let active_prov = providers_reg.get_active_provider();
    let mut current_model = active_prov.default_model.clone();
    let mut active_skill: Option<Skill> = None;
    let mut token_tracker = SessionTokenTracker::default();
    let mut conversation: Vec<ChatMessage> =
        MemoryManager::load_session_history().unwrap_or_default();
    let mut permission_gate = PermissionGate::default();
    let mut streaming = true;

    println!();
    println!("  \x1B[1;36m◆ ctrl-cli\x1B[0m \x1B[90mv{}\x1B[0m \x1B[90m—\x1B[0m \x1B[1;37mAutonomous AI Coding Agent\x1B[0m", env!("CARGO_PKG_VERSION"));
    println!("  \x1B[90m─────────────────────────────────────────────────────────────────\x1B[0m");
    println!("  \x1B[90mProvider\x1B[0m  \x1B[1;36m{:<20}\x1B[0m \x1B[90mModel\x1B[0m  \x1B[1;37m{}\x1B[0m \x1B[90m[{} Ctx]\x1B[0m", 
        format!("{} [{}]", active_prov.name, active_prov.protocol),
        current_model,
        format_compact_num(active_prov.context_window.unwrap_or(128_000))
    );
    println!(
        "  \x1B[90mUser\x1B[0m      \x1B[33m{:<20}\x1B[0m \x1B[90mLang\x1B[0m   \x1B[32m{}\x1B[0m",
        format!(
            "{} ({})",
            user_profile.name,
            user_profile
                .tech_stack
                .first()
                .map(|s| s.as_str())
                .unwrap_or("Rust")
        ),
        user_profile.response_language
    );
    println!("  \x1B[90mFeatures\x1B[0m  \x1B[37m14 Built-in Tools • Checkpoints • Self-Healing • SSE\x1B[0m");
    if !conversation.is_empty() {
        println!("  \x1B[35m●\x1B[0m \x1B[90mMemori Sesi:\x1B[0m \x1B[37m{} pesan dipulihkan dari .ctrl/session.json\x1B[0m", conversation.len());
    }
    println!("  \x1B[90m─────────────────────────────────────────────────────────────────\x1B[0m");
    println!("  \x1B[90m💡 Ketik instruksi dan Enter. Ketik \x1B[36m'tui'\x1B[90m untuk mode grafis, \x1B[36m'/'\x1B[90m untuk menu, \x1B[36m'?'\x1B[90m untuk bantuan.\x1B[0m\n");

    let is_term = std::io::stdin().is_terminal();

    loop {
        // Inter-turn background task completion notification drain
        print_task_completion_notifications(crate::agent::tasks::TaskManager::global(), is_term);

        let prompt_label = if is_term {
            if let Some(skill) = &active_skill {
                format!(
                    "\x1B[36m[{}]\x1B[0m \x1B[35m({})\x1B[0m \x1B[1;32m❯\x1B[0m ",
                    current_model, skill.id
                )
            } else {
                format!("\x1B[36m[{}]\x1B[0m \x1B[1;32m❯\x1B[0m ", current_model)
            }
        } else {
            // Non-TTY graceful fallback: plain text without ANSI escape sequences
            if let Some(skill) = &active_skill {
                format!("[{}] ({}) > ", current_model, skill.id)
            } else {
                format!("[{}] > ", current_model)
            }
        };

        let input_line = if is_term {
            let answer = Text::new(&prompt_label)
                .with_autocomplete(SlashCompleter)
                .prompt();

            match answer {
                Ok(line) => line,
                Err(InquireError::OperationCanceled) | Err(InquireError::OperationInterrupted) => {
                    println!("\nExiting REPL. Goodbye {}!", user_profile.name);
                    break;
                }
                Err(err) => {
                    eprintln!("Input error: {}", err);
                    break;
                }
            }
        } else {
            print!("{}", prompt_label);
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            line
        };

        let trimmed = input_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Direct shortcut to switch to TUI mode without needing slash prefix
        if trimmed.eq_ignore_ascii_case("tui")
            || trimmed.eq_ignore_ascii_case(":tui")
            || trimmed.eq_ignore_ascii_case(":gui")
        {
            if !handle_slash_command(
                "/tui",
                &mut current_model,
                &mut active_skill,
                user_profile,
                providers_reg,
                &token_tracker,
                &mut permission_gate,
                &mut conversation,
                &mut streaming,
            ) {
                println!("\nExiting REPL. Goodbye {}!", user_profile.name);
                break;
            }
            continue;
        }

        // Apply prefix auto-expansion & autocorrect (e.g. /comp -> /compact, ? -> /help)
        let (resolved_cmd, auto_note) = resolve_slash_command(trimmed);
        if let Some(note) = auto_note {
            println!("\x1B[90m{}\x1B[0m", note);
        }

        if resolved_cmd.starts_with('/') {
            if !handle_slash_command(
                &resolved_cmd,
                &mut current_model,
                &mut active_skill,
                user_profile,
                providers_reg,
                &token_tracker,
                &mut permission_gate,
                &mut conversation,
                &mut streaming,
            ) {
                println!("\nExiting REPL. Goodbye {}!", user_profile.name);
                break;
            }
            continue;
        }

        let active_p = providers_reg.get_active_provider();
        let system_prompt = build_system_prompt(active_skill.as_ref(), user_profile);

        match run_agent_loop(
            trimmed,
            &mut conversation,
            &current_model,
            &active_p.api_key,
            &active_p.base_url,
            active_p.protocol,
            &mut permission_gate,
            &system_prompt,
            25,
            streaming,
            active_p.context_window,
            None,
            None,
        ) {
            Ok(turn_res) => {
                token_tracker.record(
                    Some(&turn_res.total_usage),
                    &current_model,
                    &turn_res.final_content,
                );
                if !streaming {
                    let highlighted = crate::tui::highlight_markdown_code_blocks_ansi(&turn_res.final_content);
                    println!("\n{}\n", highlighted);
                }

                // Auto-Writer Safety Net: If code was generated but model didn't call write_file
                if let Some(note) = auto_save_if_code_generated(
                    trimmed,
                    &turn_res.final_content,
                    turn_res.tools_executed,
                ) {
                    println!("{}", note);
                }

                // Persist session conversation history
                let _ = MemoryManager::save_session_history(&conversation);

                if user_profile.show_token_usage {
                    println!(
                        "{}\n",
                        format_token_badge(
                            Some(&turn_res.total_usage),
                            &current_model,
                            Some(active_p)
                        )
                    );
                }
            }
            Err(e) => eprintln!("\n❌ Agent Error: {}\n", e),
        }
    }
    Ok(())
}
