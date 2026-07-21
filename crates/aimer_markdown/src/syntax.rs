mod highlighter;
mod parser;

use std::cell::RefCell;
use std::rc::Rc;

use arborium::Highlighter;

pub use parser::CaptureSpan;

use crate::cache::LruCache;

const HIGHLIGHT_CACHE_CAPACITY: usize = 64;

thread_local! {
    static HIGHLIGHT_CACHE: RefCell<HighlightCache> = RefCell::new(HighlightCache::new(HIGHLIGHT_CACHE_CAPACITY));
}

struct HighlightCache {
    entries: LruCache<(Rc<str>, Option<Rc<str>>), Rc<[CaptureSpan]>>,
}

impl HighlightCache {
    fn new(capacity: usize) -> Self {
        Self { entries: LruCache::new(capacity) }
    }

    fn highlight(&mut self, code: &str, language: Option<&str>) -> Rc<[CaptureSpan]> {
        self.entries
            .get_or_insert_with((Rc::from(code), language.map(Rc::from)), |(code, language)| {
                Rc::from(parse_highlights(code, language.as_deref()))
            })
    }
}

pub fn highlight(code: &str, language: Option<&str>) -> Vec<CaptureSpan> {
    highlight_cached(code, language).to_vec()
}

pub(crate) fn highlight_cached(code: &str, language: Option<&str>) -> Rc<[CaptureSpan]> {
    HIGHLIGHT_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .highlight(code, language)
    })
}

fn parse_highlights(code: &str, language: Option<&str>) -> Vec<CaptureSpan> {
    let Some(language) = language.map(str::to_ascii_lowercase) else {
        return Vec::new();
    };
    let language = match language.as_str() {
        "py" => "python",
        "rs" => "rust",
        "js" => "javascript",
        "ts" => "typescript",
        language => language,
    };

    Highlighter::new()
        .highlight_spans(language, code)
        .map(|spans| {
            spans
                .into_iter()
                .map(CaptureSpan::from)
                .collect()
        })
        .unwrap_or_default()
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
    fn highlight_cache_invalidates_changed_code_or_language() {
        let mut cache = HighlightCache::new(3);

        let original = cache.highlight("fn main() {}", Some("rust"));
        let changed_code = cache.highlight("fn other() {}", Some("rust"));
        let changed_language = cache.highlight("fn main() {}", Some("python"));

        assert!(!Rc::ptr_eq(&original, &changed_code));
        assert!(!Rc::ptr_eq(&original, &changed_language));
    }

    #[test]
    fn highlights_rust_with_capture_spans() {
        assert_eq!(
            highlight("fn main(){}", Some("rust")),
            vec![
                CaptureSpan::Keyword { start: 0, end: 2 },
                CaptureSpan::Function { start: 3, end: 7 },
                CaptureSpan::Punctuation { start: 7, end: 8 },
                CaptureSpan::Punctuation { start: 8, end: 9 },
                CaptureSpan::Punctuation { start: 9, end: 10 },
                CaptureSpan::Punctuation { start: 10, end: 11 },
            ]
        );
    }

    #[test]
    fn supports_languages_and_aliases_provided_by_arborium() {
        let toml = highlight("edition = \"2024\"", Some("toml"));
        assert!(
            toml.iter()
                .any(|span| matches!(span, CaptureSpan::String { .. }))
        );
        assert!(
            highlight("def main(): pass", Some("py"))
                .iter()
                .any(|span| matches!(span, CaptureSpan::Keyword { start: 0, end: 3 }))
        );
    }

    #[test]
    fn returns_no_captures_without_a_supported_language() {
        assert!(highlight("plain text", None).is_empty());
        assert!(highlight("plain text", Some("unknown")).is_empty());
    }
}
