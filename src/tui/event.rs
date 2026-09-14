use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;

use crate::agent::permissions::PermissionResponse;
use crate::agent::tasks::TaskManager;
use crate::get_available_skills;
use crate::tui::app::{App, FocusedPane, SidebarTab};

pub fn handle_events(app: &mut App) -> anyhow::Result<()> {
    // 1. Drain any pending messages from the background agent worker thread
    while let Ok(action) = app.action_rx.try_recv() {
        app.handle_action(action);
    }

    // 2. Poll for terminal input events with a short timeout (e.g. 30ms) for smooth rendering
    if event::poll(Duration::from_millis(30))? {
        match event::read()? {
            Event::Key(key) => {
                // On Windows, Crossterm emits Press and Release events; handle only Press
                if key.kind == KeyEventKind::Press {
                    handle_key_event(app, key);
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                event::MouseEventKind::ScrollUp => {
                    app.chat_scroll = app.chat_scroll.saturating_sub(3);
                    app.auto_scroll = false;
                }
                event::MouseEventKind::ScrollDown => {
                    app.chat_scroll = app.chat_scroll.saturating_add(3);
                }
                _ => {}
            },
            _ => {}
        }
    }

    // 3. Clear expired status banner after timeout
    app.clear_expired_status();

    Ok(())
}

fn handle_key_event(app: &mut App, key: KeyEvent) {
    // Priority 1: Permission Dialog active modal intercepts all keys
    if app.active_permission_request.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.respond_permission(PermissionResponse::AllowOnce);
                app.set_status("Izin diberikan (sekali).");
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                app.respond_permission(PermissionResponse::AlwaysAllow);
                app.set_status("Izin diberikan otomatis untuk seluruh sesi.");
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.respond_permission(PermissionResponse::Deny);
                app.set_status("Eksekusi tool ditolak oleh user.");
            }
            _ => {}
        }
        return;
    }

    // Priority 2: Global Quitting and Navigation
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') => {
                app.cancel_agent();
                app.should_quit = true;
                return;
            }
            KeyCode::Char('g') => {
                app.request_return_to_repl();
                return;
            }
            KeyCode::Char('l') => {
                app.chat_items.clear();
                app.set_status("Layar chat dibersihkan.");
                return;
            }
            _ => {}
        }
    }

    // Priority 3: Function Keys for Direct Tab Access & Return to CLI
    match key.code {
        KeyCode::F(1) => {
            app.active_tab = SidebarTab::Help;
            app.focused_pane = FocusedPane::Sidebar;
            return;
        }
        KeyCode::F(2) => {
            app.active_tab = SidebarTab::Tasks;
            app.focused_pane = FocusedPane::Sidebar;
            return;
        }
        KeyCode::F(3) => {
            app.active_tab = SidebarTab::Skills;
            app.focused_pane = FocusedPane::Sidebar;
            return;
        }
        KeyCode::F(4) => {
            app.active_tab = SidebarTab::Provider;
            app.focused_pane = FocusedPane::Sidebar;
            return;
        }
        KeyCode::F(5) => {
            app.request_return_to_repl();
            return;
        }
        _ => {}
    }

    // Priority 4: Pane Navigation with Tab / BackTab
    if key.code == KeyCode::Tab && !app.show_slash_popup {
        app.focused_pane = match app.focused_pane {
            FocusedPane::Input => FocusedPane::Chat,
            FocusedPane::Chat => FocusedPane::Sidebar,
            FocusedPane::Sidebar => FocusedPane::Input,
        };
        return;
    }

    if key.code == KeyCode::BackTab {
        app.focused_pane = match app.focused_pane {
            FocusedPane::Input => FocusedPane::Sidebar,
            FocusedPane::Sidebar => FocusedPane::Chat,
            FocusedPane::Chat => FocusedPane::Input,
        };
        return;
    }

    // Priority 5: Escape key actions
    if key.code == KeyCode::Esc {
        if app.show_slash_popup {
            app.show_slash_popup = false;
        } else if app.agent_running {
            app.cancel_agent();
        } else {
            app.focused_pane = FocusedPane::Input;
        }
        return;
    }

    // Priority 6: Autocomplete Popup Navigation
    if app.show_slash_popup && !app.slash_suggestions.is_empty() {
        match key.code {
            KeyCode::Up => {
                if app.selected_slash_index > 0 {
                    app.selected_slash_index -= 1;
                } else {
                    app.selected_slash_index = app.slash_suggestions.len().saturating_sub(1);
                }
                return;
            }
            KeyCode::Down => {
                if app.selected_slash_index + 1 < app.slash_suggestions.len() {
                    app.selected_slash_index += 1;
                } else {
                    app.selected_slash_index = 0;
                }
                return;
            }
            KeyCode::Tab | KeyCode::Enter => {
                app.apply_selected_slash_suggestion();
                return;
            }
            _ => {}
        }
    }

    // Priority 7: Pane-specific key bindings
    match app.focused_pane {
        FocusedPane::Input => handle_input_keys(app, key),
        FocusedPane::Chat => handle_chat_keys(app, key),
        FocusedPane::Sidebar => handle_sidebar_keys(app, key),
    }
}

fn handle_input_keys(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => {
            app.insert_char(c);
        }
        KeyCode::Backspace => {
            app.backspace();
        }
        KeyCode::Delete => {
            app.delete();
        }
        KeyCode::Left => {
            app.move_cursor_left();
        }
        KeyCode::Right => {
            app.move_cursor_right();
        }
        KeyCode::Home => {
            app.move_cursor_start();
        }
        KeyCode::End => {
            app.move_cursor_end();
        }
        KeyCode::Up => {
            app.history_up();
        }
        KeyCode::Down => {
            app.history_down();
        }
        KeyCode::Enter => {
            app.submit_input();
        }
        _ => {}
    }
}

fn handle_chat_keys(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => {
            app.chat_scroll = app.chat_scroll.saturating_sub(1);
            app.auto_scroll = false;
        }
        KeyCode::Down => {
            app.chat_scroll = app.chat_scroll.saturating_add(1);
        }
        KeyCode::PageUp => {
            app.chat_scroll = app.chat_scroll.saturating_sub(8);
            app.auto_scroll = false;
        }
        KeyCode::PageDown => {
            app.chat_scroll = app.chat_scroll.saturating_add(8);
        }
        KeyCode::Home => {
            app.chat_scroll = 0;
            app.auto_scroll = false;
        }
        KeyCode::End => {
            app.auto_scroll = true;
        }
        KeyCode::Char('i') | KeyCode::Enter => {
            app.focused_pane = FocusedPane::Input;
        }
        _ => {}
    }
}

fn handle_sidebar_keys(app: &mut App, key: KeyEvent) {
    match app.active_tab {
        SidebarTab::Tasks => {
            let task_count = TaskManager::global().list_tasks().len();
            match key.code {
                KeyCode::Up => {
                    if app.selected_task_index > 0 {
                        app.selected_task_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if task_count > 0 && app.selected_task_index + 1 < task_count {
                        app.selected_task_index += 1;
                    }
                }
                KeyCode::Char('c') => {
                    let tasks = TaskManager::global().list_tasks();
                    if let Some(t) = tasks.get(app.selected_task_index) {
                        if TaskManager::global().cancel_task(&t.id).is_ok() {
                            app.set_status(format!("Task '{}' dibatalkan.", t.id));
                        } else {
                            app.set_status(format!("Gagal membatalkan task '{}'.", t.id));
                        }
                    }
                }
                KeyCode::Char('x') => {
                    let cleared = TaskManager::global().clear_completed();
                    app.set_status(format!("{} tasks selesai dibersihkan.", cleared));
                }
                _ => {}
            }
        }
        SidebarTab::Skills => {
            let skills = get_available_skills();
            match key.code {
                KeyCode::Up => {
                    if app.selected_skill_index > 0 {
                        app.selected_skill_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if app.selected_skill_index + 1 < skills.len() {
                        app.selected_skill_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some(skill) = skills.get(app.selected_skill_index) {
                        app.set_status(format!("Skill diaktifkan: {}", skill.name));
                        app.active_skill = Some(skill.clone());
                    }
                }
                _ => {}
            }
        }
        SidebarTab::Provider => {
            let prov_count = app.providers_reg.providers.len();
            match key.code {
                KeyCode::Up => {
                    if app.selected_provider_index > 0 {
                        app.selected_provider_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if app.selected_provider_index + 1 < prov_count {
                        app.selected_provider_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some(prov) = app.providers_reg.providers.get(app.selected_provider_index)
                    {
                        let id = prov.id.clone();
                        if let Ok(p) = app.providers_reg.switch_active(&id) {
                            app.current_model = p.default_model.clone();
                            app.set_status(format!(
                                "Beralih ke provider: {} (Model: {})",
                                p.name, app.current_model
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        SidebarTab::Help => {
            // Can scroll or press Enter to return to input
            if key.code == KeyCode::Enter {
                app.focused_pane = FocusedPane::Input;
            }
        }
    }
}
