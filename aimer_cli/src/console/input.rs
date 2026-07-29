//! Terminal input collection for the console TUI.
//!
//! The console redraws on a fixed tick, but a terminal can hand it far more
//! events than one per tick — a scroll wheel, a mouse drag, or a key held down
//! all arrive in bursts, and a busy log stream makes each frame longer than the
//! tick itself. Reading a single event per frame therefore leaves the tty input
//! buffer growing without bound, and a hotkey pressed while the queue is backed
//! up only reaches the handler seconds later (or is dropped when the buffer
//! overflows), which reads to the user as "the key did nothing".
//!
//! [`collect_events`] drains everything the terminal has ready in one go and
//! discards the events the console has no use for, so a burst costs one frame
//! instead of one frame per event.

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEventKind};

/// Upper bound on the events consumed in a single frame.
///
/// A terminal that produces events faster than the console drains them would
/// otherwise keep [`collect_events`] spinning forever and starve the redraw.
pub const MAX_EVENTS_PER_FRAME: usize = 512;

/// Whether a key event should drive a hotkey.
///
/// Terminals that negotiate the [Kitty keyboard protocol] (Ghostty, WezTerm,
/// foot, …) report key releases as well as presses. Acting on both fires every
/// hotkey twice — `Tab` would switch two panes, `r` would restart the runner
/// twice — so releases are dropped. Auto-repeat is kept: holding `Up` should
/// keep scrolling.
///
/// [Kitty keyboard protocol]: https://sw.kovidgoyal.net/kitty/keyboard-protocol/
#[inline]
pub fn is_actionable_key(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

/// Whether the console has any use for `event`.
///
/// Bare pointer motion (no button held) is reported by some terminals and is
/// pure noise here: the console only reacts to the wheel and to left-button
/// drags. Filtering it out keeps a flood of motion from delaying the keystrokes
/// queued behind it.
#[inline]
pub fn is_relevant(event: &Event) -> bool {
    match event {
        Event::Key(key) => is_actionable_key(key),
        Event::Mouse(mouse) => !matches!(mouse.kind, MouseEventKind::Moved),
        _ => true,
    }
}

/// Collect every event the terminal has ready, waiting up to `timeout` for the
/// first one.
///
/// `poll` and `read` mirror [`crossterm::event::poll`] and
/// [`crossterm::event::read`]; they are parameters so the draining logic can be
/// exercised without a tty. Only the first poll waits — the rest use a zero
/// timeout, so this returns as soon as the queue is empty. Events rejected by
/// [`is_relevant`] are dropped but still counted against
/// [`MAX_EVENTS_PER_FRAME`], so a motion flood cannot stall the caller.
pub fn collect_events(
    mut poll: impl FnMut(Duration) -> io::Result<bool>,
    mut read: impl FnMut() -> io::Result<Event>,
    timeout: Duration,
) -> io::Result<Vec<Event>> {
    let mut events = Vec::new();
    let mut wait = timeout;

    for _ in 0..MAX_EVENTS_PER_FRAME {
        if !poll(wait)? {
            break;
        }
        wait = Duration::ZERO;
        let event = read()?;
        if is_relevant(&event) {
            events.push(event);
        }
    }

    Ok(events)
}

/// [`collect_events`] wired to the real terminal.
#[inline]
pub fn drain_terminal(timeout: Duration) -> io::Result<Vec<Event>> {
    collect_events(event::poll, event::read, timeout)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent};

    use super::*;

    fn key(code: KeyCode, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind))
    }

    fn mouse(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
    }

    /// Feed `queued` to [`collect_events`] and report what it kept, together
    /// with the timeouts it polled with.
    fn drain(queued: Vec<Event>) -> (Vec<Event>, Vec<Duration>) {
        let pending = RefCell::new(queued.into_iter().collect::<std::collections::VecDeque<_>>());
        let waits = RefCell::new(Vec::new());
        let collected = collect_events(
            |timeout| {
                waits.borrow_mut().push(timeout);
                Ok(!pending.borrow().is_empty())
            },
            || Ok(pending.borrow_mut().pop_front().unwrap()),
            Duration::from_millis(100),
        )
        .unwrap();
        (collected, waits.into_inner())
    }

    // ── is_actionable_key ────────────────────────────────────────────

    #[test]
    fn key_press_is_actionable() {
        assert!(is_actionable_key(&KeyEvent::new_with_kind(
            KeyCode::Tab,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )));
    }

    #[test]
    fn key_repeat_is_actionable() {
        assert!(is_actionable_key(&KeyEvent::new_with_kind(
            KeyCode::Up,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )));
    }

    #[test]
    fn key_release_is_not_actionable() {
        assert!(!is_actionable_key(&KeyEvent::new_with_kind(
            KeyCode::Tab,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )));
    }

    // ── is_relevant ──────────────────────────────────────────────────

    #[test]
    fn bare_pointer_motion_is_irrelevant() {
        assert!(!is_relevant(&mouse(MouseEventKind::Moved)));
    }

    #[test]
    fn wheel_and_drag_are_relevant() {
        assert!(is_relevant(&mouse(MouseEventKind::ScrollUp)));
        assert!(is_relevant(&mouse(MouseEventKind::Drag(MouseButton::Left))));
    }

    #[test]
    fn resize_is_relevant() {
        assert!(is_relevant(&Event::Resize(80, 24)));
    }

    // ── collect_events ───────────────────────────────────────────────

    #[test]
    fn collects_nothing_when_the_queue_is_empty() {
        let (events, waits) = drain(Vec::new());
        assert!(events.is_empty());
        assert_eq!(waits, vec![Duration::from_millis(100)]);
    }

    #[test]
    fn drains_the_whole_queue_in_one_call() {
        let queued = vec![
            key(KeyCode::Char('1'), KeyEventKind::Press),
            key(KeyCode::Tab, KeyEventKind::Press),
            key(KeyCode::Char('r'), KeyEventKind::Press),
        ];
        let (events, _) = drain(queued.clone());
        assert_eq!(events, queued);
    }

    #[test]
    fn only_the_first_poll_waits() {
        let (_, waits) = drain(vec![
            key(KeyCode::Tab, KeyEventKind::Press),
            key(KeyCode::Tab, KeyEventKind::Press),
        ]);
        assert_eq!(waits[0], Duration::from_millis(100));
        assert!(waits[1..].iter().all(|w| w.is_zero()));
    }

    #[test]
    fn a_hotkey_behind_a_motion_flood_still_arrives() {
        let mut queued: Vec<Event> = (0..64).map(|_| mouse(MouseEventKind::Moved)).collect();
        queued.push(key(KeyCode::Tab, KeyEventKind::Press));

        let (events, _) = drain(queued);
        assert_eq!(events, vec![key(KeyCode::Tab, KeyEventKind::Press)]);
    }

    #[test]
    fn key_releases_are_dropped_so_hotkeys_fire_once() {
        let (events, _) = drain(vec![
            key(KeyCode::Tab, KeyEventKind::Press),
            key(KeyCode::Tab, KeyEventKind::Release),
        ]);
        assert_eq!(events, vec![key(KeyCode::Tab, KeyEventKind::Press)]);
    }

    #[test]
    fn a_never_ending_stream_is_capped() {
        let pending = RefCell::new(0usize);
        let events = collect_events(
            |_| Ok(true),
            || {
                *pending.borrow_mut() += 1;
                Ok(key(KeyCode::Char('x'), KeyEventKind::Press))
            },
            Duration::from_millis(100),
        )
        .unwrap();

        assert_eq!(events.len(), MAX_EVENTS_PER_FRAME);
        assert_eq!(*pending.borrow(), MAX_EVENTS_PER_FRAME);
    }

    #[test]
    fn a_read_error_is_propagated() {
        let result = collect_events(
            |_| Ok(true),
            || Err(io::Error::other("boom")),
            Duration::from_millis(100),
        );
        assert!(result.is_err());
    }
}
