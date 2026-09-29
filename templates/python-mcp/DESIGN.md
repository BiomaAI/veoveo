# Datasheet MCP Server Design

Datasheet profiles tabular datasets and is the canonical template for a Python
MCP server hosted inside a Veoveo installation. It implements the Python surface
of the hosted-server contract in
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.
Its [compliance declaration](AGENTS.md#contract-compliance) lists the implemented
hosted-server requirements.

## Standards And Protocols

| Standard or protocol | Supported boundary |
|---|---|
| Model Context Protocol `2026-07-28` | Stateless Streamable HTTP under `/datasheet/mcp`, mandatory Discover, per-request capabilities, JSON terminal responses, and request-scoped subscription streams |
| JSON Schema 2020-12 | Complete bounded tool input schemas produced by `veoveo_mcp.schema.mcp_input_schema`, including same-document references and composition |
| Tasks extension, SEP-2663 | Server-directed `tools/call`, `tasks/get`, `tasks/update`, `tasks/cancel`, and optional `notifications/tasks` through `subscriptions/listen` |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | The server-owned `ui://datasheet/workbench.html` Workbench previews and profiles inline CSV or governed artifacts. |
| CSV and Apache Parquet | Dataset inputs resolved from shared-plane artifacts or bounded inline CSV |
| `datasheet://` URI scheme | Canonical resource identities for reports, usage, artifacts, and documents |
| Datasheet catalog cursor version 1 | Collection-bound JSON encoded as unpadded base64url, carried in the URI's `cursor` query parameter |

## Domain

`preview_dataset` and `column_stats` answer directly from a CSV or Parquet
artifact or a small inline CSV. `profile_dataset` is task-required: the
dataset is materialized while the gateway identity is live and embedded in the
durable request, so `resume` recovery re-runs the profile from persisted state
alone. The full report is stored on the shared artifact plane through a write
capability reserved at submission, usage is recorded per task, and the result
is a typed `CallToolResult` with a `datasheet://artifact/{id}` resource link.

Dataset reads pass the configured `max_dataset_bytes` ceiling to the SDK before
downloading bytes. Report resource reads use `max_artifact_bytes`. These limits
bound the materialized consumer input independently of the upload's admitted size;
large-file acceptance does not certify pandas at that scale.

## Resources

| Resource | Content |
|---|---|
| `datasheet://reports{?cursor}` | Caller-visible profile Tasks, ordered by creation time and Task ID |
| `datasheet://usage{?cursor}` and `datasheet://usage/task/{task_id}` | Usage catalog ordered by Task ID, and per-task domain usage |
| `datasheet://artifact/{artifact_id}` | Shared-plane immutable artifacts |
| `datasheet://docs` and `datasheet://docs/{doc_id}` | Embedded server documents |
| `datasheet://contract` | Machine-readable contract declaration |

Report and usage catalogs return an object with `items`, `limit: 100`, `next_cursor`
and `next_uri`. A final page has null continuation fields. The server builds each
continuation from the last item of a full page after selecting one lookahead row.
`ReportCursor` carries creation time and UUIDv7 Task ID; `UsageCursor` carries the
Task ID. Each rejects the other collection, unknown versions, extra or duplicate
fields and malformed encodings. A cursor is a position, never an authorization grant.

The owner query admits Tasks before SQL ordering and limits. Usage selection applies
that policy to the linked Task and checks the usage row's server and tenant before
grouping, limiting or decoding. Exact usage reads and Task-ID completion use the same
selection. Completion filters the prefix in SQL and returns at most 100 values, with
`hasMore` derived from one lookahead row.

`uris.py` parses addresses into resource variants and builds their components through
the standard URI library. Resource discovery lists roots and templates without reading
stored Tasks. The Workbench requests report pages through the returned `next_uri` when
the caller selects More reports.

## Task Admission

The Tasks adapter composes the SDK's `OwnerTaskQuery` with Datasheet's typed
`profile_dataset` operation name. Exact reads and subscription admission select the
current owner, tenant, profile and label clearance in SQL before decoding. Datasheet
allows its caller's Tasks across Work Contexts; the query does not add context equality.
Domains that require that restriction opt into `in_work_context()` in the SDK.

Cancellation and input-response transactions repeat the owner and operation predicates.
Waiting-task projections read pending input through the same query. Notifications
coalesce to current authorized state and do not decode retained outbox snapshots.
Subscriptions accept up to 256 Task IDs and reconcile current state every 15 seconds
to cover event-retention gaps. A disconnected source ends the stream; a new request
admits its IDs again. The SDK owns reader cleanup through acknowledgement and delivery.

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

The server ships as an OCI image built from this directory's `Dockerfile`, with the
repository root as build context and the local SDK source, runs as UID 10001, and is
deployed as the `datasheet-mcp` domain service of the versioned `veoveo` Helm
chart with a `Recreate` replacement strategy. Durable tasks live in the shared
SurrealDB platform store; schema migrations remain owned by `platform/store`.
