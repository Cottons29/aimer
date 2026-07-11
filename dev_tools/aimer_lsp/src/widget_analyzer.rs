use serde::{Deserialize, Serialize};

/// The type of Aimer widget detected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WidgetKind {
    Stateless,
    Stateful,
    Router,
    RawWidget,
    /// A function annotated with `#[aimer::main]` or `#[main]`.
    EntryPoint,
}

impl WidgetKind {
    pub fn label(&self) -> &'static str {
        match self {
            WidgetKind::Stateless => "Stateless",
            WidgetKind::Stateful => "Stateful",
            WidgetKind::Router => "Router",
            WidgetKind::RawWidget => "RawWidget",
            WidgetKind::EntryPoint => "EntryPoint",
        }
    }
}

/// A parsed `#[widget]` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetInfo {
    /// Name of the struct or enum.
    pub name: String,
    /// The widget variant.
    pub kind: WidgetKind,
    /// File URI where this widget is declared.
    pub file_uri: String,
    /// Line number (0-based) of the `#[widget(...)]` attribute.
    pub line: u32,
    /// Character offset of the attribute start.
    pub character: u32,
    /// Whether the struct has a `key` field.
    pub has_key: bool,
    /// For Stateful widgets: whether `create_state` was found in the same file.
    pub has_create_state: bool,
    /// For Stateful widgets: whether a companion State impl was found.
    pub has_state_impl: bool,
    /// For Router: the route paths found on variants.
    pub routes: Vec<String>,
}

/// Analyze a single Rust source file for `#[widget(...)]` declarations
/// and `#[aimer::main]` / `#[main]` entry points.
pub fn analyze_source(source: &str, file_uri: &str) -> Vec<WidgetInfo> {
    let mut widgets = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        // Look for #[widget(...)] attribute
        if let Some(widget_kind) = parse_widget_attribute(line) {
            // The struct/enum declaration should be on the next non-empty, non-attribute line
            let (name, _decl_line, has_key) = find_declaration(&lines, i + 1);

            if let Some(name) = name {
                let has_create_state = source.contains(&"fn create_state".to_string())
                    || source.contains(&format!("impl State<{}>", name));

                let has_state_impl = source.contains(&format!("impl State<{}>", name));

                let routes = if widget_kind == WidgetKind::Router {
                    extract_routes(source)
                } else {
                    vec![]
                };

                widgets.push(WidgetInfo {
                    name,
                    kind: widget_kind,
                    file_uri: file_uri.to_string(),
                    line: i as u32,
                    character: 0,
                    has_key,
                    has_create_state,
                    has_state_impl,
                    routes,
                });
            }
        }

        // Look for #[aimer::main] or #[main] entry point attributes
        if is_entry_point_attribute(line) && let Some((name, _fn_line)) = find_function_declaration(&lines, i + 1) {
            widgets.push(WidgetInfo {
                name,
                kind: WidgetKind::EntryPoint,
                file_uri: file_uri.to_string(),
                line: i as u32,
                character: 0,
                has_key: false,
                has_create_state: false,
                has_state_impl: false,
                routes: vec![],
            });
        }

        i += 1;
    }

    widgets
}

/// Parse `#[widget(Stateless)]` (or Stateful/Router/RawWidget) from a line.
fn parse_widget_attribute(line: &str) -> Option<WidgetKind> {
    let trimmed = line.trim();
    if !trimmed.starts_with("#[widget(") {
        return None;
    }

    let inner = trimmed
        .strip_prefix("#[widget(")?
        .strip_suffix(")]")?
        .trim()
        .to_lowercase();

    match inner.as_str() {
        "stateless" => Some(WidgetKind::Stateless),
        "stateful" => Some(WidgetKind::Stateful),
        "router" => Some(WidgetKind::Router),
        "rawwidget" => Some(WidgetKind::RawWidget),
        _ => None,
    }
}

/// Starting from `start_line`, find the struct/enum declaration and return its name.
fn find_declaration(lines: &[&str], start_line: usize) -> (Option<String>, usize, bool) {
    for i in start_line..lines.len().min(start_line + 5) {
        let line = lines[i].trim();

        // Skip empty lines and other attributes
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        // Check for struct or enum
        let keywords = ["pub struct ", "struct ", "pub enum ", "enum "];
        for kw in keywords {
            if let Some(rest) = line.strip_prefix(kw) {
                let name = rest
                    .split(|c: char| c == '{' || c == '<' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                if !name.is_empty() {
                    let has_key = check_has_key_field(lines, i);
                    return (Some(name), i, has_key);
                }
            }
        }
    }
    (None, 0, false)
}

/// Check if a struct has a `key` field.
fn check_has_key_field(lines: &[&str], struct_line: usize) -> bool {
    // Look for fields between { and }
    let mut brace_depth = 0;
    for line in lines.iter().take(lines.len().min(struct_line + 50)).skip(struct_line) {
        for ch in line.chars() {
            if ch == '{' {
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
                if brace_depth == 0 {
                    return false; // End of struct without finding key
                }
            }
        }
        if brace_depth > 0 {
            let trimmed = line.trim();
            if trimmed.starts_with("pub key") || trimmed.starts_with("key:") {
                return true;
            }
            // Check for: key: Option<Key>
            if trimmed.contains("key") && trimmed.contains("Option") {
                return true;
            }
        }
    }
    false
}

/// Check if a line is an `#[aimer::main]` or `#[main]` attribute.
fn is_entry_point_attribute(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "#[aimer::main]" || trimmed == "#[main]"
}

/// Starting from `start_line`, find a `fn` declaration and return its name.
fn find_function_declaration(lines: &[&str], start_line: usize) -> Option<(String, usize)> {

    for (i, line) in lines.iter().enumerate().take(lines.len().min(start_line + 5)).skip(start_line)  {
        let line = line.trim();

        // Skip empty lines and other attributes
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        // Check for fn declaration
        let keywords = ["pub fn ", "pub(crate) fn ", "fn ", "pub async fn ", "async fn "];
        for kw in keywords {
            if let Some(rest) = line.strip_prefix(kw) {
                let name = rest
                    .split(|c: char| c == '(' || c == '<' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                if !name.is_empty() {
                    return Some((name, i));
                }
            }
        }
    }
    None
}

/// Extract route paths from a Router enum.
fn extract_routes(source: &str) -> Vec<String> {
    let mut routes = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // #[route("/path")] or #[route = "/path"]
        if (trimmed.starts_with("#[route(") || trimmed.starts_with("#[route =")) && let Some(path) = extract_quoted_string(trimmed) {
            routes.push(path);
        }
        // #[routes("/a", "/b")] or #[routes = ["/a", "/b"]]
        if trimmed.starts_with("#[routes(") || trimmed.starts_with("#[routes =") {
            routes.extend(extract_multiple_quoted_strings(trimmed));
        }
    }
    routes
}

/// Extract a single quoted string from a line.
fn extract_quoted_string(line: &str) -> Option<String> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

/// Extract multiple quoted strings from a line.
fn extract_multiple_quoted_strings(line: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut remaining = line;
    while let Some(start) = remaining.find('"') {
        let after_start = &remaining[start + 1..];
        if let Some(end) = after_start.find('"') {
            strings.push(after_start[..end].to_string());
            remaining = &after_start[end + 1..];
        } else {
            break;
        }
    }
    strings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stateless() {
        let source = r#"
#[widget(Stateless)]
pub struct MyWidget {
    label: String,
}
"#;
        let widgets = analyze_source(source, "file:///test.rs");
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].name, "MyWidget");
        assert_eq!(widgets[0].kind, WidgetKind::Stateless);
    }

    #[test]
    fn test_parse_stateful() {
        let source = r#"
#[widget(Stateful)]
pub struct Counter {
    initial: i32,
}

impl State<Counter> for CounterState {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Text::new("hello")
    }
}
"#;
        let widgets = analyze_source(source, "file:///test.rs");
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].name, "Counter");
        assert_eq!(widgets[0].kind, WidgetKind::Stateful);
        assert!(widgets[0].has_state_impl);
    }

    #[test]
    fn test_parse_router() {
        let source = r#"
#[widget(Router)]
pub enum AppRouter {
    #[route("/")]
    Home,
    #[route("/settings")]
    Settings,
}
"#;
        let widgets = analyze_source(source, "file:///test.rs");
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].name, "AppRouter");
        assert_eq!(widgets[0].kind, WidgetKind::Router);
        assert_eq!(widgets[0].routes, vec!["/", "/settings"]);
    }

    #[test]
    fn test_no_widget() {
        let source = r#"
pub struct RegularStruct {
    value: i32,
}
"#;
        let widgets = analyze_source(source, "file:///test.rs");
        assert!(widgets.is_empty());
    }

    #[test]
    fn test_entry_point_main() {
        let source = r#"
#[main]
pub fn start_app() {
    AimerApp::start(Text::new("Hello"));
}
"#;
        let widgets = analyze_source(source, "file:///test.rs");
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].name, "start_app");
        assert_eq!(widgets[0].kind, WidgetKind::EntryPoint);
    }

    #[test]
    fn test_entry_point_aimer_main() {
        let source = r#"
#[aimer::main]
pub fn main() {
    AimerApp::start(Text::new("Hello"));
}
"#;
        let widgets = analyze_source(source, "file:///test.rs");
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].name, "main");
        assert_eq!(widgets[0].kind, WidgetKind::EntryPoint);
    }

    #[test]
    fn test_widget_and_entry_point() {
        let source = r#"
#[widget(Stateless)]
pub struct MyWidget {
    label: String,
}

#[main]
pub fn start_app() {
    AimerApp::start(MyWidget::new("Hello"));
}
"#;
        let widgets = analyze_source(source, "file:///test.rs");
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[0].name, "MyWidget");
        assert_eq!(widgets[0].kind, WidgetKind::Stateless);
        assert_eq!(widgets[1].name, "start_app");
        assert_eq!(widgets[1].kind, WidgetKind::EntryPoint);
    }
}
