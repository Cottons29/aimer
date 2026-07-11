use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::widget_analyzer::WidgetInfo;

/// Generate Aimer-specific diagnostics for a file's widgets.
pub fn generate_diagnostics(widgets: &[WidgetInfo]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for widget in widgets {
        match widget.kind {
            crate::widget_analyzer::WidgetKind::Stateful => {
                // Error: Stateful widget should have a State impl
                if !widget.has_state_impl {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: widget.line,
                                character: widget.character,
                            },
                            end: Position {
                                line: widget.line,
                                character: widget.character + 20,
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("aimer-lsp".to_string()),
                        message: format!(
                            "#[widget(Stateful)] on `{}` requires a `State<{0}>` impl in the same file",
                            widget.name
                        ),
                        ..Default::default()
                    });
                }

                // Warning: Stateful widget should have create_state
                if !widget.has_create_state {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: widget.line,
                                character: widget.character,
                            },
                            end: Position {
                                line: widget.line,
                                character: widget.character + 20,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("aimer-lsp".to_string()),
                        message: format!(
                            "#[widget(Stateful)] on `{}` likely needs a `create_state()` method",
                            widget.name
                        ),
                        ..Default::default()
                    });
                }
            }
            crate::widget_analyzer::WidgetKind::Router  => {
                // Warning: Router with no routes
                #[allow(clippy::collapsible_match)]
                if widget.routes.is_empty() {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: widget.line,
                                character: widget.character,
                            },
                            end: Position {
                                line: widget.line,
                                character: widget.character + 18,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("aimer-lsp".to_string()),
                        message: format!(
                            "#[widget(Router)] on `{}` has no `#[route]` attributes on its variants",
                            widget.name
                        ),
                        ..Default::default()
                    });
                }
            }
            _ => {}
        }
    }

    diagnostics
}
