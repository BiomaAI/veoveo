# Datasheet MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Dataset profiling server and the canonical Python hosted-server template.
`preview_dataset` and `column_stats` answer directly; `profile_dataset` runs
as a durable MCP task that stores its report on the shared artifact plane and
records per-task usage. New Python servers are copied from this directory, so
every change here must keep the template a complete, working reference.

## Invariants

- Owns the `datasheet://` URI scheme; all addressable state — reports, usage,
  artifacts, documents, and the contract declaration — lives under it.
- `profile_dataset` executes only as a durable task on the shared task
  runtime through the final task extension; direct `tools/call` rejects it.
- The dataset is materialized while the gateway identity is live and embedded
  in the durable request, so `resume` recovery replays from persisted state
  without re-minting identity.
- Artifact bytes flow through the shared artifact plane using the forwarded
  gateway identity or a task-bound write capability reserved at submission.
- The middleware order is fixed: host validation outermost, then gateway
  internal-auth, then the final task extension, then the streamable HTTP MCP
  session (`json_response=False`, `stateless=False`).
- The well-known surface is consumed from `veoveo_mcp.contract.docs`: this
  directory's `AGENTS.md` and `DESIGN.md` are embedded into the wheel and
  served at `datasheet://docs`, `datasheet://docs/{doc_id}`,
  `datasheet://contract`, and the read-only admin projection
  `/datasheet/admin/docs/llms.txt` plus `/datasheet/admin/docs/{doc_id}`.
- Tool inputs are published with `mcp_input_schema`; recursive tool arguments
  are not supported.

## Build And Test

- `uv sync --locked --all-extras` against the installation's configured
  private index (`UV_DEFAULT_INDEX`), then `uv run pytest`.
- Task-runtime integration tests use the SurrealDB container fixture from the
  `veoveo-mcp` SDK test suite; docs, engine, and schema tests run offline.
- The container builds from `templates/python-mcp/Dockerfile`; the repository
  image build with the workspace SDK lives in `tools/image-build/datasheet`.
- Helm material is the `datasheet-mcp` domain service in
  `deploy/helm/veoveo`; the gateway binding is the `datasheet` server entry
  in the typed catalog.

## Contract Compliance

Contract revision: 2

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met
- C12: met
- C13: met
- C14: met
- C15: met
- C16: met
- C17: pending — the gateway catalog entry's metadata does not state the contract revision
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C30: met — the gateway owns pooled upstream transport while this server retains MCP session state
