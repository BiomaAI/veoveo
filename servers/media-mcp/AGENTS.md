# Media MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Owns generation jobs submitted to external media providers: durable tasks that
complete through provider webhooks. Serves the model catalog, live prediction
state, task usage records, and generated artifacts under the `media://` scheme.

## Invariants

- Canonical URIs come from `src/uris.rs`: `media://models`,
  `media://model/{model_id}`, `media://prediction/{id}`,
  `media://artifact/{artifact_id}`, `media://usage/task/{task_id}`. Do not
  mint URIs elsewhere.
- Durable task and prediction state lives in the installation SurrealDB
  through `veoveo_platform_store` (`src/state.rs`). The server keeps no
  private database.
- Provider completion arrives through the webhook path (`src/webhook.rs`) and
  the shared webhook waiters. Do not add a status polling fallback.
- Artifact bytes flow through the shared artifact plane with the forwarded
  internal identity (`src/artifacts.rs`). The server has no byte route.
- The `models`, `model_schema`, and `artifact` tools are projections over the
  same catalog types and URIs the resources expose. Keep them additive; the
  resources stay canonical.

## Build And Test

- `cargo check -p veoveo-media-mcp`
- `cargo test -p veoveo-media-mcp`
- `tests/surreal_integration.rs` is opt in: it exits early unless
  `VEOVEO_SURREAL_INTEGRATION=1`. It needs SurrealDB reachable at
  `ws://127.0.0.1:8000` (override with `VEOVEO_SURREAL_URL`,
  `VEOVEO_SURREAL_USER`, `VEOVEO_SURREAL_PASSWORD`), typically a Docker
  container.

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
- C17: pending — gateway registration does not state the contract revision
- C18: pending — well-known surface not yet wired
- C19: pending — well-known surface not yet wired
- C20: pending — well-known surface not yet wired
- C21: pending — well-known surface not yet wired
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C24: met
