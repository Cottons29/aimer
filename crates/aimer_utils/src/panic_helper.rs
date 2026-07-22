use std::borrow::Cow;
use std::iter;
use std::panic::Location;

#[cfg(not(target_arch = "wasm32"))]
use std::{env, fs};

#[cfg(debug_assertions)]
mod embedded_sources {
    include!(concat!(env!("OUT_DIR"), "/embedded_sources.rs"));
}

/// Formats concise diagnostics for tracked panic call sites.
pub struct PanicHelper;

impl PanicHelper {
    /// Formats source coordinates and highlights the source expression when it was embedded at
    /// compile time or remains available on the native filesystem.
    pub fn location(location: &Location<'_>) -> String {
        format_location(location.file(), location.line(), location.column())
    }
}

fn format_location(file: &str, line: u32, column: u32) -> String {
    let coordinates = format!("{file}:{line}:{column}");
    let Some(source_line) = read_source(file).and_then(|source| {
        source
            .lines()
            .nth(line.saturating_sub(1) as usize)
            .map(str::to_owned)
    }) else {
        return coordinates;
    };
    let Some(highlight) = highlight_source_line(&source_line, column) else {
        return coordinates;
    };

    format!("{coordinates}\n{source_line}\n{highlight}")
}

fn read_source(file: &str) -> Option<Cow<'static, str>> {
    if let Some(source) = read_embedded_source(file) {
        return Some(Cow::Borrowed(source));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(source) = fs::read_to_string(file) {
            return Some(Cow::Owned(source));
        }

        let mut base = env::current_dir().ok()?;
        while base.pop() {
            if let Ok(source) = fs::read_to_string(base.join(file)) {
                return Some(Cow::Owned(source));
            }
        }
    }
    None
}

#[cfg(debug_assertions)]
fn read_embedded_source(file: &str) -> Option<&'static str> {
    let file = file.replace('\\', "/");
    if let Some((_, source)) = embedded_sources::SOURCES
        .iter()
        .find(|(path, _)| {
            file == *path
                || file
                    .strip_suffix(path)
                    .is_some_and(|prefix| prefix.ends_with('/'))
        })
    {
        return Some(source);
    }

    let suffix = format!("/{file}");
    let mut matches = embedded_sources::SOURCES
        .iter()
        .filter(|(path, _)| path.ends_with(&suffix));
    let (_, source) = matches.next()?;
    matches
        .next()
        .is_none()
        .then_some(*source)
}

#[cfg(not(debug_assertions))]
fn read_embedded_source(_file: &str) -> Option<&'static str> {
    None
}

fn highlight_source_line(source_line: &str, column: u32) -> Option<String> {
    let start = column.saturating_sub(1) as usize;
    let prefix: String = source_line
        .chars()
        .take(start)
        .map(|character| if character == '\t' { '\t' } else { ' ' })
        .collect();
    let expression = source_line
        .chars()
        .skip(start)
        .collect::<String>();
    let expression = expression
        .trim_end()
        .trim_end_matches(';')
        .trim_end();
    if expression.is_empty() {
        return None;
    }

    Some(prefix + &iter::repeat_n('^', expression.chars().count()).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn caller_location() -> String {
        PanicHelper::location(Location::caller())
    }

    #[test]
    fn location_highlights_the_tracked_source_expression() {
        let location = caller_location();

        assert!(location.contains(file!()), "{location}");
        assert!(
            location.contains("let location = caller_location();"),
            "{location}"
        );
        assert!(location.contains("^^^^^^^^^^^^^^^^^"), "{location}");
    }

    #[test]
    fn compiled_source_is_available_without_runtime_file_access() {
        let source = read_embedded_source(file!()).expect("this source file should be embedded");

        assert!(source.contains("fn compiled_source_is_available_without_runtime_file_access()"));
    }

    #[test]
    fn location_falls_back_to_coordinates_when_source_is_unavailable() {
        let location = format_location("missing/aimer/source.rs", 17, 9);

        assert_eq!(location, "missing/aimer/source.rs:17:9");
    }

    #[test]
    fn source_highlight_uses_character_columns_for_unicode_prefixes() {
        let highlight = highlight_source_line("let π = PanicPosition::of(ctx);", 9).unwrap();

        assert_eq!(highlight, "        ^^^^^^^^^^^^^^^^^^^^^^");
    }
}
