# Python MCP server template

This directory is the canonical template for a Python MCP server hosted inside
a Veoveo installation. It ships as a complete working server — `datasheet`, a
dataset profiling service built on pandas — so every platform obligation has a
running reference implementation rather than a description.

The shared platform surface lives in `sdk/python`. The template stays
thin: it owns its domain contract, its computation, and one durable task type.

## What the platform contract requires

A hosted server, in any language, provides all of the following. The file
listed next to each obligation is where this template satisfies it.

| Obligation | Where |
|---|---|
| Sessionful Streamable HTTP MCP with event-stream responses at `/{slug}/mcp` | `server/main.py` |
| `/{slug}/healthz` and `/{slug}/readyz` | `server/main.py` |
| Host-authority allowlist, 421 for untrusted hosts | `veoveo_mcp.host` |
| Gateway Ed25519 assertion verification, `kid` required | `veoveo_mcp.internal_auth` |
| Self-contained JSON Schema 2020-12 tool inputs with explicit property types | `veoveo_mcp.schema` |
| Full MCP surface: tools, resources, templates, prompts, completions, pagination, typed structured content | `server/mcp_server.py` |
| Final task extension, protocol `2026-06-30`: `server/discover`, task-augmented `tools/call`, `tasks/get`, `tasks/update`, `tasks/cancel`, `subscriptions/listen` SSE | `veoveo_mcp.task_extension` + `server/task_extension.py` |
| Durable tasks in the SurrealDB platform store with atomic outbox events, UUIDv7 ids, leases, recovery classes, retention pins | `veoveo_mcp.tasks` + `server/profile_task.py` |
| Artifact output through task-bound write capabilities; no identity minting in background work | `server/profile_task.py` |
| Per-task domain usage rows and `{scheme}://usage/task/{id}` resources | `server/profile_task.py`, `server/mcp_server.py` |
| Task ownership checks by principal, profile, tenant, and data labels | `server/ownership.py` |

## Creating a new server from this template

1. Copy `templates/python-mcp` to a working directory and rename the package
   (`datasheet_mcp` → `yourdomain_mcp`), the slug, the URI scheme in `uris.py`,
   and the default port.
2. Replace `contract.py` and `engine.py` with your domain types and
   computation. Publish request models with `mcp_input_schema`; recursive tool
   arguments are not supported. Keep the engine pure; it runs inside worker threads.
3. Keep `server/` structurally intact: config, ownership, the task-extension
   handler, and the durable task module change names, not shape.
4. Add the workload to `deploy/helm/veoveo`, register it in the intended
   profile-owned gateway JSON, add its slug to the artifact service's allowed
   audiences, and extend the Rust smoke
   (`testing/smoke/src/bin/smoke/scenarios/datasheet.rs` is the model). All
   smoke logic stays in Rust.

## Running locally

```
uv sync --all-extras
uv run pytest
uv run datasheet-mcp --port 8798 --public-base-url https://veoveo.example \
    --allow-loopback-hosts --artifact-service-url http://127.0.0.1:8790
```

SurrealDB credentials and the internal trust JWKS come from the same
`VEOVEO_SURREAL_*` and `VEOVEO_INTERNAL_TRUST_JWKS` variables the Rust servers
use. Schema migrations remain owned by `platform/store`; this server
never applies them.

## The example domain

`datasheet` profiles tabular datasets:

- `preview_dataset` and `column_stats` answer directly from a CSV/Parquet
  artifact or small inline CSV.
- `profile_dataset` is task-required. The dataset is materialized while the
  gateway identity is live and embedded in the durable request, so `resume`
  recovery re-runs the profile from persisted state alone. The full report is
  stored on the shared artifact plane through a capability reserved at
  submission, usage is recorded per task, and the result is a typed
  `CallToolResult` with a `datasheet://artifact/{id}` resource link.
