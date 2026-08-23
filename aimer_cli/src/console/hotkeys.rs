//! Neutral keyboard actions for the interactive console.
//!
//! This module deliberately knows nothing about the event loop or a renderer.
//! It translates terminal key events into actions that inline and full-screen
//! consumers can handle independently.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// The console pane selected by a numeric pane shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolePane {
    /// Application output and selection.
    App,
    /// Build and compiler output.
    Build,
    /// The inspector view.
    Inspector,
}

/// A renderer-independent result of handling one console key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleAction {
    /// Exit the running console session.
    Quit,
    /// Request an in-place hot reload of the running application.
    HotReload,
    /// Request a full stop, rebuild, and relaunch of the application.
    HotRestart,
    /// Select a numbered console pane.
    SelectPane(ConsolePane),
    /// Move focus to the next console pane.
    NextPane,
    /// Toggle the inspector and focus its pane.
    ToggleInspector,
    /// Toggle the inspector's full-tree view.
    ToggleInspectorTree,
    /// Toggle source locations on application log entries.
    ToggleSourceLocations,
    /// Toggle Vim-style mouse selection mode.
    ToggleSelectionMode,
    /// Copy the active selection and clear it when copying succeeds.
    YankSelection,
    /// Copy the focused pane.
    CopyPane,
    /// Copy the active selection, or the focused pane when there is no selection.
    CopySelectionOrPane,
    /// Clear the focused app or build pane.
    ClearPane,
    /// Scroll the focused view up by one line.
    ScrollUp,
    /// Scroll the focused view down by one line.
    ScrollDown,
    /// Scroll the focused view up by one page.
    PageUp,
    /// Scroll the focused view down by one page.
    PageDown,
    /// Expand or collapse the selected inline stage.
    ExpandStage,
}

/// Translate one terminal key event into a neutral console action.
///
/// `Press` and Kitty keyboard-protocol `Repeat` events are actionable. Kitty
/// `Release` events are ignored so a key cannot trigger the same operation
/// twice. The returned action contains no renderer or event-loop state; the
/// consumer decides, for example, whether inspector-only actions are valid for
/// the currently focused pane.
#[inline]
pub fn map_key_event(key: &KeyEvent) -> Option<ConsoleAction> {
    if matches!(key.kind, KeyEventKind::Release) {
        return None;
    }

    let modifiers = key.modifiers;

    match key.code {
        // A shifted `r` can arrive as either `Char('r') + SHIFT` or an
        // uppercase character with SHIFT, depending on terminal encoding.
        KeyCode::Char('r') if modifiers.contains(KeyModifiers::SHIFT) => {
            Some(ConsoleAction::HotRestart)
        }
        KeyCode::Char('R') if modifiers.contains(KeyModifiers::SHIFT) => {
            Some(ConsoleAction::HotRestart)
        }
        KeyCode::Char('r') => Some(ConsoleAction::HotReload),

        // Keep both forms used by terminals for Shift+Q. The uppercase event
        // is intentionally accepted even when the modifier bit is absent.
        KeyCode::Char('Q') => Some(ConsoleAction::Quit),
        KeyCode::Char('q') if modifiers.contains(KeyModifiers::SHIFT) => {
            Some(ConsoleAction::Quit)
        }

        KeyCode::Char('1') => Some(ConsoleAction::SelectPane(ConsolePane::App)),
        KeyCode::Char('2') => Some(ConsoleAction::SelectPane(ConsolePane::Build)),
        KeyCode::Char('3') => Some(ConsoleAction::SelectPane(ConsolePane::Inspector)),
        KeyCode::Tab => Some(ConsoleAction::NextPane),
        KeyCode::F(12) => Some(ConsoleAction::ToggleInspector),
        KeyCode::Char('t') => Some(ConsoleAction::ToggleInspectorTree),

        KeyCode::Char('e') | KeyCode::Char('E') => {
            Some(ConsoleAction::ToggleSourceLocations)
        }
        KeyCode::Char('s') | KeyCode::Char('S') => Some(ConsoleAction::ToggleSelectionMode),
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(ConsoleAction::YankSelection),

        // Shift+C is the clear command. Control+C and Command+C copy the
        // selection, falling back to the focused pane when there is none.
        KeyCode::Char('c') | KeyCode::Char('C')
            if modifiers == KeyModifiers::SHIFT => Some(ConsoleAction::ClearPane),
        KeyCode::Char('c') | KeyCode::Char('C')
            if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER) =>
        {
            Some(ConsoleAction::CopySelectionOrPane)
        }
        KeyCode::Char('c') | KeyCode::Char('C') => Some(ConsoleAction::CopyPane),

        // Enter belongs to inline stage expansion; source-location toggling
        // remains on `e`/`E` above.
        KeyCode::Enter => Some(ConsoleAction::ExpandStage),

        KeyCode::Up => Some(ConsoleAction::ScrollUp),
        KeyCode::Down => Some(ConsoleAction::ScrollDown),
        KeyCode::PageUp => Some(ConsoleAction::PageUp),
        KeyCode::PageDown => Some(ConsoleAction::PageDown),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{ConsoleAction, ConsolePane, map_key_event};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn key_with_kind(
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: KeyEventKind,
    ) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, kind)
    }

    #[test]
    fn lower_r_reloads_and_shifted_upper_r_restarts() {
        assert_eq!(
            map_key_event(&key(KeyCode::Char('r'), KeyModifiers::NONE)),
            Some(ConsoleAction::HotReload),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('R'), KeyModifiers::SHIFT)),
            Some(ConsoleAction::HotRestart),
        );
    }

    #[test]
    fn enter_expands_a_stage_without_stealing_source_toggle() {
        assert_eq!(
            map_key_event(&key(KeyCode::Enter, KeyModifiers::NONE)),
            Some(ConsoleAction::ExpandStage),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('e'), KeyModifiers::NONE)),
            Some(ConsoleAction::ToggleSourceLocations),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('E'), KeyModifiers::SHIFT)),
            Some(ConsoleAction::ToggleSourceLocations),
        );
    }

    #[test]
    fn pane_selection_and_navigation_controls_are_preserved() {
        assert_eq!(
            map_key_event(&key(KeyCode::Char('1'), KeyModifiers::NONE)),
            Some(ConsoleAction::SelectPane(ConsolePane::App)),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('2'), KeyModifiers::NONE)),
            Some(ConsoleAction::SelectPane(ConsolePane::Build)),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('3'), KeyModifiers::NONE)),
            Some(ConsoleAction::SelectPane(ConsolePane::Inspector)),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Tab, KeyModifiers::NONE)),
            Some(ConsoleAction::NextPane),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::F(12), KeyModifiers::NONE)),
            Some(ConsoleAction::ToggleInspector),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('t'), KeyModifiers::NONE)),
            Some(ConsoleAction::ToggleInspectorTree),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Up, KeyModifiers::NONE)),
            Some(ConsoleAction::ScrollUp),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Down, KeyModifiers::NONE)),
            Some(ConsoleAction::ScrollDown),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::PageUp, KeyModifiers::NONE)),
            Some(ConsoleAction::PageUp),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::PageDown, KeyModifiers::NONE)),
            Some(ConsoleAction::PageDown),
        );
    }

    #[test]
    fn quit_selection_and_copy_controls_are_preserved() {
        assert_eq!(
            map_key_event(&key(KeyCode::Char('Q'), KeyModifiers::NONE)),
            Some(ConsoleAction::Quit),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('q'), KeyModifiers::SHIFT)),
            Some(ConsoleAction::Quit),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('s'), KeyModifiers::NONE)),
            Some(ConsoleAction::ToggleSelectionMode),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('S'), KeyModifiers::SHIFT)),
            Some(ConsoleAction::ToggleSelectionMode),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('y'), KeyModifiers::NONE)),
            Some(ConsoleAction::YankSelection),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(ConsoleAction::CopyPane),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('C'), KeyModifiers::SHIFT)),
            Some(ConsoleAction::ClearPane),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(ConsoleAction::CopySelectionOrPane),
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('C'), KeyModifiers::SUPER)),
            Some(ConsoleAction::CopySelectionOrPane),
        );
    }

    #[test]
    fn kitty_key_releases_are_ignored_but_repeats_are_mapped() {
        assert_eq!(
            map_key_event(&key_with_kind(
                KeyCode::Char('r'),
                KeyModifiers::NONE,
                KeyEventKind::Repeat,
            )),
            Some(ConsoleAction::HotReload),
        );
        assert_eq!(
            map_key_event(&key_with_kind(
                KeyCode::Up,
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
            None,
        );
    }

    #[test]
    fn unrelated_keys_are_ignored() {
        assert_eq!(
            map_key_event(&key(KeyCode::Esc, KeyModifiers::NONE)),
            None,
        );
        assert_eq!(
            map_key_event(&key(KeyCode::Char('q'), KeyModifiers::NONE)),
            None,
        );
    }
}
