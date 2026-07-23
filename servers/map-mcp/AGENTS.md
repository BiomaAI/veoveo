# Map MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 1.

## Purpose

Veoveo's Earth geography and logistics routing domain: places, facilities,
borders, coordinates, transport restrictions, routes, matrices, reachable
areas, governed source acquisition with immutable release activation, and
Work Context owned feature authoring. Administration runs through the same
typed MCP surface, including the admin and editor MCP Apps.

## Invariants

- Canonical identity: slug `map`, URI scheme `map://`, endpoint `/map/mcp`,
  apps `ui://map/admin.html` and `ui://map/editor.html`. Resource identities
  keep the `map://` scheme under the gateway `map__` projection.
- SurrealDB is the canonical operational catalog. The tenant keyed DuckDB
  Spatial schema is a derived analytical projection and must stay
  rebuildable. Immutable bytes live in the artifact plane as
  `artifact://{artifact_id}` and are projected as
  `map://artifact/{artifact_id}` only after normal artifact policy.
- Coordinate exchange is WGS84 longitude and latitude with optional
  ellipsoidal height. PROJ handles bounded two dimensional projected CRS
  conversion; geocentric EPSG:4978 and vertical values are rejected rather
  than silently copied.
- Dataset releases are immutable and activation moves pointers. Acquisition
  runs only through registered sources with pinned host, redirect, media
  type, byte, time, and filesystem controls.
- Valhalla is a supervised loopback engine and an internal projection, never
  a public Map API.
- Domain profile pins (DESIGN.md, Standards And Protocols): GeoJSON RFC 7946,
  OGC JSON-FG 1.0, RFC 8142 text sequences, Basic CQL2-JSON from OGC CQL2
  1.0, GeoParquet 1.0.0, Mapbox Vector Tile 2.1, MapLibre Style 8, final task
  extension `2026-06-30`, apps extension `2026-01-26`.

## Build And Test

- `cargo check -p veoveo-map-mcp`
- `cargo test -p veoveo-map-mcp`
- Native builds need a C/C++ toolchain, CMake, pkg-config, SQLite development
  files, and PROJ build dependencies (root README, Develop And Verify). The
  DuckDB C library links through the pinned `duckdb-rs` fork.
- Docker is required for SurrealDB backed tests and deployment work.
- The image build verifies the Spatial extension digest and copies native map
  utilities from pinned sources (`servers/map-mcp/Dockerfile`).

## Contract Compliance

Contract revision: 1

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
- C17: pending — registration does not state the contract revision
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
