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
│  (rmcp)   │ ◀──── notifications ───── │ /media/mcp /media/...   │                   │           │
└──────────┘                            └─────────────────────────┘                    └───────────┘
                                              ▲ public base URL via Cloudflare Tunnel
```

- `/media/mcp` — MCP over streamable HTTP (rmcp 2.0)
- `/media/webhooks` — internal provider callback receiver
- `/media/files/*` — optional static dir so the provider can fetch input media by URL
- `/media/artifacts/*` — GET-only immutable content route for artifact bytes already surfaced by MCP

## MCP surface

One tool, everything else is protocol:

| Surface | What |
|---|---|
| tool `run(model, input)` | task-**required** (SEP-1319); input validated against the model's JSON Schema before submit; advertises a typed structured output schema |
| prompts | `media-model-select`, `media-image-edit`, `media-video-generate`, `media-task-review` |
| resource `media://models` | compact catalog of all models (id, type, description, price) |
| template `media://model/{model_id}` | full input JSON Schema + pricing for one model |
| template `media://prediction/{id}` | live prediction state; **subscribable** — webhook arrival fires `notifications/resources/updated` |
| template `media://artifact/{sha256}` | server-owned immutable artifact bytes stored in RustFS/S3-compatible storage |
| resource `media://usage` | index of task usage resources |
| template `media://usage/task/{task_id}` | usage estimates/actuals for one task |
| `completion/complete` | model-id autocompletion over the whole catalog |
| notifications | `tasks/status`, `progress`, `resources/updated`, `resources/list_changed` |

Task lifecycle: `tools/call` (+`task` metadata) → `CreateTaskResult` → poll `tasks/get`
(statusMessage carries the prediction id) → `tasks/result` returns `media://artifact/{sha256}`
resource links + structured content. `tasks/cancel` aborts. Provider webhook delivery is
the only server-side completion path.

List surfaces owned by Veoveo servers (`tools/list`, `prompts/list`, `resources/list`,
`resources/templates/list`, and `tasks/list`) honor MCP pagination cursors.

## Public Routing

`PUBLIC_BASE_URL` is the public origin for the whole Veoveo deployment. Its hostname is
opaque to the contract; `https://veoveo.bioma.ai`,
`https://staging.veoveo.bioma.ai`, and an enterprise-owned hostname are all equivalent
as long as they route to the deployment.

Each MCP server owns one path segment below that origin:

| Server | MCP endpoint | Provider webhook | Input files |
|---|---|---|---|
| media | `{PUBLIC_BASE_URL}/media/mcp` | `{PUBLIC_BASE_URL}/media/webhooks` | `{PUBLIC_BASE_URL}/media/files/*` |

## Setup

`.env`:

```
MEDIA_PROVIDER_API_KEY=...
MEDIA_PROVIDER_WEBHOOK_SECRET=whsec_...   # optional; enables webhook signature verification
PUBLIC_BASE_URL=https://veoveo.bioma.ai
```

## Run

### Docker Compose

The default development stack runs `media-mcp`, RustFS, and the managed Cloudflare
tunnel. RustFS image/version and local S3-compatible wiring are defined in `compose.yaml`.

```sh
cp .env.example .env
# fill MEDIA_PROVIDER_API_KEY, MEDIA_PROVIDER_WEBHOOK_SECRET, PUBLIC_BASE_URL, and
# CLOUDFLARED_TUNNEL_TOKEN for the managed Cloudflare tunnel.

just compose-up
```

The media image uses BuildKit cache mounts for Cargo registry, git, and target output.
The workspace also pins `DUCKDB_DOWNLOAD_LIB=1` in `.cargo/config.toml`, so builds link
the matching prebuilt DuckDB library instead of compiling DuckDB C++ sources. First builds
still download crates and DuckDB; rebuilds reuse the BuildKit caches.

RustFS is available locally at `http://localhost:9000` with the development credentials
defined in Compose. Compose also creates the `media-artifacts` bucket. Those credentials
are for the local stack only.

Task, prediction, artifact metadata, and usage metadata are persisted in DuckDB. The
shared contract crate owns the DuckDB usage analytics schema so every MCP server can
record estimates and actual billing rows the same way. Local runs default to
`state.duckdb`; Compose stores the media server's state at
`/var/lib/veoveo/media/state.duckdb` on the `media_state` volume. RustFS stores artifact
bytes only.

### Logs

The server writes operational logs to stdout/stderr. Docker Compose exposes those logs
through:

```sh
just logs media-mcp
just logs cloudflared
```

Enterprise deployments should collect container stdout/stderr with their platform-native
logging stack, such as Kubernetes logging, Docker logging drivers, CloudWatch, Splunk,
Datadog, or an OpenTelemetry collector. Veoveo does not store application logs in DuckDB
or RustFS. DuckDB is for task, artifact, prediction, and usage analytics state; object
storage is for artifact bytes.

Gateway secret references are resolved by source, not stored in control data. Local
`env` secrets name the required variable. HashiCorp Vault and HCP Vault use KV v2 locators
such as `kv2://secret/veoveo/gateway#client_secret` or
`kv2://secret/veoveo/gateway?version=3#client_secret`, and require explicit `VAULT_ADDR`
and `VAULT_TOKEN`.

Deployment profiles declare gateway-to-server service-to-service security explicitly.
Local Compose uses gateway-signed internal JWTs over the private Docker network. Enterprise
and regulated profiles must use mTLS or service-mesh mTLS transport plus gateway-signed
assertions.

### Local Process

```sh
# 1. ensure PUBLIC_BASE_URL routes to this process, using your ingress/proxy/tunnel

# 2. server (requires a reachable S3-compatible artifact store)
export AWS_ACCESS_KEY_ID=rustfsadmin
export AWS_SECRET_ACCESS_KEY=rustfsadmin
export AWS_DEFAULT_REGION=us-east-1
cargo run -p veoveo-media-mcp --bin server -- --port 8787 --static-dir assets \
    --public-base-url https://veoveo.bioma.ai \
    --artifact-endpoint http://localhost:9000 --artifact-bucket media-artifacts

# 3. conformance CLI
cargo run -p veoveo-mcp-contract --bin conformance -- info
cargo run -p veoveo-mcp-contract --bin conformance -- prompts
cargo run -p veoveo-mcp-contract --bin conformance -- prompt media-image-edit \
    --arguments '{"image_url":"https://veoveo.bioma.ai/media/files/gol-real-roblox.jpeg","edit_goal":"add a red wizard hat"}'
cargo run -p veoveo-mcp-contract --bin conformance -- models kling --type image-to-video
cargo run -p veoveo-mcp-contract --bin conformance -- complete gpt-image
cargo run -p veoveo-mcp-contract --bin conformance -- schema openai/gpt-image-2/edit
cargo run -p veoveo-mcp-contract --bin conformance -- run openai/gpt-image-2/edit \
    --input '{"prompt":"add a red wizard hat","images":["https://veoveo.bioma.ai/media/files/gol-real-roblox.jpeg"]}'
cargo run -p veoveo-mcp-contract --bin conformance -- usage <task-id>
cargo run -p veoveo-mcp-contract --bin conformance -- artifact <sha256>
```

`--public-base-url` is required for generation because providers must be able to deliver
webhook callbacks. `/media/files` URLs also need that public base URL to be reachable by the
provider.

## Layout

```
Cargo.toml                                      veoveo workspace manifest
crates/mcp-contract/                           reusable Veoveo MCP contract crate
crates/mcp-contract/src/bin/conformance.rs     generic Veoveo MCP conformance CLI
crates/mcp-contract/src/analytics.rs           shared DuckDB usage analytics schema/store
crates/mcp-contract/src/deployment.rs          shared public URL/server mount contract
crates/mcp-contract/src/storage.rs             artifact store contract/types
crates/mcp-contract/src/usage.rs               usage contract/types
crates/media-mcp/src/lib.rs                    shared media MCP crate (veoveo_media_mcp)
crates/media-mcp/src/artifacts.rs              S3-compatible artifact store implementation
crates/media-mcp/src/provider.rs               internal provider API client + types
crates/media-mcp/src/state.rs                  per-server DuckDB task/prediction/artifact state
crates/media-mcp/src/webhook.rs                internal webhook signature verification
crates/media-mcp/src/uris.rs                   media:// URI scheme
crates/media-mcp/src/bin/server.rs             MCP server
```

`cargo test --workspace` covers signature verification, URI parsing, schema extraction,
and the shared contract crate.

## Command Recipes

Use `just --list` for the maintained command recipes. The common path is:

```sh
just compose-up
just health https://veoveo.bioma.ai
just e2e https://veoveo.bioma.ai
```
