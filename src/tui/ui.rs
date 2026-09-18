use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap,
        Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::agent::tasks::{TaskManager, TaskStatus};
use crate::get_available_skills;
use crate::tui::app::{App, ChatItemKind, FocusedPane, GitFileBadge, SidebarTab};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

#[allow(clippy::duplicate_mod)]
#[path = "highlight.rs"]
pub mod highlight;
use highlight::{get_language_label, highlight_code_line_spans};

static SPINNER_TICK: AtomicUsize = AtomicUsize::new(0);

fn get_spinner_char() -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let tick = SPINNER_TICK.fetch_add(1, Ordering::Relaxed);
    FRAMES[tick % FRAMES.len()]
}

fn get_contextual_hints(app: &App) -> (&'static str, &'static str) {
    match app.focused_pane {
        FocusedPane::Input => {
            if app.agent_running {
                ("⌨ INPUT", "[Esc] Batalkan Agen  [Tab] Navigasi")
            } else {
                ("⌨ INPUT", "[Enter] Kirim  [Tab] Pindah Panel  [F1-F4] Menu  [/] Perintah")
            }
        }
        FocusedPane::Chat => {
            ("💬 CHAT", "[↑/↓] Gulir  [PgUp/PgDn] Halaman  [Home/End] Awal/Akhir  [i/Enter] Ketik")
        }
        FocusedPane::Sidebar => match app.active_tab {
            SidebarTab::Tasks => ("📋 TASKS", "[↑/↓] Pilih  [c] Batalkan  [x] Bersihkan Selesai  [Tab] Pindah"),
            SidebarTab::Skills => ("🎯 SKILLS", "[↑/↓] Pilih  [Enter] Aktifkan Peran  [Tab] Pindah"),
            SidebarTab::Provider => ("⚡ PROVIDER", "[↑/↓] Pilih  [Enter] Beralih  [Tab] Pindah"),
            SidebarTab::Files => ("📂 FILES", "[↑/↓] Pilih  [Tab] Pindah"),
            SidebarTab::Help => ("❓ HELP", "[F1-F5] Ganti Tab  [Tab] Kembali ke Input  [F5] Mode REPL"),
        },
    }
}

trait AsStrSlice {
    fn as_str_slice(&self) -> &str;
}

impl AsStrSlice for str {
    fn as_str_slice(&self) -> &str {
        self
    }
}

impl AsStrSlice for Cow<'static, str> {
    fn as_str_slice(&self) -> &str {
        self.as_ref()
    }
}

// Global UI Runtime States (Preserves 100% test compatibility)
pub static CURRENT_THEME_ID: AtomicUsize = AtomicUsize::new(0);
pub static SIDEBAR_COLLAPSED: AtomicBool = AtomicBool::new(false);
pub static THOUGHT_PROCESS_FOLDED: AtomicBool = AtomicBool::new(false);
pub static SHOW_COMMAND_PALETTE: AtomicBool = AtomicBool::new(false);
pub static SELECTED_PALETTE_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static PALETTE_SEARCH_QUERY: Mutex<String> = Mutex::new(String::new());

pub const THEME_NAMES: &[&str] = &[
    "Tokyo Night",
    "Catppuccin Mocha",
    "Gruvbox Dark",
    "Cyberpunk Matrix",
    "Monokai Pro",
];

pub fn get_theme_count() -> usize {
    THEME_NAMES.len()
}

pub fn get_current_theme_id() -> usize {
    CURRENT_THEME_ID.load(Ordering::Relaxed) % THEME_NAMES.len()
}

pub fn get_current_theme_name() -> &'static str {
    THEME_NAMES[get_current_theme_id()]
}

pub fn cycle_theme() -> &'static str {
    let next = (get_current_theme_id() + 1) % THEME_NAMES.len();
    CURRENT_THEME_ID.store(next, Ordering::Relaxed);
    THEME_NAMES[next]
}

pub fn set_theme_by_index(idx: usize) -> &'static str {
    let valid_idx = idx % THEME_NAMES.len();
    CURRENT_THEME_ID.store(valid_idx, Ordering::Relaxed);
    THEME_NAMES[valid_idx]
}

pub fn set_theme_by_name(name: &str) -> Option<&'static str> {
    let norm = name.trim().to_lowercase();
    for (idx, &theme_name) in THEME_NAMES.iter().enumerate() {
        if theme_name.to_lowercase().contains(&norm) || norm.contains(&theme_name.to_lowercase()) {
            CURRENT_THEME_ID.store(idx, Ordering::Relaxed);
            return Some(theme_name);
        }
    }
    None
}

pub fn toggle_sidebar() -> bool {
    let next = !SIDEBAR_COLLAPSED.load(Ordering::Relaxed);
    SIDEBAR_COLLAPSED.store(next, Ordering::Relaxed);
    next
}

pub fn is_sidebar_collapsed() -> bool {
    SIDEBAR_COLLAPSED.load(Ordering::Relaxed)
}

pub fn toggle_thought_folding() -> bool {
    let next = !THOUGHT_PROCESS_FOLDED.load(Ordering::Relaxed);
    THOUGHT_PROCESS_FOLDED.store(next, Ordering::Relaxed);
    next
}

pub fn is_thought_process_folded() -> bool {
    THOUGHT_PROCESS_FOLDED.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAccordionMode {
    Compact,
    Preview,
    Full,
}

pub static TOOL_ACCORDION_MODE: AtomicUsize = AtomicUsize::new(1); // default 1 = Preview

pub fn get_tool_accordion_mode() -> ToolAccordionMode {
    match TOOL_ACCORDION_MODE.load(Ordering::Relaxed) % 3 {
        0 => ToolAccordionMode::Compact,
        1 => ToolAccordionMode::Preview,
        2 => ToolAccordionMode::Full,
        _ => ToolAccordionMode::Preview,
    }
}

pub fn cycle_tool_accordion_mode() -> &'static str {
    let next = (TOOL_ACCORDION_MODE.load(Ordering::Relaxed) + 1) % 3;
    TOOL_ACCORDION_MODE.store(next, Ordering::Relaxed);
    match next {
        0 => "Compact (Accordion 1 baris)",
        1 => "Preview (Ringkasan 6 baris)",
        2 => "Full (Output lengkap)",
        _ => "Preview",
    }
}

pub fn set_tool_accordion_mode(mode: ToolAccordionMode) {
    let val = match mode {
        ToolAccordionMode::Compact => 0,
        ToolAccordionMode::Preview => 1,
        ToolAccordionMode::Full => 2,
    };
    TOOL_ACCORDION_MODE.store(val, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug)]
pub struct PaletteCommand {
    pub icon: &'static str,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub action_id: &'static str,
}

pub const PALETTE_COMMANDS: &[PaletteCommand] = &[
    PaletteCommand { icon: "🎨", title: "Ganti Tema: Tokyo Night", shortcut: "F6", action_id: "theme:0" },
    PaletteCommand { icon: "🎨", title: "Ganti Tema: Catppuccin Mocha", shortcut: "F6", action_id: "theme:1" },
    PaletteCommand { icon: "🎨", title: "Ganti Tema: Gruvbox Dark", shortcut: "F6", action_id: "theme:2" },
    PaletteCommand { icon: "🎨", title: "Ganti Tema: Cyberpunk Matrix", shortcut: "F6", action_id: "theme:3" },
    PaletteCommand { icon: "🎨", title: "Ganti Tema: Monokai Pro", shortcut: "F6", action_id: "theme:4" },
    PaletteCommand { icon: "🪟", title: "Toggle Zen Mode (Sidebar)", shortcut: "F9", action_id: "zen" },
    PaletteCommand { icon: "💭", title: "Toggle Lipat Thought Process", shortcut: "z", action_id: "fold_thought" },
    PaletteCommand { icon: "🗂️", title: "Ganti Mode Tool Results (Accordion/Preview/Full)", shortcut: "t", action_id: "toggle_tool_mode" },
    PaletteCommand { icon: "📂", title: "Buka Tab Workspace & Git Explorer", shortcut: "F7", action_id: "tab_files" },
    PaletteCommand { icon: "🧹", title: "Bersihkan Chat Stream", shortcut: "Ctrl+L", action_id: "clear_chat" },
    PaletteCommand { icon: "📋", title: "Buka Tab Background Tasks", shortcut: "F2", action_id: "tab_tasks" },
    PaletteCommand { icon: "🎯", title: "Buka Tab AI Skills", shortcut: "F3", action_id: "tab_skills" },
    PaletteCommand { icon: "⚡", title: "Buka Tab AI Providers", shortcut: "F4", action_id: "tab_providers" },
    PaletteCommand { icon: "❓", title: "Buka Tab Bantuan / Help", shortcut: "F1", action_id: "tab_help" },
    PaletteCommand { icon: "📦", title: "Kompaksi Konteks Token", shortcut: "/compact", action_id: "compact" },
    PaletteCommand { icon: "🔄", title: "Kembali ke Mode REPL / CLI", shortcut: "F5", action_id: "repl" },
    PaletteCommand { icon: "🚪", title: "Keluar dari ctrl-cli", shortcut: "Ctrl+C", action_id: "quit" },
];

pub fn toggle_command_palette() -> bool {
    let next = !SHOW_COMMAND_PALETTE.load(Ordering::Relaxed);
    SHOW_COMMAND_PALETTE.store(next, Ordering::Relaxed);
    if next {
        SELECTED_PALETTE_INDEX.store(0, Ordering::Relaxed);
        if let Ok(mut q) = PALETTE_SEARCH_QUERY.lock() {
            q.clear();
        }
    }
    next
}

pub fn close_command_palette() {
    SHOW_COMMAND_PALETTE.store(false, Ordering::Relaxed);
}

pub fn is_command_palette_open() -> bool {
    SHOW_COMMAND_PALETTE.load(Ordering::Relaxed)
}

pub fn get_palette_query() -> String {
    PALETTE_SEARCH_QUERY.lock().map(|q| q.clone()).unwrap_or_default()
}

pub fn palette_append_char(c: char) {
    if let Ok(mut q) = PALETTE_SEARCH_QUERY.lock() {
        q.push(c);
    }
    SELECTED_PALETTE_INDEX.store(0, Ordering::Relaxed);
}

pub fn palette_backspace() {
    if let Ok(mut q) = PALETTE_SEARCH_QUERY.lock() {
        q.pop();
    }
    SELECTED_PALETTE_INDEX.store(0, Ordering::Relaxed);
}

pub fn get_filtered_palette_commands() -> Vec<&'static PaletteCommand> {
    let query = get_palette_query().to_lowercase();
    if query.is_empty() {
        PALETTE_COMMANDS.iter().collect()
    } else {
        PALETTE_COMMANDS
            .iter()
            .filter(|cmd| {
                cmd.title.to_lowercase().contains(&query)
                    || cmd.shortcut.to_lowercase().contains(&query)
                    || cmd.action_id.to_lowercase().contains(&query)
            })
            .collect()
    }
}

pub fn palette_move_up() {
    let cmds = get_filtered_palette_commands();
    if cmds.is_empty() {
        return;
    }
    let cur = SELECTED_PALETTE_INDEX.load(Ordering::Relaxed);
    if cur > 0 {
        SELECTED_PALETTE_INDEX.store(cur - 1, Ordering::Relaxed);
    } else {
        SELECTED_PALETTE_INDEX.store(cmds.len().saturating_sub(1), Ordering::Relaxed);
    }
}

pub fn palette_move_down() {
    let cmds = get_filtered_palette_commands();
    if cmds.is_empty() {
        return;
    }
    let cur = SELECTED_PALETTE_INDEX.load(Ordering::Relaxed);
    if cur + 1 < cmds.len() {
        SELECTED_PALETTE_INDEX.store(cur + 1, Ordering::Relaxed);
    } else {
        SELECTED_PALETTE_INDEX.store(0, Ordering::Relaxed);
    }
}

pub fn get_selected_palette_command() -> Option<&'static PaletteCommand> {
    let cmds = get_filtered_palette_commands();
    let idx = SELECTED_PALETTE_INDEX.load(Ordering::Relaxed);
    cmds.get(idx).copied()
}

pub fn get_git_branch() -> Option<String> {
    for path in &[".git/HEAD", "../.git/HEAD", "../../.git/HEAD"] {
        if let Ok(content) = std::fs::read_to_string(path) {
            let trimmed = content.trim();
            if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
                return Some(branch.to_string());
            } else if trimmed.len() >= 7 {
                return Some(trimmed[..7].to_string());
            }
        }
    }
    None
}

/// Unified aesthetic dark color theme with multiple switchable presets.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub border_normal: Color,
    pub border_focused: Color,
    pub border_subtle: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub text_main: Color,
    pub text_dim: Color,
    pub text_bright: Color,
    pub card_bg: Color,
    pub highlight_bg: Color,
}

impl Theme {
    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night",
            border_normal: Color::Rgb(76, 86, 106),
            border_focused: Color::Rgb(136, 192, 208),
            border_subtle: Color::Rgb(59, 66, 82),
            primary: Color::Rgb(136, 192, 208),
            secondary: Color::Rgb(180, 142, 173),
            accent: Color::Rgb(235, 203, 139),
            success: Color::Rgb(163, 190, 140),
            warning: Color::Rgb(235, 203, 139),
            error: Color::Rgb(191, 97, 106),
            text_main: Color::Rgb(229, 233, 240),
            text_dim: Color::Rgb(129, 140, 160),
            text_bright: Color::Rgb(255, 255, 255),
            card_bg: Color::Rgb(46, 52, 64),
            highlight_bg: Color::Rgb(67, 76, 94),
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha",
            border_normal: Color::Rgb(88, 91, 112),
            border_focused: Color::Rgb(203, 166, 247),
            border_subtle: Color::Rgb(69, 71, 90),
            primary: Color::Rgb(137, 180, 250),
            secondary: Color::Rgb(203, 166, 247),
            accent: Color::Rgb(245, 194, 231),
            success: Color::Rgb(166, 227, 161),
            warning: Color::Rgb(249, 226, 175),
            error: Color::Rgb(243, 139, 168),
            text_main: Color::Rgb(205, 214, 244),
            text_dim: Color::Rgb(147, 153, 178),
            text_bright: Color::Rgb(255, 255, 255),
            card_bg: Color::Rgb(30, 30, 46),
            highlight_bg: Color::Rgb(49, 50, 68),
        }
    }

    pub fn gruvbox_dark() -> Self {
        Self {
            name: "Gruvbox Dark",
            border_normal: Color::Rgb(102, 92, 84),
            border_focused: Color::Rgb(250, 189, 47),
            border_subtle: Color::Rgb(80, 73, 69),
            primary: Color::Rgb(142, 192, 124),
            secondary: Color::Rgb(211, 134, 155),
            accent: Color::Rgb(254, 128, 25),
            success: Color::Rgb(184, 187, 38),
            warning: Color::Rgb(250, 189, 47),
            error: Color::Rgb(251, 73, 52),
            text_main: Color::Rgb(235, 219, 178),
            text_dim: Color::Rgb(168, 153, 132),
            text_bright: Color::Rgb(253, 244, 193),
            card_bg: Color::Rgb(40, 40, 40),
            highlight_bg: Color::Rgb(60, 56, 54),
        }
    }

    pub fn cyberpunk_matrix() -> Self {
        Self {
            name: "Cyberpunk Matrix",
            border_normal: Color::Rgb(0, 100, 50),
            border_focused: Color::Rgb(0, 255, 102),
            border_subtle: Color::Rgb(20, 40, 30),
            primary: Color::Rgb(0, 255, 102),
            secondary: Color::Rgb(0, 229, 255),
            accent: Color::Rgb(255, 0, 128),
            success: Color::Rgb(0, 255, 102),
            warning: Color::Rgb(255, 230, 0),
            error: Color::Rgb(255, 34, 85),
            text_main: Color::Rgb(220, 255, 230),
            text_dim: Color::Rgb(0, 170, 90),
            text_bright: Color::Rgb(255, 255, 255),
            card_bg: Color::Rgb(10, 20, 15),
            highlight_bg: Color::Rgb(20, 45, 30),
        }
    }

    pub fn monokai_pro() -> Self {
        Self {
            name: "Monokai Pro",
            border_normal: Color::Rgb(90, 85, 95),
            border_focused: Color::Rgb(255, 216, 102),
            border_subtle: Color::Rgb(60, 55, 65),
            primary: Color::Rgb(120, 220, 232),
            secondary: Color::Rgb(171, 157, 242),
            accent: Color::Rgb(255, 97, 136),
            success: Color::Rgb(169, 220, 118),
            warning: Color::Rgb(255, 216, 102),
            error: Color::Rgb(255, 97, 136),
            text_main: Color::Rgb(252, 252, 250),
            text_dim: Color::Rgb(147, 146, 147),
            text_bright: Color::Rgb(255, 255, 255),
            card_bg: Color::Rgb(45, 42, 46),
            highlight_bg: Color::Rgb(64, 60, 65),
        }
    }

    pub fn current() -> Self {
        match get_current_theme_id() {
            0 => Self::tokyo_night(),
            1 => Self::catppuccin_mocha(),
            2 => Self::gruvbox_dark(),
            3 => Self::cyberpunk_matrix(),
            4 => Self::monokai_pro(),
            _ => Self::tokyo_night(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::tokyo_night()
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    app.tick();
    let theme = Theme::current();
    let size = frame.area();

    // Clear entire frame buffer first to eliminate any ghost cells or leftover text
    frame.render_widget(Clear, size);

    // Main vertical layout: Header, Main Body (Chat + Sidebar), Input & Status
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Chat + Sidebar
            Constraint::Length(4), // Input box
            Constraint::Length(1), // Keybinding bar
        ])
        .split(size);

    render_header(frame, app, main_chunks[0], &theme);
    render_body(frame, app, main_chunks[1], &theme);
    render_input(frame, app, main_chunks[2], &theme);
    render_footer(frame, app, main_chunks[3], &theme);

    // Render Autocomplete Popup if active
    if app.show_slash_popup && !app.slash_suggestions.is_empty() {
        render_slash_popup(frame, app, main_chunks[2], &theme);
    }

    // Render Permission Modal Dialog if active
    if app.active_permission_request.is_some() {
        render_permission_modal(frame, app, size, &theme);
    }

    // Render Command Palette Modal if active
    if is_command_palette_open() {
        render_command_palette(frame, size, &theme);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);

    let active_prov = app.providers_reg.get_active_provider();
    let skill_name = app
        .active_skill
        .as_ref()
        .map(|s| s.name.as_str_slice())
        .unwrap_or("General Agent");

    let status_spans = if app.agent_running {
        if let Some((name, _)) = &app.active_tool_call {
            vec![
                Span::styled(
                    format!(" ⚙ TOOL [{}] ", name),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]
        } else {
            vec![
                Span::styled(
                    format!(" {} THINKING... ", get_spinner_char()),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]
        }
    } else {
        vec![
            Span::styled(" ● ", Style::default().fg(theme.success)),
            Span::styled(
                "IDLE ",
                Style::default()
                    .fg(theme.text_main)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    };

    let git_spans = if let Some(branch) = get_git_branch() {
        vec![
            Span::styled("│ ", Style::default().fg(theme.border_subtle)),
            Span::styled(
                format!(" 🌿 {} ", branch),
                Style::default()
                    .bg(theme.card_bg)
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };

    let zen_spans = if is_sidebar_collapsed() {
        vec![
            Span::styled("│ ", Style::default().fg(theme.border_subtle)),
            Span::styled(
                " 🪟 ZEN (F9) ",
                Style::default()
                    .bg(theme.card_bg)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };

    let theme_spans = vec![
        Span::styled("│ ", Style::default().fg(theme.border_subtle)),
        Span::styled(
            format!(" 🎨 {} (F6) ", theme.name),
            Style::default()
                .bg(theme.card_bg)
                .fg(theme.secondary)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    let mut title_spans = vec![
        Span::styled(
            " ⚡ CTRL-CLI ",
            Style::default()
                .bg(theme.primary)
                .fg(Color::Rgb(24, 28, 36))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            concat!(" v", env!("CARGO_PKG_VERSION"), " "),
            Style::default().fg(theme.text_dim),
        ),
        Span::styled("│ ", Style::default().fg(theme.border_subtle)),
        Span::styled("Provider: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format!("{} ", active_prov.name),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(theme.border_subtle)),
        Span::styled("Model: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format!("{} ", app.current_model),
            Style::default()
                .fg(theme.text_bright)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(theme.border_subtle)),
        Span::styled("Skill: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format!("🎯 {} ", skill_name),
            Style::default()
                .fg(theme.secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(theme.border_subtle)),
        Span::styled("Status: ", Style::default().fg(theme.text_dim)),
    ];
    title_spans.extend(status_spans);

    // Conditionally include optional badges only if terminal width permits to avoid wrapping overflow
    if area.width >= 105 {
        title_spans.extend(git_spans);
    }
    if area.width >= 120 {
        title_spans.extend(theme_spans);
    }
    if area.width >= 135 {
        title_spans.extend(zen_spans);
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_normal));

    let header_para = Paragraph::new(Line::from(title_spans)).block(block);
    frame.render_widget(header_para, area);
}

fn parse_markdown_spans<'a>(text: &'a str, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        if let Some(start_tick) = rest.find('`') {
            if start_tick > 0 {
                spans.push(Span::styled(&rest[..start_tick], Style::default().fg(theme.text_main)));
            }
            let after_start = &rest[start_tick + 1..];
            if let Some(end_tick) = after_start.find('`') {
                let code_content = &after_start[..end_tick];
                spans.push(Span::styled(
                    format!(" {} ", code_content),
                    Style::default()
                        .fg(theme.accent)
                        .bg(theme.card_bg)
                        .add_modifier(Modifier::BOLD),
                ));
                rest = &after_start[end_tick + 1..];
            } else {
                spans.push(Span::styled(&rest[start_tick..], Style::default().fg(theme.text_main)));
                break;
            }
        } else {
            spans.push(Span::styled(rest, Style::default().fg(theme.text_main)));
            break;
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    spans
}

fn render_body(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);
    if is_sidebar_collapsed() {
        render_chat(frame, app, area, theme);
    } else {
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(63), // Chat
                Constraint::Percentage(37), // Sidebar
            ])
            .split(area);

        render_chat(frame, app, body_chunks[0], theme);
        render_sidebar(frame, app, body_chunks[1], theme);
    }
}

fn render_chat(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);
    let is_focused = app.focused_pane == FocusedPane::Chat;
    let border_color = if is_focused {
        theme.border_focused
    } else {
        theme.border_normal
    };

    let title = if is_focused {
        Span::styled(
            " 💬 Chat & Execution Stream [Fokus: ↑/↓ Gulir • Esc Kembali] ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " 💬 Chat & Execution Stream ",
            Style::default().add_modifier(Modifier::BOLD),
        )
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let mut lines = Vec::new();

    if app.chat_items.is_empty() && !app.agent_running {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("   ⚡ ", Style::default().fg(theme.primary)),
            Span::styled(
                "Selamat datang di ctrl-cli v",
                Style::default()
                    .fg(theme.text_bright)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                env!("CARGO_PKG_VERSION"),
                Style::default()
                    .fg(theme.text_bright)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" [TUI Mode]", Style::default().fg(theme.secondary)),
        ]));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("   • Ketik instruksi atau pertanyaan pada panel input di bawah.", Style::default().fg(theme.text_main)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   • Tekan ", Style::default().fg(theme.text_main)),
            Span::styled("[Tab]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" untuk berpindah fokus antara Input, Chat, dan Sidebar.", Style::default().fg(theme.text_main)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   • Ketik ", Style::default().fg(theme.text_main)),
            Span::styled("'/'", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" untuk menampilkan perintah cepat (/help, /model, /skill, dll.).", Style::default().fg(theme.text_main)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   • Tekan ", Style::default().fg(theme.text_main)),
            Span::styled("[F5]", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" atau ketik ", Style::default().fg(theme.text_main)),
            Span::styled(":cli", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" kapan saja untuk kembali ke mode REPL biasa.", Style::default().fg(theme.text_main)),
        ]));
        lines.push(Line::raw(""));
    }

    for item in &app.chat_items {
        match &item.kind {
            ChatItemKind::User => {
                lines.push(Line::from(vec![
                    Span::styled("╭─ 👤 You ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("( {} )", item.timestamp), Style::default().fg(theme.text_dim)),
                    Span::styled(" ──────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                for text_line in item.text.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                        Span::styled(text_line, Style::default().fg(theme.text_bright)),
                    ]));
                }
                lines.push(Line::from(vec![
                    Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                lines.push(Line::raw(""));
            }
            ChatItemKind::Assistant => {
                lines.push(Line::from(vec![
                    Span::styled("╭─ 🤖 Assistant ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("( {} )", item.timestamp), Style::default().fg(theme.text_dim)),
                    Span::styled(" ────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));

                let mut in_code_block = false;
                let mut code_lang = String::new();
                let mut code_line_num = 0;

                for text_line in item.text.lines() {
                    if text_line.starts_with("```") {
                        if !in_code_block {
                            in_code_block = true;
                            code_lang = text_line.trim_start_matches("```").trim().to_lowercase();
                            code_line_num = 0;
                            let lang_label = get_language_label(&code_lang);
                            lines.push(Line::from(vec![
                                Span::styled("│  ┌─ ", Style::default().fg(theme.border_subtle)),
                                Span::styled(lang_label, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
                                Span::styled(" ────────────────────────────────────┐", Style::default().fg(theme.border_subtle)),
                            ]));
                        } else {
                            in_code_block = false;
                            lines.push(Line::from(vec![
                                Span::styled("│  └────────────────────────────────────────────────┘", Style::default().fg(theme.border_subtle)),
                            ]));
                        }
                    } else if in_code_block {
                        code_line_num += 1;
                        let mut line_spans = vec![
                            Span::styled("│  │ ", Style::default().fg(theme.border_subtle)),
                            Span::styled(format!("{:2} │ ", code_line_num), Style::default().fg(theme.text_dim)),
                        ];
                        line_spans.extend(highlight_code_line_spans(text_line, &code_lang));
                        lines.push(Line::from(line_spans));
                    } else {
                        if text_line.starts_with("# ") {
                            lines.push(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                                Span::styled(text_line, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                            ]));
                        } else if text_line.starts_with("## ") {
                            lines.push(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                                Span::styled(text_line, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
                            ]));
                        } else if text_line.starts_with("### ") {
                            lines.push(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                                Span::styled(text_line, Style::default().fg(theme.text_bright).add_modifier(Modifier::BOLD)),
                            ]));
                        } else if text_line.starts_with("- ") || text_line.starts_with("* ") {
                            let mut line_spans = vec![
                                Span::styled("│  • ", Style::default().fg(theme.accent)),
                            ];
                            line_spans.extend(parse_markdown_spans(&text_line[2..], theme));
                            lines.push(Line::from(line_spans));
                        } else if let Some(stripped) = text_line.strip_prefix("> ") {
                            lines.push(Line::from(vec![
                                Span::styled("│  ▎ ", Style::default().fg(theme.secondary)),
                                Span::styled(stripped, Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                            ]));
                        } else {
                            let mut line_spans = vec![
                                Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                            ];
                            line_spans.extend(parse_markdown_spans(text_line, theme));
                            lines.push(Line::from(line_spans));
                        }
                    }
                }
                if in_code_block {
                    lines.push(Line::from(vec![
                        Span::styled("│  └────────────────────────────────────────────────┘", Style::default().fg(theme.border_subtle)),
                    ]));
                }

                lines.push(Line::from(vec![
                    Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                lines.push(Line::raw(""));
            }
            ChatItemKind::Reasoning => {
                if is_thought_process_folded() {
                    lines.push(Line::from(vec![
                        Span::styled("╭─ 💭 Thought Process ", Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                        Span::styled("[Dilipat - tekan 'z' di panel chat untuk membuka] ", Style::default().fg(theme.accent)),
                        Span::styled("─────────────────╯", Style::default().fg(theme.border_subtle)),
                    ]));
                    lines.push(Line::raw(""));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("╭─ 💭 Thought Process ", Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                        Span::styled("───────────────────────────────────", Style::default().fg(theme.border_subtle)),
                    ]));
                    for r_line in item.text.lines() {
                        lines.push(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                            Span::styled(r_line, Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                        ]));
                    }
                    lines.push(Line::from(vec![
                        Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                    ]));
                    lines.push(Line::raw(""));
                }
            }
            ChatItemKind::ToolCall { name, args } => {
                lines.push(Line::from(vec![
                    Span::styled("╭─ ⚡ PEMANGGILAN TOOL: ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                    Span::styled(name, Style::default().fg(theme.text_bright).add_modifier(Modifier::BOLD)),
                    Span::styled(" ──────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("│  Argumen: ", Style::default().fg(theme.text_dim)),
                    Span::styled(args, Style::default().fg(theme.accent)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                lines.push(Line::raw(""));
            }
            ChatItemKind::ToolResult {
                name,
                result,
                success,
            } => {
                let (sym, col) = if *success {
                    ("✔ SUKSES", theme.success)
                } else {
                    ("✖ GAGAL", theme.error)
                };

                let all_lines: Vec<&str> = result.lines().collect();

                match get_tool_accordion_mode() {
                    ToolAccordionMode::Compact => {
                        lines.push(Line::from(vec![
                            Span::styled("╭─ ⚙ [", Style::default().fg(theme.border_subtle)),
                            Span::styled(sym, Style::default().fg(col).add_modifier(Modifier::BOLD)),
                            Span::styled("] ", Style::default().fg(theme.border_subtle)),
                            Span::styled(name, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                            Span::styled(format!(" ({} baris output)", all_lines.len()), Style::default().fg(theme.text_dim)),
                            Span::styled(" ── [t: Expand] ────────────────────╯", Style::default().fg(theme.border_subtle)),
                        ]));
                        lines.push(Line::raw(""));
                    }
                    ToolAccordionMode::Preview => {
                        lines.push(Line::from(vec![
                            Span::styled(format!("╭─ {} ", sym), Style::default().fg(col).add_modifier(Modifier::BOLD)),
                            Span::styled(format!("( {} )", name), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                            Span::styled(" ──────────────────────────────────", Style::default().fg(theme.border_subtle)),
                        ]));

                        let limit = 6;
                        let preview = &all_lines[..all_lines.len().min(limit)];
                        for r in preview {
                            lines.push(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                                Span::styled(*r, Style::default().fg(theme.text_main)),
                            ]));
                        }
                        if all_lines.len() > limit {
                            lines.push(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                                Span::styled(
                                    format!("... (+{} baris tersembunyi • tekan 't' untuk mode Full)", all_lines.len() - limit),
                                    Style::default().fg(theme.accent).add_modifier(Modifier::ITALIC),
                                ),
                            ]));
                        }
                        lines.push(Line::from(vec![
                            Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                        ]));
                        lines.push(Line::raw(""));
                    }
                    ToolAccordionMode::Full => {
                        lines.push(Line::from(vec![
                            Span::styled(format!("╭─ {} [FULL] ", sym), Style::default().fg(col).add_modifier(Modifier::BOLD)),
                            Span::styled(format!("( {} )", name), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                            Span::styled(" ── [t: Lipat] ──────────────────────", Style::default().fg(theme.border_subtle)),
                        ]));

                        for (idx, r) in all_lines.iter().enumerate() {
                            let line_no = format!("{:3} │ ", idx + 1);
                            let content_style = if r.starts_with('+') && !r.starts_with("+++") {
                                Style::default().fg(theme.success)
                            } else if r.starts_with('-') && !r.starts_with("---") {
                                Style::default().fg(theme.error)
                            } else {
                                Style::default().fg(theme.text_main)
                            };
                            lines.push(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                                Span::styled(line_no, Style::default().fg(theme.text_dim)),
                                Span::styled(*r, content_style),
                            ]));
                        }
                        lines.push(Line::from(vec![
                            Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                        ]));
                        lines.push(Line::raw(""));
                    }
                }
            }
            ChatItemKind::SystemInfo => {
                lines.push(Line::from(vec![
                    Span::styled("╭─ ℹ SISTEM ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled("────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                for text_line in item.text.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                        Span::styled(text_line, Style::default().fg(theme.primary)),
                    ]));
                }
                lines.push(Line::from(vec![
                    Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                lines.push(Line::raw(""));
            }
            ChatItemKind::Error => {
                lines.push(Line::from(vec![
                    Span::styled("╭─ ❌ ERROR ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
                    Span::styled("─────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                for text_line in item.text.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                        Span::styled(text_line, Style::default().fg(theme.error)),
                    ]));
                }
                lines.push(Line::from(vec![
                    Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                lines.push(Line::raw(""));
            }
        }
    }

    // In-flight streaming display
    if app.agent_running {
        if !app.streaming_reasoning.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(format!("╭─ 💭 {} Berpikir (Reasoning)... ", get_spinner_char()), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled("─────────────────────────────", Style::default().fg(theme.border_subtle)),
            ]));
            for r_line in app
                .streaming_reasoning
                .lines()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                lines.push(Line::from(vec![
                    Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                    Span::styled(r_line, Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
            ]));
            lines.push(Line::raw(""));
        }

        if !app.streaming_content.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(format!("╭─ 🤖 {} Asisten Mengetik... ", get_spinner_char()), Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
                Span::styled("───────────────────────────", Style::default().fg(theme.border_subtle)),
            ]));
            let s_lines: Vec<&str> = app.streaming_content.lines().collect();
            for (idx, s_line) in s_lines.iter().enumerate() {
                let is_last = idx + 1 == s_lines.len();
                let mut content_spans = vec![
                    Span::styled("│  ", Style::default().fg(theme.border_subtle)),
                    Span::styled(*s_line, Style::default().fg(theme.text_bright)),
                ];
                if is_last {
                    content_spans.push(Span::styled(" █", Style::default().fg(theme.primary)));
                }
                lines.push(Line::from(content_spans));
            }
            lines.push(Line::from(vec![
                Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
            ]));
            lines.push(Line::raw(""));
        }

        if let Some((name, args)) = &app.active_tool_call {
            lines.push(Line::from(vec![
                Span::styled(format!("╭─ ⚡ {} Menjalankan Tool: ", get_spinner_char()), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                Span::styled(name, Style::default().fg(theme.text_bright).add_modifier(Modifier::BOLD)),
                Span::styled(" ────────────────────────", Style::default().fg(theme.border_subtle)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("│  Argumen: ", Style::default().fg(theme.text_dim)),
                Span::styled(args, Style::default().fg(theme.accent)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
            ]));
            lines.push(Line::raw(""));
        }
    }

    let total_lines = lines.len() as u16;
    let visible_height = area.height.saturating_sub(2);

    if app.auto_scroll {
        if total_lines > visible_height {
            app.chat_scroll = total_lines - visible_height;
        } else {
            app.chat_scroll = 0;
        }
    }

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.chat_scroll, 0));

    frame.render_widget(para, area);

    // Render Scrollbar on the right border if total_lines exceeds visible_height
    if total_lines > visible_height {
        let max_scroll = total_lines.saturating_sub(visible_height) as usize;
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        let mut scrollbar_state = ScrollbarState::new(max_scroll)
            .position(app.chat_scroll as usize);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }
}

fn render_sidebar(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);

    let is_focused = app.focused_pane == FocusedPane::Sidebar;
    let border_color = if is_focused {
        theme.border_focused
    } else {
        theme.border_normal
    };

    let sidebar_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(5),    // Tab contents
        ])
        .split(area);

    frame.render_widget(Clear, sidebar_layout[0]);
    frame.render_widget(Clear, sidebar_layout[1]);

    let tab_titles: Vec<Line> = SidebarTab::all()
        .iter()
        .map(|t| {
            let is_sel = *t == app.active_tab;
            let title = if area.width >= 50 {
                match t {
                    SidebarTab::Tasks => "📋 Tasks (F2)",
                    SidebarTab::Skills => "🎯 Skills (F3)",
                    SidebarTab::Provider => "⚡ Prov (F4)",
                    SidebarTab::Files => "📂 Files (F7)",
                    SidebarTab::Help => "❓ Help (F1)",
                }
            } else if area.width >= 35 {
                match t {
                    SidebarTab::Tasks => "Tasks(F2)",
                    SidebarTab::Skills => "Skills(F3)",
                    SidebarTab::Provider => "Prov(F4)",
                    SidebarTab::Files => "Files(F7)",
                    SidebarTab::Help => "Help(F1)",
                }
            } else {
                match t {
                    SidebarTab::Tasks => "F2",
                    SidebarTab::Skills => "F3",
                    SidebarTab::Provider => "F4",
                    SidebarTab::Files => "F7",
                    SidebarTab::Help => "F1",
                }
            };
            if is_sel {
                Line::from(Span::styled(
                    format!("[{}]", title),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(
                    format!(" {} ", title),
                    Style::default().fg(theme.text_dim),
                ))
            }
        })
        .collect();

    let tab_index = match app.active_tab {
        SidebarTab::Tasks => 0,
        SidebarTab::Skills => 1,
        SidebarTab::Provider => 2,
        SidebarTab::Files => 3,
        SidebarTab::Help => 4,
    };

    let tabs_widget = Tabs::new(tab_titles)
        .select(tab_index)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        )
        .highlight_style(
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs_widget, sidebar_layout[0]);

    // Explicitly wipe tab content area before rendering child tab view
    frame.render_widget(Clear, sidebar_layout[1]);

    match app.active_tab {
        SidebarTab::Tasks => render_tasks_tab(frame, app, sidebar_layout[1], theme),
        SidebarTab::Skills => render_skills_tab(frame, app, sidebar_layout[1], theme),
        SidebarTab::Provider => render_provider_tab(frame, app, sidebar_layout[1], theme),
        SidebarTab::Files => render_files_tab(frame, app, sidebar_layout[1], theme),
        SidebarTab::Help => render_help_tab(frame, app, sidebar_layout[1], theme),
    }
}

fn render_tasks_tab(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);

    let tasks_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55), // Task list table
            Constraint::Percentage(45), // Task logs preview
        ])
        .split(area);

    frame.render_widget(Clear, tasks_layout[0]);
    frame.render_widget(Clear, tasks_layout[1]);

    let tm = TaskManager::global();
    let task_snapshots = tm.list_tasks();

    let rows: Vec<Row> = task_snapshots
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let is_selected = idx == app.selected_task_index;
            let (status_text, status_style) = match t.status {
                TaskStatus::Running => (
                    "● RUNNING",
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                TaskStatus::Completed => ("✔ DONE", Style::default().fg(theme.success)),
                TaskStatus::Failed => ("✖ GAGAL", Style::default().fg(theme.error)),
                TaskStatus::Cancelled => ("◼ BATAL", Style::default().fg(theme.secondary)),
                TaskStatus::Queued => ("◌ ANTRI", Style::default().fg(theme.warning)),
            };

            let row_style = if is_selected {
                Style::default()
                    .bg(theme.highlight_bg)
                    .fg(theme.text_bright)
            } else {
                Style::default()
            };

            let sel_indicator = if is_selected { "▶ " } else { "  " };

            Row::new(vec![
                ratatui::widgets::Cell::from(format!("{}{}", sel_indicator, t.id)),
                ratatui::widgets::Cell::from(t.name.clone()),
                ratatui::widgets::Cell::from(status_text).style(status_style),
                ratatui::widgets::Cell::from(t.elapsed_human.clone()),
            ])
            .style(row_style)
            .bottom_margin(0)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Min(12),
            Constraint::Length(11),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new(vec!["ID", "NAMA TUGAS", "STATUS", "DURASI"])
            .style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .title(" 📋 Background Tasks ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_normal)),
    );

    frame.render_widget(table, tasks_layout[0]);

    // Logs preview of selected task
    let mut log_lines = Vec::new();
    if let Some(selected_task) = task_snapshots.get(app.selected_task_index) {
        if let Some(logs) = tm.get_task_logs(&selected_task.id) {
            let recent_logs = if logs.len() > 15 {
                &logs[logs.len() - 15..]
            } else {
                &logs[..]
            };
            for (line_idx, l) in recent_logs.iter().enumerate() {
                log_lines.push(Line::from(vec![
                    Span::styled(format!("{:2} │ ", line_idx + 1), Style::default().fg(theme.border_subtle)),
                    Span::styled(l.clone(), Style::default().fg(theme.text_main)),
                ]));
            }
        }
        if log_lines.is_empty() {
            log_lines.push(Line::from(Span::styled(
                "  Belum ada log output untuk task ini.",
                Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC),
            )));
        }
    } else {
        log_lines.push(Line::from(Span::styled(
            "  Belum ada background task aktif.",
            Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC),
        )));
    }

    let logs_widget = Paragraph::new(log_lines)
        .block(
            Block::default()
                .title(" 📜 Task Logs [c: batal | x: bersihkan] ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border_subtle)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(logs_widget, tasks_layout[1]);
}

fn render_skills_tab(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);
    let skills = get_available_skills();
    let items: Vec<ListItem> = skills
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let is_sel = idx == app.selected_skill_index;
            let is_active = app
                .active_skill
                .as_ref()
                .map(|sk| sk.id == s.id)
                .unwrap_or(false);

            let icon = match s.id.to_lowercase().as_str() {
                id if id.contains("rust") => "🦀",
                id if id.contains("review") => "🔍",
                id if id.contains("test") => "🧪",
                id if id.contains("web") || id.contains("crawl") => "🌐",
                id if id.contains("git") => "📦",
                id if id.contains("doc") => "📚",
                _ => "🎯",
            };

            let prefix = if is_active {
                "★ [ACTIVE] "
            } else if is_sel {
                "▶ "
            } else {
                "  "
            };

            let title_style = if is_active {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else if is_sel {
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_bright)
            };

            let content = vec![
                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(if is_active { theme.success } else { theme.accent })),
                    Span::styled(format!("{} ", icon), Style::default()),
                    Span::styled(s.name.as_str_slice(), title_style),
                    Span::styled(format!(" ({})", s.id), Style::default().fg(theme.text_dim)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(s.description.as_str_slice(), Style::default().fg(theme.text_dim)),
                ]),
            ];

            if is_sel {
                ListItem::new(content).style(Style::default().bg(theme.highlight_bg))
            } else {
                ListItem::new(content)
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" 🎯 Specialized AI Skills (Enter untuk memilih) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_normal)),
    );

    frame.render_widget(list, area);
}

fn render_provider_tab(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);
    let active_id = &app.providers_reg.active_provider_id;
    let items: Vec<ListItem> = app
        .providers_reg
        .providers
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let is_sel = idx == app.selected_provider_index;
            let is_active = &p.id == active_id;

            let prefix = if is_active {
                "★ [ACTIVE] "
            } else if is_sel {
                "▶ "
            } else {
                "  "
            };

            let title_style = if is_active {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else if is_sel {
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_bright)
            };

            let ctx_str = p
                .context_window
                .map(|c| format!("{} tokens", c))
                .unwrap_or_else(|| "128k".to_string());

            let content = vec![
                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(if is_active { theme.success } else { theme.accent })),
                    Span::styled("⚡ ", Style::default()),
                    Span::styled(&p.name, title_style),
                    Span::styled(
                        format!(" [{}]", p.protocol),
                        Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("    Model: "),
                    Span::styled(&p.default_model, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                    Span::styled(" │ Ctx: ", Style::default().fg(theme.border_subtle)),
                    Span::styled(ctx_str, Style::default().fg(theme.text_dim)),
                ]),
                Line::from(vec![
                    Span::raw("    Endpoint: "),
                    Span::styled(&p.base_url, Style::default().fg(theme.text_dim)),
                ]),
            ];

            if is_sel {
                ListItem::new(content).style(Style::default().bg(theme.highlight_bg))
            } else {
                ListItem::new(content)
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" ⚡ AI Providers & Endpoints (Enter untuk beralih) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_normal)),
    );

    frame.render_widget(list, area);
}

fn render_files_tab(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // File list table
            Constraint::Percentage(50), // File preview / git diff
        ])
        .split(area);

    frame.render_widget(Clear, chunks[0]);
    frame.render_widget(Clear, chunks[1]);

    // 1. Files List Table
    let rows: Vec<Row> = app
        .files_list
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let is_sel = i == app.selected_file_index;
            let sel_indicator = if is_sel { "▶ " } else { "  " };
            let icon = if f.is_dir { "📁 " } else { "📄 " };
            let name_str = format!("{}{}{}", sel_indicator, icon, f.relative_path);

            let (git_str, git_style) = match f.git_status {
                Some(GitFileBadge::Modified) => ("MODIFIED", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
                Some(GitFileBadge::Staged) => ("STAGED", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
                Some(GitFileBadge::Untracked) => ("NEW", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                Some(GitFileBadge::Deleted) => ("DELETED", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
                None => ("-", Style::default().fg(theme.text_dim)),
            };

            let size_str = if f.is_dir {
                "-".to_string()
            } else if f.size_bytes < 1024 {
                format!("{} B", f.size_bytes)
            } else if f.size_bytes < 1024 * 1024 {
                format!("{:.1} KB", f.size_bytes as f64 / 1024.0)
            } else {
                format!("{:.1} MB", f.size_bytes as f64 / (1024.0 * 1024.0))
            };

            let row_style = if is_sel {
                Style::default().bg(theme.highlight_bg).fg(theme.text_bright)
            } else {
                Style::default().fg(theme.text_main)
            };

            Row::new(vec![
                ratatui::widgets::Cell::from(name_str),
                ratatui::widgets::Cell::from(git_str).style(git_style),
                ratatui::widgets::Cell::from(size_str).style(Style::default().fg(theme.text_dim)),
            ])
            .style(row_style)
        })
        .collect();

    let is_focused = app.focused_pane == FocusedPane::Sidebar;
    let border_color = if is_focused {
        theme.border_focused
    } else {
        theme.border_normal
    };

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(55),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
        ],
    )
    .header(
        Row::new(vec!["NAMA BERKAS", "STATUS GIT", "UKURAN"])
            .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .title(" 📂 Workspace & Git Files [r: Refresh] ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(table, chunks[0]);

    // 2. File Preview / Diff Pane
    let selected_name = app
        .files_list
        .get(app.selected_file_index)
        .map(|f| f.relative_path.as_str())
        .unwrap_or("Pilih Berkas");

    let preview_title = format!(" 🔍 Preview: {} [PgUp/PgDn: Gulir] ", selected_name);

    let preview_lines: Vec<Line> = if let Some(ref content) = app.file_preview_content {
        let is_diff = content.starts_with("--- Git Diff");
        content
            .lines()
            .skip(app.file_preview_scroll as usize)
            .take(chunks[1].height.saturating_sub(2) as usize)
            .enumerate()
            .map(|(idx, l)| {
                if is_diff {
                    let style = if l.starts_with('+') && !l.starts_with("+++") {
                        Style::default().fg(theme.success)
                    } else if l.starts_with('-') && !l.starts_with("---") {
                        Style::default().fg(theme.error)
                    } else if l.starts_with("@@") {
                        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_dim)
                    };
                    Line::from(Span::styled(l, style))
                } else {
                    let line_no = (app.file_preview_scroll as usize) + idx + 1;
                    Line::from(vec![
                        Span::styled(format!("{:3} │ ", line_no), Style::default().fg(theme.text_dim)),
                        Span::styled(l, Style::default().fg(theme.text_main)),
                    ])
                }
            })
            .collect()
    } else {
        vec![Line::from(Span::styled(
            "  (Tidak ada pratinjau yang tersedia)",
            Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC),
        ))]
    };

    let preview_para = Paragraph::new(preview_lines).block(
        Block::default()
            .title(preview_title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_normal)),
    );

    frame.render_widget(preview_para, chunks[1]);
}

fn render_help_tab(frame: &mut Frame, _app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);
    let keycap = |k: &'static str| Span::styled(k, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD));
    let slash = |s: &'static str| Span::styled(s, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD));
    let desc = |d: &'static str| Span::styled(d, Style::default().fg(theme.text_main));

    let lines = vec![
        Line::from(Span::styled(
            "⌨ Navigasi & Pintasan Keyboard:",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::raw("  "),
            keycap("[Tab]"),
            desc("            : Pindah fokus antar panel (Input ⇄ Chat ⇄ Sidebar)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[Enter]"),
            desc("          : Kirim prompt / Pilih opsi terpilih"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[Esc]"),
            desc("            : Batalkan eksekusi prompt yang sedang berjalan"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F1]"),
            desc("             : Tab Bantuan (Help)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F2]"),
            desc("             : Tab Background Tasks (Lihat status & log subagent)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F3]"),
            desc("             : Tab Skills (Pilih peran AI spesialis)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F4]"),
            desc("             : Tab Provider (Pilih endpoint & model AI)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F5] / :cli"),
            desc("      : Kembali ke mode CLI / REPL (atau Ctrl+G)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F6] / /theme"),
            desc("    : Ganti tema warna (Tokyo Night, Catppuccin, Gruvbox, Matrix, Monokai)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F7] / /files"),
            desc("    : Tab Workspace & Git Explorer (Pohon berkas & git diff)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F8] / [Ctrl+P]"),
            desc("  : Buka Command Palette (pencarian aksi modal cepat)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[F9] / [Ctrl+B]"),
            desc("  : Toggle Zen Mode (sembunyikan / tampilkan sidebar)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[t]"),
            desc("              : Ganti mode tampilan tool (Compact / Preview / Full)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[z] / [Space]"),
            desc("    : Lipat / buka reasoning (saat fokus di panel Chat)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[PgUp] / [PgDn]"),
            desc("  : Gulir riwayat pesan percakapan"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[↑] / [↓]"),
            desc("        : Navigasi riwayat input & pilihan daftar"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            keycap("[Ctrl+C]"),
            desc("         : Keluar dari ctrl-cli TUI"),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "⚡ Perintah Slash Populer:",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::raw("  "),
            slash("/theme [nama]"),
            desc("   : Pilih tema spesifik atau rotasi"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/zen"),
            desc("            : Masuk / keluar Zen Mode layar penuh"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/help"),
            desc("            : Bantuan lengkap"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/clear, /reset"),
            desc("   : Bersihkan riwayat percakapan sesi"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/tasks [clear]"),
            desc("   : Kelola atau bersihkan task subagent"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/skill <id>"),
            desc("      : Ganti peran ke rust-expert, code-reviewer, dll."),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/model <name>"),
            desc("    : Ganti model target (GLM, Claude, GPT, dll.)"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/compact"),
            desc("         : Kompaksi konteks percakapan hemat token"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/cli, /repl"),
            desc("      : Kembali ke mode CLI / REPL"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            slash("/exit, /quit"),
            desc("     : Keluar dari aplikasi"),
        ]),
    ];

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" ❓ Bantuan & Panduan Cepat ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border_normal)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(para, area);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);
    let is_focused = app.focused_pane == FocusedPane::Input;
    let border_color = if is_focused {
        theme.border_focused
    } else {
        theme.border_normal
    };

    let title = if app.agent_running {
        Line::from(vec![
            Span::styled(
                format!(" ⚙ [Eksekusi Berjalan {} - Tekan Esc untuk Batal] ", get_spinner_char()),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if is_focused {
        Line::from(vec![
            Span::styled(
                " ⌨ PROMPT ",
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[Enter: Kirim • Esc: Batal • Tab: Pindah] ",
                Style::default().fg(theme.text_dim),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " ⌨ Prompt ",
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(
                "(Tekan Tab atau i untuk fokus) ",
                Style::default().fg(theme.border_subtle),
            ),
        ])
    };

    let prompt_prefix = "❯ ";
    let content_lines = if app.input.is_empty() && !app.agent_running {
        vec![Line::from(vec![
            Span::styled(prompt_prefix, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(
                "Ketik instruksi Anda atau '/' untuk perintah cepat...",
                Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC),
            ),
        ])]
    } else {
        vec![Line::from(vec![
            Span::styled(prompt_prefix, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(&app.input, Style::default().fg(theme.text_bright)),
        ])]
    };

    let char_count = app.input.chars().count();
    let right_title = if char_count > 0 {
        format!(" {} char ", char_count)
    } else {
        String::new()
    };

    let input_para = Paragraph::new(content_lines).block(
        Block::default()
            .title(title)
            .title_bottom(Line::from(Span::styled(right_title, Style::default().fg(theme.text_dim))).alignment(Alignment::Right))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(input_para, area);

    // Render blinking/visible cursor in input box
    if is_focused {
        let cursor_x = area.x + 1 + (prompt_prefix.len() + app.cursor_position) as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

/// Formats a `ProcessMetrics` snapshot into a compact footer status string.
///
/// Output format: `RAM: X.X M (Pk Y.Y M) │ CPU: Z.Z% │ Th: N`
/// When metrics are unavailable (`None`), returns fallback: `RAM: -- │ CPU: -- │ Th: --`.
pub fn format_footer_telemetry(metrics: Option<&crate::telemetry::ProcessMetrics>) -> String {
    match metrics {
        Some(m) => {
            let rss_mb = m.memory.rss_bytes as f64 / (1024.0 * 1024.0);
            let peak_mb = m.memory.peak_rss_bytes as f64 / (1024.0 * 1024.0);
            format!(
                "RAM: {:.1} M (Pk {:.1} M) │ CPU: {:.1}% │ Th: {}",
                rss_mb, peak_mb, m.cpu.process_pct, m.threads.active_threads
            )
        }
        None => "RAM: -- │ CPU: -- │ Th: --".to_string(),
    }
}

/// Renders styled Ratatui Spans for the footer telemetry indicator.
pub fn render_footer_telemetry_line(
    metrics: Option<&crate::telemetry::ProcessMetrics>,
) -> Line<'static> {
    match metrics {
        Some(m) => {
            let rss_mb = m.memory.rss_bytes as f64 / (1024.0 * 1024.0);
            let peak_mb = m.memory.peak_rss_bytes as f64 / (1024.0 * 1024.0);
            let ram_alert = rss_mb > 50.0;
            let cpu_alert = m.cpu.process_pct > 80.0;

            let ram_style = if ram_alert {
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::LightCyan)
            };

            let cpu_style = if cpu_alert {
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::LightCyan)
            };

            let dim_style = Style::default().fg(Color::DarkGray);
            let val_style = Style::default().fg(Color::LightCyan);

            Line::from(vec![
                Span::styled("RAM: ", dim_style),
                Span::styled(format!("{:.1} M", rss_mb), ram_style),
                Span::styled(" (Pk ", dim_style),
                Span::styled(format!("{:.1} M", peak_mb), ram_style),
                Span::styled(") │ CPU: ", dim_style),
                Span::styled(format!("{:.1}%", m.cpu.process_pct), cpu_style),
                Span::styled(" │ Th: ", dim_style),
                Span::styled(format!("{}", m.threads.active_threads), val_style),
            ])
        }
        None => Line::from(vec![Span::styled(
            "RAM: -- │ CPU: -- │ Th: --",
            Style::default().fg(Color::DarkGray),
        )]),
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);

    let (focus_name, hints) = get_contextual_hints(app);

    let status_line = if let Some((msg, _)) = &app.status_message {
        Line::from(vec![
            Span::styled(
                " 🔔 ",
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                msg,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                format!(" {} ", focus_name),
                Style::default()
                    .bg(theme.card_bg)
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" │ [Tab] Fokus  {}", hints),
                Style::default().fg(theme.text_dim),
            ),
        ])
    };

    let limit = app.providers_reg.get_active_provider().context_window.unwrap_or(128_000);
    let pct = if limit > 0 {
        ((app.total_tokens as f64 / limit as f64) * 100.0).min(100.0) as usize
    } else {
        0
    };
    let gauge_blocks = 8;
    let filled = (pct * gauge_blocks) / 100;
    let empty = gauge_blocks.saturating_sub(filled);
    let bar_str = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
    let gauge_color = if pct > 85 {
        theme.error
    } else if pct > 60 {
        theme.warning
    } else {
        theme.success
    };

    let token_line = if area.width >= 115 {
        Line::from(vec![
            Span::styled("Ctx ", Style::default().fg(theme.text_dim)),
            Span::styled(bar_str, Style::default().fg(gauge_color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}% │ ", pct), Style::default().fg(gauge_color)),
            Span::styled(
                format!("Tok: {} in / {} out", app.total_prompt_tokens, app.total_completion_tokens),
                Style::default().fg(theme.text_dim),
            ),
        ])
    } else if area.width >= 80 {
        Line::from(vec![
            Span::styled("Ctx ", Style::default().fg(theme.text_dim)),
            Span::styled(format!("{}% ", pct), Style::default().fg(gauge_color).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} tok)", app.total_tokens), Style::default().fg(theme.text_dim)),
        ])
    } else {
        Line::from(vec![])
    };

    // Leave the very last column (width - 1) on the bottom row completely untouched.
    // In Windows Console Host (conhost.exe), writing a character to (width-1, height-1) triggers
    // an automatic line wrap/scroll event that shifts the entire screen buffer up by 1 line!
    let safe_footer_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(1),
        height: area.height,
    };

    let layout = if safe_footer_area.width >= 114 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(25),
                Constraint::Length(38),
                Constraint::Length(45),
            ])
            .split(safe_footer_area)
    } else if safe_footer_area.width >= 79 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(20),
                Constraint::Length(38),
                Constraint::Length(22),
            ])
            .split(safe_footer_area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(0),
                Constraint::Length(0),
            ])
            .split(safe_footer_area)
    };

    let status_para = Paragraph::new(status_line);
    let telemetry_para = Paragraph::new(render_footer_telemetry_line(app.metrics.as_ref()))
        .alignment(Alignment::Center);
    let token_para = Paragraph::new(token_line).alignment(Alignment::Right);

    frame.render_widget(status_para, layout[0]);
    if safe_footer_area.width >= 79 {
        frame.render_widget(telemetry_para, layout[1]);
        frame.render_widget(token_para, layout[2]);
    }
}

fn render_slash_popup(frame: &mut Frame, app: &App, input_area: Rect, theme: &Theme) {
    let popup_height = (app.slash_suggestions.len() as u16 + 2).min(8);
    let popup_y = input_area.y.saturating_sub(popup_height);
    let popup_area = Rect {
        x: input_area.x + 2,
        y: popup_y,
        width: input_area.width.saturating_sub(4).min(70),
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .slash_suggestions
        .iter()
        .enumerate()
        .map(|(idx, (cmd, desc))| {
            let is_sel = idx == app.selected_slash_index;
            let (prefix, style) = if is_sel {
                (
                    "▶ ",
                    Style::default()
                        .bg(theme.primary)
                        .fg(Color::Rgb(24, 28, 36))
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    "  ",
                    Style::default().fg(theme.text_bright),
                )
            };

            let line = Line::from(vec![
                Span::styled(prefix, if is_sel { style } else { Style::default().fg(theme.primary) }),
                Span::styled(format!("{:<16}", cmd), style),
                Span::styled(
                    format!(" - {}", desc),
                    if is_sel {
                        style
                    } else {
                        Style::default().fg(theme.text_dim)
                    },
                ),
            ]);
            if is_sel {
                ListItem::new(line).style(style)
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" 💡 Saran Perintah (Tab / Enter untuk memilih) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_focused)),
    );

    frame.render_widget(list, popup_area);
}

fn render_permission_modal(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let Some(req) = &app.active_permission_request else {
        return;
    };

    let modal_width = 72.min(area.width.saturating_sub(4));
    let modal_height = 10.min(area.height.saturating_sub(4));
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect {
        x: modal_x,
        y: modal_y,
        width: modal_width,
        height: modal_height,
    };

    frame.render_widget(Clear, modal_area);

    let content = vec![
        Line::from(vec![Span::styled(
            " ⚠️  Permintaan Izin Eksekusi Mutasi Sistem:",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("   • Tool Target : ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("[ {} ]", req.tool_name),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("   • Argumen     : ", Style::default().fg(theme.text_dim)),
            Span::styled(&req.arguments_json, Style::default().fg(theme.accent)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw("   "),
            Span::styled(
                " [Y] Izinkan Sekali ",
                Style::default()
                    .bg(theme.card_bg)
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(
                " [A] Selalu Izinkan (Sesi) ",
                Style::default()
                    .bg(theme.card_bg)
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(
                " [N / Esc] Tolak ",
                Style::default()
                    .bg(theme.card_bg)
                    .fg(theme.error)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let modal_widget = Paragraph::new(content)
        .block(
            Block::default()
                .title(" 🛡️ Konfirmasi Keamanan (Permission Gate) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(theme.warning)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(modal_widget, modal_area);
}

fn render_command_palette(frame: &mut Frame, area: Rect, theme: &Theme) {
    let modal_width = 68.min(area.width.saturating_sub(4));
    let modal_height = 18.min(area.height.saturating_sub(2));
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect {
        x: modal_x,
        y: modal_y,
        width: modal_width,
        height: modal_height,
    };

    frame.render_widget(Clear, modal_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(4),    // Command list
        ])
        .split(modal_area);

    let query = get_palette_query();
    let search_bar = Paragraph::new(Line::from(vec![
        Span::styled(" 🔍 ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        if query.is_empty() {
            Span::styled(
                "Ketik untuk mencari aksi (Esc tutup, ↑/↓ navigasi, Enter pilih)...",
                Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC),
            )
        } else {
            Span::styled(
                &query,
                Style::default()
                    .fg(theme.text_bright)
                    .add_modifier(Modifier::BOLD),
            )
        },
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_focused))
            .title(Span::styled(
                " ⚡ COMMAND PALETTE (Ctrl+P / F8) ",
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
    );

    frame.render_widget(search_bar, chunks[0]);

    let filtered = get_filtered_palette_commands();
    let sel_idx = SELECTED_PALETTE_INDEX.load(Ordering::Relaxed);

    let items: Vec<ListItem> = if filtered.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::styled(
                "   Tidak ada perintah yang cocok.",
                Style::default()
                    .fg(theme.text_dim)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]))]
    } else {
        filtered
            .iter()
            .enumerate()
            .map(|(idx, cmd)| {
                let is_sel = idx == sel_idx;
                let (prefix, bg, fg, key_style) = if is_sel {
                    (
                        " ▶ ",
                        theme.highlight_bg,
                        theme.text_bright,
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    (
                        "   ",
                        Color::Reset,
                        theme.text_main,
                        Style::default().fg(theme.text_dim),
                    )
                };

                let line = Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(theme.primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("{} ", cmd.icon), Style::default()),
                    Span::styled(
                        format!("{:<38}", cmd.title),
                        Style::default().fg(fg).add_modifier(if is_sel {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                    ),
                    Span::styled(format!("[{:>6}]", cmd.shortcut), key_style),
                ]);

                ListItem::new(line).style(Style::default().bg(bg))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_normal)),
    );

    frame.render_widget(list, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_presets_and_cycling() {
        assert_eq!(get_theme_count(), 5);

        // Cycle through themes
        let initial_theme = get_current_theme_name();
        let cycled = cycle_theme();
        assert_ne!(initial_theme, cycled);

        // Explicit set by name
        assert_eq!(set_theme_by_name("cyberpunk"), Some("Cyberpunk Matrix"));
        assert_eq!(get_current_theme_name(), "Cyberpunk Matrix");

        assert_eq!(set_theme_by_name("gruvbox"), Some("Gruvbox Dark"));
        assert_eq!(get_current_theme_name(), "Gruvbox Dark");

        assert_eq!(set_theme_by_name("catppuccin"), Some("Catppuccin Mocha"));
        assert_eq!(get_current_theme_name(), "Catppuccin Mocha");

        assert_eq!(set_theme_by_name("monokai"), Some("Monokai Pro"));
        assert_eq!(get_current_theme_name(), "Monokai Pro");

        assert_eq!(set_theme_by_name("tokyo"), Some("Tokyo Night"));
        assert_eq!(get_current_theme_name(), "Tokyo Night");

        assert_eq!(set_theme_by_name("non_existent_theme"), None);
    }

    #[test]
    fn test_zen_mode_toggle() {
        let initial = is_sidebar_collapsed();
        let toggled = toggle_sidebar();
        assert_eq!(toggled, !initial);
        assert_eq!(is_sidebar_collapsed(), toggled);
        // Toggle back
        let reverted = toggle_sidebar();
        assert_eq!(reverted, initial);
    }

    #[test]
    fn test_thought_folding_toggle() {
        let initial = is_thought_process_folded();
        let toggled = toggle_thought_folding();
        assert_eq!(toggled, !initial);
        assert_eq!(is_thought_process_folded(), toggled);
        // Toggle back
        let reverted = toggle_thought_folding();
        assert_eq!(reverted, initial);
    }

    #[test]
    fn test_command_palette_filtering_and_actions() {
        assert!(!is_command_palette_open());
        toggle_command_palette();
        assert!(is_command_palette_open());

        let all_cmds = get_filtered_palette_commands();
        assert_eq!(all_cmds.len(), PALETTE_COMMANDS.len());

        // Test search query
        palette_append_char('z');
        palette_append_char('e');
        palette_append_char('n');
        let filtered = get_filtered_palette_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].action_id, "zen");

        // Test navigation
        palette_move_down();
        palette_move_up();

        // Test backspace
        palette_backspace();
        palette_backspace();
        palette_backspace();
        assert_eq!(get_palette_query(), "");

        close_command_palette();
        assert!(!is_command_palette_open());
    }

    #[test]
    fn test_tool_accordion_mode_cycling() {
        set_tool_accordion_mode(ToolAccordionMode::Compact);
        assert_eq!(get_tool_accordion_mode(), ToolAccordionMode::Compact);

        let next = cycle_tool_accordion_mode();
        assert_eq!(get_tool_accordion_mode(), ToolAccordionMode::Preview);
        assert!(next.contains("Preview"));

        let next2 = cycle_tool_accordion_mode();
        assert_eq!(get_tool_accordion_mode(), ToolAccordionMode::Full);
        assert!(next2.contains("Full"));

        let next3 = cycle_tool_accordion_mode();
        assert_eq!(get_tool_accordion_mode(), ToolAccordionMode::Compact);
        assert!(next3.contains("Compact"));
    }
}


