# Media MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Owns generation jobs submitted to external media providers: durable tasks that
complete through provider webhooks. Serves the model catalog, live prediction
state, task usage records, and generated artifacts under the `media://` scheme.

## Invariants

- Public requests, model and prediction IDs, all resource variants, cursors and pages
  belong to the library's isolated `contract` feature. Keep owner types through
  handlers and consumers; `src/uris.rs` declares fixed roots and templates only.
- Startup and discovery consume checked `MediaContract` setup. Reads and subscription
  admission share `MediaResource` parsing. Preserve the empty domain scope vocabulary
  and the existing gateway/current-owner access policy.
- Build model routes from `MediaModelId` components and Artifact addresses from
  `ArtifactId`. The model template uses RFC 6570 reserved expansion for slashes.
  Provider HTTP paths and callback query values use the URL library.
- Public prediction and usage selection belongs to `src/reads/`. Apply current
  caller policy and linked-record agreement in SQL before ordering and limits.
  Discovery declares roots/templates; instance catalogs supply paged links.
- Generation results use `MediaGenerationResult` and `MediaGenerationUri`. Read them
  from the linked successful Task through `MediaReads`. Require the current result
  shape and matching native Task identity; `task_results` owns the MCP handoff.
  Change contracts by hard cut and update consumers together.
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
- `tests/reads.rs` and `tests/surreal_integration.rs` use the shared Store fixture:
  Docker with the pinned image already present, disposable loopback databases,
  separate clients, bounded timeouts and owned cleanup. They run by default.
- `cargo test -p veoveo-media-mcp --no-default-features --features contract
  --test usage_contract --test prediction_contract --test generation_contract
  --test resource_contract`
  checks the public types; also resolve a separate consumer to check dependency isolation.
- `cargo xtask smoke media-task-run` qualifies the native MCP/provider/Artifact path.
  It also restarts the current server and checks the stored generation resource.
- The Console Workbench pagination test is headless behavioral coverage.

## Contract Compliance

Contract revision: 3

- C01: met
- C02: met — terminal publication and authorized Task delivery return the checked current result, one canonical result_uri/link and identity-free status
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
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met — SQL-scoped prediction and Task usage admission, authenticated catalog pages, and contents-only invalidations use Store LIVE and change-feed recovery; installed replica acceptance is pending
- C28: met
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: pending — installed Discover and list readiness qualification is pending
