use std::path::PathBuf;

use website_backend::{BlogStore, Config, app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content_dir = std::env::var_os("AIMER_BLOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/blogs"));
    let config = Config::load("aimer.toml")?;
    let address = config.server().address();
    let listener = tokio::net::TcpListener::bind(address).await?;
    let store = BlogStore::load(content_dir).map_err(std::io::Error::other)?;

    println!("Website backend listening on http://{address}");
    axum::serve(listener, app(store, config.server().cors_origins())).await?;
    Ok(())
}
