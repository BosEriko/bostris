mod app;
mod bag;
mod board;
mod config;
mod finesse;
mod game;
mod highscores;
mod input;
mod piece;
mod scoring;
mod settings;
mod ui;

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{
    DisableFocusChange, EnableFocusChange, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};

use crate::app::App;
use crate::highscores::HighScores;

static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);
static KEYBOARD_ENHANCED: AtomicBool = AtomicBool::new(false);

fn restore_terminal() {
    if !TERMINAL_ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut stdout = io::stdout();
    if KEYBOARD_ENHANCED.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(stdout, DisableFocusChange, Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    let _ = stdout.flush();
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

fn setup_terminal() -> io::Result<(TerminalGuard, bool)> {
    enable_raw_mode()?;
    TERMINAL_ACTIVE.store(true, Ordering::SeqCst);
    let guard = TerminalGuard;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide, EnableFocusChange)?;
    let enhanced = matches!(supports_keyboard_enhancement(), Ok(true));
    if enhanced {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
        KEYBOARD_ENHANCED.store(true, Ordering::SeqCst);
    }
    Ok((guard, enhanced || cfg!(windows)))
}

fn run(app_config: config::Config, scores: HighScores) -> io::Result<Option<String>> {
    install_panic_hook();
    let (_guard, precise_keys) = setup_terminal()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let mut app = App::new(app_config, scores, precise_keys);
    app.run(&mut terminal)?;
    Ok(app.save_error().map(str::to_owned))
}

fn print_help() {
    let path = |p: Option<std::path::PathBuf>| {
        p.map_or_else(|| "unavailable".to_owned(), |p| p.display().to_string())
    };
    println!(
        "bostris {}\n\n\
         A fast, polished Tetris for the terminal.\n\n\
         Usage: bostris [--help | --version]\n\n\
         Controls: ←/→ move, ↓ soft drop, Space hard drop, ↑/X rotate clockwise,\n\
         \x20         Z rotate counterclockwise, C hold, P/Esc pause, R restart, Q quit\n\n\
         Config:      {}\n\
         High scores: {}",
        env!("CARGO_PKG_VERSION"),
        path(config::config_path()),
        path(highscores::default_path()),
    );
}

fn main() -> ExitCode {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("bostris {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("bostris: unknown argument '{other}' (try --help)");
                return ExitCode::from(2);
            }
        }
    }

    let (app_config, warnings) = config::load();
    let scores = HighScores::load(highscores::default_path());
    let result = run(app_config, scores);

    for warning in &warnings {
        eprintln!("bostris: config: {warning}");
    }
    match result {
        Ok(save_error) => {
            if let Some(err) = save_error {
                eprintln!("bostris: {err}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("bostris: {err}");
            ExitCode::FAILURE
        }
    }
}
