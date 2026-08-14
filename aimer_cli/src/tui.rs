use std::fmt;
use std::io::stdout;

use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};
use crossterm::{Command, cursor, execute};

/// Turn on the mouse reporting modes the console actually reads.
///
/// Deliberately *not* [`EnableMouseCapture`], which also turns on `?1003h`
/// ("report any-event tracking"): with it every pixel of pointer movement over
/// the terminal becomes an event, and on a busy log stream that flood delays —
/// or, once the tty buffer overflows, swallows — the keystrokes queued behind
/// it. The console only reacts to the wheel and to left-button drags, so it
/// asks for normal button tracking (`?1000h`) plus button-event tracking
/// (`?1002h`, motion *while a button is held*), with the SGR extended
/// coordinate encodings (`?1015h`, `?1006h`) so wide terminals report
/// correctly.
///
/// [`EnableMouseCapture`]: crossterm::event::EnableMouseCapture
#[derive(Debug, Clone, Copy)]
pub struct EnableButtonMouseCapture;

impl Command for EnableButtonMouseCapture {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[?1000h\x1B[?1002h\x1B[?1015h\x1B[?1006h")
    }

    #[cfg(target_os = "windows")]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Undo [`EnableButtonMouseCapture`], in reverse order.
#[derive(Debug, Clone, Copy)]
pub struct DisableButtonMouseCapture;

impl Command for DisableButtonMouseCapture {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1B[?1006l\x1B[?1015l\x1B[?1002l\x1B[?1000l")
    }

    #[cfg(target_os = "windows")]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Ok(())
    }
}

/// RAII guard that puts the terminal into raw mode and guarantees it is
/// restored on drop — even if the surrounding code panics or returns early.
///
/// Without this, a `.unwrap()`/panic while the terminal is in raw mode would
/// leave the user's terminal corrupted (no echo, hidden cursor, stuck in the
/// alternate screen).
pub struct RawModeGuard {
    alternate_screen: bool,
    mouse_capture: bool,
    supports_enhancement: bool,
}

impl RawModeGuard {
    /// Enable raw mode and hide the cursor. Used by the simple device picker.
    pub fn new() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), cursor::Hide)?;
        Ok(Self {
            alternate_screen: false,
            mouse_capture: false,
            supports_enhancement: false,
        })
    }

    /// Enable raw mode, enter the alternate screen and capture the mouse.
    /// Used by the full-screen console TUI.
    pub fn with_alternate_screen() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let supports_enhancement = supports_keyboard_enhancement().unwrap_or(false);
        execute!(
            stdout(),
            EnterAlternateScreen,
            EnableButtonMouseCapture,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
        Ok(Self {
            alternate_screen: true,
            mouse_capture: true,
            supports_enhancement,
        })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        if self.mouse_capture {
            let _ = execute!(out, DisableButtonMouseCapture);
        }
        if self.alternate_screen {
            let _ = execute!(out, LeaveAlternateScreen);
        };
        let _ = execute!(out, cursor::Show);
        if self.supports_enhancement {
            let _ = execute!(out, PopKeyboardEnhancementFlags);
        }
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansi(command: impl Command) -> String {
        let mut out = String::new();
        command.write_ansi(&mut out).unwrap();
        out
    }

    #[test]
    fn mouse_capture_asks_for_button_tracking() {
        let sequence = ansi(EnableButtonMouseCapture);
        assert!(sequence.contains("\x1B[?1000h"));
        assert!(sequence.contains("\x1B[?1002h"));
    }

    #[test]
    fn mouse_capture_does_not_ask_for_any_event_tracking() {
        // `?1003h` reports bare pointer motion, which floods the input queue
        // and delays hotkeys behind it.
        assert!(!ansi(EnableButtonMouseCapture).contains("1003"));
    }

    #[test]
    fn mouse_capture_uses_the_extended_coordinate_encodings() {
        let sequence = ansi(EnableButtonMouseCapture);
        assert!(sequence.contains("\x1B[?1015h"));
        assert!(sequence.contains("\x1B[?1006h"));
    }

    #[test]
    fn disabling_resets_every_mode_that_was_set() {
        let disable = ansi(DisableButtonMouseCapture);
        for mode in ["1000", "1002", "1015", "1006"] {
            assert!(
                disable.contains(&format!("\x1B[?{mode}l")),
                "mode {mode} left enabled"
            );
        }
    }
}
