use std::cell::UnsafeCell;
use std::sync::Arc;

/// A controller for managing and interacting with a text field's content.
///
/// `TextFieldController` provides a mechanism to safely share
/// and mutate a text field's state across different parts of an application.
/// It provides interior mutability to modify the text even
/// when the `TextFieldController` instance is immutable.
///
/// Includes an undo/redo stack so every mutation can be reversed.
///
/// # Example
/// ```rust
/// use aimer_input::input::TextFieldController;
///
/// let controller = TextFieldController::with_initial("Initial text");
/// assert_eq!(controller.text(), "Initial text");
/// ```
pub struct TextFieldController {
    text: Arc<UnsafeCell<String>>,
    undo_stack: Arc<UnsafeCell<Vec<String>>>,
    redo_stack: Arc<UnsafeCell<Vec<String>>>,
}

unsafe impl Send for TextFieldController {}
unsafe impl Sync for TextFieldController {}

impl Clone for TextFieldController {
    fn clone(&self) -> Self {
        Self { text: self.text.clone(), undo_stack: self.undo_stack.clone(), redo_stack: self.redo_stack.clone() }
    }
}

impl Default for TextFieldController {
    fn default() -> Self {
        Self::new()
    }
}

impl TextFieldController {
    /// Creates a new instance of the TextFieldController with empty text.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn new() -> Self {
        Self {
            text: Arc::new(UnsafeCell::new(String::new())),
            undo_stack: Arc::new(UnsafeCell::new(Vec::new())),
            redo_stack: Arc::new(UnsafeCell::new(Vec::new())),
        }
    }

    /// Creates a new instance with the given initial text.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn with_initial(text: impl Into<String>) -> Self {
        Self {
            text: Arc::new(UnsafeCell::new(text.into())),
            undo_stack: Arc::new(UnsafeCell::new(Vec::new())),
            redo_stack: Arc::new(UnsafeCell::new(Vec::new())),
        }
    }

    /// Returns a shared reference to the text stored within the current instance.
    pub fn text(&self) -> &str {
        unsafe { &*self.text.get() }
    }

    /// Consumes the content of the `text` field, returning its value while also clearing it.
    pub fn take(&self) -> String {
        self.save_undo();
        let s = unsafe { self.text_mut() };
        let t = s.clone();
        s.clear();
        t
    }

    /// Provides mutable access to the `text` field of the current object.
    ///
    /// # Safety
    /// The rendering pipeline is single-threaded, so concurrent access does not occur.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn text_mut(&self) -> &mut String {
        unsafe { &mut *self.text.get() }
    }

    /// Sets the text content of the object.
    pub fn set_text(&self, text: String) {
        self.save_undo();
        unsafe {
            *self.text_mut() = text;
        }
    }

    /// Inserts a single character at a specified character offset within the text.
    ///
    /// # Safety
    /// Be careful about the index out of bounds or invalid utf-8 char.
    pub unsafe fn insert_char(&self, ch: impl Into<char>, offset: usize) {
        self.save_undo();
        let s = unsafe { self.text_mut() };
        let byte_offset = s.char_indices().nth(offset).map(|(i, _)| i).unwrap_or(s.len());
        s.insert(byte_offset, ch.into());
    }

    /// Deletes a character from the text at the specified character offset.
    pub fn delete_char(&self, offset: usize) {
        self.save_undo();
        let s = unsafe { self.text_mut() };
        if let Some((byte_offset, _ch)) = s.char_indices().nth(offset) {
            s.remove(byte_offset);
        }
    }

    /// Clears the internal text buffer.
    pub fn clear(&self) {
        self.save_undo();
        unsafe {
            self.text_mut().clear();
        }
    }

    /// Returns the number of characters in the text.
    pub fn char_count(&self) -> usize {
        self.text().chars().count()
    }

    /// Returns the substring between two character offsets.
    pub fn get_range(&self, start: usize, end: usize) -> String {
        self.text().chars().skip(start).take(end.saturating_sub(start)).collect()
    }

    /// Deletes characters in the range `[start, end)` and returns the removed text.
    pub fn delete_range(&self, start: usize, end: usize) -> String {
        self.save_undo();
        let removed = self.get_range(start, end);
        let s = unsafe { self.text_mut() };
        let byte_start = s.char_indices().nth(start).map(|(i, _)| i).unwrap_or(s.len());
        let byte_end = s.char_indices().nth(end).map(|(i, _)| i).unwrap_or(s.len());
        s.drain(byte_start..byte_end);
        removed
    }

    /// Inserts a string at the given character offset.
    pub fn insert_str(&self, text: &str, offset: usize) {
        self.save_undo();
        let s = unsafe { self.text_mut() };
        let byte_offset = s.char_indices().nth(offset).map(|(i, _)| i).unwrap_or(s.len());
        s.insert_str(byte_offset, text);
    }

    // ── Undo / Redo ──────────────────────────────────────────────────

    /// Maximum number of undo snapshots retained.
    const MAX_UNDO_DEPTH: usize = 200;

    /// Snapshot the current text onto the undo stack and clear the redo stack.
    /// Called automatically before every mutation.
    fn save_undo(&self) {
        let current = self.text().to_owned();
        let undo = unsafe { &mut *self.undo_stack.get() };
        // Avoid pushing duplicate snapshots back-to-back
        if undo.last().map_or(true, |last| last != &current) {
            undo.push(current);
            // Cap undo stack size
            if undo.len() > Self::MAX_UNDO_DEPTH {
                undo.remove(0);
            }
        }
        // Any new mutation invalidates the redo history
        unsafe { &mut *self.redo_stack.get() }.clear();
    }

    /// Revert to the previous text state. Returns `true` if an undo was performed.
    pub fn undo(&self) -> bool {
        let undo = unsafe { &mut *self.undo_stack.get() };
        if let Some(prev) = undo.pop() {
            let current = self.text().to_owned();
            let redo = unsafe { &mut *self.redo_stack.get() };
            redo.push(current);
            unsafe { *self.text_mut() = prev };
            true
        } else {
            false
        }
    }

    /// Re-apply a previously undone text state. Returns `true` if a redo was performed.
    pub fn redo(&self) -> bool {
        let redo = unsafe { &mut *self.redo_stack.get() };
        if let Some(next) = redo.pop() {
            let current = self.text().to_owned();
            let undo = unsafe { &mut *self.undo_stack.get() };
            undo.push(current);
            unsafe { *self.text_mut() = next };
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let c = TextFieldController::new();
        assert_eq!(c.text(), "");
        assert_eq!(c.char_count(), 0);
    }

    #[test]
    fn test_with_initial() {
        let c = TextFieldController::with_initial("hello");
        assert_eq!(c.text(), "hello");
        assert_eq!(c.char_count(), 5);
    }

    #[test]
    fn test_set_text() {
        let c = TextFieldController::new();
        c.set_text("world".to_string());
        assert_eq!(c.text(), "world");
    }

    #[test]
    fn test_insert_char_ascii() {
        let c = TextFieldController::with_initial("hello");
        unsafe {
            c.insert_char('!', 5);
        }
        assert_eq!(c.text(), "hello!");
    }

    #[test]
    fn test_insert_char_middle() {
        let c = TextFieldController::with_initial("hlo");
        unsafe {
            c.insert_char('e', 1);
        }
        assert_eq!(c.text(), "helo");
    }

    #[test]
    fn test_insert_char_unicode() {
        let c = TextFieldController::with_initial("helo");
        unsafe {
            c.insert_char('🌟', 2);
        }
        assert_eq!(c.text(), "he🌟lo");
    }

    #[test]
    fn test_delete_char() {
        let c = TextFieldController::with_initial("hello");
        c.delete_char(1); // remove 'e'
        assert_eq!(c.text(), "hllo");
    }

    #[test]
    fn test_delete_char_out_of_bounds() {
        let c = TextFieldController::with_initial("hi");
        c.delete_char(99); // no-op
        assert_eq!(c.text(), "hi");
    }

    #[test]
    fn test_char_count_unicode() {
        let c = TextFieldController::with_initial("he🌟lo");
        assert_eq!(c.char_count(), 5);
    }

    #[test]
    fn test_get_range() {
        let c = TextFieldController::with_initial("hello world");
        assert_eq!(c.get_range(0, 5), "hello");
        assert_eq!(c.get_range(6, 11), "world");
    }

    #[test]
    fn test_delete_range() {
        let c = TextFieldController::with_initial("hello world");
        let removed = c.delete_range(5, 11);
        assert_eq!(removed, " world");
        assert_eq!(c.text(), "hello");
    }

    #[test]
    fn test_insert_str() {
        let c = TextFieldController::with_initial("hlo");
        c.insert_str("el", 1);
        assert_eq!(c.text(), "hello");
    }

    #[test]
    fn test_insert_str_unicode() {
        let c = TextFieldController::with_initial("hlo");
        c.insert_str("é🌟", 1);
        assert_eq!(c.text(), "hé🌟lo");
    }

    #[test]
    fn test_take() {
        let c = TextFieldController::with_initial("hello");
        let taken = c.take();
        assert_eq!(taken, "hello");
        assert_eq!(c.text(), "");
    }

    #[test]
    fn test_clear() {
        let c = TextFieldController::with_initial("hello");
        c.clear();
        assert_eq!(c.text(), "");
    }

    #[test]
    fn test_clone_shares_state() {
        let c1 = TextFieldController::with_initial("shared");
        let c2 = c1.clone();
        c2.set_text("modified".to_string());
        assert_eq!(c1.text(), "modified");
    }

    #[test]
    fn test_undo_basic() {
        let c = TextFieldController::with_initial("hello");
        c.set_text("world".to_string());
        assert_eq!(c.text(), "world");
        assert!(c.undo());
        assert_eq!(c.text(), "hello");
    }

    #[test]
    fn test_undo_empty_stack() {
        let c = TextFieldController::new();
        assert!(!c.undo());
        assert_eq!(c.text(), "");
    }

    #[test]
    fn test_redo_basic() {
        let c = TextFieldController::with_initial("hello");
        c.set_text("world".to_string());
        assert!(c.undo());
        assert_eq!(c.text(), "hello");
        assert!(c.redo());
        assert_eq!(c.text(), "world");
    }

    #[test]
    fn test_redo_empty_stack() {
        let c = TextFieldController::new();
        assert!(!c.redo());
    }

    #[test]
    fn test_undo_insert_char() {
        let c = TextFieldController::with_initial("hl");
        unsafe {
            c.insert_char('e', 1);
        }
        assert_eq!(c.text(), "hel");
        assert!(c.undo());
        assert_eq!(c.text(), "hl");
    }

    #[test]
    fn test_undo_delete_range() {
        let c = TextFieldController::with_initial("hello world");
        c.delete_range(5, 11);
        assert_eq!(c.text(), "hello");
        assert!(c.undo());
        assert_eq!(c.text(), "hello world");
    }

    #[test]
    fn test_new_mutation_invalidates_redo() {
        let c = TextFieldController::with_initial("a");
        c.set_text("b".to_string());
        c.set_text("c".to_string());
        assert!(c.undo()); // back to "b"
        assert!(c.undo()); // back to "a"
        c.set_text("d".to_string()); // new edit — redo stack should clear
        assert!(!c.redo()); // nothing to redo
        assert_eq!(c.text(), "d");
    }

    #[test]
    fn test_undo_multiple_steps() {
        let c = TextFieldController::new();
        c.set_text("a".to_string());
        c.set_text("ab".to_string());
        c.set_text("abc".to_string());
        assert!(c.undo());
        assert_eq!(c.text(), "ab");
        assert!(c.undo());
        assert_eq!(c.text(), "a");
        assert!(c.undo());
        assert_eq!(c.text(), "");
        assert!(!c.undo()); // nothing left
    }
}
