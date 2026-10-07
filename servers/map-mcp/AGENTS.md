# Map MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Veoveo's Earth geography and logistics routing domain: places, facilities,
borders, coordinates, transport restrictions, routes, matrices, reachable
areas, cuOpt-ready travel models, governed source acquisition with immutable
release activation, and Work Context owned feature authoring. Administration
runs through the same typed MCP surface and its single permission-aware
Map Explorer App.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `MapMcp` implements
  `DomainServer` over `MapAddress`, which composes the owner parsers for direct resources,
  product addresses, catalog pages, filtered features and knowledge. Readers dispatch
  the admitted typed target; knowledge reads are `DomainRead::no_store`. `MapSubscriptions` is its
  `ResourceSubscriptions` source, and `/map/healthz` reports the spatial engine and
  routing process through the host's liveness check. Do not add a `ServerHandler`, router, host check or authentication
  middleware here.
- Canonical identity: slug `map`, URI scheme `map://`, endpoint `/map/mcp`,
  app `ui://map/workspace.html`. Resource identities
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
- `build_travel_model` is the canonical Map-to-Optimization boundary. It
  preserves shared location order, binds each vehicle type to an exact
  mobility-profile version, records unavailable cells, and publishes
  `veoveo.ai/travel-model-artifact/v2`. Never reconstruct these matrices in
  Optimization.
- Travel-model references use Map's `MapTravelModelUri` in producers and consumers.
  Exact reads, collection pages and completion use `TravelModelReads`; owner, stored
  identity, Work Context and successful-result predicates run in SQL before limits.
- Restriction addresses and pages use the contract's typed builders. The domain reader
  applies tenant and operational time/family/withdrawal predicates in SQL. It rejects
  selected document/index disagreement and operations above 10,000 effective restrictions.
- Source addresses, summaries and pages use the contract types. Tenant selection runs
  in SQL before limits; selected documents must agree with indexed metadata. Public
  summaries omit acquisition endpoints, credentials and publisher key references.
- Exact release, source feature, raster, derivation and route addresses use
  `contract/product_uri.rs`. Keep the owner's IDs in builders and readers, and share
  these types with scene consumers. Dataset and release parent checks belong to SQL.
- Direct authoring and catalog addresses use `MapResource` and its typed helpers.
  Parsers return each parent's owner ID; do not convert them to text and parse again.
  Operational catalog continuations use `MapCatalogPage`; retain its specific position
  ID through the reader and convert it to text only when binding the Store query.
  Startup and discovery consume `mcp/setup.rs`; keep current-scope filtering and the
  configured App origin when adding a discovery resource.
- Active-release tool selection joins pointers to releases in SQL. Tenant, source
  and dataset predicates precede its limit; selected documents must match indexed
  metadata. Keep the complete internal pointer inventory separate from this tool.
- Mobility profiles use typed versions and resource addresses. Catalog SQL selects the
  tenant before its ID/numeric-version keyset and limit. Selected documents must agree
  with indexed identity, family, version and validity; completion binds typed parents.
- Routing authority uses the domain SQL reader across active pointers, releases and
  enabled sources. Preserve tenant/dataset agreement, active state, departure validity
  and map-family selection; selected retained documents must agree with indexed fields.
- Knowledge collections use `contract/knowledge.rs` addresses and cursors. Summary
  resources end in `/knowledge`, link to the full source, and hash its serialized body.
  Keep geometry and unbounded property data in the full resources. Authoring summaries
  require membership in the selected Work Context; ownership grants no shortcut.
  Collection scopes come from `MapScope`, and SQL applies access before decoding or limits.
  The `knowledge` library feature exposes those collection descriptors above `contract`
  without requiring MCP transport, Store or the analytical runtime.
- Domain profile pins (DESIGN.md, Standards And Protocols): GeoJSON RFC 7946,
  OGC JSON-FG 1.0, RFC 8142 text sequences, OGC GeoPackage 1.4, Basic
  CQL2-JSON from OGC CQL2 1.0, GeoParquet 1.0.0, Mapbox Vector Tile 2.1,
  MapLibre Style 8, official MCP Tasks `2026-07-28`, apps extension
  `2026-01-26`.

## Build And Test

- `cargo check -p veoveo-map-mcp`
- `cargo test -p veoveo-map-mcp`
- `tests/gateway_source_conformance.rs` supplies authored-member updates, publication
  creations, a disposable release transition and search scope denial to the shared
  source checker. It restarts Map and reconciles its selected fixtures. Follow
  [the installed harness contract](../../testing/installed/DESIGN.md); never select
  a production routing dataset or layer as a mutation fixture.
- `cargo test -p veoveo-map-mcp --test coordinate_contract` checks geodetic ID admission
  and schema compatibility.
- `tests/metadata_contract.rs` covers typed metadata URIs, parent-bound cursor
  continuation, and scope wire/schema values. The library owns these types;
  `contract` feature builds independently with default features disabled.
- `cargo test -p veoveo-map-mcp --test map_authoring_reads` qualifies
  tenant, Work Context, label, parent, archive, and keyset selection before page limits.
- `npm --prefix servers/map-mcp/app test` qualifies page walking and refresh failure
  behavior. These Node tests provide behavioral evidence only.
- The Map-to-Optimization compatibility test must prove that serialized travel
  model artifacts deserialize directly into the Optimization contract. The consumer
  owns this check at `servers/optimization-mcp/tests/map_travel_model.rs`.
- Native builds need a C/C++ toolchain, CMake, pkg-config, SQLite development
  files, and PROJ build dependencies (root README, Develop And Verify). The
  DuckDB 1.5.6 C library links through the pinned `duckdb-rs` fork, which
  removes the upstream `comfy-table ~7.1` pin so it composes with Rerun 0.38.
- Docker is required for SurrealDB backed tests and deployment work.
- The image build verifies the Spatial extension digest and copies native map
  utilities from pinned sources (`servers/map-mcp/Dockerfile`).
- R-tree plan, correctness, and million-feature performance evidence requires
  `VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION` to name the exact pinned 1.5.6 Spatial
  extension. A skipped performance test is not acceptance evidence.
- `npm --prefix servers/map-mcp/app ci && npm --prefix servers/map-mcp/app run build`
  regenerates the self-contained workspace App from exact MapLibre GL JS and
  esbuild pins. The generated HTML must remain below the Console's 2 MiB limit.
- The App is an image asset loaded once at startup. Local runs pass
  `serve --workspace-app servers/map-mcp/assets/workspace-app.html`; restart the
  process after regenerating HTML. Use `cargo xtask image` to bind its declared
  asset context for image assembly.
- Browser acceptance for the workspace map requires headed Chrome and a proven
  hardware WebGL2 renderer. Static HTML tests or software graphics are not
  visual acceptance.
- `cargo xtask smoke map-workspace-browser-verify` serves the exact generated
  App under the Console's opaque-origin sandbox and offline CSP, completes a
  bounded immutable-publication viewport query, and records GPU and screenshot
  evidence.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met
- C02: met
- C03: met
- C04: pending — discovery is static; derivation, dataset-release, route, matrix, acquisition, travel-model, restriction, source, mobility-profile, and authoring metadata indexes use SQL-scoped cursor pages; persisted completions match and deduplicate in SQL; other catalog roots still need SQL filtering and paging
- C05: met
- C06: met
- C07: met
- C08: met
- C09: pending — metadata, travel-model, restriction, source and mobility-profile references use typed addresses; other URI families and DTO relationship admission remain
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
- C24: met
- C25: met
- C26: met
- C27: met — scoped derivation indexes and mutable resources use the shared Store LIVE/change-feed observer; native tests qualify cross-client writes, observer restart, and authoring projection recovery
- C28: met
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C31: pending — installed Discover and list readiness qualification is pending
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
