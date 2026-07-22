use std::io::stdout;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute};

/// RAII guard that puts the terminal into raw mode and guarantees it is
/// restored on drop — even if the surrounding code panics or returns early.
///
/// Without this, a `.unwrap()`/panic while the terminal is in raw mode would
/// leave the user's terminal corrupted (no echo, hidden cursor, stuck in the
/// alternate screen).
pub struct RawModeGuard {
    alternate_screen: bool,
    mouse_capture: bool,
}

impl RawModeGuard {
    /// Enable raw mode and hide the cursor. Used by the simple device picker.
    pub fn new() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), cursor::Hide)?;
        Ok(Self {
            alternate_screen: false,
            mouse_capture: false,
        })
    }

    /// Enable raw mode, enter the alternate screen and capture the mouse.
    /// Used by the full-screen console TUI.
    pub fn with_alternate_screen() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(Self {
            alternate_screen: true,
            mouse_capture: true,
        })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        if self.mouse_capture {
            let _ = execute!(out, DisableMouseCapture);
        }
        if self.alternate_screen {
            let _ = execute!(out, LeaveAlternateScreen);
        }
        let _ = execute!(out, cursor::Show);
        let _ = disable_raw_mode();
    }
}
