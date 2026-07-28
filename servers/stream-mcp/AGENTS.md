# Stream MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Stream owns admitted live and recording-replay GStreamer pipelines. Perception
is a typed pipeline profile, not a service identity. The production profile
uses NVIDIA DeepStream 9.1 and TensorRT while public MCP names remain
provider-neutral.

## Invariants

- Own `stream://` and `ui://stream/live.html`.
- Clients select stable pipeline IDs. Native launch strings, element names,
  model paths, and tracker paths are private operator configuration.
- Live inference consumes new encoded frames directly. It must not resolve or
  wait for Recording Hub.
- The App receives the existing H.264 access units. Do not add JPEG previews,
  duplicate encoders, or raw-frame CPU copies.
- Optional recording output uses the existing parsed H.264 access units and a
  bounded non-blocking worker. Recording failure must remain visible without
  delaying live graph execution.
- Recording replay authorizes canonical recording identities and captures one
  bounded task-start snapshot. It never persists a bearer token or native
  source path.
- The C++ runner is a pod-private process boundary. Its closed request,
  response, and event schemas are repository-owned adapters, not public
  protocols.
- A missing GPU, NVIDIA decoder, inference plugin, engine, catalog, or runner
  is a readiness or execution failure. There is no CPU inference fallback.
- Derived replay artifacts inherit source classification and labels.

## Build And Test

- `cargo check -p veoveo-stream-mcp`
- `cargo test -p veoveo-stream-mcp --all-targets`
- `cargo xtask image build --target stream-mcp`
- `cargo xtask smoke stream-gpu`

The C++ runner lives in `gst-runner/` and builds inside the exact DeepStream
image. GPU acceptance requires NVIDIA Container Toolkit and a model engine
compiled for the deployment GPU.

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
- C17: met
- C18: pending — the well-known documentation resources are not wired
- C19: pending — the contract declaration resource is not wired
- C20: pending — the administrative documentation routes are not wired
- C21: pending — the server does not yet embed its crate documents
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C30: met
