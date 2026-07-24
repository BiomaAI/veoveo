# Chart MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Packages the pinned upstream `flint-chart-mcp` Node.js server as a hosted
service exposing chart compilation, validation, rendering, view creation,
resources, and prompts through the gateway. The directory holds only the
Dockerfile; there is no first party source here.

## Invariants

- This is a packaged server from a third party. Do not vendor or patch
  upstream behavior in this directory; change the pinned `flint-chart-mcp`
  version in the Dockerfile and follow the root Dependency Currency policy.
- Domain behavior is stateless: `platformStore: false`, no persistence
  volume, and `--disable-file-reference` stays set. MCP sessions live in the
  singleton Veoveo launcher.
- The service listens on port 8795 and is reached only through the gateway;
  keep `--allowed-hosts chart-mcp:8795`.
- The container runs as the `veoveo` user with uid 10001.
- There is no cargo workspace member here, so repository conformance checks do
  not discover this server; its contract gaps are declared in this file.

## Build And Test

- No Rust crate exists, so `cargo check` and `cargo test` do not apply.
- Build the image: `docker build servers/chart-mcp`.
- `just helm-check` validates the chart material that registers the server.
- Upstream behavior is verified against the running container, never from
  source in this repository.

## Contract Compliance

Contract revision: 2

- C01: pending — upstream surface not audited against the protocol table
- C02: pending — unverified
- C03: pending — unverified
- C04: pending — unverified
- C05: pending — unverified
- C06: pending — unverified
- C07: pending — upstream schemas unverified against the 2020-12 profile
- C08: pending — upstream package does not use the shared schema machinery
- C09: pending — unverified
- C10: pending — does not consume shared contract crate
- C11: pending — unverified
- C12: pending — unverified
- C13: met
- C14: pending — unverified
- C15: met
- C16: met
- C17: pending — registration does not state the contract revision
- C18: pending — well-known surface not yet wired
- C19: pending — well-known surface not yet wired
- C20: pending — well-known surface not yet wired
- C21: pending — well-known surface not yet wired
- C22: met
- C23: met
- C25: met
- C26: met
- C27: pending — upstream notification behavior is not yet audited
- C28: met
- C29: met
- C24: pending — no Rust crate; the server is a pinned upstream npm package
