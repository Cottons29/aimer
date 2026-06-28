use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::diagnostics;
use crate::widget_analyzer::{self, WidgetInfo};
use crate::widget_tree;

/// The Aimer LSP server backend.
#[derive(Debug)]
pub struct AimerBackend {
    client: Client,
    /// Cached widget analysis per file URI.
    file_widgets: Arc<DashMap<String, Vec<WidgetInfo>>>,
}

impl AimerBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            file_widgets: Arc::new(DashMap::new()),
        }
    }

    /// Analyze a single file and publish diagnostics.
    async fn analyze_and_publish(&self, uri: &str, source: &str) {
        let widgets = widget_analyzer::analyze_source(source, uri);

        // Generate and publish diagnostics
        let diagnostics = diagnostics::generate_diagnostics(&widgets);
        let file_uri = uri.parse().unwrap_or_else(|_| {
            Url::parse("file:///unknown").unwrap()
        });

        self.client
            .publish_diagnostics(file_uri, diagnostics, None)
            .await;

        // Cache the analysis
        self.file_widgets.insert(uri.to_string(), widgets);
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for AimerBackend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Aimer LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let source = &params.text_document.text;
        self.analyze_and_publish(&uri, source).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let uri = params.text_document.uri.to_string();
            self.analyze_and_publish(&uri, &change.text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        // Re-read the file from disk
        if let Ok(source) = std::fs::read_to_string(
            params.text_document.uri.to_file_path().unwrap_or_default(),
        ) {
            self.analyze_and_publish(&uri, &source).await;
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();

        if let Some(widgets) = self.file_widgets.get(&uri) {
            let symbols: Vec<SymbolInformation> = widgets
                .iter()
                .map(|w| SymbolInformation {
                    name: format!("[{}] {}", w.kind.label(), w.name),
                    kind: match w.kind {
                        widget_analyzer::WidgetKind::Stateless => SymbolKind::CLASS,
                        widget_analyzer::WidgetKind::Stateful => SymbolKind::INTERFACE,
                        widget_analyzer::WidgetKind::Router => SymbolKind::ENUM,
                        widget_analyzer::WidgetKind::RawWidget => SymbolKind::STRUCT,
                        widget_analyzer::WidgetKind::EntryPoint => SymbolKind::FUNCTION,
                    },
                    location: Location {
                        uri: params.text_document.uri.clone(),
                        range: Range {
                            start: Position {
                                line: w.line,
                                character: w.character,
                            },
                            end: Position {
                                line: w.line,
                                character: w.character + 20,
                            },
                        },
                    },
                    #[allow(deprecated)]
                    deprecated: None,
                    tags: None,
                    container_name: None,
                })
                .collect();

            Ok(Some(DocumentSymbolResponse::Flat(symbols)))
        } else {
            Ok(None)
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<Value>> {
        match params.command.as_str() {
            "aimer.widgetTree" => {
                let all_widgets: Vec<WidgetInfo> = self
                    .file_widgets
                    .iter()
                    .flat_map(|entry| entry.value().clone())
                    .collect();

                let tree = widget_tree::build_widget_tree(&all_widgets);
                Ok(Some(serde_json::to_value(tree).unwrap_or_default()))
            }
            _ => Err(Error {
                code: ErrorCode::MethodNotFound,
                message: format!("Unknown command: {}", params.command).into(),
                data: None,
            }),
        }
    }
}
