//! One verb of a context menu.

use std::borrow::Cow;

/// A single row of a context menu.
///
/// An item is a label and whether it can be chosen — nothing else. What it
/// *does* is the caller's business, delivered as the item's index to the
/// callback given when the menu was opened, so the menu never has to know about
/// clipboards, selections or documents.
///
/// # Examples
///
/// ```
/// use aimer_ctxmenu::ContextMenuItem;
///
/// let copy = ContextMenuItem::new("Copy");
/// assert_eq!(copy.label(), "Copy");
/// assert!(copy.is_enabled());
///
/// let paste = ContextMenuItem::new("Paste").enabled(false);
/// assert!(!paste.is_enabled());
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextMenuItem {
    label: Cow<'static, str>,
    enabled: bool,
}

impl ContextMenuItem {
    /// Creates an enabled item labelled `label`.
    ///
    /// A `&'static str` is borrowed rather than copied, which is what every
    /// fixed verb — `Copy`, `Select All` — costs here: nothing.
    #[inline]
    pub fn new(label: impl Into<Cow<'static, str>>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
        }
    }

    /// Sets whether the item can be chosen.
    ///
    /// A disabled item is still drawn, dimmed, and still swallows the press
    /// that lands on it: a menu whose rows moved as they became available
    /// would be chosen wrongly.
    #[inline]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The label painted for this item.
    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Whether the item can be chosen.
    #[inline]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_is_enabled_until_it_is_told_otherwise() {
        assert!(ContextMenuItem::new("Copy").is_enabled());
        assert!(!ContextMenuItem::new("Copy").enabled(false).is_enabled());
    }

    #[test]
    fn an_owned_label_is_accepted_as_readily_as_a_static_one() {
        let dynamic = ContextMenuItem::new(format!("Search for {}", "rust"));

        assert_eq!(dynamic.label(), "Search for rust");
    }
}
