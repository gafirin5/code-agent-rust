pub mod agent;
pub mod cli;
pub mod context;
pub mod formatter;
pub mod profile;
pub mod repl;
pub mod server;
pub mod skill;
pub use server::telemetry;
pub mod tools;
pub mod tui;
pub mod types;

pub use cli::{Cli, Commands};
pub use context::{get_model_context_info, ModelContextInfo, SessionTokenTracker};
pub use formatter::{
    auto_save_if_code_generated, build_system_prompt, configure_inquire_theme, detect_filename,
    extract_code_block, format_compact_num, format_number, format_token_badge,
    generate_progress_bar, print_token_stats, save_code_to_file, textwrap_simple,
};
pub use profile::{load_user_profile, save_user_profile, SupportedLanguage, UserProfile};
pub use repl::{resolve_slash_command, CommandSpec, SlashCompleter, COMMAND_SPECS};
pub use skill::{get_available_skills, Skill};

use agent::provider::load_providers_registry;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

fn main() -> Result<()> {
    load_env_file();
    let mut profile = load_user_profile();
    let mut providers_reg = load_providers_registry();
    let cli = Cli::parse();

    let is_serve = matches!(cli.command, Some(Commands::Serve { .. }));
    let _web_server = if cli.web && !is_serve {
        let token = crate::agent::tasks::CancellationToken::new();
        match server::spawn_server("127.0.0.1", 3000, token.clone()) {
            Ok((port, handle)) => {
                println!("Embedded web dashboard running at http://127.0.0.1:{}", port);
                Some((token, handle))
            }
            Err(e) => {
                eprintln!("Warning: Failed to launch embedded web server: {}", e);
                None
            }
        }
    } else {
        None
    };

    match cli.command {
        Some(Commands::Serve { port, host }) => {
            println!("Starting CTRL local web dashboard on http://{}:{} ...", host, port);
            println!("Serving visual dashboard from dashboard.html");
            println!("Press Ctrl+C to stop.");
            let token = crate::agent::tasks::CancellationToken::new();
            server::run_server(&host, port, token)?;
        }
        Some(Commands::Generate {
            model,
            skill,
            tokens,
            output,
            prompt,
        }) => {
            let full_prompt = prompt.join(" ");
            let skill_obj = skill.and_then(|s_name| {
                if let Some(found) = get_available_skills()
                    .into_iter()
                    .find(|s| s.id.eq_ignore_ascii_case(&s_name))
                {
                    Some(found)
                } else {
                    let root = crate::tools::filesystem::get_workspace_root();
                    crate::tools::skills::get_skill_by_name(&s_name, &root).map(Skill::from)
                }
            });
            repl::handle_generate(
                &full_prompt,
                model.as_deref(),
                skill_obj.as_ref(),
                &profile,
                &providers_reg,
                tokens,
                output.as_deref(),
            )?;
        }
        Some(Commands::Tui) => {
            if let Err(e) = tui::run_tui(&mut profile, &mut providers_reg) {
                eprintln!("\n❌ Tidak dapat memulai mode TUI: {}", e);
                if cfg!(windows) {
                    eprintln!("💡 Tips Windows: Pastikan Anda menggunakan terminal standar (PowerShell, Windows Terminal, atau CMD).");
                    eprintln!("   Jika menggunakan Git Bash (mintty), jalankan dengan 'winpty ctrl-cli.exe tui' atau gunakan PowerShell.\n");
                }
                return Err(e);
            }
            if tui::app::take_return_to_repl() {
                repl::start_repl(&mut profile, &mut providers_reg)?;
            }
        }
        Some(Commands::Repl) => {
            repl::start_repl(&mut profile, &mut providers_reg)?;
        }
        None => {
            let exe_is_tui = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
                .map(|name| name.ends_with("-tui") || name.ends_with("_tui"))
                .unwrap_or(false);

            let env_wants_tui = std::env::var("CTRL_TUI")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
                || std::env::var("CTRL_MODE")
                    .map(|v| v.eq_ignore_ascii_case("tui"))
                    .unwrap_or(false)
                || std::env::var("CTRL_UI")
                    .map(|v| v.eq_ignore_ascii_case("tui"))
                    .unwrap_or(false);

            let profile_wants_tui = profile
                .default_ui
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("tui"))
                .unwrap_or(false);

            if cli.cli {
                repl::start_repl(&mut profile, &mut providers_reg)?;
            } else if cli.tui || exe_is_tui || env_wants_tui || profile_wants_tui {
                if let Err(e) = tui::run_tui(&mut profile, &mut providers_reg) {
                    eprintln!("\n❌ Tidak dapat memulai mode TUI: {}", e);
                    if cfg!(windows) {
                        eprintln!("💡 Tips Windows: Pastikan Anda menggunakan terminal standar (PowerShell, Windows Terminal, atau CMD).");
                        eprintln!("   Jika menggunakan Git Bash (mintty), jalankan dengan 'winpty' atau beralih ke PowerShell.\n");
                    }
                    eprintln!("Beralih ke mode CLI / REPL biasa...\n");
                    repl::start_repl(&mut profile, &mut providers_reg)?;
                } else if tui::app::take_return_to_repl() {
                    repl::start_repl(&mut profile, &mut providers_reg)?;
                }
            } else {
                repl::start_repl(&mut profile, &mut providers_reg)?;
            }
        }
    }
    Ok(())
}

fn load_env_file() {
    let mut candidates = Vec::new();
    if let Ok(curr) = std::env::current_dir() {
        candidates.push(curr.join(".env"));
    }
    candidates.push(PathBuf::from(
        r"C:\Users\Administrator\code-agent-rust\ctrl-cli\.env",
    ));

    for candidate in candidates {
        if candidate.exists() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = trimmed.split_once('=') {
                        let key = k.trim();
                        let mut val = v.trim();
                        if ((val.starts_with('"') && val.ends_with('"'))
                            || (val.starts_with('\'') && val.ends_with('\'')))
                            && val.len() >= 2
                        {
                            val = &val[1..val.len() - 1];
                        }
                        if std::env::var(key).is_err() {
                            std::env::set_var(key, val);
                        }
                    }
                }
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_pkg_version_is_0_3_0() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.3.0");
    }

    #[test]
    fn test_command_specs_contains_tui() {
        let tui_spec = COMMAND_SPECS.iter().find(|s| s.primary == "/tui");
        assert!(tui_spec.is_some(), "/tui must be present in COMMAND_SPECS");
        let spec = tui_spec.unwrap();
        assert!(spec.aliases.contains(&"/gui"));
        assert!(spec.description.contains("Ratatui TUI"));
    }

    #[test]
    fn test_command_specs_contains_stats_and_metrics() {
        let stats_spec = COMMAND_SPECS.iter().find(|s| s.primary == "/stats");
        assert!(stats_spec.is_some(), "/stats must be present in COMMAND_SPECS");
        let spec = stats_spec.unwrap();
        assert!(spec.aliases.contains(&"/metrics"));
        assert!(spec.description.contains("resource real-time"));
    }

    #[test]
    fn test_command_specs_contains_skills() {
        let skills_spec = COMMAND_SPECS.iter().find(|s| s.primary == "/skills");
        assert!(skills_spec.is_some(), "/skills must be present in COMMAND_SPECS");
        let spec = skills_spec.unwrap();
        assert!(spec.aliases.contains(&"/skill-list"));
        assert!(spec.description.contains("skill") || spec.description.contains("persona"));
    }

    #[test]
    fn test_command_specs_contains_theme_and_zen() {
        let theme_spec = COMMAND_SPECS.iter().find(|s| s.primary == "/theme");
        assert!(theme_spec.is_some(), "/theme must be present in COMMAND_SPECS");
        let t_spec = theme_spec.unwrap();
        assert!(t_spec.aliases.contains(&"/themes"));

        let zen_spec = COMMAND_SPECS.iter().find(|s| s.primary == "/zen");
        assert!(zen_spec.is_some(), "/zen must be present in COMMAND_SPECS");
        let z_spec = zen_spec.unwrap();
        assert!(z_spec.aliases.contains(&"/sidebar"));
    }


    #[test]
    fn test_resolve_slash_command_skills() {
        let (exact, note) = resolve_slash_command("/skills");
        assert_eq!(exact, "/skills");
        assert!(note.is_none());

        let (exact_list, note) = resolve_slash_command("/skills list");
        assert_eq!(exact_list, "/skills list");
        assert!(note.is_none());

        let (exact_info, note) = resolve_slash_command("/skills info researcher");
        assert_eq!(exact_info, "/skills info researcher");
        assert!(note.is_none());

        let (exact_alias, note) = resolve_slash_command("/skill-list");
        assert_eq!(exact_alias, "/skill-list");
        assert!(note.is_none());
    }

    #[test]
    fn test_resolve_slash_command_stats_and_metrics() {
        // Exact match for primary
        let (exact, note) = resolve_slash_command("/stats");
        assert_eq!(exact, "/stats");
        assert!(note.is_none());

        // Exact match with subcommands
        let (exact_json, note) = resolve_slash_command("/stats --json");
        assert_eq!(exact_json, "/stats --json");
        assert!(note.is_none());

        // Alias match
        let (alias, note) = resolve_slash_command("/metrics");
        assert_eq!(alias, "/metrics");
        assert!(note.is_none());

        // Prefix auto-expansion: "/stat" should expand to "/stats"
        let (auto, note) = resolve_slash_command("/stat");
        assert_eq!(auto, "/stats");
        assert!(note.is_some());
        assert!(note.unwrap().contains("/stats"));
    }

    #[test]
    fn test_resolve_slash_command_tui() {
        // Exact match
        let (exact, note) = resolve_slash_command("/tui");
        assert_eq!(exact, "/tui");
        assert!(note.is_none());

        // Alias match
        let (alias, note) = resolve_slash_command("/gui");
        assert_eq!(alias, "/gui");
        assert!(note.is_none());

        // Prefix auto-expansion: "/tu" should expand to "/tui"
        let (auto, note) = resolve_slash_command("/tu");
        assert_eq!(auto, "/tui");
        assert!(note.is_some());
        assert!(note.unwrap().contains("/tui"));
    }

    #[test]
    fn test_return_to_repl_flag_lifecycle() {
        use crate::tui::app::{set_return_to_repl, take_return_to_repl};

        // Initially cleared
        let _ = take_return_to_repl();
        assert!(!take_return_to_repl());

        // Set to true and take
        set_return_to_repl(true);
        assert!(take_return_to_repl());
        // Second take should be false (cleared)
        assert!(!take_return_to_repl());
    }

    #[test]
    fn test_tui_app_request_return_to_repl() {
        use crate::agent::provider::ProvidersRegistry;
        use crate::tui::app::{take_return_to_repl, App};

        let profile = UserProfile::default();
        let providers_reg = ProvidersRegistry::default();
        let mut app = App::new(profile, providers_reg);

        assert!(!app.should_quit);
        assert!(!app.return_to_repl);

        app.request_return_to_repl();

        assert!(app.should_quit);
        assert!(app.return_to_repl);
        assert!(take_return_to_repl());
    }

    #[test]
    fn test_tui_app_submit_input_return_commands() {
        use crate::agent::provider::ProvidersRegistry;
        use crate::tui::app::{take_return_to_repl, App};

        let return_cmds = [":cli", ":repl", "/cli", "/repl"];
        for cmd in return_cmds {
            let profile = UserProfile::default();
            let providers_reg = ProvidersRegistry::default();
            let mut app = App::new(profile, providers_reg);

            app.input = cmd.to_string();
            app.submit_input();

            assert!(app.should_quit, "cmd '{}' should set should_quit", cmd);
            assert!(app.return_to_repl, "cmd '{}' should set return_to_repl", cmd);
            assert!(take_return_to_repl());
        }
    }

    #[test]
    fn test_tui_app_cancellation_on_return_to_repl() {
        use crate::agent::provider::ProvidersRegistry;
        use crate::agent::tasks::CancellationToken;
        use crate::tui::app::{take_return_to_repl, App};

        let profile = UserProfile::default();
        let providers_reg = ProvidersRegistry::default();
        let mut app = App::new(profile, providers_reg);

        let token = CancellationToken::new();
        app.cancel_token = Some(token.clone());
        assert!(!token.is_cancelled());

        app.request_return_to_repl();

        assert!(token.is_cancelled(), "active agent token must be cancelled on return to repl");
        assert!(app.should_quit);
        assert!(app.return_to_repl);
        assert!(take_return_to_repl());
    }

    #[test]
    fn test_tui_app_cancellation_on_quit_command() {
        use crate::agent::provider::ProvidersRegistry;
        use crate::agent::tasks::CancellationToken;
        use crate::tui::app::{take_return_to_repl, App};

        let quit_cmds = ["/exit", "/quit", "/EXIT", "/QUIT"];
        for cmd in quit_cmds {
            let profile = UserProfile::default();
            let providers_reg = ProvidersRegistry::default();
            let mut app = App::new(profile, providers_reg);

            let token = CancellationToken::new();
            app.cancel_token = Some(token.clone());
            assert!(!token.is_cancelled());

            app.input = cmd.to_string();
            app.submit_input();

            assert!(token.is_cancelled(), "active agent token must be cancelled on '{}'", cmd);
            assert!(app.should_quit, "cmd '{}' should set should_quit", cmd);
            assert!(!app.return_to_repl, "cmd '{}' must not set return_to_repl", cmd);
            assert!(!take_return_to_repl());
        }
    }

    #[test]
    fn test_tui_app_case_insensitive_return_commands() {
        use crate::agent::provider::ProvidersRegistry;
        use crate::tui::app::{take_return_to_repl, App};

        let return_cmds = [":cli", ":CLI", ":Repl", "/cli", "/CLI", "/REPL"];
        for cmd in return_cmds {
            let profile = UserProfile::default();
            let providers_reg = ProvidersRegistry::default();
            let mut app = App::new(profile, providers_reg);

            app.input = cmd.to_string();
            app.submit_input();

            assert!(app.should_quit, "cmd '{}' should set should_quit", cmd);
            assert!(app.return_to_repl, "cmd '{}' should set return_to_repl", cmd);
            assert!(take_return_to_repl());
        }
    }

    #[test]
    fn test_slash_command_tui_exit_intent_handling() {
        use crate::tui::app::{set_return_to_repl, take_return_to_repl};

        // Case 1: When return_to_repl is true, REPL resumes
        set_return_to_repl(true);
        assert!(take_return_to_repl());

        // Case 2: When return_to_repl is false (quitting TUI), REPL terminates
        set_return_to_repl(false);
        assert!(!take_return_to_repl());
    }

    #[test]
    fn test_slash_completer_includes_discovered_skills() {
        use crate::SlashCompleter;
        use inquire::autocompletion::{Autocomplete, Replacement};

        let mut completer = SlashCompleter;

        // /skill should include hardcoded skills + reset
        let suggs = completer.get_suggestions("/skill ").unwrap();
        assert!(suggs.iter().any(|s| s.contains("rust-expert")));
        assert!(suggs.iter().any(|s| s.contains("reset")));

        // Tab completion on /skill rus should complete to /skill rust-expert
        let repl = completer.get_completion("/skill rus", None).unwrap();
        match repl {
            Replacement::Some(s) => assert_eq!(s, "/skill rust-expert"),
            _ => panic!("Expected replacement for /skill rus"),
        }
    }

    #[test]
    fn test_dynamic_skill_conversion_without_leaks() {
        use crate::tools::skills::SkillMetadata;
        use crate::{build_system_prompt, Skill, UserProfile};
        use std::path::PathBuf;

        let meta = SkillMetadata {
            name: "test-persona".to_string(),
            description: "A test persona".to_string(),
            tools: vec!["read_file".to_string()],
            prompt_template: "Custom prompt template for testing".to_string(),
            path: PathBuf::from("skills/test/SKILL.md"),
        };

        let skill = Skill::from(meta);
        assert_eq!(skill.id, "test-persona");
        assert_eq!(skill.name, "test-persona");
        assert_eq!(skill.description, "A test persona");
        assert_eq!(skill.system_prompt, "Custom prompt template for testing");

        let profile = UserProfile::default();
        let prompt = build_system_prompt(Some(&skill), &profile);
        assert!(prompt.contains("Custom prompt template for testing"));
        // Dropping skill cleanly frees heap strings without Box::leak
    }

    #[test]
    fn test_cli_short_flag_tui() {
        use clap::Parser;
        let cli = Cli::parse_from(["ctrl-cli", "-t"]);
        assert!(cli.tui, "-t flag must set cli.tui to true");
        assert!(!cli.cli);

        let cli_long = Cli::parse_from(["ctrl-cli", "--tui"]);
        assert!(cli_long.tui, "--tui flag must set cli.tui to true");
    }

    #[test]
    fn test_user_profile_default_ui_serde() {
        let mut prof = UserProfile::default();
        assert_eq!(prof.default_ui, None);

        prof.default_ui = Some("tui".to_string());
        let json = serde_json::to_string(&prof).expect("serialize");
        assert!(json.contains("\"default_ui\":\"tui\""));

        let deserialized: UserProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.default_ui, Some("tui".to_string()));

        // Backward compatibility: JSON without default_ui should deserialize with None
        let legacy_json = r#"{"name":"test","tech_stack":[],"response_language":"en","coding_style":"clean","show_token_usage":true}"#;
        let legacy_prof: UserProfile = serde_json::from_str(legacy_json).expect("legacy deserialize");
        assert_eq!(legacy_prof.default_ui, None);
    }
}

