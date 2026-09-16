use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap,
    },
    Frame,
};

use crate::agent::tasks::{TaskManager, TaskStatus};
use crate::get_available_skills;
use crate::tui::app::{App, ChatItemKind, FocusedPane, SidebarTab};

pub fn render(frame: &mut Frame, app: &mut App) {
    app.tick();
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

    render_header(frame, app, main_chunks[0]);
    render_body(frame, app, main_chunks[1]);
    render_input(frame, app, main_chunks[2]);
    render_footer(frame, app, main_chunks[3]);

    // Render Autocomplete Popup if active
    if app.show_slash_popup && !app.slash_suggestions.is_empty() {
        render_slash_popup(frame, app, main_chunks[2]);
    }

    // Render Permission Modal Dialog if active
    if app.active_permission_request.is_some() {
        render_permission_modal(frame, app, size);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let active_prov = app.providers_reg.get_active_provider();
    let skill_name = app
        .active_skill
        .as_ref()
        .map(|s| s.name)
        .unwrap_or("General Agent");

    let status_span = if app.agent_running {
        if app.active_tool_call.is_some() {
            Span::styled(
                " [RUNNING TOOL ⚙] ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                " [THINKING 💭] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        }
    } else {
        Span::styled(
            " [IDLE] ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    };

    let title_line = Line::from(vec![
        Span::styled(
            " ◆ ctrl-cli ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION"), " "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("│ Provider: "),
        Span::styled(
            &active_prov.name,
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ Model: "),
        Span::styled(
            &app.current_model,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ Skill: "),
        Span::styled(skill_name, Style::default().fg(Color::Magenta)),
        Span::raw(" │ Status: "),
        status_span,
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let header_para = Paragraph::new(title_line).block(block);
    frame.render_widget(header_para, area);
}

fn render_body(frame: &mut Frame, app: &mut App, area: Rect) {
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62), // Chat
            Constraint::Percentage(38), // Sidebar
        ])
        .split(area);

    render_chat(frame, app, body_chunks[0]);
    render_sidebar(frame, app, body_chunks[1]);
}

fn render_chat(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::Chat;
    let border_color = if is_focused {
        Color::LightCyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(Span::styled(
            " 💬 Chat & Execution Stream ",
            Style::default().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let mut lines = Vec::new();

    for item in &app.chat_items {
        match &item.kind {
            ChatItemKind::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "👤 You ",
                        Style::default()
                            .fg(Color::LightBlue)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("({})", item.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
                for text_line in item.text.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(text_line, Style::default().fg(Color::White)),
                    ]));
                }
                lines.push(Line::raw(""));
            }
            ChatItemKind::Assistant => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "🤖 Assistant ",
                        Style::default()
                            .fg(Color::LightGreen)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("({})", item.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
                for text_line in item.text.lines() {
                    let style = if text_line.starts_with("```") {
                        Style::default().fg(Color::Yellow)
                    } else if text_line.starts_with("#") {
                        Style::default()
                            .fg(Color::LightCyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Reset)
                    };
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(text_line, style),
                    ]));
                }
                lines.push(Line::raw(""));
            }
            ChatItemKind::Reasoning => {
                lines.push(Line::from(vec![Span::styled(
                    "💭 Thought Process",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )]));
                for r_line in item.text.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(r_line, Style::default().fg(Color::DarkGray)),
                    ]));
                }
                lines.push(Line::raw(""));
            }
            ChatItemKind::ToolCall { name, args } => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  ⚡ Tool Call: ",
                        Style::default()
                            .fg(Color::LightMagenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(name, Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::styled(args, Style::default().fg(Color::DarkGray)),
                ]));
            }
            ChatItemKind::ToolResult {
                name,
                result,
                success,
            } => {
                let (sym, col) = if *success {
                    ("✔ Result", Color::LightGreen)
                } else {
                    ("✖ Error", Color::LightRed)
                };
                let first_line = result.lines().next().unwrap_or("Done");
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} ({}): ", sym, name),
                        Style::default().fg(col).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(first_line, Style::default().fg(Color::White)),
                ]));
                lines.push(Line::raw(""));
            }
            ChatItemKind::SystemInfo => {
                lines.push(Line::from(vec![
                    Span::styled("ℹ ", Style::default().fg(Color::Cyan)),
                    Span::styled(&item.text, Style::default().fg(Color::Cyan)),
                ]));
                lines.push(Line::raw(""));
            }
            ChatItemKind::Error => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "❌ ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&item.text, Style::default().fg(Color::LightRed)),
                ]));
                lines.push(Line::raw(""));
            }
        }
    }

    // In-flight streaming display
    if app.agent_running {
        if !app.streaming_reasoning.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "💭 Thinking...",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )]));
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
                    Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(r_line, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }

        if !app.streaming_content.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "🤖 Assistant (Streaming...) ",
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            )]));
            for s_line in app.streaming_content.lines() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(s_line, Style::default().fg(Color::White)),
                ]));
            }
        }

        if let Some((name, args)) = &app.active_tool_call {
            lines.push(Line::from(vec![
                Span::styled(
                    "  ⚡ Executing: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(name, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(args, Style::default().fg(Color::DarkGray)),
            ]));
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
}

fn render_sidebar(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::Sidebar;
    let border_color = if is_focused {
        Color::LightCyan
    } else {
        Color::DarkGray
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
            let style = if is_sel {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(t.title(), style))
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
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs_widget, sidebar_layout[0]);

    match app.active_tab {
        SidebarTab::Tasks => render_tasks_tab(frame, app, sidebar_layout[1]),
        SidebarTab::Skills => render_skills_tab(frame, app, sidebar_layout[1], border_color),
        SidebarTab::Provider => render_provider_tab(frame, app, sidebar_layout[1], border_color),
        SidebarTab::Help => render_help_tab(frame, app, sidebar_layout[1], border_color),
    }
}

fn render_tasks_tab(frame: &mut Frame, app: &mut App, area: Rect) {
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
            let status_style = match t.status {
                TaskStatus::Running => Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
                TaskStatus::Completed => Style::default().fg(Color::Green),
                TaskStatus::Failed => Style::default().fg(Color::Red),
                TaskStatus::Cancelled => Style::default().fg(Color::Magenta),
                TaskStatus::Queued => Style::default().fg(Color::Yellow),
            };

            let row_style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            Row::new(vec![
                ratatui::widgets::Cell::from(t.id.clone()),
                ratatui::widgets::Cell::from(t.name.clone()),
                ratatui::widgets::Cell::from(t.status.as_str().to_uppercase()).style(status_style),
                ratatui::widgets::Cell::from(t.elapsed_human.clone()),
            ])
            .style(row_style)
            .bottom_margin(0)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(12),
            Constraint::Length(11),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new(vec!["ID", "NAME", "STATUS", "DURATION"])
            .style(
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .title(" Background Tasks ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray)),
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
            for l in recent_logs {
                log_lines.push(Line::from(Span::styled(
                    l.clone(),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
        if log_lines.is_empty() {
            log_lines.push(Line::from(Span::styled(
                "Belum ada log.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else {
        log_lines.push(Line::from(Span::styled(
            "Belum ada background task aktif.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let logs_widget = Paragraph::new(log_lines)
        .block(
            Block::default()
                .title(" Task Output Logs [c: cancel] ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(logs_widget, tasks_layout[1]);
}

fn render_skills_tab(frame: &mut Frame, app: &App, area: Rect, border_color: Color) {
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

            let prefix = if is_active {
                "★ [ACTIVE] "
            } else if is_sel {
                "❯ "
            } else {
                "  "
            };

            let title_style = if is_active {
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD)
            } else if is_sel {
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let content = vec![
                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Yellow)),
                    Span::styled(s.name, title_style),
                    Span::styled(format!(" ({})", s.id), Style::default().fg(Color::DarkGray)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(s.description, Style::default().fg(Color::DarkGray)),
                ]),
            ];

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Specialized AI Skills (Enter to select) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(list, area);
}

fn render_provider_tab(frame: &mut Frame, app: &App, area: Rect, border_color: Color) {
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
                "❯ "
            } else {
                "  "
            };

            let title_style = if is_active {
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD)
            } else if is_sel {
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let ctx_str = p
                .context_window
                .map(|c| format!("{} tokens", c))
                .unwrap_or_else(|| "128k".to_string());

            let content = vec![
                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Yellow)),
                    Span::styled(&p.name, title_style),
                    Span::styled(
                        format!(" [{}]", p.protocol),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("    Model: "),
                    Span::styled(&p.default_model, Style::default().fg(Color::LightYellow)),
                    Span::raw(" │ Ctx: "),
                    Span::styled(ctx_str, Style::default().fg(Color::DarkGray)),
                ]),
                Line::from(vec![
                    Span::raw("    URL: "),
                    Span::styled(&p.base_url, Style::default().fg(Color::DarkGray)),
                ]),
            ];

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" AI Providers & Endpoints (Enter to switch) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(list, area);
}

fn render_help_tab(frame: &mut Frame, _app: &App, area: Rect, border_color: Color) {
    let lines = vec![
        Line::from(Span::styled(
            "⌨ Navigasi & Pintasan Keyboard:",
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  [Tab]            : Pindah fokus antar panel (Input ⇄ Chat ⇄ Sidebar)"),
        Line::from("  [Enter]          : Kirim prompt / Pilih opsi terpilih"),
        Line::from("  [Esc]            : Batalkan eksekusi prompt yang sedang berjalan"),
        Line::from("  [F1]             : Tab Bantuan (Help)"),
        Line::from("  [F2]             : Tab Background Tasks (Lihat status & log subagent)"),
        Line::from("  [F3]             : Tab Skills (Pilih peran AI spesialis)"),
        Line::from("  [F4]             : Tab Provider (Pilih endpoint & model AI)"),
        Line::from("  [F5] / :cli      : Kembali ke mode CLI / REPL (atau Ctrl+G)"),
        Line::from("  [PgUp] / [PgDn]  : Gulir riwayat pesan percakapan"),
        Line::from("  [↑] / [↓]        : Navigasi riwayat input & pilihan daftar"),
        Line::from("  [Ctrl+C]         : Keluar dari ctrl-cli TUI"),
        Line::raw(""),
        Line::from(Span::styled(
            "⚡ Perintah Slash Populer:",
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  /help            : Bantuan lengkap"),
        Line::from("  /clear, /reset   : Bersihkan riwayat percakapan sesi"),
        Line::from("  /tasks [clear]   : Kelola atau bersihkan task subagent"),
        Line::from("  /skill <id>      : Ganti peran ke rust-expert, code-reviewer, dll."),
        Line::from("  /model <name>    : Ganti model target (GLM, Claude, GPT, dll.)"),
        Line::from("  /compact         : Kompaksi konteks percakapan hemat token"),
        Line::from("  /cli, /repl      : Kembali ke mode CLI / REPL"),
        Line::from("  /exit, /quit     : Keluar dari aplikasi"),
    ];

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Bantuan & Panduan Cepat ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(para, area);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::Input;
    let border_color = if is_focused {
        Color::LightGreen
    } else {
        Color::DarkGray
    };

    let prompt_prefix = "❯ ";
    let display_text = format!("{}{}", prompt_prefix, app.input);

    let title = if app.agent_running {
        Span::styled(
            " [Eksekusi Sedang Berjalan - Tekan Esc untuk membatalkan] ",
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled(
            " Instruksi / Prompt / Slash Command ",
            Style::default().add_modifier(Modifier::BOLD),
        )
    };

    let input_para = Paragraph::new(display_text).block(
        Block::default()
            .title(title)
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

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let status_text = if let Some((msg, _)) = &app.status_message {
        Span::styled(
            format!(" 📢 {}", msg),
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " [Tab] Fokus  [Enter] Kirim  [Esc] Batal",
            Style::default().fg(Color::DarkGray),
        )
    };

    let tokens_text = Span::styled(
        format!(
            "Tokens: {} in / {} out (Total: {}) ",
            app.total_prompt_tokens, app.total_completion_tokens, app.total_tokens
        ),
        Style::default().fg(Color::DarkGray),
    );

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(25),
            Constraint::Length(38),
            Constraint::Length(35),
        ])
        .split(area);

    let status_para = Paragraph::new(Line::from(status_text));
    let telemetry_para = Paragraph::new(render_footer_telemetry_line(app.metrics.as_ref()))
        .alignment(Alignment::Center);
    let token_para = Paragraph::new(Line::from(tokens_text)).alignment(Alignment::Right);

    frame.render_widget(status_para, layout[0]);
    frame.render_widget(telemetry_para, layout[1]);
    frame.render_widget(token_para, layout[2]);
}

fn render_slash_popup(frame: &mut Frame, app: &App, input_area: Rect) {
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
            let style = if is_sel {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let line = Line::from(vec![
                Span::styled(format!("{:<15}", cmd), style),
                Span::styled(
                    format!(" - {}", desc),
                    if is_sel {
                        style
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Saran Perintah (Tab / Enter untuk memilih) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, popup_area);
}

fn render_permission_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(req) = &app.active_permission_request else {
        return;
    };

    let modal_width = 70.min(area.width.saturating_sub(4));
    let modal_height = 9.min(area.height.saturating_sub(4));
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
            "⚠️ Permintaan Izin Eksekusi Tool Mutasi:",
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::raw("  • Tool Name : "),
            Span::styled(
                &req.tool_name,
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  • Arguments : "),
            Span::styled(&req.arguments_json, Style::default().fg(Color::DarkGray)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "  [Y] Allow Once   ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  [A] Always Allow (Session)   ",
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  [N / Esc] Tolak",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let modal_widget = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Konfirmasi Keamanan (Permission Gate) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(modal_widget, modal_area);
}
