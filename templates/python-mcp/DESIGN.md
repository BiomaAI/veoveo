# Datasheet MCP Server Design

Datasheet profiles tabular datasets and is the canonical template for a Python
MCP server hosted inside a Veoveo installation. Every obligation of the
hosted-server contract in [`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md),
revision 3, has a running Python reference implementation here.

## Standards And Protocols

| Standard or protocol | Supported boundary |
|---|---|
| Model Context Protocol `2026-07-28` | Stateless Streamable HTTP under `/datasheet/mcp`, mandatory Discover, per-request capabilities, JSON terminal responses, and request-scoped subscription streams |
| JSON Schema 2020-12 | Complete bounded tool input schemas produced by `veoveo_mcp.schema.mcp_input_schema`, including same-document references and composition |
| Tasks extension, SEP-2663 | Server-directed `tools/call`, `tasks/get`, `tasks/update`, `tasks/cancel`, and optional `notifications/tasks` through `subscriptions/listen` |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | The server-owned `ui://datasheet/workbench.html` Workbench previews and profiles inline CSV or governed artifacts. |
| CSV and Apache Parquet | Dataset inputs resolved from shared-plane artifacts or bounded inline CSV |
| `datasheet://` URI scheme | Canonical resource identities for reports, usage, artifacts, and documents |

## Domain

`preview_dataset` and `column_stats` answer directly from a CSV or Parquet
artifact or a small inline CSV. `profile_dataset` is task-required: the
dataset is materialized while the gateway identity is live and embedded in the
durable request, so `resume` recovery re-runs the profile from persisted state
alone. The full report is stored on the shared artifact plane through a write
capability reserved at submission, usage is recorded per task, and the result
is a typed `CallToolResult` with a `datasheet://artifact/{id}` resource link.

## Resources

| Resource | Content |
|---|---|
| `datasheet://reports` | Profile tasks visible to the caller |
| `datasheet://usage` and `datasheet://usage/task/{task_id}` | Per-task domain usage |
| `datasheet://artifact/{artifact_id}` | Shared-plane immutable artifacts |
| `datasheet://docs` and `datasheet://docs/{doc_id}` | Embedded server documents |
| `datasheet://contract` | Machine-readable contract declaration |

## Well-Known Surface

The server serves its document index at `datasheet://docs`, the `agents` and
`design` bodies at `datasheet://docs/{doc_id}`, and the contract declaration at
`datasheet://contract` through `veoveo_mcp.contract.docs`. The administrative
mount projects the same material read-only at `/datasheet/admin/docs/llms.txt`
and `/datasheet/admin/docs/{doc_id}`. The projection requires the same
gateway-issued internal identity as MCP. This directory's `AGENTS.md` and
`DESIGN.md` are embedded into the wheel at build time, so a deployed container
serves the manual of exactly the version it runs.

## Deployment

The server ships as an OCI image built from this directory's `Dockerfile`
against the installation's private Python index, runs as UID 10001, and is
deployed as the `datasheet-mcp` domain service of the versioned `veoveo` Helm
chart with a `Recreate` replacement strategy. Durable tasks live in the shared
SurrealDB platform store; schema migrations remain owned by `platform/store`.
