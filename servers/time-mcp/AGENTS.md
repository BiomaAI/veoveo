# Time MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

One temporal authority for civil time, military date time groups, GNSS time,
mission epochs, operational calendars, clock quality, and temporal events.
Every resolved instant carries the authority releases and uncertainty used to
interpret it, so other servers consume time without reconstructing timezone or
leap second assumptions.

## Invariants

- Owns the `time://` URI scheme. Identity: slug `time`, MCP `/time/mcp`,
  admin REST `/time/admin`, port 8800.
- The canonical instant is `TimeInstant`: integral TAI seconds plus nanosecond,
  uncertainty, and the TZDB and leap second release ids. Never emit an instant
  without its authority binding. Deterministic results also emit both exact
  release URIs, version labels, canonical SHA-256 digests, and explicit
  bootstrap or acquisition provenance. Intervals are half open `[start, end)`.
- Deterministic resolution and conversion do not contain a live clock
  observation. `time://clock/current` carries the effective clock policy and
  measured quality, including holdover evidence.
- Durable state lives in the SurrealDB platform tables (`time_*`) and the
  authority release volume under `/var/lib/veoveo/time`. The server never
  applies migrations. Tenant engine caches are derived and rebuilt from the
  active release pair.
- Authority activation is atomic: optimistic versions plus a full preflight
  load of the prospective TZDB and leap second pair; one active release per
  family per tenant. Acquisition downloads run under fixed host, media,
  digest, size, and time policy with archive traversal rejected.
- `expand_schedule` and `validate_timeline` run only through the final Task
  API extension on `veoveo-task-runtime`; a direct call returns an instruction
  to use the task form.
- Civil fold and gap resolution defaults to `reject`; military zone `J` is
  rejected by the DTG parser. A positive leap second keeps its `:60`
  representation.

## Build And Test

- `cargo check -p veoveo-time-mcp`
- `cargo test -p veoveo-time-mcp`
- `cargo test -p veoveo-time-mcp --no-default-features --features contract` checks
  public contract validation and compile-fail examples. Qualify dependency isolation
  with a separately resolved consumer workspace; a workspace-wide build may enable
  dependencies through other packages.
- `cargo clippy -p veoveo-time-mcp --no-default-features --features runtime --all-targets -- -D warnings`
  checks runtime composition independently from the MCP feature.
- Time owns its private `src/persistence/` queries, driver records and mutation
  validation. Store owns migrations `0019_time_domain.surql` and
  `0043_time_acquisition_release_index.surql`. Runtime library tests use the shared
  isolated SurrealDB fixture; schema changes also require Store migration checks.
- `node --test tests/workbench-pagination.test.mjs` in `apps/console/web` checks
  page navigation behavior headlessly; it provides no visual or GPU acceptance.
- The container builds from `servers/time-mcp/Dockerfile` (needs Docker);
  Helm material is the `time-mcp` domain service in `deploy/helm/veoveo`.
  No GPU requirement.
- `src/catalog/tests.rs` qualifies SQL tenant and owner predicates, latest epoch
  selection, event transition isolation, page boundaries, requested epoch batches,
  and bounded distinct completion against the
  pinned disposable SurrealDB fixture.
- The optional ntpd-rs observation socket is a deployment concern; unit tests
  use bounded fake observations.

## Contract Compliance

Contract revision: 3

The library exposes the contract feature and owns its typed scopes, resource variants,
and collection cursors. Every resource route uses the shared URI builder and parser.
The MCP feature associates those types through `McpServerContract`; hosted startup,
discovery and scope membership consume its checked setup.
The runtime owns typed persistence inputs and the stored UUID key profile. Broader
DTO typing, retained-body consistency and installed qualification remain work in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

- C01: met
- C02: met
- C03: met
- C04: met — stable discovery, SQL-scoped pages of 100 records, exact reads, and bounded completions; native tests cover tenant/owner isolation and cursor ordering
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
- C27: met
- C28: met — static discovery advertises no list-change capability; Store observations invalidate resource contents
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: pending — installed Discover and list readiness qualification is pending
