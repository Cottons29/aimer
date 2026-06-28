mod backend;
mod diagnostics;
mod widget_analyzer;
mod widget_tree;

use backend::AimerBackend;
use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::new(|client| AimerBackend::new(client));
    Server::new(stdin(), stdout(), socket).serve(service).await;
}
