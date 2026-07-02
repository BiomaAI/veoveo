# veoveo

Veoveo's media MCP exposes image, video, audio, 3D, and LLM generation models through a
single protocol-maximal MCP server. Long-running generation is handled by MCP **tasks**
(SEP-1319) and provider webhooks instead of blocking calls.

The media server lives in its own crate; the generic full-protocol conformance CLI lives in
the contract crate.

## Architecture

```
┌──────────┐   MCP (streamable HTTP)   ┌─────────────────────────┐   provider API     ┌───────────┐
│  client   │ ────────────────────────▶ │  server (axum, :8787)   │ ◀────────────────▶ │ provider  │
│  (rmcp)   │ ◀──── notifications ───── │  /mcp  /webhooks  /files│                    │           │
└──────────┘                            └─────────────────────────┘                    └───────────┘
                                              ▲ public URL via cloudflared tunnel
```

- `/mcp` — MCP over streamable HTTP (rmcp 2.0)
- `/webhooks/*` — internal provider callback receivers
- `/files/*` — optional static dir so the provider can fetch input media by URL

## MCP surface

One tool, everything else is protocol:

| Surface | What |
|---|---|
| tool `run(model, input)` | task-**required** (SEP-1319); input validated against the model's JSON Schema before submit |
| resource `media://models` | compact catalog of all models (id, type, description, price) |
| template `media://model/{model_id}` | full input JSON Schema + pricing for one model |
| template `media://prediction/{id}` | live prediction state; **subscribable** — webhook arrival fires `notifications/resources/updated` |
| `completion/complete` | model-id autocompletion over the whole catalog |
| notifications | `tasks/status`, `progress`, `resources/updated`, `resources/list_changed` |

Task lifecycle: `tools/call` (+`task` metadata) → `CreateTaskResult` → poll `tasks/get`
(statusMessage carries the prediction id) → `tasks/result` returns output URLs as resource
links + structured content. `tasks/cancel` aborts. Provider webhook delivery is the only
server-side completion path.

## Setup

`.env`:

```
MEDIA_PROVIDER_API_KEY=...
MEDIA_PROVIDER_WEBHOOK_SECRET=whsec_...   # optional; enables webhook signature verification
```

## Run

### Docker Compose

The default development stack runs `media-mcp`, RustFS, and an optional named Cloudflare
tunnel. RustFS image/version and local S3-compatible wiring are defined in `compose.yaml`.

```sh
cp .env.example .env
# fill MEDIA_PROVIDER_API_KEY, MEDIA_PROVIDER_WEBHOOK_SECRET, PUBLIC_URL, and
# CLOUDFLARED_TUNNEL_TOKEN for the named tunnel.

docker compose --profile dev up --build

# include the tunnel service when the named tunnel token is configured
docker compose --profile dev --profile tunnel up --build
```

RustFS is available locally at `http://localhost:9000` with the development credentials
defined in Compose. Those credentials are for the local stack only.

Task and prediction metadata are persisted in SQLite. Local runs default to
`state.sqlite`; Compose stores the media server's state at
`/var/lib/veoveo/media/state.sqlite` on the `media_state` volume. RustFS is for artifact
bytes, not task durability.

### Local Process

```sh
# 1. public endpoint so the provider can reach the webhook + input files
cloudflared tunnel --url http://localhost:8787
# note the printed https://….trycloudflare.com URL

# 2. server
cargo run -p veoveo-media-mcp --bin server -- --port 8787 --static-dir assets \
    --public-url https://….trycloudflare.com

# 3. conformance CLI
cargo run -p veoveo-mcp-contract --bin conformance -- info
cargo run -p veoveo-mcp-contract --bin conformance -- models kling --type image-to-video
cargo run -p veoveo-mcp-contract --bin conformance -- complete gpt-image
cargo run -p veoveo-mcp-contract --bin conformance -- schema openai/gpt-image-2/edit
cargo run -p veoveo-mcp-contract --bin conformance -- run openai/gpt-image-2/edit \
    --input '{"prompt":"add a red wizard hat","images":["https://….trycloudflare.com/files/gol-real-roblox.jpeg"]}'
```

`--public-url` is required for generation because providers must be able to deliver
webhook callbacks. `/files` URLs also need that public base URL to be reachable by the
provider.

## Layout

```
Cargo.toml                                      veoveo workspace manifest
crates/mcp-contract/                           reusable Veoveo MCP contract crate
crates/mcp-contract/src/bin/conformance.rs     generic Veoveo MCP conformance CLI
crates/media-mcp/src/lib.rs                    shared media MCP crate (veoveo_media_mcp)
crates/media-mcp/src/provider.rs               internal provider API client + types
crates/media-mcp/src/state.rs                  per-server SQLite task/prediction state
crates/media-mcp/src/webhook.rs                internal webhook signature verification
crates/media-mcp/src/uris.rs                   media:// URI scheme
crates/media-mcp/src/bin/server.rs             MCP server
```

`cargo test --workspace` covers signature verification, URI parsing, schema extraction,
and the shared contract crate.
