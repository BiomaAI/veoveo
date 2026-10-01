# Timeseries MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Forecasting server. The `forecast` tool materializes a typed DuckDB source,
fits the configured method per series, logs observed rows, forecast quantiles,
and provenance into a Rerun RRD artifact on the shared artifact plane, and
returns structured output with a bounded chartable preview.

## Invariants

- Owns the `timeseries://` URI scheme plus the `ui://timeseries/forecast.html`
  app view.
- `forecast` executes only as a durable task on the shared task runtime
  through official Tasks; there is no alternate completion path.
- The immutable RRD artifact is the full resolution record. The `preview`
  layer is derived, capped at 500 points per series by
  `PREVIEW_POINTS_PER_SERIES`, and exists so clients chart without re-reading
  the RRD.
- Artifact operations use the caller's forwarded gateway identity; bytes flow
  through the artifact plane (`timeseries://artifact/{artifact_id}`).
- The app view is self contained by contract (no external fetches, HTML at
  most 2 MiB, guarded by `forecast_app_is_self_contained`) and drives the real
  `forecast` tool through the host bridge. Never add convenience tools for the
  app.
- The gateway manifest keeps `resource_projection: server_owned`,
  `capabilities.apps: true`, and `resource_schemes: ["timeseries","ui"]`.
- Forecast completion exposes one canonical `result_uri`, one resource link,
  and identity-free status text. Usage discovery is bounded and cursor-paged;
  exact task usage remains directly addressable.
- Import tabular source types and read SQL helpers from `veoveo-duckdb-mcp` with
  default features disabled and `contract` enabled. Timeseries keeps ownership of
  source materialization and forecasting; MCP core contains no source vocabulary.
- Keep source URLs and nonempty URL lists typed through provenance and the shared
  download API. Source syntax admission does not authorize a host or its DNS answers.
- Usage reads use `TimeseriesUsage` and TaskRuntime's SQL owner policy before limits.
  Keep Task IDs and cursor positions typed through query admission. The library's
  isolated `contract` feature owns usage addresses and pages; use its component
  builders at callers and keep runtime dependencies outside that feature.

- Use `TimeseriesResource` for hosted route admission and its typed document and Artifact
  variants for construction. The contract feature owns these routes without MCP or
  service dependencies. Startup and discovery consume the checked server setup.

## Build And Test

- `cargo check -p veoveo-timeseries-mcp`
- `cargo test -p veoveo-timeseries-mcp`
- `cargo test -p veoveo-timeseries-mcp --no-default-features --features contract`
  checks public types. Independently resolve a contract-only consumer to qualify
  dependency isolation outside workspace feature unification.
- `cargo clippy -p veoveo-timeseries-mcp --no-default-features --features runtime --all-targets -- -D warnings`
  checks the runtime library without hosted binary features.
- Forecast tests use bundled DuckDB through `veoveo-duckdb-runtime` and fixtures under
  `servers/timeseries-mcp/testdata/`. Usage integration tests require Docker for the
  isolated pinned SurrealDB fixture and clean it up on completion. No GPU is required.
- The container builds from `servers/timeseries-mcp/Dockerfile` (needs
  Docker); Helm material is the `timeseries-mcp` domain service in
  `deploy/helm/veoveo`.

## Contract Compliance

Contract revision: 3

Typed resource contracts, contract-only consumption and checked MCP setup are
implemented. Installed qualification remains in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

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
- C17: met
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met — the shared task-only filter rejects resource observations; accepted task IDs use the durable task source
- C28: met — resource subscription and resource-list change capabilities are absent
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: met
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
