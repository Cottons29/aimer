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
}

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Key {
    /// Generate a globally unique key. Each call returns a key that does not
    /// equal any other key — useful for forcing a state reset on rebuild.
    pub fn unique() -> Self {
        Key::Object(UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed) as usize)
    }
}
