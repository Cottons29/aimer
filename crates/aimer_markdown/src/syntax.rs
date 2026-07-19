mod parser;

use arborium::Highlighter;

pub use parser::CaptureSpan;

pub fn highlight(code: &str, language: Option<&str>) -> Vec<CaptureSpan> {
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
    use super::*;

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
