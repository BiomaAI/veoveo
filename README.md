# wavespeed-mcp-server

An MCP (Model Context Protocol) gateway to [WaveSpeed](https://wavespeed.ai) — every model in
the WaveSpeed catalog (~1000: image, video, audio, 3D, LLM) exposed through a single,
protocol-maximal MCP server, with long-running generation handled by MCP **tasks** (SEP-1319)
and WaveSpeed **webhooks** instead of blocking calls.

Both sides live here: a server binary and a full-protocol client CLI, sharing one lib crate.

## Architecture

```
┌──────────┐   MCP (streamable HTTP)   ┌─────────────────────────┐   REST + webhook   ┌───────────┐
│  client   │ ────────────────────────▶ │  server (axum, :8787)   │ ◀────────────────▶ │ WaveSpeed │
│  (rmcp)   │ ◀──── notifications ───── │  /mcp  /webhooks  /files│                    │    API    │
└──────────┘                            └─────────────────────────┘                    └───────────┘
                                              ▲ public URL via cloudflared tunnel
```

- `/mcp` — MCP over streamable HTTP (rmcp 2.0)
- `/webhooks/wavespeed` — WaveSpeed callback receiver, HMAC-SHA256 verified
- `/files/*` — optional static dir so WaveSpeed can fetch input media by URL

## MCP surface

One tool, everything else is protocol:

| Surface | What |
|---|---|
| tool `run(model, input)` | task-**required** (SEP-1319); input validated against the model's JSON Schema before submit |
| resource `wavespeed://models` | compact catalog of all models (id, type, description, price) |
| template `wavespeed://model/{model_id}` | full input JSON Schema + pricing for one model |
| template `wavespeed://prediction/{id}` | live prediction state; **subscribable** — webhook arrival fires `notifications/resources/updated` |
| `completion/complete` | model-id autocompletion over the whole catalog |
| notifications | `tasks/status`, `progress`, `resources/updated`, `resources/list_changed` |

Task lifecycle: `tools/call` (+`task` metadata) → `CreateTaskResult` → poll `tasks/get`
(statusMessage carries the prediction id) → `tasks/result` returns output URLs as resource
links + structured content. `tasks/cancel` aborts. Webhook push resolves the task; a slow
poll of the WaveSpeed API is the fallback when no public URL is configured.

## Setup

`.env`:

```
WAVESPEED_API_KEY=...
WAVESPEED_WEBHOOK_SECRET=whsec_...   # optional; enables webhook signature verification
```

## Run

```sh
# 1. public endpoint so WaveSpeed can reach the webhook + input files
cloudflared tunnel --url http://localhost:8787
# note the printed https://….trycloudflare.com URL

# 2. server
cargo run -p wavespeed-mcp-server --bin server -- --port 8787 --static-dir assets \
    --public-url https://….trycloudflare.com

# 3. client
cargo run -p wavespeed-mcp-server --bin client -- info
cargo run -p wavespeed-mcp-server --bin client -- models kling --type image-to-video
cargo run -p wavespeed-mcp-server --bin client -- complete gpt-image
cargo run -p wavespeed-mcp-server --bin client -- schema openai/gpt-image-2/edit
cargo run -p wavespeed-mcp-server --bin client -- run openai/gpt-image-2/edit \
    --input '{"prompt":"add a red wizard hat","images":["https://….trycloudflare.com/files/gol-real-roblox.jpeg"]}'
```

Without `--public-url` the server still works — it just polls WaveSpeed instead of
receiving webhooks, and `/files` URLs won't be reachable by WaveSpeed.

## Layout

```
Cargo.toml                                      workspace manifest
crates/mcp-foundation/                         reusable MCP server foundation
crates/wavespeed-mcp-server/src/lib.rs         shared WaveSpeed crate (wavespeed_mcp)
crates/wavespeed-mcp-server/src/wavespeed.rs   WaveSpeed v3 API client + types
crates/wavespeed-mcp-server/src/webhook.rs     HMAC-SHA256 webhook verification
crates/wavespeed-mcp-server/src/uris.rs        wavespeed:// URI scheme
crates/wavespeed-mcp-server/src/bin/server.rs  MCP server
crates/wavespeed-mcp-server/src/bin/client.rs  MCP client CLI
```

`cargo test --workspace` covers signature verification, URI parsing, schema extraction,
and the shared foundation crate.
