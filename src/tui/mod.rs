pub mod app;
pub mod event;
pub mod highlight;
pub mod ui;

pub use highlight::highlight_markdown_code_blocks_ansi;

use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use std::sync::Arc;

use crate::agent::provider::ProvidersRegistry;
use crate::UserProfile;
use app::App;

pub type PanicHook = Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

/// RAII Drop Guard ensuring terminal state and panic hook are unconditionally restored.
pub struct TerminalGuard {
    active: bool,
    prev_panic_hook: Option<PanicHook>,
}

impl TerminalGuard {
    /// Enters raw mode, switches to alternate screen, enables mouse capture,
    /// and installs a safety panic hook that restores the terminal before unwinding/aborting.
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut guard = Self {
            active: true,
            prev_panic_hook: None,
        };

        if let Err(e) = execute!(stdout(), EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(e.into());
        }

        let prev_hook: PanicHook = Arc::from(std::panic::take_hook());
        let hook_clone = prev_hook.clone();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(
                stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                Show
            );
            hook_clone(info);
        }));
        guard.prev_panic_hook = Some(prev_hook);

        Ok(guard)
    }

    /// Explicit restoration of terminal attributes and panic hook. Idempotent.
    pub fn restore(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
            let _ = execute!(
                stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                Show
            );
            self.active = false;
        }
        if let Some(prev) = self.prev_panic_hook.take() {
            std::panic::set_hook(Box::new(move |info| prev(info)));
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Launches and runs the full-screen Terminal User Interface (TUI).
pub fn run_tui(
    user_profile: &mut UserProfile,
    providers_reg: &mut ProvidersRegistry,
) -> Result<()> {
    // Setup terminal with RAII drop guard
    let mut guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(user_profile.clone(), providers_reg.clone());

    // Main TUI render & event loop
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &mut app))?;
        if let Err(e) = event::handle_events(&mut app) {
            app.set_status(format!("Event error: {}", e));
        }
    }

    // Cancel active background agent worker if still running before teardown
    app.cancel_agent();

    // Clean terminal restoration and panic hook reinstallation
    guard.restore();

    // Sync state back to caller
    *user_profile = app.user_profile;
    *providers_reg = app.providers_reg;

    // Only print goodbye when quitting the application, not when returning to REPL
    if !app.return_to_repl {
        println!(
            "\nTerima kasih telah menggunakan ctrl-cli! Sampai jumpa, {}.",
            user_profile.name
        );
    }
    Ok(())
}
