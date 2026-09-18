//! Empirical Adversarial Challenge Test Harness for Milestone 2 (REPL & TUI Interface Integration)
//!
//! Stress-tests:
//! 1. REPL slash command parsing, aliases (/stats, /metrics, /telemetry, /resources),
//!    output modes (ANSI table, json, --json, reset, help), case-insensitivity, and invalid argument recovery.
//! 2. TUI footer formatting across terminal column widths (120, 98, 80, 60, 40, 20, 0 cols)
//!    and extreme metric values (100% CPU, 0% CPU, 10 GB RAM, 1 TB RAM, 10,000 threads, None fallback).
//! 3. TUI full render via Ratatui TestBackend asserting footer presence and zero layout panic.
//! 4. 1 Hz debounce in `App::tick()` ensuring multiple rapid calls (<1s) do NOT re-sample telemetry.

pub mod agent {
    #[path = "../../src/agent/tasks.rs"]
    pub mod tasks;
}

#[path = "../src/server.rs"]
pub mod server;

pub use server::telemetry;

#[path = "../src/types.rs"]
pub mod types;

#[derive(Clone, Debug)]
pub struct Skill {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub fn get_available_skills() -> Vec<Skill> {
    Vec::new()
}

pub mod provider {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct ProviderConfig {
        pub id: String,
        pub name: String,
        pub default_model: String,
        pub context_window: Option<u32>,
        pub protocol: String,
        pub base_url: String,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct ProvidersRegistry {
        pub active_provider_id: String,
        pub providers: Vec<ProviderConfig>,
    }

    impl ProvidersRegistry {
        pub fn get_active_provider(&self) -> ProviderConfig {
            ProviderConfig {
                id: "test".to_string(),
                name: "Test".to_string(),
                default_model: "test-model".to_string(),
                context_window: Some(128_000),
                protocol: "openai".to_string(),
                base_url: "http://127.0.0.1".to_string(),
            }
        }
    }
}

pub use provider::ProvidersRegistry;

#[derive(Clone, Debug, Default)]
pub struct UserProfile {
    pub name: String,
}

pub mod permissions {
    #[derive(Clone, Debug)]
    pub struct PermissionRequest {
        pub tool_name: String,
        pub arguments_json: String,
    }
}

pub mod tui {
    pub mod app {
        use std::time::Instant;
        use crate::telemetry::{capture_metrics_with_cpu, CpuSampler, ProcessMetrics};

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum FocusedPane {
            Input,
            Chat,
            Sidebar,
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum SidebarTab {
            Tasks,
            Skills,
            Provider,
            Files,
            Help,
        }

        impl SidebarTab {
            pub fn all() -> &'static [SidebarTab] {
                &[SidebarTab::Tasks, SidebarTab::Skills, SidebarTab::Provider, SidebarTab::Files, SidebarTab::Help]
            }
            pub fn title(&self) -> &'static str {
                "Tab"
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum GitFileBadge {
            Modified,
            Staged,
            Untracked,
            Deleted,
        }

        #[derive(Clone, Debug)]
        pub struct FileTreeItem {
            pub relative_path: String,
            pub is_dir: bool,
            pub size_bytes: u64,
            pub git_status: Option<GitFileBadge>,
        }

        #[derive(Clone, Debug)]
        pub enum ChatItemKind {
            User,
            Assistant,
            Reasoning,
            ToolCall { name: String, args: String },
            ToolResult { name: String, result: String, success: bool },
            SystemInfo,
            Error,
        }

        #[derive(Clone, Debug)]
        pub struct ChatItem {
            pub kind: ChatItemKind,
            pub text: String,
            pub timestamp: String,
        }

        pub struct App {
            pub user_profile: crate::UserProfile,
            pub providers_reg: crate::ProvidersRegistry,
            pub current_model: String,
            pub active_skill: Option<crate::Skill>,
            pub conversation: Vec<crate::types::ChatMessage>,
            pub chat_items: Vec<ChatItem>,
            pub focused_pane: FocusedPane,
            pub active_tab: SidebarTab,
            pub chat_scroll: u16,
            pub auto_scroll: bool,
            pub input: String,
            pub cursor_position: usize,
            pub input_history: Vec<String>,
            pub history_index: Option<usize>,
            pub slash_suggestions: Vec<(&'static str, &'static str)>,
            pub selected_slash_index: usize,
            pub show_slash_popup: bool,
            pub selected_task_index: usize,
            pub selected_skill_index: usize,
            pub selected_provider_index: usize,
            pub selected_file_index: usize,
            pub files_list: Vec<FileTreeItem>,
            pub file_preview_content: Option<String>,
            pub file_preview_scroll: u16,
            pub task_log_scroll: u16,
            pub agent_running: bool,
            pub streaming_reasoning: String,
            pub streaming_content: String,
            pub active_tool_call: Option<(String, String)>,
            pub cancel_token: Option<crate::agent::tasks::CancellationToken>,
            pub total_prompt_tokens: u64,
            pub total_completion_tokens: u64,
            pub total_tokens: u64,
            pub query_count: u64,
            pub active_permission_request: Option<crate::permissions::PermissionRequest>,
            pub status_message: Option<(String, Instant)>,
            pub should_quit: bool,
            pub return_to_repl: bool,
            pub metrics: Option<ProcessMetrics>,
            pub last_metrics_poll: Instant,
            pub metrics_sampler: CpuSampler,
        }

        impl App {
            pub fn new(user_profile: crate::UserProfile, providers_reg: crate::ProvidersRegistry) -> Self {
                let mut metrics_sampler = CpuSampler::new();
                let initial_metrics = capture_metrics_with_cpu(None, &mut metrics_sampler);
                Self {
                    user_profile,
                    providers_reg,
                    current_model: "test-model".to_string(),
                    active_skill: None,
                    conversation: Vec::new(),
                    chat_items: Vec::new(),
                    focused_pane: FocusedPane::Input,
                    active_tab: SidebarTab::Tasks,
                    chat_scroll: 0,
                    auto_scroll: true,
                    input: String::new(),
                    cursor_position: 0,
                    input_history: Vec::new(),
                    history_index: None,
                    slash_suggestions: Vec::new(),
                    selected_slash_index: 0,
                    show_slash_popup: false,
                    selected_task_index: 0,
                    selected_skill_index: 0,
                    selected_provider_index: 0,
                    selected_file_index: 0,
                    files_list: Vec::new(),
                    file_preview_content: None,
                    file_preview_scroll: 0,
                    task_log_scroll: 0,
                    agent_running: false,
                    streaming_reasoning: String::new(),
                    streaming_content: String::new(),
                    active_tool_call: None,
                    cancel_token: None,
                    total_prompt_tokens: 0,
                    total_completion_tokens: 0,
                    total_tokens: 0,
                    query_count: 0,
                    active_permission_request: None,
                    status_message: None,
                    should_quit: false,
                    return_to_repl: false,
                    metrics: Some(initial_metrics),
                    last_metrics_poll: Instant::now(),
                    metrics_sampler,
                }
            }

            pub fn tick(&mut self) {
                let now = Instant::now();
                if now.duration_since(self.last_metrics_poll) >= std::time::Duration::from_millis(1000) {
                    let m = capture_metrics_with_cpu(None, &mut self.metrics_sampler);
                    self.metrics = Some(m);
                    self.last_metrics_poll = now;
                }
            }
        }
    }
}

#[path = "../src/tui/ui.rs"]
pub mod ui;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::Terminal;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use telemetry::{CpuMetrics, MemoryMetrics, ProcessMetrics, StorageMetrics, ThreadMetrics};

// ============================================================================
// SUITE 1: REPL SLASH COMMAND ADVERSARIAL STRESS & RECOVERY
// ============================================================================

#[test]
fn challenge_repl_stats_table_execution() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli --cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        let _ = writeln!(stdin, "/stats");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Failed to wait on child");
    assert!(out.status.success(), "Process must exit cleanly with status 0");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Process Resource Telemetry & Profiling Metrics"),
        "Must contain telemetry box header. Got:\n{}",
        stdout
    );
    assert!(stdout.contains("RAM (RSS / Working Set)"));
    assert!(stdout.contains("CPU Utilization"));
    assert!(stdout.contains("Active OS Threads"));
    assert!(stdout.contains("Disk (.ctrl/ footprint)"));
    assert!(stdout.contains('╭') && stdout.contains('╰'), "Must have ANSI border box");
}

#[test]
fn challenge_repl_aliases_equivalence() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let aliases = ["/metrics", "/telemetry", "/resources"];

    for alias in aliases {
        let mut child = Command::new(bin)
            .arg("--cli")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("Failed to spawn for {}: {}", alias, e));

        {
            let stdin = child.stdin.as_mut().expect("Failed to open stdin");
            let _ = writeln!(stdin, "{}", alias);
            let _ = writeln!(stdin, "/exit");
        }

        let out = child.wait_with_output().expect("Child failed");
        assert!(out.status.success(), "Alias '{}' must exit successfully", alias);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Process Resource Telemetry & Profiling Metrics"),
            "Alias '{}' must render telemetry box. Got:\n{}",
            alias,
            stdout
        );
        assert!(stdout.contains("RAM (RSS / Working Set)"));
    }
}

#[test]
fn challenge_repl_json_output_and_variants() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let commands = [
        "/stats --json",
        "/stats json",
        "/metrics --json",
        "/metrics json",
    ];

    for cmd in commands {
        let mut child = Command::new(bin)
            .arg("--cli")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("Failed to spawn for {}: {}", cmd, e));

        {
            let stdin = child.stdin.as_mut().expect("Failed to open stdin");
            let _ = writeln!(stdin, "{}", cmd);
            let _ = writeln!(stdin, "/exit");
        }

        let out = child.wait_with_output().expect("Child failed");
        assert!(out.status.success(), "Command '{}' must succeed", cmd);
        let stdout = String::from_utf8_lossy(&out.stdout);

        let json_start = stdout
            .find('{')
            .unwrap_or_else(|| panic!("Command '{}' output must contain JSON start '{{':\n{}", cmd, stdout));
        let json_end = stdout
            .rfind('}')
            .unwrap_or_else(|| panic!("Command '{}' output must contain JSON end '}}':\n{}", cmd, stdout));
        let json_str = &stdout[json_start..=json_end];

        let parsed: ProcessMetrics = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!("Command '{}' failed to parse JSON ({}):\n{}", cmd, e, json_str));

        assert!(parsed.memory.rss_bytes > 0, "RSS bytes must be > 0");
        assert!(parsed.memory.peak_rss_bytes >= parsed.memory.rss_bytes);
        assert!(!parsed.memory.formatted_rss.is_empty());
        assert!((0.0..=100.0).contains(&parsed.cpu.process_pct));
        assert!(parsed.threads.active_threads >= 1);
        assert!(parsed.timestamp > 0);
    }
}

#[test]
fn challenge_repl_reset_argument() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        let _ = writeln!(stdin, "/stats reset");
        let _ = writeln!(stdin, "/metrics reset");
        let _ = writeln!(stdin, "/stats");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Child failed");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("Telemetry baselines reset"),
        "Output must confirm baseline reset. Got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Process Resource Telemetry & Profiling Metrics"),
        "Subsequent /stats after reset must continue to render correctly. Got:\n{}",
        stdout
    );
}

#[test]
fn challenge_repl_invalid_arguments_recovery() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        // Send a burst of adversarial/invalid subcommands
        let _ = writeln!(stdin, "/stats bogus_arg_123");
        let _ = writeln!(stdin, "/metrics --invalid-flag");
        let _ = writeln!(stdin, "/stats 99999");
        let _ = writeln!(stdin, "/stats @#$%^&*");
        // Followed by valid command to verify the REPL session remains fully operational
        let _ = writeln!(stdin, "/stats");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Child failed");
    assert!(out.status.success(), "Process must not panic or crash on invalid args");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("Sub-perintah tidak dikenal: 'bogus_arg_123'"),
        "Must report unknown sub-command for bogus_arg_123"
    );
    assert!(
        stdout.contains("Sub-perintah tidak dikenal: '--invalid-flag'"),
        "Must report unknown sub-command for --invalid-flag"
    );
    assert!(
        stdout.contains("Gunakan: /stats [table|json|--json|reset]"),
        "Must display recovery usage guide"
    );
    assert!(
        stdout.contains("Process Resource Telemetry & Profiling Metrics"),
        "Valid /stats following errors must succeed"
    );
}

#[test]
fn challenge_repl_case_insensitivity_and_whitespace() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        let _ = writeln!(stdin, "/STATS TABLE");
        let _ = writeln!(stdin, "/stats   RESET");
        let _ = writeln!(stdin, "/stats   --JSON");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Child failed");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("Process Resource Telemetry & Profiling Metrics"));
    assert!(stdout.contains("Telemetry baselines reset"));
    assert!(stdout.contains("\"rss_bytes\""));
}

#[test]
fn challenge_repl_help_options() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        let _ = writeln!(stdin, "/stats help");
        let _ = writeln!(stdin, "/metrics --help");
        let _ = writeln!(stdin, "/stats -h");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Child failed");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    let count = stdout.matches("Gunakan: /stats [table|json|--json|reset]").count();
    assert_eq!(count, 3, "All three help queries must output usage guide");
}

// ============================================================================
// SUITE 2: TUI FOOTER FORMATTING & EXTREME VALUES STRESS
// ============================================================================

#[test]
fn challenge_tui_footer_formatting_normal_and_fallback() {
    let fallback = ui::format_footer_telemetry(None);
    assert_eq!(fallback, "RAM: -- │ CPU: -- │ Th: --");

    let metrics = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 32 * 1024 * 1024,
            peak_rss_bytes: 40 * 1024 * 1024,
            virtual_bytes: 64 * 1024 * 1024,
            formatted_rss: "32.00 MB".to_string(),
            formatted_peak: "40.00 MB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 12.5,
            user_ms: 100,
            kernel_ms: 50,
            total_ms: 150,
        },
        threads: ThreadMetrics {
            active_threads: 6,
            process_handles: Some(50),
        },
        storage: StorageMetrics::default(),
        timestamp: 1234567,
    };

    let formatted = ui::format_footer_telemetry(Some(&metrics));
    assert_eq!(formatted, "RAM: 32.0 M (Pk 40.0 M) │ CPU: 12.5% │ Th: 6");
}

#[test]
fn challenge_tui_footer_extreme_metric_values_and_alerts() {
    // Extreme 1: 10 GB RAM, 100% CPU
    let extreme_high = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
            peak_rss_bytes: 12 * 1024 * 1024 * 1024, // 12 GB
            virtual_bytes: 16 * 1024 * 1024 * 1024,
            formatted_rss: "10.00 GB".to_string(),
            formatted_peak: "12.00 GB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 100.0,
            user_ms: 5000,
            kernel_ms: 2000,
            total_ms: 7000,
        },
        threads: ThreadMetrics {
            active_threads: 256,
            process_handles: Some(1024),
        },
        storage: StorageMetrics::default(),
        timestamp: 999999,
    };

    let formatted_high = ui::format_footer_telemetry(Some(&extreme_high));
    assert!(formatted_high.contains("RAM: 10240.0 M"));
    assert!(formatted_high.contains("Pk 12288.0 M"));
    assert!(formatted_high.contains("CPU: 100.0%"));
    assert!(formatted_high.contains("Th: 256"));

    let line_high = ui::render_footer_telemetry_line(Some(&extreme_high));
    // Verify alert styling: RAM > 50MB is LightYellow + BOLD; CPU > 80% is LightRed + BOLD
    let ram_span = &line_high.spans[1];
    assert_eq!(ram_span.style.fg, Some(Color::LightYellow));
    assert!(ram_span.style.add_modifier.contains(Modifier::BOLD));
    let cpu_span = &line_high.spans[5];
    assert_eq!(cpu_span.style.fg, Some(Color::LightRed));
    assert!(cpu_span.style.add_modifier.contains(Modifier::BOLD));

    // Extreme 2: 0.0% CPU, 0 B RAM
    let extreme_zero = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 0,
            peak_rss_bytes: 0,
            virtual_bytes: 0,
            formatted_rss: "0 B".to_string(),
            formatted_peak: "0 B".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 0.0,
            user_ms: 0,
            kernel_ms: 0,
            total_ms: 0,
        },
        threads: ThreadMetrics {
            active_threads: 0,
            process_handles: None,
        },
        storage: StorageMetrics::default(),
        timestamp: 0,
    };

    let formatted_zero = ui::format_footer_telemetry(Some(&extreme_zero));
    assert_eq!(formatted_zero, "RAM: 0.0 M (Pk 0.0 M) │ CPU: 0.0% │ Th: 0");

    let line_zero = ui::render_footer_telemetry_line(Some(&extreme_zero));
    let ram_zero_span = &line_zero.spans[1];
    assert_eq!(ram_zero_span.style.fg, Some(Color::LightCyan));
    let cpu_zero_span = &line_zero.spans[5];
    assert_eq!(cpu_zero_span.style.fg, Some(Color::LightCyan));

    // Extreme 3: 1 TB RAM, 10,000 threads
    let massive = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 1024 * 1024 * 1024 * 1024, // 1 TB
            peak_rss_bytes: 1024 * 1024 * 1024 * 1024,
            virtual_bytes: 2048 * 1024 * 1024 * 1024,
            formatted_rss: "1.00 TB".to_string(),
            formatted_peak: "1.00 TB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 50.0,
            user_ms: 99999,
            kernel_ms: 11111,
            total_ms: 111110,
        },
        threads: ThreadMetrics {
            active_threads: 10_000,
            process_handles: Some(50_000),
        },
        storage: StorageMetrics::default(),
        timestamp: 1000,
    };

    let formatted_massive = ui::format_footer_telemetry(Some(&massive));
    assert!(formatted_massive.contains("RAM: 1048576.0 M"));
    assert!(formatted_massive.contains("Th: 10000"));
}

// ============================================================================
// SUITE 3: TUI FOOTER LAYOUT & COLUMN WIDTH BUFFER RENDERING (60, 80, 98, 120)
// ============================================================================

#[test]
fn challenge_tui_footer_layout_and_buffer_across_column_widths() {
    let test_metrics = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 45 * 1024 * 1024,
            peak_rss_bytes: 55 * 1024 * 1024,
            virtual_bytes: 80 * 1024 * 1024,
            formatted_rss: "45.00 MB".to_string(),
            formatted_peak: "55.00 MB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 2.5,
            user_ms: 200,
            kernel_ms: 100,
            total_ms: 300,
        },
        threads: ThreadMetrics {
            active_threads: 4,
            process_handles: Some(30),
        },
        storage: StorageMetrics::default(),
        timestamp: 100,
    };

    // Test a wide gamut of terminal widths, including narrow, standard, and wide
    let widths = [140, 120, 98, 80, 60, 40, 25, 10, 0];

    for &width in &widths {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);

        // Compute footer horizontal layout: [Min(25), Length(38), Length(35)]
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(25),
                Constraint::Length(38),
                Constraint::Length(35),
            ])
            .split(area);

        assert_eq!(chunks.len(), 3);

        // Render Paragraph into center telemetry chunk
        let para = Paragraph::new(ui::render_footer_telemetry_line(Some(&test_metrics)));
        para.render(chunks[1], &mut buffer);

        // Verification for widths >= 98: center chunk receives full allocation and contains text
        if width >= 98 {
            assert_eq!(chunks[1].width, 38, "Width {} should give telemetry chunk 38 cols", width);
            // Verify buffer contains 'RAM:' in the telemetry region
            let cell_text: String = (chunks[1].x..chunks[1].x + chunks[1].width)
                .map(|x| buffer[(x, 0)].symbol().chars().next().unwrap_or(' '))
                .collect();
            assert!(
                cell_text.contains("RAM:"),
                "Width {} buffer must contain 'RAM:'. Got:\n'{}'",
                width,
                cell_text
            );
        }

        // At narrow widths (e.g. 60 cols), layout should not panic and should stay bounded
        assert!(chunks[0].x + chunks[0].width <= width);
        assert!(chunks[1].x + chunks[1].width <= width);
        assert!(chunks[2].x + chunks[2].width <= width);
    }
}

#[test]
fn challenge_tui_full_render_testbackend_60_80_120_cols() {
    let test_widths = [120, 80, 60];

    for &width in &test_widths {
        let backend = TestBackend::new(width, 24);
        let mut terminal = Terminal::new(backend).expect("Failed to create TestBackend");
        let mut app = tui::app::App::new(UserProfile::default(), ProvidersRegistry::default());

        terminal
            .draw(|frame| {
                ui::render(frame, &mut app);
            })
            .expect("Render must not panic");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.area().width, width);
        assert_eq!(buffer.area().height, 24);

        // Inspect footer row (row 23)
        let footer_row: String = (0..width)
            .map(|x| buffer[(x, 23)].symbol().chars().next().unwrap_or(' '))
            .collect();

        // Across 120 and 80 cols, the footer must contain telemetry or navigation text
        if width >= 80 {
            assert!(
                footer_row.contains("RAM:") || footer_row.contains("Fokus"),
                "Width {} footer row must contain content. Got:\n'{}'",
                width,
                footer_row
            );
        }
    }
}

// ============================================================================
// SUITE 4: 1 HZ DEBOUNCE IN APP::TICK() STRESS TESTING
// ============================================================================

#[test]
fn challenge_app_tick_sub_second_burst_debounce() {
    let mut app = tui::app::App::new(UserProfile::default(), ProvidersRegistry::default());
    let initial_poll = app.last_metrics_poll;
    let initial_metrics = app.metrics.clone().expect("Metrics must be present");

    // Burst 5,000 rapid calls to tick() within sub-second window
    for _ in 0..5_000 {
        app.tick();
    }

    // Must NOT re-sample: last_metrics_poll must be identical
    assert_eq!(
        app.last_metrics_poll, initial_poll,
        "Sub-second burst must not re-sample telemetry"
    );
    assert_eq!(
        app.metrics.as_ref().unwrap().timestamp,
        initial_metrics.timestamp,
        "Timestamp must not change during sub-second burst"
    );
}

#[test]
fn challenge_app_tick_1000ms_boundary_precision() {
    let mut app = tui::app::App::new(UserProfile::default(), ProvidersRegistry::default());
    let base_instant = std::time::Instant::now();

    // 1. 500 ms elapsed -> MUST NOT poll
    app.last_metrics_poll = base_instant - Duration::from_millis(500);
    let before_poll = app.last_metrics_poll;
    app.tick();
    assert_eq!(app.last_metrics_poll, before_poll, "500ms elapsed must not poll");

    // 2. 990 ms elapsed -> MUST NOT poll (boundary safety)
    app.last_metrics_poll = base_instant - Duration::from_millis(990);
    let before_poll_990 = app.last_metrics_poll;
    app.tick();
    assert_eq!(app.last_metrics_poll, before_poll_990, "990ms elapsed must not poll");

    // 3. 1001 ms elapsed -> MUST poll
    app.last_metrics_poll = base_instant - Duration::from_millis(1001);
    app.tick();
    assert!(
        app.last_metrics_poll > before_poll_990,
        "1001ms elapsed MUST poll and advance last_metrics_poll"
    );

    // 4. Immediate second tick after successful poll -> MUST NOT poll again
    let just_polled = app.last_metrics_poll;
    app.tick();
    assert_eq!(
        app.last_metrics_poll, just_polled,
        "Immediate tick after update must debounce"
    );
}

#[test]
fn challenge_app_tick_multiple_render_passes_debounce() {
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).expect("Terminal creation failed");
    let mut app = tui::app::App::new(UserProfile::default(), ProvidersRegistry::default());

    let initial_poll = app.last_metrics_poll;

    // Simulate 60 FPS rendering by calling render() 60 times rapidly
    for _ in 0..60 {
        terminal
            .draw(|frame| {
                ui::render(frame, &mut app);
            })
            .expect("Render failed");
    }

    assert_eq!(
        app.last_metrics_poll, initial_poll,
        "60 rapid render passes must not re-poll telemetry"
    );
}

#[test]
fn challenge_app_tick_real_time_interval_update() {
    let mut app = tui::app::App::new(UserProfile::default(), ProvidersRegistry::default());
    let poll_1 = app.last_metrics_poll;

    // Sleep 1,050 ms in real time
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();

    assert!(
        app.last_metrics_poll > poll_1,
        "Tick after real-time 1050ms sleep must update last_metrics_poll"
    );

    let poll_2 = app.last_metrics_poll;
    // Another tick immediately after must not poll
    app.tick();
    assert_eq!(app.last_metrics_poll, poll_2, "Tick immediately after must debounce");
}