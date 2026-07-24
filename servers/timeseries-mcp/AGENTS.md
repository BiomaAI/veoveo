# Timeseries MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Forecasting server. The `forecast` tool materializes a typed DuckDB source,
fits the configured method per series, logs observed rows, forecast quantiles,
and provenance into a Rerun RRD artifact on the shared artifact plane, and
returns structured output with a bounded chartable preview.

## Invariants

- Owns the `timeseries://` URI scheme plus the `ui://timeseries/forecast.html`
  app view.
- `forecast` executes only as a durable task on the shared task runtime
  through the final task extension; there is no alternate completion path.
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

## Build And Test

- `cargo check -p veoveo-timeseries-mcp`
- `cargo test -p veoveo-timeseries-mcp`
- Tests use bundled DuckDB through `veoveo-duckdb-runtime` and fixtures under
  `servers/timeseries-mcp/testdata/`; no GPU and no external services.
- The container builds from `servers/timeseries-mcp/Dockerfile` (needs
  Docker); Helm material is the `timeseries-mcp` domain service in
  `deploy/helm/veoveo`.

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
