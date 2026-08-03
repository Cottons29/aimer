//! The Cloudflare Worker entry point.
//!
//! Only compiled for `wasm32`, because it is the one place that talks to the
//! Workers runtime. `worker-build` turns the module below into the JavaScript
//! entry `wrangler.toml` points at; everything it serves comes from [`app`].

use std::sync::OnceLock;

use axum::Router;
use axum::body::Body;
use axum::http::Response;
use tower_service::Service;
use worker::{Context, Env, HttpRequest, Result, event};

use crate::{BlogStore, Config, SERVER_CORS, app};

/// The router, built on the first request and reused by every later one.
///
/// An isolate serves many requests, and neither the embedded content nor the
/// CORS policy can change while it lives, so validating them once keeps the
/// per-request work down to a reference count bump.
static ROUTER: OnceLock<Router> = OnceLock::new();

/// Serves the blog API.
///
/// Requests outside `/api/*` fall through to the router's own `404`, so the
/// Worker answers with the same JSON shape whatever the path.
#[event(fetch)]
async fn fetch(request: HttpRequest, env: Env, _context: Context) -> Result<Response<Body>> {
    Ok(router(&env)?.call(request).await?)
}

/// Returns the shared router, building it on first use.
fn router(env: &Env) -> Result<Router> {
    if let Some(router) = ROUTER.get() {
        return Ok(router.clone());
    }

    let origins = env.var(SERVER_CORS)?.to_string();
    let config = Config::from_origins(&origins).map_err(|error| error.to_string())?;
    let store = BlogStore::embedded()?;

    Ok(ROUTER
        .get_or_init(|| app(store, config.cors_origins()))
        .clone())
}
