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
use crate::tui::app::{App, ChatItemKind, FocusedPane, SidebarTab};
use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};

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
            SidebarTab::Help => ("❓ HELP", "[F1-F4] Ganti Tab  [Tab] Kembali ke Input  [F5] Mode REPL"),
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

/// Unified aesthetic dark color theme inspired by Tokyo Night / Nord Slate.
pub struct Theme {
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

impl Default for Theme {
    fn default() -> Self {
        Self {
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
}

pub fn render(frame: &mut Frame, app: &mut App) {
    app.tick();
    let theme = Theme::default();
    let size = frame.area();

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
}

fn render_header(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
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

    let title_line = Line::from(
        vec![
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
        ]
        .into_iter()
        .chain(status_spans)
        .collect::<Vec<_>>(),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_normal));

    let header_para = Paragraph::new(title_line).block(block);
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

fn render_chat(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
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
                        } else if text_line.starts_with("> ") {
                            lines.push(Line::from(vec![
                                Span::styled("│  ▎ ", Style::default().fg(theme.secondary)),
                                Span::styled(&text_line[2..], Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)),
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
                lines.push(Line::from(vec![
                    Span::styled(format!("╭─ {} ", sym), Style::default().fg(col).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("( {} )", name), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                    Span::styled(" ──────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));

                let all_lines: Vec<&str> = result.lines().collect();
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
                            format!("... (+{} baris disembunyikan)", all_lines.len() - limit),
                            Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }
                lines.push(Line::from(vec![
                    Span::styled("╰───────────────────────────────────────────────────", Style::default().fg(theme.border_subtle)),
                ]));
                lines.push(Line::raw(""));
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

    let tab_titles: Vec<Line> = SidebarTab::all()
        .iter()
        .map(|t| {
            let is_sel = *t == app.active_tab;
            let title = match t {
                SidebarTab::Tasks => "📋 Tasks (F2)",
                SidebarTab::Skills => "🎯 Skills (F3)",
                SidebarTab::Provider => "⚡ Provider (F4)",
                SidebarTab::Help => "❓ Help (F1)",
            };
            if is_sel {
                Line::from(Span::styled(
                    format!(" [ {} ] ", title),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(
                    format!("   {}   ", title),
                    Style::default().fg(theme.text_dim),
                ))
            }
        })
        .collect();

    let tab_index = match app.active_tab {
        SidebarTab::Tasks => 0,
        SidebarTab::Skills => 1,
        SidebarTab::Provider => 2,
        SidebarTab::Help => 3,
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

    match app.active_tab {
        SidebarTab::Tasks => render_tasks_tab(frame, app, sidebar_layout[1], theme),
        SidebarTab::Skills => render_skills_tab(frame, app, sidebar_layout[1], theme),
        SidebarTab::Provider => render_provider_tab(frame, app, sidebar_layout[1], theme),
        SidebarTab::Help => render_help_tab(frame, app, sidebar_layout[1], theme),
    }
}

fn render_tasks_tab(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let tasks_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55), // Task list table
            Constraint::Percentage(45), // Task logs preview
        ])
        .split(area);

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

fn render_help_tab(frame: &mut Frame, _app: &App, area: Rect, theme: &Theme) {
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

    let tokens_text = Span::styled(
        format!(
            "Tokens: {} in / {} out (Total: {}) ",
            app.total_prompt_tokens, app.total_completion_tokens, app.total_tokens
        ),
        Style::default().fg(theme.text_dim),
    );

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(25),
            Constraint::Length(38),
            Constraint::Length(35),
        ])
        .split(area);

    let status_para = Paragraph::new(status_line);
    let telemetry_para = Paragraph::new(render_footer_telemetry_line(app.metrics.as_ref()))
        .alignment(Alignment::Center);
    let token_para = Paragraph::new(Line::from(tokens_text)).alignment(Alignment::Right);

    frame.render_widget(status_para, layout[0]);
    frame.render_widget(telemetry_para, layout[1]);
    frame.render_widget(token_para, layout[2]);
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
