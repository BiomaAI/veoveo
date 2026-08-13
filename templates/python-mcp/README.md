# Python MCP server template

This directory is the canonical template for a Python MCP server hosted inside
a Veoveo installation. It ships as a complete working server — `datasheet`, a
dataset profiling service built on pandas — so every platform obligation has a
running reference implementation rather than a description.

The shared platform surface comes from the exact `veoveo-mcp` release selected by the
Veoveo compatibility manifest. The template stays thin: it owns its domain contract,
its computation, and one durable task type.

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

1. Copy `templates/python-mcp` to a working directory and rename the package
   (`datasheet_mcp` → `yourdomain_mcp`), the slug, the URI scheme in `uris.py`,
   and the default port.
2. Configure the installation's authenticated Python index outside source control.
   Keep the exact supported `veoveo-mcp` version, then run `uv lock`. The resulting
   lock belongs to the extension repository.
3. Replace `contract.py` and `engine.py` with your domain types and
   computation. Publish complete JSON Schema 2020-12 request models with
   `mcp_input_schema`. Keep the engine pure; it runs inside worker threads.
4. Keep `server/` structurally intact: config, ownership, the official Tasks
   handler, and the durable task module change names, not shape.
5. Package the workload with the private `veoveo-extension` Helm library. Publish an
   extension-owned gateway server fragment, image, application chart, domain smoke
   evidence, and extension release manifest. The installation repository owns the
   gateway binding, authorization, registry coordinates, selected release, and
   digest-pinned Helm or GitOps values.

The extension never edits the Veoveo chart or a complete Veoveo gateway document.
Follow the coding-agent runbook in
[`docs/EXTERNAL_REPOSITORY_INTEGRATION.md`](../../docs/EXTERNAL_REPOSITORY_INTEGRATION.md)
when publishing and integrating a repository created from this template.

## Running locally

```sh
export UV_DEFAULT_INDEX=https://packages.example.internal/simple
uv lock
uv sync --locked --all-extras
uv run pytest
uv run datasheet-mcp --port 8798 --public-base-url https://veoveo.example \
    --allow-loopback-hosts --artifact-service-url http://127.0.0.1:8790
```

For an image build, pass the index URL as a BuildKit secret. The URL may refer to a
customer-operated or Veoveo-operated private service and may be reachable only over
the installation network or VPN:

```sh
printf '%s' "$UV_DEFAULT_INDEX" | docker build \
  --secret id=veoveo-python-index,src=/dev/stdin \
  -t registry.example.internal/extensions/datasheet:0.1.0 .
```

The private index must provide the complete locked dependency set. The build does not
fall through to a public index.

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
