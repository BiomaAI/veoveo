# Simulation Qualification Types

## Standards And Protocols

`veoveo.io/simulation-runtime-build-lock/v1` records exact runtime inputs.
`veoveo.io/simulation-conformance-result/v2` ties hardware observations to a base
image and overlay. `veoveo.io/simulation-runtime-release-evidence/v1` combines
qualified overlay results. JSON Schema 2020-12 describes these serialized types.
OCI coordinates and SHA-256 identities use `veoveo-deploy-contract` artifact types.

## Responsibility

This crate owns simulation dependency tuples, immutable source and wheel inputs,
GPU requirements, conformance results and release qualification. It executes no
processes. The sibling runtime [design](../DESIGN.md) owns the actual GPU workflow.

The validators require the complete runtime tuple, hardware rendering and motion
results, and matching base identities across the UAV and anonymous overlay probes.
The anonymous overlay is a GPU test application; its historical profile label does
not enable independently distributed Veoveo extensions. Existing qualified result
formats stay valid when their Rust types move into this crate.

## Tests

`cargo test -p veoveo-simulation-contract` checks typed deserialization, schema
closure and rejection of incomplete GPU results. These tests validate result
structure; real hardware qualification still runs the runtime probes.
