# Python MCP server template

This directory is the template to copy for a Python MCP server hosted inside
a Veoveo installation. It ships as a complete working server, `datasheet`, a
dataset profiling service built on pandas. Each platform requirement in the table
below points to the code that meets it.

Shared platform code comes from `sdk/python` in the same Veoveo fork. The template itself holds only its domain contract, its
computation, and one durable task type.

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
| MCP `2026-07-28` with mandatory Discover, official Tasks, and request-scoped `subscriptions/listen` | MCP Python SDK 2.0 + `veoveo_mcp.task_extension` + `server/task_extension.py` |
| Durable tasks in the SurrealDB platform store with atomic outbox events, UUIDv7 ids, leases, recovery classes, retention pins | `veoveo_mcp.tasks` + `server/profile_task.py` |
| Artifact output through task-bound write capabilities; no identity minting in background work | `server/profile_task.py` |
| Per-task domain usage rows and `{scheme}://usage/task/{id}` resources | `server/profile_task.py`, `server/mcp_server.py` |
| Task ownership checks by principal, profile, tenant, and data labels | `server/ownership.py` |
| Well-known surface: `datasheet://docs`, `datasheet://contract`, and the read-only admin `docs/llms.txt` projection from embedded `AGENTS.md` and `DESIGN.md` | `docs.py`, `server/mcp_server.py`, `server/main.py` |

## Creating a new server from this template

1. Copy this template to `servers/<domain>-mcp` in the fork. Rename the package,
   slug, URI scheme and default port.
2. Point `tool.uv.sources.veoveo-mcp` at `../../sdk/python` and update the lockfile.
3. Replace the domain contract and computation while preserving auth, Tasks,
   subscriptions, artifact handling and embedded server documentation.
4. Add a root Bake target and Helm workload using the existing domain patterns.
5. Register the server and Apps in the complete gateway control plane. Installation
   policy continues to own exposure, scopes, endpoints and Secret references.

Follow [`Fork Development`](../../docs/FORK_DEVELOPMENT.md) for upstream merges and
publication. Run local commands from this directory:

```sh
uv sync --locked --all-extras
uv run --locked --all-extras pytest -q
```

The container builds from the repository root using the template Dockerfile and
local SDK source. The root `datasheet-mcp` Bake target uses that same path, so the
image build needs no package-index credentials.

SurrealDB credentials and the internal trust JWKS come from the same
`VEOVEO_SURREAL_*` and `VEOVEO_INTERNAL_TRUST_JWKS` variables the Rust servers
use. `platform/store` owns schema migrations; this
server never applies them.

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
