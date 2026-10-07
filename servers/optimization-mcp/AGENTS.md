# Optimization MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Own Veoveo's cuOpt-native decision domain. The server accepts typed vehicle
routing, route-scenario, continuous convex, and linear mixed-integer models.
It runs every operation as a durable MCP task on a required NVIDIA GPU,
independently verifies solver output, and publishes separate immutable problem,
run, solution, and evidence resources.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `OptimizationMcp`
  implements `DomainServer`, `OptimizationSubscriptions` implements
  `ResourceSubscriptions` with resource-list changes, and `/optimization/readyz`
  reports ready only while the cuOpt GPU executor does. Do not add a
  `ServerHandler`, router, host check or authentication middleware here.
- Canonical identity: slug `optimization`, URI scheme `optimization://`,
  endpoint `/optimization/mcp`.
- The public tools are `optimize_routes`, `optimize_route_scenarios`,
  `solve_convex`, `solve_milp`, and `verify_solution`. All require the final
  MCP Task API.
- Problem, run, and solution IDs are disjoint. Canonical JSON and recorded
  SHA-256 digests define immutable decision evidence.
- NVIDIA cuOpt 26.08 in the pinned CUDA 13.3 image is the only execution
  engine. Missing or unhealthy GPU access fails closed. Never add a CPU solver
  or optional GPU mode.
- The Rust container owns the MCP contract, compilation, identity, tasks,
  artifacts, and verification. The Python sidecar owns only private cuOpt and
  CUDA execution through `veoveo.ai/cuopt-executor/v2`.
- Map owns travel feasibility and `map://travel-model` resources. Optimization
  consumes only an attested immutable travel-model artifact or explicit inline
  matrices.
- Artifact bytes flow through the shared artifact plane with the caller's
  identity. Durable task and usage state lives in SurrealDB. Prepared problem
  files are digest-verified staging, not a private control database.
- Verification checks feasibility and objective consistency. It does not prove
  arbitrary quadratic input is convex or independently reproduce an
  optimality proof.
- Solver output is advisory. No code path actuates routes or decisions.
- A solve publishes one top-level canonical `resultUri`, one result resource
  link, and identity-free status text. Verification does not invent a product.
- Dynamic indexes, completion search, and usage discovery stay bounded at the
  authoritative store. Exact reads never scan the full task collection.
- Catalog, exact and completion reads use `OptimizationReads`. Match indexed and
  envelope ownership and Work Context in SQL before limits; reject malformed selected
  rows explicitly. Collection positions use the contract's typed cursor and URI.
- Hosted resources use `OptimizationResource` and domain address builders. Keep IDs
  typed through loaders and queries. Startup and discovery consume the checked setup.
- The domain scope vocabulary is empty. Preserve gateway operation policy and current
  resource authority; adding a scope requires an explicit authorization contract change.

## Module Boundaries

- `src/contract/`: public controlled types and versions.
- `src/usage.rs`: current owner and Work Context policy through TaskRuntime SQL.
- `src/reads.rs`: problem, run and solution SQL selection and retained-row validation.
- `src/task_records.rs`: runtime-only retained requests shared with the Task adapter.
- `src/compiler/`: deterministic public-to-private solver compilation.
- `src/verification/`: cuOpt-independent solution checks.
- `src/executor/`: private typed protocol and Unix-socket client.
- `src/problem_store.rs`: bounded digest-verified task staging.
- `src/profiles.rs`: immutable solver policy.
- `src/bin/server/`: MCP, task, identity, artifact, and resource wiring.
- `executor/veoveo_cuopt_executor/`: Python GPU adapter.

Keep these responsibilities separate. Public domain types must not import the
Python adapter, and executor-native indices must not become the MCP contract.

## Build And Test

- `cargo check -p veoveo-optimization-mcp --no-default-features --features contract`
- `cargo check -p veoveo-optimization-mcp --all-targets`
- `cargo test -p veoveo-optimization-mcp --all-targets`
- `PYTHONPATH=servers/optimization-mcp/executor python -m unittest discover -s servers/optimization-mcp/executor/tests`
- `docker buildx bake cuopt-executor`
- `VEOVEO_CUOPT_TEST_SOCKET=/absolute/path/executor.sock cargo test -p veoveo-optimization-mcp --test cuopt_gpu -- --ignored --nocapture`

The last test requires a real NVIDIA GPU and the pinned cuOpt executor image.
It must exercise health, routing, convex LP, and MILP through the Rust client.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: met — one canonical surface; no compatibility projection
- C07: met
- C08: met
- C09: pending — DTO relationship admission remains
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
- C27: met — authorized subscriptions share committed task and usage changes through the qualified Store LIVE and changefeed source
- C28: met
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C31: pending — checked MCP setup is implemented; installed readiness qualification remains
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
