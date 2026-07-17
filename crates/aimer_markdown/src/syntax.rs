#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxTokenKind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxToken {
    pub kind: SyntaxTokenKind,
    pub text: String,
}

pub fn highlight(code: &str, language: Option<&str>) -> Vec<SyntaxToken> {
    let Some(language) = language.map(str::to_ascii_lowercase) else {
        return plain(code);
    };
    let dialect = match language.as_str() {
        "py" | "python" => Dialect::Python,
        "rs" | "rust" => Dialect::Rust,
        "js" | "javascript" | "ts" | "typescript" => Dialect::JavaScript,
        _ => return plain(code),
    };
    tokenize(code, dialect)
}

#[derive(Clone, Copy)]
enum Dialect {
    Python,
    Rust,
    JavaScript,
}

fn plain(code: &str) -> Vec<SyntaxToken> {
    vec![SyntaxToken { kind: SyntaxTokenKind::Plain, text: code.to_string() }]
}

fn tokenize(code: &str, dialect: Dialect) -> Vec<SyntaxToken> {
    let chars: Vec<char> = code.chars().collect();
    let mut result = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let start = index;
        let current = chars[index];
        if is_comment_start(&chars, index, dialect) {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            push(&mut result, SyntaxTokenKind::Comment, &chars[start..index]);
        } else if current == '\'' || current == '"' || is_string_prefix(&chars, index, dialect) {
            index = tokenize_string(&chars, index, &mut result);
        } else if current.is_ascii_digit() {
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '.' | '_'))
            {
                index += 1;
            }
            push(&mut result, SyntaxTokenKind::Number, &chars[start..index]);
        } else if current.is_alphabetic() || current == '_' {
            index += 1;
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            let text: String = chars[start..index]
                .iter()
                .collect();
            let kind = if is_keyword(&text, dialect) {
                SyntaxTokenKind::Keyword
            } else {
                SyntaxTokenKind::Plain
            };
            result.push(SyntaxToken { kind, text });
        } else {
            index += 1;
            while index < chars.len()
                && !chars[index].is_alphanumeric()
                && chars[index] != '_'
                && chars[index] != '\''
                && chars[index] != '"'
                && !is_comment_start(&chars, index, dialect)
            {
                index += 1;
            }
            push(&mut result, SyntaxTokenKind::Plain, &chars[start..index]);
        }
    }
    result
}

fn tokenize_string(chars: &[char], start: usize, result: &mut Vec<SyntaxToken>) -> usize {
    let quote_index = if matches!(chars[start], 'f' | 'r' | 'b')
        && chars
            .get(start + 1)
            .is_some_and(|c| matches!(c, '\'' | '"'))
    {
        start + 1
    } else {
        start
    };
    let quote = chars[quote_index];
    let mut index = quote_index + 1;
    let mut segment = start;
    while index < chars.len() {
        if chars[index] == '\\' {
            index = (index + 2).min(chars.len());
            continue;
        }
        if chars[index].is_ascii_digit() {
            push(result, SyntaxTokenKind::String, &chars[segment..index]);
            let number_start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            push(result, SyntaxTokenKind::Number, &chars[number_start..index]);
            segment = index;
            continue;
        }
        index += 1;
        if chars[index - 1] == quote {
            break;
        }
    }
    push(result, SyntaxTokenKind::String, &chars[segment..index]);
    index
}

fn push(result: &mut Vec<SyntaxToken>, kind: SyntaxTokenKind, chars: &[char]) {
    if !chars.is_empty() {
        result.push(SyntaxToken { kind, text: chars.iter().collect() });
    }
}

fn is_comment_start(chars: &[char], index: usize, dialect: Dialect) -> bool {
    match dialect {
        Dialect::Python => chars[index] == '#',
        Dialect::Rust | Dialect::JavaScript => {
            chars[index] == '/' && chars.get(index + 1) == Some(&'/')
        }
    }
}

fn is_string_prefix(chars: &[char], index: usize, dialect: Dialect) -> bool {
    matches!(dialect, Dialect::Python)
        && matches!(chars[index], 'f' | 'r' | 'b')
        && chars
            .get(index + 1)
            .is_some_and(|character| matches!(character, '\'' | '"'))
}

fn is_keyword(value: &str, dialect: Dialect) -> bool {
    let keywords: &[&str] = match dialect {
        Dialect::Python => &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
            "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
            "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return",
            "True", "try", "while", "with", "yield",
        ],
        Dialect::Rust => &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ],
        Dialect::JavaScript => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "let",
            "new",
            "null",
            "return",
            "static",
            "super",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "var",
            "void",
            "while",
            "with",
            "yield",
        ],
    };
    keywords.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_python_keywords_strings_numbers_and_comments_without_losing_text() {
        let source = "def greet(name):\n    # message\n    return f\"Hi {name} {42}\"";
        let tokens = highlight(source, Some("python"));

        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SyntaxTokenKind::Keyword && token.text == "def")
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SyntaxTokenKind::Comment
                    && token.text.contains("message"))
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SyntaxTokenKind::String && token.text.contains("Hi"))
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SyntaxTokenKind::Number && token.text == "42")
        );
        assert_eq!(
            tokens
                .iter()
                .map(|token| token.text.as_str())
                .collect::<String>(),
            source
        );
    }

    #[test]
    fn highlights_rust_and_unknown_languages_losslessly() {
        let rust = "pub fn answer() -> u32 { 42 } // value";
        let tokens = highlight(rust, Some("rust"));
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SyntaxTokenKind::Keyword && token.text == "pub")
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == SyntaxTokenKind::Comment)
        );
        assert_eq!(
            tokens
                .iter()
                .map(|token| token.text.as_str())
                .collect::<String>(),
            rust
        );

        let plain = "anything <goes>";
        assert_eq!(
            highlight(plain, Some("unknown")),
            vec![SyntaxToken { kind: SyntaxTokenKind::Plain, text: plain.to_string() }]
        );
    }
}
