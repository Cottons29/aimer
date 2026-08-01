# website_backend

The blog API behind [aimer.cottonsofficial.com](https://aimer.cottonsofficial.com). It is an
[axum](https://github.com/tokio-rs/axum) server that reads the markdown in `content/blogs` and
exposes it as JSON:

| Route            | Description                              |
|------------------|------------------------------------------|
| `GET /api/blogs` | Every blog summary, newest first.        |
| `GET /api/blogs/{id}` | One blog, including its markdown.   |

## Configuration

The server takes three settings and supplies no defaults, so a deployment mistake fails at startup
instead of silently listening on the wrong address:

| Variable      | Meaning                                                         |
|---------------|-----------------------------------------------------------------|
| `SERVER_IP`   | IP address the listener binds to.                                |
| `SERVER_PORT` | TCP port the listener binds to.                                  |
| `SERVER_CORS` | Comma separated allowed CORS origins; may be empty to allow none.|

They are read from a `.env` file when one exists, otherwise from the process environment
(see `Config::resolve`). Two extra variables tune the process itself: `AIMER_ENV_FILE` picks a
different dotenv file and `AIMER_BLOG_DIR` a different content directory.

```bash
cp .env.example .env
cargo run -p website_backend
```

## Docker

The image is built from the repository root, because the crate is a Cargo workspace member:

```bash
docker build -f website_backend/Dockerfile -t aimer-website-backend ..
docker run --rm -p 3200:3200 -e SERVER_CORS=http://localhost:3000 aimer-website-backend
```

No `.env` is baked into the image; the `SERVER_*` variables are set as defaults in the `Dockerfile`
and overridden by whatever runs the container.

## Cloudflare Containers

`wrangler.toml` deploys the image as a [Container](https://developers.cloudflare.com/containers/)
fronted by the Worker in `worker/index.ts`. The Worker forwards `/api/*` to one of
`INSTANCE_COUNT` interchangeable container instances and answers everything else with `404`; the
`[vars]` table is passed to the container process as its `SERVER_*` environment.

Deploying requires a running Docker daemon — Wrangler builds the image locally and pushes it to
Cloudflare's managed registry:

```bash
npm install
npm run deploy        # wrangler deploy
npx wrangler deploy --dry-run --outdir=.wrangler/dry-run   # validate without publishing
npx wrangler containers list
```

Changing the port means changing `SERVER_PORT` in `[vars]` only: the Worker derives the container's
`defaultPort` from it and the binary binds to it.
