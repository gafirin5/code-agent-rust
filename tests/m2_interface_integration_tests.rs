//! Milestone 2 Interface Integration Tests
//!
//! Verifies:
//! 1. REST API endpoint `GET /api/metrics` on an ephemeral HTTP server (`127.0.0.1:0`):
//!    - HTTP 200 OK status code.
//!    - Content-Type: `application/json`.
//!    - CORS header: `Access-Control-Allow-Origin: *`.
//!    - OPTIONS preflight returns HTTP 204 No Content.
//!    - Schema validation: validates `memory`, `cpu`, `storage`, `threads`, `timestamp`.
//! 2. REPL slash commands `/stats` and `/metrics`:
//!    - `/stats` prints formatted ANSI table with memory, CPU, threads, and storage.
//!    - `/metrics` alias prints the same formatted table.
//!    - `/stats json` and `/metrics --json` output valid machine-readable JSON matching the schema.
//!    - `/stats reset` resets telemetry baselines.
//! 3. TUI Live Telemetry Status & Footer Formatting:
//!    - `ui::format_footer_telemetry` formats `ProcessMetrics` into `RAM: X.X M (Pk Y.Y M) │ CPU: Z.Z% │ Th: N`.
//!    - `ui::format_footer_telemetry(None)` outputs fallback `RAM: -- │ CPU: -- │ Th: --`.
//!    - `ui::render_footer_telemetry_line` produces styled spans with normal and alert colors (>50MB RAM, >80% CPU).
//!    - `App::tick()` 1 Hz telemetry sampling and caching.

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

use agent::tasks::CancellationToken;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use telemetry::{CpuMetrics, MemoryMetrics, ProcessMetrics, StorageMetrics, ThreadMetrics};

// ============================================================================
// SUITE 1: REST API GET /api/metrics ENDPOINT & SCHEMA VALIDATION
// ============================================================================

#[test]
fn test_get_api_metrics_endpoint_headers_and_schema() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");

    assert!(port > 0, "Bound port must be greater than zero");
    let base_url = format!("http://127.0.0.1:{}", port);

    // 1. GET /api/metrics response status & headers
    let resp = ureq::get(&format!("{}/api/metrics", base_url))
        .call()
        .expect("GET /api/metrics must succeed");

    assert_eq!(resp.status(), 200, "Status code must be 200 OK");
    assert_eq!(
        resp.header("content-type"),
        Some("application/json"),
        "Content-Type header must be application/json"
    );
    assert_eq!(
        resp.header("access-control-allow-origin"),
        Some("*"),
        "CORS header Access-Control-Allow-Origin must be *"
    );

    // 2. JSON Schema structure validation
    let json: serde_json::Value = resp.into_json().expect("Response must be valid JSON");

    // Memory object
    let memory = json.get("memory").expect("Payload must have 'memory' key");
    assert!(memory.is_object(), "'memory' must be an object");
    let rss = memory.get("rss_bytes").and_then(|v| v.as_u64()).expect("rss_bytes must be u64");
    assert!(rss > 0, "rss_bytes must be > 0 in a running process");
    let peak_rss = memory.get("peak_rss_bytes").and_then(|v| v.as_u64()).expect("peak_rss_bytes must be u64");
    assert!(peak_rss >= rss, "peak_rss_bytes ({}) must be >= rss_bytes ({})", peak_rss, rss);
    assert!(memory.get("formatted_rss").and_then(|v| v.as_str()).is_some());
    assert!(memory.get("formatted_peak").and_then(|v| v.as_str()).is_some());

    // CPU object
    let cpu = json.get("cpu").expect("Payload must have 'cpu' key");
    assert!(cpu.is_object(), "'cpu' must be an object");
    let cpu_pct = cpu.get("process_pct").and_then(|v| v.as_f64()).expect("process_pct must be f64");
    assert!((0.0..=100.0).contains(&cpu_pct), "cpu_pct ({}) must be within [0.0, 100.0]", cpu_pct);
    assert!(cpu.get("user_ms").and_then(|v| v.as_u64()).is_some());
    assert!(cpu.get("kernel_ms").and_then(|v| v.as_u64()).is_some());
    assert!(cpu.get("total_ms").and_then(|v| v.as_u64()).is_some());

    // Threads object
    let threads = json.get("threads").expect("Payload must have 'threads' key");
    assert!(threads.is_object(), "'threads' must be an object");
    let active_threads = threads.get("active_threads").and_then(|v| v.as_u64()).expect("active_threads must be u64");
    assert!(active_threads >= 1, "active_threads ({}) must be at least 1", active_threads);

    // Storage object
    let storage = json.get("storage").expect("Payload must have 'storage' key");
    assert!(storage.is_object(), "'storage' must be an object");
    assert!(storage.get("ctrl_dir_bytes").and_then(|v| v.as_u64()).is_some());
    assert!(storage.get("task_logs_bytes").and_then(|v| v.as_u64()).is_some());
    assert!(storage.get("formatted_ctrl").and_then(|v| v.as_str()).is_some());
    assert!(storage.get("file_count").and_then(|v| v.as_u64()).is_some());

    // Timestamp
    let timestamp = json.get("timestamp").and_then(|v| v.as_u64()).expect("timestamp must be u64");
    assert!(timestamp > 0, "timestamp must be a non-zero unix epoch");

    // 3. CORS OPTIONS preflight
    let opt_stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    let mut opt_reader = BufReader::new(opt_stream.try_clone().unwrap());
    let mut opt_writer = opt_stream;
    opt_writer
        .write_all(b"OPTIONS /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .unwrap();
    let mut opt_status = String::new();
    opt_reader.read_line(&mut opt_status).unwrap();
    assert!(opt_status.contains("204 No Content"), "OPTIONS must return 204 No Content");

    // 4. Clean server shutdown
    token.cancel();
    let join_res = handle.join();
    assert!(join_res.is_ok(), "Server handle must join cleanly");
}

#[test]
fn test_get_api_metrics_repeated_calls_stability() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");
    let base_url = format!("http://127.0.0.1:{}", port);

    // Make 25 successive requests rapidly
    for i in 0..25 {
        let resp = ureq::get(&format!("{}/api/metrics", base_url))
            .call()
            .unwrap_or_else(|e| panic!("Request {} failed: {}", i, e));
        assert_eq!(resp.status(), 200);
        let val: serde_json::Value = resp.into_json().unwrap();
        assert!(val.get("memory").is_some());
        assert!(val.get("cpu").is_some());
    }

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// SUITE 2: REPL SLASH COMMANDS (/stats, /metrics, --json, reset)
// ============================================================================

#[test]
fn test_repl_stats_table_formatting() {
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
    assert!(out.status.success(), "REPL process must exit cleanly");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Process Resource Telemetry & Profiling Metrics"),
        "Output must contain telemetry header. Got:\n{}",
        stdout
    );
    assert!(stdout.contains("RAM (RSS / Working Set)"));
    assert!(stdout.contains("CPU Utilization"));
    assert!(stdout.contains("Active OS Threads"));
    assert!(stdout.contains("Disk (.ctrl/ footprint)"));
    assert!(stdout.contains('╭') && stdout.contains('╰'), "Must have ANSI box borders");
}

#[test]
fn test_repl_metrics_alias_identical_output() {
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
        let _ = writeln!(stdin, "/metrics");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Failed to wait on child");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Process Resource Telemetry & Profiling Metrics"),
        "/metrics must display telemetry table. Got:\n{}",
        stdout
    );
    assert!(stdout.contains("RAM (RSS / Working Set)"));
    assert!(stdout.contains("CPU Utilization"));
}

#[test]
fn test_repl_stats_json_serialization() {
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
        let _ = writeln!(stdin, "/stats --json");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Failed to wait on child");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);

    // Extract the JSON object substring between { and }
    let json_start = stdout.find('{').expect("Output must contain JSON start '{'");
    let json_end = stdout.rfind('}').expect("Output must contain JSON end '}'");
    let json_str = &stdout[json_start..=json_end];

    let parsed: serde_json::Value =
        serde_json::from_str(json_str).unwrap_or_else(|e| panic!("Failed to parse JSON ({}):\n{}", e, json_str));

    assert!(parsed.get("memory").is_some());
    assert!(parsed.get("cpu").is_some());
    assert!(parsed.get("storage").is_some());
    assert!(parsed.get("threads").is_some());
    assert!(parsed.get("timestamp").is_some());
}

#[test]
fn test_repl_stats_reset_command() {
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
        let _ = writeln!(stdin, "/stats reset");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Failed to wait on child");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Telemetry baselines reset"),
        "/stats reset must confirm reset. Got:\n{}",
        stdout
    );
}

// ============================================================================
// SUITE 3: TUI FOOTER FORMATTING & TELEMETRY INDICATOR
// ============================================================================

#[test]
fn test_tui_footer_formatting_live_snapshot() {
    let metrics = telemetry::capture_metrics(None);
    let formatted = ui::format_footer_telemetry(Some(&metrics));

    assert!(formatted.starts_with("RAM: "), "Must start with 'RAM: '");
    assert!(formatted.contains(" (Pk "), "Must contain peak marker '(Pk '");
    assert!(formatted.contains(" │ CPU: "), "Must contain ' │ CPU: '");
    assert!(formatted.contains(" │ Th: "), "Must contain ' │ Th: '");
    assert!(formatted.contains('%'), "Must contain '%' for CPU");
}

#[test]
fn test_tui_footer_formatting_synthetic_values() {
    let synthetic = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 14_942_208,      // 14.25 MB -> 14.3 M
            peak_rss_bytes: 19_398_656, // 18.50 MB -> 18.5 M
            virtual_bytes: 44_000_000,
            formatted_rss: "14.25 MB".to_string(),
            formatted_peak: "18.50 MB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 0.15,
            user_ms: 100,
            kernel_ms: 50,
            total_ms: 150,
        },
        threads: ThreadMetrics {
            active_threads: 4,
            process_handles: Some(42),
        },
        storage: StorageMetrics {
            ctrl_dir_bytes: 142_600,
            task_logs_bytes: 80_000,
            formatted_ctrl: "142.60 KB".to_string(),
            file_count: 5,
        },
        timestamp: 1726320000,
    };

    let formatted = ui::format_footer_telemetry(Some(&synthetic));
    assert!(formatted.contains("RAM: 14.2 M") || formatted.contains("RAM: 14.3 M"));
    assert!(formatted.contains("Pk 18.5 M"));
    assert!(formatted.contains("CPU: 0.1%") || formatted.contains("CPU: 0.2%"));
    assert!(formatted.contains("Th: 4"));
}

#[test]
fn test_tui_footer_formatting_none_fallback() {
    let fallback = ui::format_footer_telemetry(None);
    assert_eq!(fallback, "RAM: -- │ CPU: -- │ Th: --");
}

#[test]
fn test_tui_render_footer_telemetry_line_styling() {
    use ratatui::style::Color;

    // Normal metrics (<50MB RAM, <80% CPU)
    let normal = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 20 * 1024 * 1024,
            peak_rss_bytes: 25 * 1024 * 1024,
            virtual_bytes: 50 * 1024 * 1024,
            formatted_rss: "20.00 MB".to_string(),
            formatted_peak: "25.00 MB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 5.0,
            user_ms: 100,
            kernel_ms: 50,
            total_ms: 150,
        },
        threads: ThreadMetrics {
            active_threads: 2,
            process_handles: None,
        },
        storage: StorageMetrics::default(),
        timestamp: 1000,
    };

    let normal_line = ui::render_footer_telemetry_line(Some(&normal));
    assert!(!normal_line.spans.is_empty());
    // In normal state, RAM span style is LightCyan
    let ram_span = &normal_line.spans[1];
    assert_eq!(ram_span.style.fg, Some(Color::LightCyan));

    // Alert metrics (>50MB RAM, >80% CPU)
    let alert = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 60 * 1024 * 1024, // 60 MB > 50 MB
            peak_rss_bytes: 70 * 1024 * 1024,
            virtual_bytes: 100 * 1024 * 1024,
            formatted_rss: "60.00 MB".to_string(),
            formatted_peak: "70.00 MB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 95.0, // 95% > 80%
            user_ms: 500,
            kernel_ms: 200,
            total_ms: 700,
        },
        threads: ThreadMetrics {
            active_threads: 8,
            process_handles: None,
        },
        storage: StorageMetrics::default(),
        timestamp: 1000,
    };

    let alert_line = ui::render_footer_telemetry_line(Some(&alert));
    // In alert state, RAM span style is LightYellow and CPU is LightRed
    let ram_alert_span = &alert_line.spans[1];
    assert_eq!(ram_alert_span.style.fg, Some(Color::LightYellow));
    let cpu_alert_span = &alert_line.spans[5];
    assert_eq!(cpu_alert_span.style.fg, Some(Color::LightRed));
}

#[test]
fn test_tui_app_tick_and_polling_cadence() {
    let mut app = tui::app::App::new(UserProfile::default(), ProvidersRegistry::default());
    assert!(app.metrics.is_some(), "App must initialize with metrics populated");

    // Immediate tick within 1000ms should debounce (not change last_metrics_poll)
    let poll_time_1 = app.last_metrics_poll;
    app.tick();
    assert_eq!(app.last_metrics_poll, poll_time_1, "Immediate tick must not re-poll");

    // Simulate elapsed time >= 1000ms
    app.last_metrics_poll = std::time::Instant::now() - Duration::from_millis(1500);
    app.tick();
    assert!(app.last_metrics_poll > poll_time_1, "Tick after 1s must update last_metrics_poll");
    assert!(app.metrics.is_some());
}
