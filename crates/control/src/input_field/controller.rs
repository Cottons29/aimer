use std::cell::UnsafeCell;
use std::sync::Arc;
use utils::debug;
///
/// A controller for managing and interacting with a text field's content.
///
/// `TextFieldController` provides a mechanism to safely share
/// and mutate a text field's state across different parts of an application.
/// It provides interior mutability to modify the text even
/// when the `TextFieldController` instance is immutable.
///
/// # Example
/// ```rust
/// use std::sync::Arc;
/// use std::cell::UnsafeCell;
/// use control::input::TextFieldController;
/// use crate::TextFieldController;
///
/// let controller = TextFieldController::with_initial("Initial text");
///
/// assert_eq!(controller.text(), "Initial text");
/// ```
pub struct TextFieldController {
    text: Arc<UnsafeCell<String>>,
}

unsafe impl Send for TextFieldController {}
unsafe impl Sync for TextFieldController {}

impl Clone for TextFieldController {
    fn clone(&self) -> Self {
        Self { text: self.text.clone() }
    }
}

// impl Drop for TextFieldController {
//     fn drop(&mut self) {
//         debug!("Dropping TextFieldController: {:?}", self.text);
//     }
// }

impl TextFieldController {
    ///
    /// Creates a new instance of the TextFieldController.
    ///
    /// # Returns
    ///
    /// A new instance initialized with an empty `String`, wrapped in an atomic reference counter (`Arc`)
    /// and an unsafe cell (`UnsafeCell`). This combination allows for potential interior mutability
    /// while maintaining thread safety under certain conditions.
    ///
    /// # Example
    ///
    /// ```rust
    ///
    /// use control::input::TextFieldController;
    ///
    /// let controller = TextFieldController::new();
    /// ```
    ///
    pub fn new() -> Self {
        Self { text: Arc::new(UnsafeCell::new(String::new())) }
    }


    ///
    /// Creates a new instance of the struct with the given initial text.
    ///
    /// # Parameters
    /// - `text`: A value that can be converted into a `String`. This will be used as the initial value
    ///   for the `text` field of the struct.
    ///
    /// # Returns
    /// A new instance of the struct, where the `text` field is initialized with the provided input,
    /// stored in an `Arc` and wrapped in an `UnsafeCell` for potential interior mutability use cases.
    ///
    /// # Example
    /// ```
    /// use control::input::TextFieldController;
    ///
    /// let instance = TextFieldController::with_initial("Hello, world!");
    ///
    /// assert_eq!(instance.text(), "Hello, world!");
    /// ```
    ///
    pub fn with_initial(text: impl Into<String>) -> Self {
        Self { text: Arc::new(UnsafeCell::new(text.into())) }
    }

    ///
    /// Returns a shared reference to the text stored within the current instance.
    ///
    /// # Safety
    /// This method uses an unsafe block to dereference a raw pointer (`self.text.get()`)
    /// to get the string slice. The caller must ensure that:
    /// - The underlying `self.text` is valid and properly initialized.
    /// - No mutable references to `self.text` exist when calling this method, to prevent
    ///   undefined behavior due to aliasing violations.
    ///
    /// # Returns
    /// - A shared reference to an immutable string slice (`&str`) representing the text.
    ///
    /// # Examples
    /// ```
    /// use control::input::TextFieldController;
    /// let instance = TextFieldController::new("example text".to_string());
    /// let text_ref: &str = instance.text();
    /// assert_eq!(text_ref, "example text");
    /// ```
    pub fn text(&self) -> &str {
        unsafe { &*self.text.get() }
    }

    ///
    /// Consumes the content of the `text` field, returning its value while also clearing it.
    ///
    /// This method clones the current value of `text`, clears the original value,
    /// and then returns the cloned value. It is useful when you need to extract the
    /// content of the field without leaving stale data behind.
    ///
    /// # Returns
    ///
    /// A `String` containing the value of the `text` field before it was cleared.
    ///
    /// # Example
    ///
    /// ```
    /// use control::input::TextFieldController;
    /// let mut my_struct = TextFieldController::new("Hello, world!".to_string());
    /// let text = my_struct.take();
    /// assert_eq!(text, "Hello, world!");
    /// assert_eq!(my_struct.text(), "");
    /// ```
    ///
    pub fn take(&self) -> String {
        let s = unsafe { self.text_mut() };
        let t = s.clone();
        s.clear();
        t
    }

    ///
    /// Provides mutable access to the `text` field of the current object.
    ///
    /// # Safety
    /// This function uses an `unsafe` block to cast a raw pointer obtained
    /// from `self.text.get()` to a mutable reference. This is inherently
    /// unsafe because it assumes that the caller ensures this method
    /// is not invoked simultaneously with any other code accessing
    /// `self.text` (either mutably or immutably). Violating this assumption
    /// can lead to data races or undefined behavior.
    ///
    /// Ensure proper synchronization or single-threaded usage when calling
    /// this function to avoid unsafe behavior.
    ///
    /// # Returns
    /// * `&mut String` - A mutable reference to the `text` field of the object.
    ///
    /// # Examples
    /// ```rust
    /// use control::input::TextFieldController;
    /// let my_object = TextFieldController::new();
    /// let text = my_object.text_mut();
    /// text.push_str("Hello, world!");
    /// assert_eq!(my_object.text(), "Hello, world!")
    /// ```
    ///
    pub unsafe fn text_mut(&self) -> &mut String {
        unsafe { &mut *self.text.get() }
    }

    ///
    /// Sets the text content of the object.
    ///
    /// This function updates the internal text value of the object with the provided `text`
    /// argument. The method uses a mutable reference to the text field obtained via
    /// the `text_mut` method.
    ///
    /// # Parameters
    /// - `text`: A `String` containing the new text value to set.
    ///
    /// # Example
    /// ```rust
    /// use control::input::TextFieldController;
    ///
    /// let obj = TextFieldController::new();
    /// obj.set_text("Hello, World!".to_string());
    /// assert_eq!(obj.text(), "Hello, World!");
    /// ```
    ///
    /// # Notes
    /// - Ensure the object is mutable when calling this method to avoid runtime errors.
    /// - It's assumed that `text_mut` provides mutable access to the underlying text field.
    ///
    pub fn set_text(&self, text: String) {
        unsafe {
            *self.text_mut() = text;
        }
    }

    ///
    /// Inserts a single character at a specified character offset within the text.
    ///
    /// # Parameters
    /// - `ch`: The character to insert.
    /// - `offset`: The character offset at which to insert the given character. If the
    ///   offset is out of bounds (greater than the number of characters in the text),
    ///   the character will be appended at the end.
    ///
    /// # Behavior
    /// - Converts the character offset into a byte offset, as Rust strings are UTF-8 encoded.
    /// - If the specified offset corresponds to a valid character boundary within the string,
    ///   the character is inserted at that position.
    /// - If the specified offset exceeds the total number of characters in the string,
    ///   the character is appended at the end.
    ///
    /// # Panics
    /// - This function will panic if the offset is out of bounds or invalid utf-8 char.
    ///
    /// # Examples
    /// ```
    /// use control::input::TextFieldController;
    /// let mut editor = TextFieldController::with_initial("hello");
    /// unsafe {editor.insert_char('!', 5);}
    /// {assert_eq!(editor.text(), "hello!");}
    ///
    /// unsafe {editor.insert_char(' ', 0);}
    /// assert_eq!(editor.text(), " hello!");
    ///
    /// unsafe {editor.insert_char('🌟', 2);}
    /// assert_eq!(editor.text(), " h🌟ello!");
    /// ```
    ///
    pub unsafe fn insert_char(&self, ch: impl Into<char>, offset: usize) {
        let s = unsafe { self.text_mut() };
        let byte_offset = s
            .char_indices()
            .nth(offset)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        s.insert(byte_offset, ch.into());
    }

    ///
    /// Deletes a character from the text at the specified character offset.
    ///
    /// # Arguments
    ///
    /// * `offset` - The character index (0-based) at which the character should be removed.
    ///              This is based on character (not byte) indices in the string.
    ///
    /// # Behavior
    ///
    /// This function removes the character located at the specified `offset` within the
    /// mutable string. It uses character boundaries to compute the correct byte index for
    /// removal. If the `offset` is out of bounds (i.e., greater than or equal to the number
    /// of characters in the string), the function does nothing.
    ///
    /// # Panics
    ///
    /// This function will panic if the string is updated concurrently or if called in an
    /// improper state where mutable access to the underlying string is not allowed.
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// use control::input::TextFieldController;
    ///
    /// let mut my_struct = TextFieldController::new();
    /// unsafe {my_struct.delete_char(1)};
    /// assert_eq!(my_struct.text(), "hllo"); // Removes 'e' at position 1
    /// ```
    ///
    /// # Note
    /// - This function assumes UTF-8 encoding and handles multi-byte characters correctly.
    /// - Ensure that the `offset` accurately represents a valid character position within
    ///   the string to avoid unexpected behavior.
    ///
    /// # See Also
    /// - [`char_indices`](https://doc.rust-lang.org/std/primitive.str.html#method.char_indices)
    /// - [`remove`](https://doc.rust-lang.org/std/string/struct.String.html#method.remove)
    ///
    pub fn delete_char(&self, offset: usize) {
        let s = unsafe { self.text_mut() };
        if let Some((byte_offset, _ch)) = s.char_indices().nth(offset) {
            s.remove(byte_offset);
        }
    }

    ///
    /// Clears the internal text buffer associated with the object.
    ///
    /// # Safety
    /// This method performs an unsafe operation by mutably accessing the internal
    /// text buffer. It relies on the assumption that the internal state of the object
    /// is managed correctly and that no undefined behavior will occur due to
    /// concurrent or improper access.
    ///
    /// # Examples
    /// ```
    /// use control::input::TextFieldController;
    ///
    /// let example = TextFieldController::with_initial("Hello, world!");
    /// example.clear(); // Clears the internal text buffer
    ///
    /// assert_eq!(example.text(), "");
    /// ```
    ///
    /// # Notes
    /// - Ensure proper synchronization or exclusive ownership when calling this
    ///   method in a multi-threaded context, as it is not thread-safe.
    /// - Use this method cautiously, as improperly using unsafe code can lead to
    ///   undefined behavior.
    ///
    pub fn clear(&self) {
        unsafe {
            self.text_mut().clear();
        }
    }

    ///
    /// Returns the number of characters in the text.
    ///
    /// This method counts the total number of Unicode scalar values (characters)
    /// in the text associated with the current instance. It excludes any specific
    /// handling for grapheme clusters, so combined characters or emoji sequences
    /// are treated as individual scalar values.
    ///
    /// # Returns
    ///
    /// A `usize` representing the total number of characters.
    ///
    /// # Example
    ///
    /// ```rust
    /// use control::input::TextFieldController;
    /// let example = TextFieldController::with_initial("Hello, world!");
    /// assert_eq!(example.char_count(), 13);
    /// ```
    ///
    pub fn char_count(&self) -> usize {
        self.text().chars().count()
    }
}
