use std::sync::atomic::{AtomicU64, Ordering};

/// A stable identity token attached to a [`Widget`](crate::Widget) so the
/// reconciliation algorithm can match old elements to new ones across rebuilds.
///
/// # Variants
///
/// - [`Key::Value`] — equality-based. Two widgets with the same value are
///   considered "the same element". This is the most common key type and covers
///   list-item IDs, tab names, route paths, etc.
/// - [`Key::Object`] — identity-based. Matched by pointer/identity, not by
///   value. Use [`Key::unique()`] to generate a key that never matches anything
///   else (useful for forcing a full state reset).
///
/// # Example
///
/// ```rust,ignore
/// use aimer::Key;
///
/// Column::create_new(vec![
///     MyItem::create_new(Some(Key::Value("item-1".into())), ...),
///     MyItem::create_new(Some(Key::Value("item-2".into())), ...),
/// ])
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// Equality-based key. Two widgets with the same `Value` are "the same".
    Value(String),
    /// Identity-based key. Matched by `ptr::eq` semantics — two `Object` keys
    /// are equal only if they originate from the same [`Key::unique()`] call.
    Object(usize),

    /// Compile-time-based key. Matched by pointer/identity, not by value. use
    /// the `key!` macro to generate a key that can keep the state across
    /// the builds
    Static(&'static str),
}

/// Ergonomic conversions so callers can pass a bare string literal or `String`
/// wherever a `Key` is expected (e.g. `page_storage::read_or("my-tab", 0)`),
/// producing the common equality-based [`Key::Value`].
impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Key::Value(s.to_owned())
    }
}

impl From<String> for Key {
    fn from(s: String) -> Self {
        Key::Value(s)
    }
}

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Key {
    /// Generate a globally unique key. Each call returns a key that does not
    /// equal any other key — useful for forcing a state reset on rebuild.
    pub fn unique() -> Self {
        Key::Object(UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed) as usize)
    }
}

#[cfg(test)]
mod test {
    use aimer_macro::key;

    use super::*;
    #[test]
    fn test_unique() {
        let key1 = Key::unique();
        let key2 = Key::unique();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_compile_tile_unique() {
        let key1 = key!();
        let key2 = key!();
        assert_ne!(key1, key2);
    }
}
