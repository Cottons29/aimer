//! The clipboard menu of a text field: asking for one, raising it, running it.
//!
//! Kept beside [`RawTextField`] rather than inside it because the field is
//! already the largest element in the crate, and because none of this needs the
//! rest of it: a menu is a request, an anchor and a verb.

use std::rc::Rc;

use aimer_attribute::position::Vec2d;
use aimer_ctxmenu::{ContextMenuAnchor, ContextMenuRequest, ContextMenuStyle};

use super::{MenuOrigin, RawTextField, clipboard_read, clipboard_write};
use crate::input_field::context_menu::{FieldAction, FieldMenuState, actions_for, items};

impl RawTextField {
    /// Asks for a context menu, to be raised by the next frame.
    ///
    /// The verbs depend on what is selected, and what is selected is only known
    /// once [`Drawable::draw`] has turned the deferred click into a caret
    /// offset — the field has no canvas to measure with while handling an
    /// event. Requesting here and raising there is what lets a hold over a word
    /// offer `Copy` rather than only `Paste`.
    pub(super) fn request_menu(&self, origin: MenuOrigin) {
        self.pending_menu.set(Some(origin));
    }

    /// What the field can do right now.
    pub(super) fn menu_state(&self) -> FieldMenuState {
        let count = self.controller.grapheme_count();
        let selection = self.cursor.selection_range();
        FieldMenuState {
            has_selection: selection.is_some(),
            has_text: count > 0,
            all_selected: matches!(selection, Some((0, end)) if end >= count),
            read_only: self.read_only,
            clipboard_has_text: clipboard_read().is_some_and(|text| !text.is_empty()),
        }
    }

    /// Raises the menu asked for by [`RawTextField::request_menu`], if any.
    ///
    /// A menu with nothing worth offering — an empty, read-only field with an
    /// empty clipboard — is not raised at all, rather than shown as an empty
    /// panel the user has to dismiss.
    pub(super) fn raise_pending_menu(&self) {
        let Some(origin) = self.pending_menu.take() else {
            return;
        };
        let actions = actions_for(self.menu_state());
        if actions.is_empty() {
            self.menu.hide();
            return;
        }

        let anchor = match origin {
            MenuOrigin::Click(pos) => ContextMenuAnchor::Point(pos),
            MenuOrigin::Hold => match self.cached_bounds.get_bounds() {
                Some(bounds) => ContextMenuAnchor::Rect(bounds),
                None => return,
            },
        };
        let style = match origin {
            MenuOrigin::Click(_) => ContextMenuStyle::List,
            MenuOrigin::Hold => ContextMenuStyle::Pill,
        };

        *self.menu_actions.borrow_mut() = actions.clone();
        self.menu_origin.set(Some(origin));

        // The menu cannot reach back into the element — it outlives no part of
        // it and holds no reference to it — so a choice is recorded here and
        // applied by `on_event` the moment `handle_event` returns.
        let chosen = Rc::clone(&self.chosen_action);
        let rows = Rc::clone(&self.menu_actions);
        self.menu.show(
            ContextMenuRequest::new()
                .style(style)
                .at(anchor)
                .items(items(&actions))
                .on_select(move |index| {
                    chosen.set(rows.borrow().get(index).copied());
                }),
        );
    }

    /// Runs a verb chosen from the menu.
    ///
    /// Every verb but `Select All` finishes the job and closes the menu.
    /// `Select All` only reshapes what the menu acts on, so the menu is offered
    /// again in the same place — now with `Cut` and `Copy` in it, which is what
    /// the user reached for it to do.
    pub(super) fn run_menu_action(&self, action: FieldAction) {
        match action {
            FieldAction::Cut if !self.read_only => {
                if let Some((start, end)) = self.cursor.selection_range() {
                    let removed = self.controller.delete_range(start, end);
                    clipboard_write(&removed);
                    self.cursor.set_offset(start);
                    self.cursor.clear_selection();
                    self.on_changed.call(self.controller.text());
                }
                self.menu.hide();
            }
            FieldAction::Copy => {
                if let Some((start, end)) = self.cursor.selection_range() {
                    clipboard_write(&self.controller.get_range(start, end));
                }
                self.menu.hide();
            }
            FieldAction::Paste if !self.read_only => {
                if let Some(text) = clipboard_read() {
                    self.insert_text(&text);
                }
                self.menu.hide();
            }
            FieldAction::SelectAll => {
                self.cursor.set_selection_anchor(Some(0));
                self.cursor.set_offset(self.controller.grapheme_count());
                if let Some(origin) = self.menu_origin.get() {
                    self.request_menu(origin);
                } else {
                    self.menu.hide();
                }
            }
            _ => self.menu.hide(),
        }
        self.cursor.reset_blink();
    }

    /// Closes the menu and forgets any press that was about to raise one.
    ///
    /// Typing, losing focus or a cancelled gesture all invalidate a menu built
    /// around a selection that no longer means what it did.
    pub(super) fn dismiss_menu(&self) {
        self.pending_menu.set(None);
        self.menu_origin.set(None);
        self.touch_hold.forget();
        self.menu.hide();
    }

    /// Selects the word under a deferred click, resolved by the next frame.
    ///
    /// Reuses the double-click path: the offset under a position is only
    /// reachable with a canvas, and `draw` already resolves one that way.
    pub(super) fn select_word_under(&self, pos: Vec2d) {
        self.pending_click.set(Some(pos));
        self.click_count.set(2);
    }

}
