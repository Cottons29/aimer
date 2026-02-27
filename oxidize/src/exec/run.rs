use std::io;
use std::io::{Read, Write};
use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

pub struct RunCommand;

impl RunCommand {
    pub(crate) fn run () -> io::Result<()> {
        enable_raw_mode()?; // raw mode: no buffering, no Ctrl+C, full chaos mode 😈

        println!("Press any key (Esc to exit)...");
        io::stdout().flush()?;

        loop {
            if let Event::Key(key_event) = event::read()? {
                // Only react on key press (avoid repeats from release)
                if key_event.kind == KeyEventKind::Press {
                    match key_event.code {
                        KeyCode::Esc => {
                            println!("\nBye bye 👋 (Esc detected)");
                            break;
                        }
                        other => {
                            println!("\r\rYou pressed: {:?}\r", other);
                        }
                    }
                }
            }
        }
        disable_raw_mode()
    }
}