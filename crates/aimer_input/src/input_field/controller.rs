use std::cell::UnsafeCell;
use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;

/// A controller for managing and interacting with a text field's content.
///
/// `TextFieldController` provides a mechanism to safely share
/// and mutate a text field's state across different parts of an application.
/// It provides interior mutability to modify the text even
/// when the `TextFieldController` instance is immutable.
///
/// Includes an undo/redo stack so every mutation can be reversed.
///
/// # Offsets
///
/// Every offset in this API counts **grapheme clusters** — what a reader calls
/// a character — not `char`s and not bytes. A cluster can span several code
/// points: `"👨‍👩‍👧"` is five `char`s joined by zero-width joiners and `"é"` may be
/// `e` plus a combining accent, yet each is a single offset step. This is the
/// same unit the field uses to measure and draw text, so the caret always
/// lands where the edit does.
///
/// ```rust
/// use aimer_input::input::TextFieldController;
///
/// let controller = TextFieldController::with_initial("👨‍👩‍👧b");
/// assert_eq!(controller.grapheme_count(), 2);
/// controller.insert_str("a", 1);
/// assert_eq!(controller.text(), "👨‍👩‍👧ab");
/// ```
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
        Self {
            text: self.text.clone(),
            undo_stack: self.undo_stack.clone(),
            redo_stack: self.redo_stack.clone(),
        }
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

    /// Returns a shared reference to the text stored within the current
    /// instance.
    pub fn text(&self) -> &str {
        unsafe { &*self.text.get() }
    }

    /// Consumes the content of the `text` field, returning its value while also
    /// clearing it.
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
    /// The rendering pipeline is single-threaded, so concurrent access does not
    /// occur.
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

    /// Resolves a grapheme offset into a byte offset inside `text`.
    ///
    /// Offsets past the last cluster clamp to the end of the text, so "one past
    /// the last grapheme" names the insertion point at the end. The result is
    /// always a `char` boundary, which is what makes the slicing below sound.
    fn byte_offset(text: &str, grapheme_offset: usize) -> usize {
        text.grapheme_indices(true)
            .nth(grapheme_offset)
            .map(|(byte, _)| byte)
            .unwrap_or(text.len())
    }

    /// Inserts a single character at the given grapheme offset.
    ///
    /// # Safety
    /// Be careful about the index out of bounds or invalid utf-8 char.
    pub unsafe fn insert_char(&self, ch: impl Into<char>, offset: usize) {
        self.save_undo();
        let s = unsafe { self.text_mut() };
        let byte_offset = Self::byte_offset(s, offset);
        s.insert(byte_offset, ch.into());
    }

    /// Deletes the grapheme cluster at `offset` and returns the removed text.
    ///
    /// A whole cluster goes at once, so one backspace removes an entire family
    /// emoji or an accented letter together with its combining mark instead of
    /// leaving a mangled remainder behind. An offset past the end removes
    /// nothing and returns an empty string.
    ///
    /// # Example
    /// ```rust
    /// use aimer_input::input::TextFieldController;
    ///
    /// let controller = TextFieldController::with_initial("a👨‍👩‍👧b");
    /// assert_eq!(controller.delete_grapheme(1), "👨‍👩‍👧");
    /// assert_eq!(controller.text(), "ab");
    /// ```
    pub fn delete_grapheme(&self, offset: usize) -> String {
        self.delete_range(offset, offset + 1)
    }

    /// Clears the internal text buffer.
    pub fn clear(&self) {
        self.save_undo();
        unsafe {
            self.text_mut().clear();
        }
    }

    /// Returns the number of grapheme clusters in the text.
    ///
    /// This is the length of the text in the same unit its offsets use, so it
    /// doubles as the last valid cursor position.
    pub fn grapheme_count(&self) -> usize {
        self.text().graphemes(true).count()
    }

    /// Returns the substring between two grapheme offsets.
    ///
    /// An inverted range yields an empty string rather than panicking.
    pub fn get_range(&self, start: usize, end: usize) -> String {
        let text = self.text();
        let byte_start = Self::byte_offset(text, start);
        let byte_end = Self::byte_offset(text, start.max(end));
        text[byte_start..byte_end].to_owned()
    }

    /// Deletes the grapheme clusters in the range `[start, end)` and returns
    /// the removed text.
    pub fn delete_range(&self, start: usize, end: usize) -> String {
        self.save_undo();
        let s = unsafe { self.text_mut() };
        let byte_start = Self::byte_offset(s, start);
        let byte_end = Self::byte_offset(s, start.max(end));
        s.drain(byte_start..byte_end).collect()
    }

    /// Inserts a string at the given grapheme offset.
    pub fn insert_str(&self, text: &str, offset: usize) {
        self.save_undo();
        let s = unsafe { self.text_mut() };
        let byte_offset = Self::byte_offset(s, offset);
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
        if undo.last() != Some(&current) {
            undo.push(current);
            // Cap undo stack size
            if undo.len() > Self::MAX_UNDO_DEPTH {
                undo.remove(0);
            }
        }
        // Any new mutation invalidates the redo history
        unsafe { &mut *self.redo_stack.get() }.clear();
    }

    /// Revert to the previous text state. Returns `true` if an undo was
    /// performed.
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

    /// Re-apply a previously undone text state. Returns `true` if a redo was
    /// performed.
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
        assert_eq!(c.grapheme_count(), 0);
    }

    #[test]
    fn test_with_initial() {
        let c = TextFieldController::with_initial("hello");
        assert_eq!(c.text(), "hello");
        assert_eq!(c.grapheme_count(), 5);
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
    fn test_delete_grapheme() {
        let c = TextFieldController::with_initial("hello");
        c.delete_grapheme(1); // remove 'e'
        assert_eq!(c.text(), "hllo");
    }

    #[test]
    fn test_grapheme_count_unicode() {
        let c = TextFieldController::with_initial("he🌟lo");
        assert_eq!(c.grapheme_count(), 5);
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

    // ── Grapheme offsets ────────────────────────────────────────────

    /// A single family emoji: three code points joined by zero-width joiners,
    /// five `char`s and eighteen bytes, but one grapheme cluster.
    const FAMILY: &str = "👨‍👩‍👧";

    /// `e` followed by a combining acute accent — two code points, one
    /// grapheme cluster.
    const COMBINING_E: &str = "e\u{301}";

    #[test]
    fn test_grapheme_count_counts_clusters_not_chars() {
        assert_eq!(TextFieldController::with_initial(FAMILY).grapheme_count(), 1);
        assert_eq!(
            TextFieldController::with_initial(COMBINING_E).grapheme_count(),
            1
        );
        // Six code points, three extended clusters: the virama binds "स्ते"
        // together.
        assert_eq!(
            TextFieldController::with_initial("नमस्ते").grapheme_count(),
            3
        );
        assert_eq!(
            TextFieldController::with_initial(format!("{FAMILY}a")).grapheme_count(),
            2
        );
    }

    #[test]
    fn test_insert_str_at_grapheme_offset() {
        let c = TextFieldController::with_initial(format!("{FAMILY}b"));
        c.insert_str("a", 1);
        assert_eq!(c.text(), format!("{FAMILY}ab"));
    }

    #[test]
    fn test_insert_str_past_the_end_appends() {
        let c = TextFieldController::with_initial(FAMILY);
        c.insert_str("!", 99);
        assert_eq!(c.text(), format!("{FAMILY}!"));
    }

    #[test]
    fn test_insert_char_at_grapheme_offset() {
        let c = TextFieldController::with_initial(FAMILY);
        unsafe {
            c.insert_char('\n', 1);
        }
        assert_eq!(c.text(), format!("{FAMILY}\n"));
    }

    #[test]
    fn test_delete_grapheme_removes_whole_cluster() {
        let c = TextFieldController::with_initial(format!("a{FAMILY}b"));
        let removed = c.delete_grapheme(1);
        assert_eq!(removed, FAMILY);
        assert_eq!(c.text(), "ab");
    }

    #[test]
    fn test_delete_grapheme_removes_combining_mark_with_its_base() {
        let c = TextFieldController::with_initial(format!("{COMBINING_E}x"));
        c.delete_grapheme(0);
        assert_eq!(c.text(), "x");
    }

    #[test]
    fn test_delete_grapheme_out_of_bounds() {
        let c = TextFieldController::with_initial("hi");
        assert_eq!(c.delete_grapheme(99), "");
        assert_eq!(c.text(), "hi");
    }

    #[test]
    fn test_delete_range_uses_grapheme_offsets() {
        let c = TextFieldController::with_initial(format!("a{FAMILY}b"));
        let removed = c.delete_range(1, 2);
        assert_eq!(removed, FAMILY);
        assert_eq!(c.text(), "ab");
    }

    #[test]
    fn test_get_range_uses_grapheme_offsets() {
        let c = TextFieldController::with_initial(format!("{COMBINING_E}x"));
        assert_eq!(c.get_range(0, 1), COMBINING_E);
        assert_eq!(c.get_range(1, 2), "x");
    }

    #[test]
    fn test_get_range_clamps_an_inverted_range() {
        let c = TextFieldController::with_initial("hello");
        assert_eq!(c.get_range(4, 1), "");
    }

    #[test]
    fn test_undo_delete_grapheme() {
        let c = TextFieldController::with_initial(format!("a{FAMILY}"));
        c.delete_grapheme(1);
        assert_eq!(c.text(), "a");
        assert!(c.undo());
        assert_eq!(c.text(), format!("a{FAMILY}"));
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
