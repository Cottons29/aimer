//! Syntax highlighting for fenced code blocks.
//!
//! Two backends produce the same [`CaptureSpan`]s:
//!
//! - `synoptic` (default) — a lightweight regex based highlighter.
//! - `arborium` (`arborium` feature) — a tree-sitter based highlighter with a
//!   deeper understanding of the code, at a much higher build cost.
//!
//! Enabling the `arborium` feature swaps the backend out; nothing else in the
//! crate changes.

#[cfg(feature = "arborium")]
mod arborium;
mod parser;
#[cfg(not(feature = "arborium"))]
mod synoptic;

use std::cell::RefCell;
use std::rc::Rc;

pub use parser::CaptureSpan;

#[cfg(feature = "arborium")]
use self::arborium::ArboriumBackend as Backend;
#[cfg(not(feature = "arborium"))]
use self::synoptic::SynopticBackend as Backend;
use crate::cache::LruCache;

const HIGHLIGHT_CACHE_CAPACITY: usize = 64;
type HighlightEntries = LruCache<(Rc<str>, Option<Rc<str>>), Rc<[CaptureSpan]>>;

thread_local! {
    static HIGHLIGHT_CACHE: RefCell<HighlightCache> = RefCell::new(HighlightCache::new(HIGHLIGHT_CACHE_CAPACITY));
}

struct HighlightCache {
    entries: HighlightEntries,
    backend: Backend,
}

impl HighlightCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: LruCache::new(capacity),
            backend: Backend::new(),
        }
    }

    fn highlight(&mut self, code: &str, language: Option<&str>) -> Rc<[CaptureSpan]> {
        let backend = &mut self.backend;
        self.entries.get_or_insert_with(
            (Rc::from(code), language.map(Rc::from)),
            |(code, language)| Rc::from(backend.highlight(code, language.as_deref())),
        )
    }
}

pub fn highlight(code: &str, language: Option<&str>) -> Vec<CaptureSpan> {
    highlight_cached(code, language).to_vec()
}

pub(crate) fn highlight_cached(code: &str, language: Option<&str>) -> Rc<[CaptureSpan]> {
    HIGHLIGHT_CACHE.with(|cache| cache.borrow_mut().highlight(code, language))
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    #[test]
    fn highlight_cache_reuses_unchanged_code() {
        let mut cache = HighlightCache::new(2);

        let first = cache.highlight("fn main() {}", Some("rust"));
        let second = cache.highlight("fn main() {}", Some("rust"));

        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn highlight_cache_reuses_the_backend_across_cache_misses() {
        let mut cache = HighlightCache::new(2);
        let backend = &cache.backend as *const Backend;

        cache.highlight("fn first() {}", Some("rust"));
        cache.highlight("fn second() {}", Some("rust"));

        assert_eq!(backend, &cache.backend as *const Backend);
    }

    #[test]
    fn highlight_cache_invalidates_changed_code_or_language() {
        let mut cache = HighlightCache::new(3);

        let original = cache.highlight("fn main() {}", Some("rust"));
        let changed_code = cache.highlight("fn other() {}", Some("rust"));
        let changed_language = cache.highlight("fn main() {}", Some("python"));

        assert!(!Rc::ptr_eq(&original, &changed_code));
        assert!(!Rc::ptr_eq(&original, &changed_language));
    }

    #[test]
    fn highlights_rust_keywords_whichever_backend_is_active() {
        assert!(
            highlight("fn main(){}", Some("rust"))
                .contains(&CaptureSpan::Keyword { start: 0, end: 2 })
        );
    }

    #[test]
    fn returns_no_captures_without_a_supported_language() {
        assert!(highlight("plain text", None).is_empty());
        assert!(highlight("plain text", Some("unknown")).is_empty());
    }
}
