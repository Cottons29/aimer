use crate::style::constraints::BoxConstraint;
use attribute::size::ResolvedSize;
use std::cell::UnsafeCell;

/// Caches the result of `computed_size` and `content_size` for a single frame.
/// The cache is keyed by `(BoxConstraint, scale)` so that if the same element
/// is queried multiple times with the same inputs, the result is returned instantly.
/// yeah it save the CPU and GPU and reduce power consuming :))

pub struct LayoutCache {
    #[cfg(not(target_arch = "wasm32"))]
    computed: UnsafeCell<Option<(BoxConstraint, u32, ResolvedSize)>>,
    #[cfg(not(target_arch = "wasm32"))]
    content: UnsafeCell<Option<(BoxConstraint, u32, ResolvedSize)>>,
    #[cfg(target_arch = "wasm32")]
    computed: UnsafeCell<Option<(BoxConstraint, u64, ResolvedSize)>>,
    #[cfg(target_arch = "wasm32")]
    content: UnsafeCell<Option<(BoxConstraint, u64, ResolvedSize)>>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self { computed: UnsafeCell::new(None), content: UnsafeCell::new(None) }
    }

    /// Returns cached computed_size if constraint and scale match, otherwise None.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_computed(&self, constraint: BoxConstraint, scale_bits: u32) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.computed.get() };
        match *guard {
            Some((c, s, size)) if c == constraint && s == scale_bits => Some(size),
            _ => None,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn get_computed(&self, constraint: BoxConstraint, scale_bits: u64) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.computed.get() };
        match *guard {
            Some((c, s, size)) if c == constraint && s == scale_bits => Some(size),
            _ => None,
        }
    }

    /// Stores computed_size result.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_computed(&self, constraint: BoxConstraint, scale_bits: u32, size: ResolvedSize) {
        let guard = unsafe { &mut *self.computed.get() };
        *guard = Some((constraint, scale_bits, size));
    }

    #[cfg(target_arch = "wasm32")]
    pub fn set_computed(&self, constraint: BoxConstraint, scale_bits: u64, size: ResolvedSize) {
        let guard = unsafe { &mut *self.computed.get() };
        *guard = Some((constraint, scale_bits, size));
    }

    /// Returns cached content_size if constraint and scale match, otherwise None.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_content(&self, constraint: BoxConstraint, scale_bits: u32) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.computed.get() };
        match *guard {
            Some((c, s, size)) if c == constraint && s == scale_bits => Some(size),
            _ => None,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn get_content(&self, constraint: BoxConstraint, scale_bits: u64) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.computed.get() };
        match *guard {
            Some((c, s, size)) if c == constraint && s == scale_bits => Some(size),
            _ => None,
        }
    }

    /// Stores content_size result.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_content(&self, constraint: BoxConstraint, scale_bits: u32, size: ResolvedSize) {
        let guard = unsafe { &mut *self.computed.get() };
        *guard = Some((constraint, scale_bits, size));
    }

    #[cfg(target_arch = "wasm32")]
    pub fn set_content(&self, constraint: BoxConstraint, scale_bits: u64, size: ResolvedSize) {
        let guard = unsafe { &mut *self.computed.get() };
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
