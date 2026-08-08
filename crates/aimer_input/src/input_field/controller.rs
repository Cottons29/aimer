use crate::TextEditingController;

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
pub(crate) struct TextFieldController {
    inner: TextEditingController,
}

impl Clone for TextFieldController {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl From<TextFieldController> for TextEditingController {
    fn from(controller: TextFieldController) -> Self {
        controller.inner
    }
}

impl Default for TextFieldController {
    fn default() -> Self {
        Self::new()
    }
}

impl TextFieldController {
    /// Creates a new instance of the TextFieldController with empty text.
    pub fn new() -> Self {
        Self {
            inner: TextEditingController::new(),
        }
    }

    /// Creates a new instance with the given initial text.
    pub fn with_initial(text: impl Into<String>) -> Self {
        Self {
            inner: TextEditingController::with_text(text),
        }
    }

    /// Returns a shared reference to the text stored within the current
    /// instance.
    pub fn text(&self) -> String {
        self.inner.text()
    }

    /// Consumes the content of the `text` field, returning its value while also
    /// clearing it.
    pub fn take(&self) -> String {
        self.inner.take()
    }

    /// Sets the text content of the object.
    pub fn set_text(&self, text: String) {
        self.inner.set_text(text);
    }


    /// Inserts a single character at the given grapheme offset.
    ///
    /// # Safety
    /// Be careful about the index out of bounds or invalid utf-8 char.
    pub unsafe fn insert_char(&self, ch: impl Into<char>, offset: usize) {
        unsafe { self.inner.insert_char(ch, offset) };
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
        self.inner.delete_grapheme(offset)
    }

    /// Clears the internal text buffer.
    pub fn clear(&self) {
        self.inner.clear();
    }

    /// Returns the number of grapheme clusters in the text.
    ///
    /// This is the length of the text in the same unit its offsets use, so it
    /// doubles as the last valid cursor position.
    pub fn grapheme_count(&self) -> usize {
        self.inner.grapheme_count()
    }

    /// Returns the substring between two grapheme offsets.
    ///
    /// An inverted range yields an empty string rather than panicking.
    pub fn get_range(&self, start: usize, end: usize) -> String {
        self.inner.get_range(start, end)
    }

    /// Deletes the grapheme clusters in the range `[start, end)` and returns
    /// the removed text.
    pub fn delete_range(&self, start: usize, end: usize) -> String {
        self.inner.delete_range(start, end)
    }

    /// Inserts a string at the given grapheme offset.
    pub fn insert_str(&self, text: &str, offset: usize) {
        self.inner.insert_str(text, offset);
    }

    // ── Undo / Redo ──────────────────────────────────────────────────


    /// Revert to the previous text state. Returns `true` if an undo was
    /// performed.
    pub fn undo(&self) -> bool {
        self.inner.undo()
    }

    /// Re-apply a previously undone text state. Returns `true` if a redo was
    /// performed.
    pub fn redo(&self) -> bool {
        self.inner.redo()
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
