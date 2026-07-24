# Time MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

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
  without its authority binding. Intervals are half open `[start, end)`.
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
- Platform store behavior lives in `platform/store` (`src/time.rs`,
  migration `0019_time_domain.surql`); run its tests when touching
  persistence. The shared SurrealDB integration harness covers the store
  boundary.
- The container builds from `servers/time-mcp/Dockerfile` (needs Docker);
  Helm material is the `time-mcp` domain service in `deploy/helm/veoveo`.
  No GPU requirement.
- The optional ntpd-rs observation socket is a deployment concern; unit tests
  use bounded fake observations.

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
