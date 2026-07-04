# veoveo

Veoveo exposes hosted MCP servers through a production gateway. The current hosted server
is `media-mcp`, which exposes image, video, audio, 3D, and LLM generation models.
Long-running generation is handled by MCP **tasks** (SEP-1319) and provider webhooks
instead of blocking calls.

The gateway is the normal client entrypoint. The media server lives in its own crate; the
generic full-protocol conformance CLI lives in its own conformance crate.

## Architecture

```
┌──────────┐   MCP (streamable HTTP)   ┌─────────────────────┐   single origin   ┌─────────────────────────┐
│  client  │ ────────────────────────▶ │ Cloudflare Tunnel   │ ────────────────▶ │ edge proxy (:8780)     │
│  (rmcp)  │ ◀──── notifications ───── │ or enterprise edge  │                   │ /mcp, /oauth, /media   │
└──────────┘                            └─────────────────────┘                   └───────────┬─────────────┘
                                                                                                │
                                        ┌─────────────────────────┐   internal MCP              │
                                        │ gateway (axum, :8788)  │ ◀────────────────────────────┤
                                        │ /mcp/operator          │ ────────────────────────────▶ │
                                        └───────────┬─────────────┘                              │
                                                    │ internal MCP                               │ provider/content paths
                                                    ▼                                            ▼
                                           ┌─────────────────────────┐                   ┌───────────┐
                                           │ media-mcp (axum, :8787)│ ────────────────▶ │ provider  │
                                           │ /media/mcp /media/...  │  provider API     └───────────┘
                                           └─────────────────────────┘
```

- `/mcp/operator` — operator gateway MCP profile over streamable HTTP (rmcp 2.0), routed by the edge proxy to `mcp-gateway`
- `/media/mcp` — internal media MCP endpoint for conformance and service composition; not routed by the public edge and requires a gateway-signed internal token
- `/media/webhooks` — provider callback receiver routed by the edge proxy to `media-mcp`
- `/media/files/*` — optional static dir so the provider can fetch input media by URL
- `/media/artifacts/*` — GET-only immutable content route for artifact bytes already surfaced by MCP

## MCP surface

One media tool, everything else is protocol. Direct media exposes `run`; the gateway exposes
that tool as `media__run` because it collapses all hosted servers into one outward MCP
surface.

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

External MCP clients use the gateway profile endpoint. The edge proxy is the public
boundary for the single origin. Hosted servers still own provider plumbing paths below the
same origin:

| Surface | Endpoint |
|---|---|
| operator gateway profile | `{PUBLIC_BASE_URL}/mcp/operator` |
| admin gateway profile | `{PUBLIC_BASE_URL}/mcp/admin` |
| media webhook | `{PUBLIC_BASE_URL}/media/webhooks` |
| media input files | `{PUBLIC_BASE_URL}/media/files/*` |
| media artifact bytes | `{PUBLIC_BASE_URL}/media/artifacts/*` |

`/media/mcp` is intentionally not a public client route. For local conformance or service
debugging, use the direct service endpoint with a gateway-signed internal token, such as
`http://localhost:8787/media/mcp` in the development Compose stack.

## Setup

`.env`:

```
MEDIA_PROVIDER_API_KEY=...
MEDIA_PROVIDER_WEBHOOK_SECRET=whsec_...   # optional; enables webhook signature verification
VEOVEO_INTERNAL_TOKEN_SECRET=...           # signs gateway-to-server assertions
VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64=...
PUBLIC_BASE_URL=https://veoveo.bioma.ai
```

## Run

### Docker Compose

The default development stack runs the edge proxy, `mcp-gateway`, `media-mcp`, RustFS,
Postgres for gateway control data, an OpenTelemetry collector, and the managed
Cloudflare tunnel. RustFS image/version, Postgres image/version, edge routing, and local
S3-compatible wiring are defined in `compose.yaml`.
The Cloudflare named tunnel should route the public hostname to `http://edge:8080`;
individual MCP server containers are not public tunnel targets.
Published development ports bind to `127.0.0.1` only. The local edge is available at
`http://localhost:8780`; direct gateway, media, RustFS, and OTEL ports are local debugging
surfaces, not public ingress.

```sh
cp .env.example .env
# fill MEDIA_PROVIDER_API_KEY, MEDIA_PROVIDER_WEBHOOK_SECRET, PUBLIC_BASE_URL,
# VEOVEO_INTERNAL_TOKEN_SECRET, VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64,
# and CLOUDFLARED_TUNNEL_TOKEN for the managed Cloudflare tunnel.

just compose-up
just info
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
record estimates and actual billing rows the same way. Gateway profiles, tenants,
clients, policies, and hosted-server manifests are seeded into Postgres as immutable
control-plane revisions; `mcp-gateway` loads the active Postgres revision in Compose.
Deployment profiles declare typed DuckDB state stores for the gateway and each hosted MCP
server. Local runs default to `state.duckdb`; Compose stores the media server's state at
`/var/lib/veoveo/media/state.duckdb` on the `media_state` volume. Gateway runtime state
and audit evidence live at `/var/lib/veoveo/gateway/state.duckdb` on the `gateway_state`
volume. RustFS stores artifact bytes only.

### Logs

The server writes operational logs to stdout/stderr. Docker Compose exposes those logs
through:

```sh
just logs mcp-gateway
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

Deployment network rules are typed by target kind. Profiles must declare explicit ingress
and egress for the gateway, hosted MCP servers, object stores, telemetry collectors,
enterprise secret managers, identity providers, and external provider APIs instead of
leaving those boundaries as opaque hostnames.

Gateway control-plane server manifests also declare upstream transport security next to
the upstream URL. Local loopback tests use `loopback_http`, Docker Compose service-name
routes use `compose_internal_http`, and production manifests should use `tls`,
`mutual_tls`, or `service_mesh_mtls`. Public plaintext HTTP upstreams are rejected by
contract validation.

Gateway OAuth client registrations are also validated against implemented token endpoint
paths. Browser authorization-code and enterprise-managed authorization clients use
`auth_methods: ["none"]`; headless client-credentials clients use
`auth_methods: ["private_key_jwt"]`. Client-secret and TLS-client-auth OAuth clients are
not accepted until those flows are implemented end to end.

### Admin Operations

Gateway admin operations are authenticated and policy-gated through `/admin/{profile}`.
Control-plane reload/apply, JWT revocation, and revocation pruning emit structured audit
events in the gateway DuckDB state. Each operation records the policy decision and a
separate `*/result` event with `operation_status` evidence such as `succeeded`,
`rejected`, or `failed`. The maintained local recipes call the admin API; they do not
mutate the gateway state database directly:

```sh
just gateway-revoke-jwt <jwt-id> 2026-07-02T20:00:00Z
just gateway-prune-revoked-jwts
cargo run -p veoveo-mcp-gateway --bin gateway -- audit-metadata-summary \
  --state-db data/gateway/state.duckdb --metadata-key operation_status
cargo run -p veoveo-mcp-gateway --bin gateway -- auth-audit-method-summary \
  --state-db data/gateway/state.duckdb
cargo run -p veoveo-mcp-gateway --bin gateway -- auth-audit-metadata-summary \
  --state-db data/gateway/state.duckdb --metadata-key principal_data_labels
```

Artifact metadata carries typed compliance fields from the gateway principal, including
`tenant_id`, `owner_id`, `data_labels`, and `retention_expires_at`. Media still enforces
artifact and task access from durable owner rows; object metadata is exported evidence, not
the only authorization source.

### Contract Schemas

External Python and TypeScript MCP servers should use the Rust contract as the source of
truth, not hand-maintained copies. Export JSON Schemas from `veoveo-mcp-contract`:

```sh
just contract-schemas schemas
```

The export includes gateway control data, policy/audit events, deployment profiles,
gateway runtime projection records, internal identity assertions, artifact metadata,
generation outputs, and usage reports. The maintained smoke suite runs
`just smoke-contract-schemas` so schema generation stays wired into the contract.

### Local Process

Run the media server and gateway in separate shells, with the same
`VEOVEO_INTERNAL_TOKEN_SECRET` value in both.

```sh
# 1. ensure PUBLIC_BASE_URL routes to this process, using your ingress/proxy/tunnel

# 2. media server (requires a reachable S3-compatible artifact store)
export AWS_ACCESS_KEY_ID=rustfsadmin
export AWS_SECRET_ACCESS_KEY=rustfsadmin
export AWS_DEFAULT_REGION=us-east-1
export VEOVEO_INTERNAL_TOKEN_SECRET=local-development-secret-at-least-32-bytes
cargo run -p veoveo-media-mcp --bin server -- --port 8787 --static-dir assets \
    --public-base-url https://veoveo.bioma.ai \
    --artifact-endpoint http://localhost:9000 --artifact-bucket media-artifacts

# 3. gateway
export VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64="$(cargo run -q -p veoveo-mcp-conformance --bin conformance -- gateway-private-key-der-b64)"
export VEOVEO_GATEWAY_CONTROL_DB_URL=postgresql://veoveo_gateway:veoveo_gateway@localhost:5432/veoveo_gateway
cargo run -p veoveo-mcp-gateway --bin gateway -- control-plane-seed \
    --control-db-url "$VEOVEO_GATEWAY_CONTROL_DB_URL" \
    --control-plane configs/gateway.bioma.json \
    --applied-by local#operator
cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port 8788 \
    --public-base-url https://veoveo.bioma.ai \
    --control-db-url "$VEOVEO_GATEWAY_CONTROL_DB_URL" \
    --state-db data/gateway/state.duckdb

# 4. conformance CLI through the gateway
unset VEOVEO_INTERNAL_TOKEN_SECRET
export MCP_BEARER_TOKEN="$(cargo run -q -p veoveo-mcp-conformance --bin conformance -- gateway-token-exchange \
    --token-url http://localhost:8788/oauth/token --scope operator:use)"
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator info
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator prompts
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator prompt media-image-edit \
    --arguments '{"image_url":"https://veoveo.bioma.ai/media/files/gol-real-roblox.jpeg","edit_goal":"add a red wizard hat"}'
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator models kling --type image-to-video
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator complete gpt-image
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator schema openai/gpt-image-2/edit
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator run openai/gpt-image-2/edit \
    --tool-name media__run \
    --input '{"prompt":"add a red wizard hat","images":["https://veoveo.bioma.ai/media/files/gol-real-roblox.jpeg"]}'
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator usage <task-id>
cargo run -p veoveo-mcp-conformance --bin conformance -- --url http://localhost:8788/mcp/operator artifact <sha256>
```

`--public-base-url` is required for generation because providers must be able to deliver
webhook callbacks. `/media/files` URLs also need that public base URL to be reachable by the
provider.

## Layout

```
Cargo.toml                                      veoveo workspace manifest
crates/mcp-contract/                           reusable Veoveo MCP contract crate
crates/mcp-contract/src/analytics.rs           shared DuckDB usage analytics schema/store
crates/mcp-contract/src/deployment.rs          shared public URL/server mount contract
crates/mcp-contract/src/storage.rs             artifact store contract/types
crates/mcp-contract/src/usage.rs               usage contract/types
crates/mcp-conformance/src/bin/conformance.rs  generic Veoveo MCP conformance CLI
crates/mcp-gateway/src/bin/gateway.rs          production MCP gateway
crates/mcp-gateway/src/mcp.rs                  full-protocol gateway MCP handler
crates/mcp-gateway/src/state.rs                gateway DuckDB runtime/audit state
configs/gateway.bioma.json                     typed Bioma demo gateway control plane
configs/gateway.local.json                     typed local placeholder gateway control plane
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
just info
just e2e https://veoveo.bioma.ai
```
