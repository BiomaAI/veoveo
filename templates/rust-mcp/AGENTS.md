# Glossary MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3. This crate is
the Rust server template; [`README.md`](README.md) describes how to copy it.

## Purpose

Serves a fixed glossary of Veoveo hosting terms. The domain is small on purpose,
so the crate shows every part of a hosted Rust server: a typed contract, a checked
setup, tools, typed reads, prompts, completion and in-process tests.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `GlossaryMcp`
  implements `DomainServer`. Do not add a `ServerHandler`, router, host check or
  authentication middleware here.
- Canonical identity: slug `glossary`, URI scheme `glossary://`, endpoint
  `/glossary/mcp`, health `/glossary/healthz`.
- The library's `contract` feature holds the public types and builds with default
  features disabled. It depends on `schemars`, `serde` and `veoveo-types` only;
  MCP, transport and runtime dependencies belong to `runtime` or `mcp`.
- `GlossaryResource` is the complete address vocabulary. Every descriptor in
  `setup.rs` is built from one of its values, and every address round-trips through
  one spelling.
- Identifiers are types. `TermId` admits lowercase slugs only. The host rejects a
  read of another spelling as Invalid Params, `explain_term` rejects it while decoding
  its arguments, and `define` returns a tool error result.
- `glossary.rs` holds domain logic under the `runtime` feature, without MCP types.

## Build And Test

- `cargo test -p veoveo-glossary-mcp` runs the contract, domain, setup and
  in-process gateway tests.
- `cargo test -p veoveo-glossary-mcp --no-default-features --features contract`
  checks the public types alone.
- `cargo tree -p veoveo-glossary-mcp --no-default-features --features contract -e normal`
  lists the contract build's dependencies; it must contain no MCP or runtime crate.
- `cargo clippy -p veoveo-glossary-mcp --all-targets -- -D warnings`

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met
- C02: met — `define` declares input and output schemas and creates no product
- C03: met — the template declares no durable operation
- C04: met
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met — the template performs no Artifact or recording operation
- C12: met
- C13: met
- C14: met
- C15: pending — a server copied from the template adds its image and Helm chart
- C16: pending — a server copied from the template registers its gateway entry
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
- C27: met — the default `NoTasks` support admits no subscriptions
- C28: met — list-change capabilities are absent
- C29: met — the server keeps no state between requests
- C30: met — the server makes no upstream requests
- C31: met
- C32: pending — Shared checked setup declares the docs knowledge collection; installed K01–K10 qualification remains pending.
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
