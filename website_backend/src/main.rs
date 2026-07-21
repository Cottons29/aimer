use std::path::PathBuf;

use website_backend::{BlogStore, app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content_dir = std::env::var_os("AIMER_BLOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/blogs"));
    let address = std::env::var("AIMER_BLOG_ADDRESS").unwrap_or_else(|_| "0.0.0.0:3200".to_owned());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let store = BlogStore::load(content_dir).map_err(std::io::Error::other)?;

    println!("Website backend listening on http://{address}");
    axum::serve(listener, app(store)).await?;
    Ok(())
}
