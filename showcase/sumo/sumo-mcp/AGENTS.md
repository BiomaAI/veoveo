# SUMO MCP Server — Agent Manual

## Purpose

Serves traffic scenarios, vehicle controls and durable SUMO tasks through the installation gateway.

## Invariants

Authenticate forwarded internal identity before domain access. Preserve task fencing and the simulation owner. Visual and simulator qualification requires its declared hardware and provider prerequisites.

## Build And Test

Use `cargo check --locked -p veoveo-sumo-mcp --all-targets` for the full profile and
`cargo check --locked -p veoveo-sumo-mcp --no-default-features --features contract --lib`
for the public library. `cargo xtask enforce rust --boundaries-only` inspects its
independent contract normal/build graph. The owning deployment-contract tests verify
all nine current camelCase DTO families and refusal cases. Existing service/Task
controls inspect the full tool catalog and safely observed products with the
maintained naming inspector. Public DTO changes require the coordinated server
and receiver drain described in DESIGN.md. Task names and external simulator
formats keep their declared wire profiles. Installed naming, simulation and GPU
acceptance run separately with their explicit prerequisites.

`cargo test -p veoveo-sumo-mcp --lib sumo_worker_admission_and_mutation_settlement_use_current_task_fences`
uses the maintained disposable Store fixture and Docker cleanup. Its 90-second
control checks Task admission and delivery with inert next-work futures. It does
not execute SUMO, offline commands, Recording, Artifact writes or GPU work.
Preserve RunBatch's InterruptedIndeterminate class and refusal to resume; canceled
delivery never authorizes replay or undoes already stepped state.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C02: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C03: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C04: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C05: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C06: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C07: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C08: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C09: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C10: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C11: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C12: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C13: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C14: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C15: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C16: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C17: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C18: met
- C19: met
- C20: met
- C21: met
- C22: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C23: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C24: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C25: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C26: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C27: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C28: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C29: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C30: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C31: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C32: pending — SUMO has not completed hosted contract qualification for this requirement; simulation/GPU execution is separately qualified.
- C33: pending — Source producers, typed paired consumers and local naming controls are qualified; installed naming, simulation and hardware qualification remain pending.
<!-- veoveo:contract-compliance:end -->
