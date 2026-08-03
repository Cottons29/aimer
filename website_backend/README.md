# website_backend

The blog API behind [aimers.dev](https://aimers.dev). It is an
[axum](https://github.com/tokio-rs/axum) router compiled to WebAssembly and deployed as a single
[Cloudflare Worker](https://developers.cloudflare.com/workers/languages/rust/) — no container, no
Docker, no Durable Object:

| Route                 | Description                        |
|-----------------------|------------------------------------|
| `GET /api/blogs`      | Every blog summary, newest first.   |
| `GET /api/blogs/{id}` | One blog, including its markdown.   |

Anything else answers `404` with the same `{"error": ...}` shape.

## Content

`content/blogs/index.json` lists the published posts and `content/blogs/<id>.md` holds each body.
A Worker has no filesystem, so `build.rs` embeds every markdown file into the module at compile
time and `BlogStore::embedded` validates the pair on startup: an id must be a lowercase dashed slug,
its metadata must be complete, and it must have a document. Markdown that no index entry claims stays
embedded but unreachable, which is how an unpublished draft is kept next to the published posts.

Publishing a post is therefore an edit of `index.json` plus a redeploy — the listing and its JSON
encoding are computed once when the isolate starts, so serving it costs nothing per request.

## Configuration

| Variable      | Meaning                                                          |
|---------------|------------------------------------------------------------------|
| `SERVER_CORS` | Comma separated allowed CORS origins; may be empty to allow none. |

It is the only setting left — a Worker binds no address — and it lives in the `[vars]` table of
`wrangler.toml`, which `wrangler dev` also uses locally. Do not put it in a `.env`: Wrangler loads
that file as secrets, and a secret silently overrides `[vars]` in a deployment.

## Development

```bash
cargo test -p website_backend        # the router, the store, and the embedded content
npm install
npm run dev                          # wrangler dev — serves the real module on localhost
```

`cargo test` runs on the host toolchain: everything but `src/entry.rs` is target independent, so the
same router that answers in production is what the tests exercise.

## Deploying

```bash
rustup target add wasm32-unknown-unknown
npm run deploy                                              # wrangler deploy
npx wrangler deploy --dry-run --outdir=.wrangler/dry-run     # validate without publishing
```

`wrangler.toml` builds the crate with [`worker-build`](https://github.com/cloudflare/workers-rs),
which runs `wasm-bindgen` and `wasm-opt` and writes `build/index.js` next to the `.wasm` module. Two
workspace settings are overridden for that build and the reasons are recorded in `wrangler.toml`:
the `atomics` target feature (Workers have no shared memory) and `strip = "symbols"` (it removes
symbols `wasm-bindgen` needs).

The first deploy after the container was removed also applies the `v2` migration that deletes the
`AimerApiContainer` Durable Object class. Keep both `[[migrations]]` entries: they are a history,
not a state.
