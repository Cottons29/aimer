use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

/// A text payload that is either a `'static` literal or reference-counted,
/// owned string data.
///
/// [`Text`](crate::Text) and [`RawTextWidget`](crate::RawTextWidget) rebuild
/// every frame, so the common case — a label built from a string literal —
/// must not pay an allocation on every rebuild. `Rc<str>` cannot help here:
/// constructing one from a `&str` or `String` always allocates and copies,
/// because its layout places the strong/weak counters next to the string
/// bytes in one block. `TextSource` instead keeps the `&'static str` case as a
/// bare pointer, and only falls back to an `Rc<str>` for data that is
/// genuinely owned or shared.
///
/// # Example
///
/// ```
/// use aimer_text::Text;
///
/// // No allocation: the literal is stored as a `&'static str`.
/// let label = Text::new("Aimer");
///
/// // One allocation, same as constructing an `Rc<str>` directly.
/// let counter = Text::new(format!("Count: {}", 3));
/// ```
#[derive(Clone)]
pub enum TextSource {
    /// A string literal or other data that lives for the entire program.
    ///
    /// Constructing this variant is free: it stores the pointer and length
    /// directly, with no allocation and no refcount.
    Static(&'static str),
    /// Owned string data shared through a reference count.
    ///
    /// Cloning this variant is a refcount bump. Constructing it from a
    /// `String` allocates once, exactly like building an `Rc<str>` directly.
    Shared(Rc<str>),
}

impl TextSource {
    /// Borrows the underlying string, regardless of which variant this is.
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            TextSource::Static(text) => text,
            TextSource::Shared(text) => text,
        }
    }

    /// Returns a reference-counted handle to this text.
    ///
    /// [`TextSource::Shared`] is cloned for free; [`TextSource::Static`]
    /// allocates once to produce the handle. Use this only at the boundary
    /// that actually needs an `Rc<str>` — for example, registering selectable
    /// text with a selection session — rather than on every rebuild.
    #[inline]
    pub fn to_rc(&self) -> Rc<str> {
        match self {
            TextSource::Static(text) => Rc::from(*text),
            TextSource::Shared(text) => Rc::clone(text),
        }
    }
}

impl Default for TextSource {
    #[inline]
    fn default() -> Self {
        TextSource::Static("")
    }
}

impl Deref for TextSource {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for TextSource {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for TextSource {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for TextSource {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl PartialEq for TextSource {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for TextSource {}

impl From<&'static str> for TextSource {
    #[inline]
    fn from(text: &'static str) -> Self {
        TextSource::Static(text)
    }
}

impl From<String> for TextSource {
    #[inline]
    fn from(text: String) -> Self {
        TextSource::Shared(text.into())
    }
}

impl From<Rc<str>> for TextSource {
    #[inline]
    fn from(text: Rc<str>) -> Self {
        TextSource::Shared(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_text_borrows_without_allocating_an_rc() {
        let source = TextSource::from("literal");
        assert!(matches!(source, TextSource::Static("literal")));
        assert_eq!(&*source, "literal");
    }

    #[test]
    fn owned_text_is_stored_as_shared() {
        let source = TextSource::from(String::from("owned"));
        assert!(matches!(source, TextSource::Shared(_)));
        assert_eq!(&*source, "owned");
    }

    #[test]
    fn to_rc_reuses_an_existing_allocation() {
        let rc: Rc<str> = Rc::from("shared");
        let source = TextSource::from(Rc::clone(&rc));
        assert!(Rc::ptr_eq(&rc, &source.to_rc()));
    }

    #[test]
    fn to_rc_allocates_once_for_static_text() {
        let source = TextSource::from("literal");
        let rc = source.to_rc();
        assert_eq!(&*rc, "literal");
    }

    #[test]
    fn equality_compares_by_content_across_variants() {
        assert_eq!(TextSource::from("same"), TextSource::from(String::from("same")));
    }
}
