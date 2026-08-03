//! The blog API behind [aimers.dev](https://aimers.dev), running as a
//! Cloudflare Worker compiled to WebAssembly.
//!
//! | Route                 | Description                        |
//! |-----------------------|------------------------------------|
//! | `GET /api/blogs`      | Every blog summary, newest first.   |
//! | `GET /api/blogs/{id}` | One blog, including its markdown.   |
//!
//! The markdown in `content/blogs` is embedded at compile time by `build.rs`,
//! because a Worker has no filesystem. [`BlogStore::embedded`] validates that
//! content and [`app`] turns it into an `axum` [`Router`](axum::Router); the
//! fetch handler wiring them together lives in `entry`, which is only compiled
//! for `wasm32`. The same router is what the tests exercise natively.

mod blog;
mod config;
mod content;

#[cfg(target_arch = "wasm32")]
mod entry;

pub use blog::{BlogStore, BlogSummary, app};
pub use config::{Config, ConfigError, SERVER_CORS};
