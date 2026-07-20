use std::cell::UnsafeCell;

use aimer_attribute::BoxConstraint;
use aimer_attribute::size::ResolvedSize;

/// Caches the result of `computed_size` and `content_size` for a single frame.
/// The cache is keyed by `(BoxConstraint, scale)` so that if the same element
/// is queried multiple times with the same inputs, the result is returned
/// instantly. yeah it saves the CPU and GPU and reduces power consuming :))
pub struct LayoutCache {
    computed: UnsafeCell<Option<(BoxConstraint, u32, ResolvedSize)>>,
    content: UnsafeCell<Option<(BoxConstraint, u32, ResolvedSize)>>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self { computed: UnsafeCell::new(None), content: UnsafeCell::new(None) }
    }

    /// Returns cached computed_size if constraint and scale match, otherwise
    /// None.
    pub fn get_computed(&self, constraint: BoxConstraint, scale_bits: u32) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.computed.get() };
        match *guard {
            Some((c, s, size)) if c == constraint && s == scale_bits => Some(size),
            _ => None,
        }
    }

    /// Stores computed_size result.
    pub fn set_computed(&self, constraint: BoxConstraint, scale_bits: u32, size: ResolvedSize) {
        let guard = unsafe { &mut *self.computed.get() };
        *guard = Some((constraint, scale_bits, size));
    }

    /// Returns cached content_size if constraint and scale match, otherwise
    /// None.
    pub fn get_content(&self, constraint: BoxConstraint, scale_bits: u32) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.content.get() };
        match *guard {
            Some((c, s, size)) if c == constraint && s == scale_bits => Some(size),
            _ => None,
        }
    }

    /// Stores content_size result.
    pub fn set_content(&self, constraint: BoxConstraint, scale_bits: u32, size: ResolvedSize) {
        let guard = unsafe { &mut *self.content.get() };
        *guard = Some((constraint, scale_bits, size));
    }

    /// Clears all cached values (call at the start of each frame).
    pub fn invalidate(&self) {
        unsafe {
            *self.computed.get() = None;
            *self.content.get() = None;
        }
    }
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}
