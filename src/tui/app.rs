use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

pub static RETURN_TO_REPL: AtomicBool = AtomicBool::new(false);

pub fn set_return_to_repl(val: bool) {
    RETURN_TO_REPL.store(val, Ordering::SeqCst);
}

pub fn take_return_to_repl() -> bool {
    RETURN_TO_REPL.swap(false, Ordering::SeqCst)
}

use crate::agent::memory::MemoryManager;
use crate::agent::orchestrator::run_agent_loop;
use crate::agent::permissions::{
    PermissionGate, PermissionMode, PermissionRequest, PermissionResponse,
};
use crate::agent::provider::ProvidersRegistry;
use crate::agent::tasks::{AgentUiEvent, CancellationToken, OutputSink, TaskManager};
use crate::telemetry::{capture_metrics_with_cpu, CpuSampler, ProcessMetrics};
use crate::types::{ChatMessage, MessageRole};
use crate::{build_system_prompt, get_available_skills, Skill, UserProfile, COMMAND_SPECS};

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
        &[
            SidebarTab::Tasks,
            SidebarTab::Skills,
            SidebarTab::Provider,
            SidebarTab::Files,
            SidebarTab::Help,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            SidebarTab::Tasks => "Tasks (F2)",
            SidebarTab::Skills => "Skills (F3)",
            SidebarTab::Provider => "Provider (F4)",
            SidebarTab::Files => "Files (F7)",
            SidebarTab::Help => "Help (F1)",
        }
    }
}

#[derive(Clone, Debug)]
pub enum ChatItemKind {
    User,
    Assistant,
    Reasoning,
    ToolCall {
        name: String,
        args: String,
    },
    ToolResult {
        name: String,
        result: String,
        success: bool,
    },
    SystemInfo,
    Error,
}

#[derive(Clone, Debug)]
pub struct ChatItem {
    pub kind: ChatItemKind,
    pub text: String,
    pub timestamp: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitFileBadge {
    Modified,
    Staged,
    Untracked,
    Deleted,
}

impl GitFileBadge {
    pub fn label(&self) -> &'static str {
        match self {
            GitFileBadge::Modified => "MODIFIED",
            GitFileBadge::Staged => "STAGED",
            GitFileBadge::Untracked => "UNTRACKED",
            GitFileBadge::Deleted => "DELETED",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileTreeItem {
    pub relative_path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub git_status: Option<GitFileBadge>,
}

pub enum UiAction {
    AgentEvent(AgentUiEvent),
    PermissionReq(PermissionRequest),
    WorkerFinished(Result<crate::agent::orchestrator::AgentTurnResult, String>),
}

pub struct App {
    pub user_profile: UserProfile,
    pub providers_reg: ProvidersRegistry,
    pub current_model: String,
    pub active_skill: Option<Skill>,
    pub conversation: Vec<ChatMessage>,
    pub chat_items: Vec<ChatItem>,

    // Layout and navigation
    pub focused_pane: FocusedPane,
    pub active_tab: SidebarTab,
    pub chat_scroll: u16,
    pub auto_scroll: bool,

    // Input state
    pub input: String,
    pub cursor_position: usize,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,

    // Slash autocomplete
    pub slash_suggestions: Vec<(&'static str, &'static str)>,
    pub selected_slash_index: usize,
    pub show_slash_popup: bool,

    // Sidebar states
    pub selected_task_index: usize,
    pub selected_skill_index: usize,
    pub selected_provider_index: usize,
    pub selected_file_index: usize,
    pub files_list: Vec<FileTreeItem>,
    pub file_preview_content: Option<String>,
    pub file_preview_scroll: u16,
    pub task_log_scroll: u16,

    // Agent loop state
    pub agent_running: bool,
    pub streaming_reasoning: String,
    pub streaming_content: String,
    pub active_tool_call: Option<(String, String)>,
    pub cancel_token: Option<CancellationToken>,

    // Token tracking
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    pub query_count: u64,

    // Permission Dialog
    pub active_permission_request: Option<PermissionRequest>,

    // Notification message
    pub status_message: Option<(String, Instant)>,
    pub should_quit: bool,
    pub return_to_repl: bool,
    pub needs_clear: bool,

    // Communication channels
    pub action_tx: Sender<UiAction>,
    pub action_rx: Receiver<UiAction>,
    pub perm_tx: Sender<PermissionRequest>,

    // Telemetry state
    pub metrics: Option<ProcessMetrics>,
    pub last_metrics_poll: Instant,
    pub metrics_sampler: CpuSampler,

    // Visual animation tick
    pub spinner_tick: usize,
}

impl App {
    pub fn new(user_profile: UserProfile, providers_reg: ProvidersRegistry) -> Self {
        let (action_tx, action_rx) = mpsc::channel();
        let (perm_tx, perm_rx) = mpsc::channel();

        // Forward permission requests into action channel
        let action_tx_clone = action_tx.clone();
        std::thread::spawn(move || {
            while let Ok(req) = perm_rx.recv() {
                if action_tx_clone.send(UiAction::PermissionReq(req)).is_err() {
                    break;
                }
            }
        });

        let active_prov = providers_reg.get_active_provider();
        let current_model = active_prov.default_model.clone();
        let loaded_history = MemoryManager::load_session_history().unwrap_or_default();
        let mut metrics_sampler = CpuSampler::new();
        let initial_metrics = capture_metrics_with_cpu(None, &mut metrics_sampler);

        let mut chat_items = Vec::new();
        let now_str = chrono_compact_now();

        chat_items.push(ChatItem {
            kind: ChatItemKind::SystemInfo,
            text: format!(
                "Selamat datang di ctrl-cli v{} [TUI Mode]! Ketik instruksi dan tekan [Enter]. Tekan [Tab] untuk pindah panel, [F5] atau :cli untuk kembali ke REPL.",
                env!("CARGO_PKG_VERSION")
            ),
            timestamp: now_str.clone(),
        });

        for msg in &loaded_history {
            match msg.role {
                MessageRole::User => {
                    if let Some(content) = &msg.content {
                        chat_items.push(ChatItem {
                            kind: ChatItemKind::User,
                            text: content.clone(),
                            timestamp: now_str.clone(),
                        });
                    }
                }
                MessageRole::Assistant => {
                    if let Some(content) = &msg.content {
                        chat_items.push(ChatItem {
                            kind: ChatItemKind::Assistant,
                            text: content.clone(),
                            timestamp: now_str.clone(),
                        });
                    }
                }
                _ => {}
            }
        }

        let mut app = Self {
            user_profile,
            providers_reg,
            current_model,
            active_skill: None,
            conversation: loaded_history,
            chat_items,
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
            needs_clear: false,
            action_tx,
            action_rx,
            perm_tx,
            metrics: Some(initial_metrics),
            last_metrics_poll: Instant::now(),
            metrics_sampler,
            spinner_tick: 0,
        };
        app.refresh_workspace_files();
        app
    }

    /// Requests a clean terminal buffer reset before next frame render.
    pub fn request_clear(&mut self) {
        self.needs_clear = true;
    }

    /// Periodic tick for telemetry polling and animation frames.
    pub fn tick(&mut self) {
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
        let now = Instant::now();
        if now.duration_since(self.last_metrics_poll) >= std::time::Duration::from_millis(1000) {
            let m = capture_metrics_with_cpu(None, &mut self.metrics_sampler);
            self.metrics = Some(m);
            self.last_metrics_poll = now;
        }
    }

    /// Returns current spinner character frame for smooth animations.
    pub fn spinner_char(&self) -> &'static str {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[self.spinner_tick % FRAMES.len()]
    }

    /// Returns context-sensitive keybinding hints for the footer based on current focus.
    pub fn contextual_hints(&self) -> (&'static str, &'static str) {
        match self.focused_pane {
            FocusedPane::Input => {
                if self.agent_running {
                    ("⌨ INPUT", "[Esc] Batalkan Agen  [Tab] Navigasi")
                } else {
                    ("⌨ INPUT", "[Enter] Kirim  [Tab] Pindah Panel  [F1-F7] Menu  [/] Perintah")
                }
            }
            FocusedPane::Chat => {
                ("💬 CHAT", "[↑/↓] Gulir  [t] Mode Tool  [z] Lipat Reasoning  [i/Enter] Ketik")
            }
            FocusedPane::Sidebar => match self.active_tab {
                SidebarTab::Tasks => ("📋 TASKS", "[↑/↓] Pilih  [c] Batalkan  [x] Bersihkan Selesai  [Tab] Pindah"),
                SidebarTab::Skills => ("🎯 SKILLS", "[↑/↓] Pilih  [Enter] Aktifkan Peran  [Tab] Pindah"),
                SidebarTab::Provider => ("⚡ PROVIDER", "[↑/↓] Pilih  [Enter] Beralih  [Tab] Pindah"),
                SidebarTab::Files => ("📂 FILES", "[↑/↓] Pilih  [Enter] Preview  [r] Refresh  [PgUp/PgDn] Gulir  [Tab] Pindah"),
                SidebarTab::Help => ("❓ HELP", "[F1-F7] Ganti Tab  [Tab] Kembali ke Input  [F5] Mode REPL"),
            },
        }
    }

    pub fn request_return_to_repl(&mut self) {
        self.cancel_agent();
        self.return_to_repl = true;
        set_return_to_repl(true);
        self.should_quit = true;
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    pub fn clear_expired_status(&mut self) {
        if let Some((_, time)) = &self.status_message {
            if time.elapsed().as_secs() > 5 {
                self.status_message = None;
            }
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
        self.update_slash_suggestions();
    }

    pub fn backspace(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
            self.update_slash_suggestions();
        }
    }

    pub fn delete(&mut self) {
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
            self.update_slash_suggestions();
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    pub fn move_cursor_start(&mut self) {
        self.cursor_position = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_position = self.input.len();
    }

    pub fn history_up(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        let new_idx = match self.history_index {
            None => self.input_history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };
        self.history_index = Some(new_idx);
        if let Some(cmd) = self.input_history.get(new_idx) {
            self.input = cmd.clone();
            self.cursor_position = self.input.len();
            self.update_slash_suggestions();
        }
    }

    pub fn history_down(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.input_history.len() {
                let new_idx = idx + 1;
                self.history_index = Some(new_idx);
                if let Some(cmd) = self.input_history.get(new_idx) {
                    self.input = cmd.clone();
                    self.cursor_position = self.input.len();
                    self.update_slash_suggestions();
                }
            } else {
                self.history_index = None;
                self.input.clear();
                self.cursor_position = 0;
                self.show_slash_popup = false;
            }
        }
    }

    pub fn update_slash_suggestions(&mut self) {
        let trimmed = self.input.trim_start();
        if trimmed.starts_with('/') {
            let prefix = trimmed.to_lowercase();
            let mut matches = Vec::new();
            for spec in COMMAND_SPECS {
                if spec.primary.to_lowercase().starts_with(&prefix) {
                    matches.push((spec.primary, spec.description));
                } else {
                    for alias in spec.aliases {
                        if alias.to_lowercase().starts_with(&prefix) {
                            matches.push((spec.primary, spec.description));
                            break;
                        }
                    }
                }
            }
            self.slash_suggestions = matches;
            self.show_slash_popup = !self.slash_suggestions.is_empty();
            if self.selected_slash_index >= self.slash_suggestions.len() {
                self.selected_slash_index = 0;
            }
        } else {
            self.show_slash_popup = false;
            self.slash_suggestions.clear();
            self.selected_slash_index = 0;
        }
    }

    pub fn apply_selected_slash_suggestion(&mut self) {
        if let Some((cmd, _)) = self.slash_suggestions.get(self.selected_slash_index) {
            self.input = format!("{} ", cmd);
            self.cursor_position = self.input.len();
            self.show_slash_popup = false;
            self.slash_suggestions.clear();
        }
    }

    pub fn submit_input(&mut self) {
        let raw = self.input.trim().to_string();
        if raw.is_empty() {
            return;
        }

        self.input_history.push(raw.clone());
        self.history_index = None;
        self.input.clear();
        self.cursor_position = 0;
        self.show_slash_popup = false;

        // Navigation shortcuts to return cleanly to CLI / REPL
        let raw_lower = raw.trim().to_ascii_lowercase();
        if raw_lower == ":cli" || raw_lower == ":repl" || raw_lower == "/cli" || raw_lower == "/repl" {
            self.request_return_to_repl();
            return;
        }

        let now_str = chrono_compact_now();

        // Handle slash commands internally if possible
        if raw.starts_with('/') {
            self.handle_slash_command_tui(&raw, &now_str);
            return;
        }

        // Add user message to chat items
        self.chat_items.push(ChatItem {
            kind: ChatItemKind::User,
            text: raw.clone(),
            timestamp: now_str.clone(),
        });
        self.auto_scroll = true;

        // Launch agent worker thread
        self.start_agent_turn(raw);
    }

    fn handle_slash_command_tui(&mut self, cmd: &str, timestamp: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let primary = parts.first().copied().unwrap_or("");
        let primary_lower = primary.to_ascii_lowercase();

        match primary_lower.as_str() {
            "/help" | "?" | "/?" => {
                self.active_tab = SidebarTab::Help;
                self.request_clear();
                self.set_status("Menampilkan tab bantuan");
            }
            "/clear" | "/reset" => {
                self.conversation.clear();
                let _ = MemoryManager::clear_session_history();
                self.chat_items.clear();
                self.chat_items.push(ChatItem {
                    kind: ChatItemKind::SystemInfo,
                    text: "Riwayat percakapan berhasil dikosongkan. Sesi baru dimulai.".to_string(),
                    timestamp: timestamp.to_string(),
                });
                self.request_clear();
                self.set_status("Riwayat percakapan dibersihkan");
            }
            "/tasks" => {
                self.active_tab = SidebarTab::Tasks;
                self.request_clear();
                if parts.len() > 1 {
                    let sub = parts[1];
                    let tm = TaskManager::global();
                    if sub == "clear" {
                        let count = tm.clear_completed();
                        self.set_status(format!("{} tasks selesai dibersihkan", count));
                    } else if sub == "cancel" && parts.len() > 2 {
                        let id = parts[2];
                        if tm.cancel_task(id).is_ok() {
                            self.set_status(format!("Task '{}' dibatalkan", id));
                        } else {
                            self.set_status(format!("Gagal membatalkan task '{}'", id));
                        }
                    }
                } else {
                    self.set_status("Membuka tab Tasks");
                }
            }
            "/skill" | "/skills" => {
                self.active_tab = SidebarTab::Skills;
                self.request_clear();
                if parts.len() > 1 {
                    let requested = parts[1];
                    let skills = get_available_skills();
                    if let Some(s) = skills
                        .into_iter()
                        .find(|sk| sk.id.eq_ignore_ascii_case(requested))
                    {
                        self.set_status(format!("Skill diaktifkan: {}", s.name));
                        self.active_skill = Some(s);
                    } else {
                        let root = crate::tools::filesystem::get_workspace_root();
                        if let Some(dyn_skill) = crate::tools::skills::get_skill_by_name(requested, &root) {
                            self.set_status(format!("Dynamic Skill diaktifkan: {}", dyn_skill.name));
                            self.active_skill = Some(crate::Skill::from(dyn_skill));
                        } else {
                            self.set_status(format!("Skill '{}' tidak ditemukan", requested));
                        }
                    }
                } else {
                    self.set_status("Pilih skill dari sidebar Skills (Enter untuk memilih)");
                }
            }
            "/model" | "/models" => {
                if parts.len() > 1 {
                    self.current_model = parts[1].to_string();
                    self.set_status(format!("Model aktif disetel ke: {}", self.current_model));
                } else {
                    self.active_tab = SidebarTab::Provider;
                    self.request_clear();
                    self.set_status("Lihat atau ganti konfigurasi model di tab Provider");
                }
            }
            "/provider" | "/providers" => {
                self.active_tab = SidebarTab::Provider;
                self.request_clear();
                if parts.len() > 2 && (parts[1] == "switch" || parts[1] == "use") {
                    let target = parts[2];
                    if let Ok(p) = self.providers_reg.switch_active(target) {
                        self.current_model = p.default_model.clone();
                        self.set_status(format!(
                            "Beralih ke provider: {} (Model: {})",
                            p.name, self.current_model
                        ));
                    } else {
                        self.set_status(format!("Provider '{}' tidak ditemukan", target));
                    }
                } else {
                    self.set_status("Tab Provider aktif");
                }
            }
            "/files" | "/file" | "/git" => {
                self.active_tab = SidebarTab::Files;
                self.refresh_workspace_files();
                self.request_clear();
                self.set_status("Tab Workspace Files & Git Explorer aktif");
            }
            "/compact" => {
                let prov = self.providers_reg.get_active_provider();
                let limit = prov.context_window.unwrap_or(128_000);
                match crate::agent::compaction::maybe_compact_context(
                    &mut self.conversation,
                    &self.current_model,
                    &prov.api_key,
                    &prov.base_url,
                    prov.protocol,
                    limit,
                ) {
                    Ok(true) => {
                        self.chat_items.push(ChatItem {
                            kind: ChatItemKind::SystemInfo,
                            text: "🧹 Percakapan lama berhasil diringkas (context compaction)."
                                .to_string(),
                            timestamp: timestamp.to_string(),
                        });
                        self.set_status("Kompaksi konteks berhasil");
                    }
                    Ok(false) => {
                        self.set_status("Konteks belum memerlukan kompaksi");
                    }
                    Err(e) => {
                        self.set_status(format!("Gagal kompaksi: {}", e));
                    }
                }
            }
            "/theme" | "/themes" => {
                self.request_clear();
                if parts.len() > 1 {
                    let req = parts[1..].join(" ");
                    if let Some(applied) = crate::tui::ui::set_theme_by_name(&req) {
                        self.set_status(format!("🎨 Tema aktif disetel ke: {}", applied));
                    } else {
                        self.set_status(format!(
                            "Tema '{}' tidak dikenal. Tersedia: {}",
                            req,
                            crate::tui::ui::THEME_NAMES.join(", ")
                        ));
                    }
                } else {
                    let next = crate::tui::ui::cycle_theme();
                    self.set_status(format!("🎨 Tema diubah ke: {} (F6 untuk ganti tema)", next));
                }
            }
            "/zen" | "/sidebar" => {
                let collapsed = crate::tui::ui::toggle_sidebar();
                self.request_clear();
                let msg = if collapsed {
                    "🪟 Zen Mode aktif (Sidebar disembunyikan - F9 / Ctrl+B untuk membuka)"
                } else {
                    "🪟 Sidebar ditampilkan kembali"
                };
                self.set_status(msg);
            }
            "/cli" | "/repl" => {
                self.request_return_to_repl();
            }
            "/exit" | "/quit" => {
                self.cancel_agent();
                self.should_quit = true;
            }
            _ => {
                self.chat_items.push(ChatItem {
                    kind: ChatItemKind::SystemInfo,
                    text: format!(
                        "Perintah '{}' diterima. Ketik /help untuk daftar perintah.",
                        cmd
                    ),
                    timestamp: timestamp.to_string(),
                });
            }
        }
    }

    fn start_agent_turn(&mut self, prompt: String) {
        if self.agent_running {
            self.set_status("Agen sedang berjalan. Harap tunggu atau batalkan dengan Esc.");
            return;
        }

        self.agent_running = true;
        self.streaming_content.clear();
        self.streaming_reasoning.clear();
        self.active_tool_call = None;

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let action_tx = self.action_tx.clone();
        let event_sink = OutputSink::channel(mpsc_channel_bridge(action_tx.clone()));

        let active_prov = self.providers_reg.get_active_provider().clone();
        let model = self.current_model.clone();
        let system_prompt = build_system_prompt(self.active_skill.as_ref(), &self.user_profile);
        let mut conversation_clone = self.conversation.clone();
        let perm_tx = self.perm_tx.clone();

        std::thread::spawn(move || {
            let mut permission_gate =
                PermissionGate::new(PermissionMode::Ask).with_tui_requester(perm_tx);

            let res = run_agent_loop(
                &prompt,
                &mut conversation_clone,
                &model,
                &active_prov.api_key,
                &active_prov.base_url,
                active_prov.protocol,
                &mut permission_gate,
                &system_prompt,
                25,
                true,
                active_prov.context_window,
                Some(&event_sink),
                Some(&cancel_token),
            );

            match res {
                Ok(turn_res) => {
                    let _ = action_tx.send(UiAction::WorkerFinished(Ok(turn_res)));
                }
                Err(e) => {
                    let _ = action_tx.send(UiAction::WorkerFinished(Err(e.to_string())));
                }
            }
        });
    }

    pub fn cancel_agent(&mut self) {
        if let Some(token) = &self.cancel_token {
            token.cancel();
            self.set_status("Membatalkan eksekusi agen...");
        }
    }

    pub fn handle_action(&mut self, action: UiAction) {
        let now_str = chrono_compact_now();
        match action {
            UiAction::AgentEvent(ev) => match ev {
                AgentUiEvent::ReasoningChunk(chunk) => {
                    self.streaming_reasoning.push_str(&chunk);
                }
                AgentUiEvent::ContentChunk(chunk) => {
                    self.streaming_content.push_str(&chunk);
                }
                AgentUiEvent::ToolStarted { name, args } => {
                    self.active_tool_call = Some((name.clone(), args.clone()));
                    self.chat_items.push(ChatItem {
                        kind: ChatItemKind::ToolCall { name, args },
                        text: String::new(),
                        timestamp: now_str,
                    });
                    self.auto_scroll = true;
                }
                AgentUiEvent::ToolFinished {
                    name,
                    result,
                    success,
                } => {
                    self.active_tool_call = None;
                    self.chat_items.push(ChatItem {
                        kind: ChatItemKind::ToolResult {
                            name,
                            result: result.clone(),
                            success,
                        },
                        text: result,
                        timestamp: now_str,
                    });
                    self.auto_scroll = true;
                }
                AgentUiEvent::Text(t) => {
                    if !t.trim().is_empty() {
                        self.chat_items.push(ChatItem {
                            kind: ChatItemKind::SystemInfo,
                            text: t,
                            timestamp: now_str,
                        });
                    }
                }
                AgentUiEvent::Spinner(s) => {
                    self.set_status(s);
                }
                AgentUiEvent::ClearSpinner => {
                    self.status_message = None;
                }
                AgentUiEvent::Error(err) => {
                    self.chat_items.push(ChatItem {
                        kind: ChatItemKind::Error,
                        text: err,
                        timestamp: now_str,
                    });
                }
                AgentUiEvent::TurnCompleted {
                    tools_executed: _,
                    final_content,
                } => {
                    if !final_content.trim().is_empty() && self.streaming_content.is_empty() {
                        self.streaming_content = final_content;
                    }
                }
            },
            UiAction::PermissionReq(req) => {
                self.active_permission_request = Some(req);
            }
            UiAction::WorkerFinished(result) => {
                self.agent_running = false;
                self.cancel_token = None;
                self.active_tool_call = None;

                match result {
                    Ok(turn_res) => {
                        let content = if !self.streaming_content.is_empty() {
                            std::mem::take(&mut self.streaming_content)
                        } else {
                            turn_res.final_content
                        };

                        if !self.streaming_reasoning.is_empty() {
                            let reasoning = std::mem::take(&mut self.streaming_reasoning);
                            self.chat_items.push(ChatItem {
                                kind: ChatItemKind::Reasoning,
                                text: reasoning,
                                timestamp: now_str.clone(),
                            });
                        }

                        if !content.trim().is_empty() {
                            self.chat_items.push(ChatItem {
                                kind: ChatItemKind::Assistant,
                                text: content.clone(),
                                timestamp: now_str,
                            });
                            self.conversation
                                .push(ChatMessage::assistant(Some(content.clone()), None));
                        }

                        // Record tokens
                        self.query_count += 1;
                        let p = turn_res.total_usage.prompt_tokens.unwrap_or(0);
                        let c = turn_res.total_usage.completion_tokens.unwrap_or(0);
                        let t = turn_res.total_usage.total_tokens.unwrap_or(p + c);
                        self.total_prompt_tokens += p;
                        self.total_completion_tokens += c;
                        self.total_tokens += t;

                        let _ = MemoryManager::save_session_history(&self.conversation);
                        self.set_status(format!("Selesai. (+{} tokens)", t));
                    }
                    Err(err) => {
                        self.chat_items.push(ChatItem {
                            kind: ChatItemKind::Error,
                            text: format!("Error: {}", err),
                            timestamp: now_str,
                        });
                        self.set_status(format!("Gagal: {}", err));
                    }
                }
                self.auto_scroll = true;
            }
        }
    }

    pub fn respond_permission(&mut self, resp: PermissionResponse) {
        if let Some(req) = self.active_permission_request.take() {
            let _ = req.response_tx.send(resp);
        }
    }

    pub fn refresh_workspace_files(&mut self) {
        let root = crate::tools::filesystem::get_workspace_root();
        let git_status = crate::tools::git::tool_git_status(Some(root.to_str().unwrap_or("."))).ok();

        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&root) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();

            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();

                if file_name.starts_with('.') && file_name != ".gitignore" {
                    continue;
                }
                if file_name == "target" || file_name == "node_modules" {
                    continue;
                }

                let is_dir = path.is_dir();
                let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);

                let badge = if let Some(ref gs) = git_status {
                    if gs.unstaged.iter().any(|p| p == &file_name || p.starts_with(&format!("{}/", file_name)) || p.starts_with(&format!("{}\\", file_name))) {
                        Some(GitFileBadge::Modified)
                    } else if gs.staged.iter().any(|p| p == &file_name || p.starts_with(&format!("{}/", file_name)) || p.starts_with(&format!("{}\\", file_name))) {
                        Some(GitFileBadge::Staged)
                    } else if gs.untracked.iter().any(|p| p == &file_name || p.starts_with(&format!("{}/", file_name)) || p.starts_with(&format!("{}\\", file_name))) {
                        Some(GitFileBadge::Untracked)
                    } else {
                        None
                    }
                } else {
                    None
                };

                let item = FileTreeItem {
                    relative_path: file_name,
                    is_dir,
                    size_bytes,
                    git_status: badge,
                };

                if is_dir {
                    dirs.push(item);
                } else {
                    files.push(item);
                }
            }

            dirs.sort_by_key(|a| a.relative_path.to_lowercase());
            files.sort_by_key(|a| a.relative_path.to_lowercase());

            items.extend(dirs);
            items.extend(files);
        }

        self.files_list = items;
        if self.files_list.is_empty() {
            self.selected_file_index = 0;
        } else if self.selected_file_index >= self.files_list.len() {
            self.selected_file_index = self.files_list.len() - 1;
        }
        self.update_file_preview();
    }

    pub fn update_file_preview(&mut self) {
        self.file_preview_scroll = 0;
        if let Some(item) = self.files_list.get(self.selected_file_index) {
            let root = crate::tools::filesystem::get_workspace_root();
            let full_path = root.join(&item.relative_path);

            if item.is_dir {
                if let Ok(sub) = std::fs::read_dir(&full_path) {
                    let mut count = 0;
                    let mut names = Vec::new();
                    for s in sub.flatten() {
                        let name = s.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') && name != ".gitignore" {
                            continue;
                        }
                        count += 1;
                        if names.len() < 15 {
                            let icon = if s.path().is_dir() { "📁 " } else { "📄 " };
                            names.push(format!("  {} {}", icon, name));
                        }
                    }
                    let preview = format!(
                        "📁 Direktori: {} ({} entri)\n\nCuplikan isi direktori:\n{}",
                        item.relative_path,
                        count,
                        if names.is_empty() { "  (kosong)".to_string() } else { names.join("\n") }
                    );
                    self.file_preview_content = Some(preview);
                } else {
                    self.file_preview_content = Some(format!("📁 Direktori: {}", item.relative_path));
                }
            } else {
                let diff_opt = if item.git_status.is_some() {
                    crate::tools::git::tool_git_diff(
                        Some(root.to_str().unwrap_or(".")),
                        false,
                        Some(&item.relative_path),
                    ).ok().filter(|d| !d.trim().is_empty())
                } else {
                    None
                };

                if let Some(diff) = diff_opt {
                    self.file_preview_content = Some(format!("--- Git Diff ({}) ---\n{}", item.relative_path, diff));
                } else if let Ok(content) = std::fs::read_to_string(&full_path) {
                    let preview = content
                        .lines()
                        .take(120)
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.file_preview_content = Some(preview);
                } else {
                    self.file_preview_content = Some(format!(
                        "📄 [Berkas Biner / Non-UTF8]\nPath: {}\nUkuran: {} bytes",
                        item.relative_path, item.size_bytes
                    ));
                }
            }
        } else {
            self.file_preview_content = None;
        }
    }
}

fn mpsc_channel_bridge(tx: Sender<UiAction>) -> Sender<AgentUiEvent> {
    let (event_tx, event_rx) = mpsc::channel::<AgentUiEvent>();
    std::thread::spawn(move || {
        while let Ok(ev) = event_rx.recv() {
            if tx.send(UiAction::AgentEvent(ev)).is_err() {
                break;
            }
        }
    });
    event_tx
}

fn chrono_compact_now() -> String {
    use std::time::SystemTime;
    crate::agent::tasks::format_utc_timestamp(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_tab_files_and_refresh() {
        assert!(SidebarTab::all().contains(&SidebarTab::Files));
        assert_eq!(SidebarTab::Files.title(), "Files (F7)");

        let user_profile = UserProfile::default();
        let providers_reg = ProvidersRegistry::default();
        let mut app = App::new(user_profile, providers_reg);

        app.active_tab = SidebarTab::Files;
        app.refresh_workspace_files();
        assert!(!app.files_list.is_empty());
        assert!(app.file_preview_content.is_some());
    }
}
