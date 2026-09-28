# Stream MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Stream owns admitted live and recording-replay GStreamer pipelines. Perception
is a typed pipeline profile, not a service identity. The production profile
uses NVIDIA DeepStream 9.1 and TensorRT while public MCP names remain
provider-neutral.

## Invariants

- Own `stream://` and `ui://stream/live.html`.
- Build hosted declarations through `McpServerContract` and `McpServerSetup` before
  Store access. Catalogs are static; accepted subscriptions contain only requested
  Tasks and mutable run/session resource families.
- Construct response identities once, derive their addresses, and reject disagreement
  in submitted wire fields. Run products must match the parent Task and pipeline.
- Keep IDs, resource addresses and collection cursors in the contract-only library.
  Use Stream constructors and the shared URI builder; serialize strings at protocol
  boundaries. Run reads and subscriptions use SQL owner selection.
- Run resources implement `TaskResourceAddress` and use the shared authorized Task watch.
  The live-owner hub supplies only live-session addresses. Recheck session access before
  sending updates; reconcile live resources on reconnect or buffer overflow.
- Keep replay result validation and `StreamArtifactUri` in the contract-only library.
  Producers and cross-server consumers share those checks. The producer also verifies
  detection bounds against its input dimensions; portable checks cannot prove GPU
  execution or authorize a source.
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
  bounded task-start snapshot. It never persists a submitted gateway bearer token or native
  source path.
- The C++ runner is a pod-private process boundary. Its closed request,
  response, and event schemas are repository-owned adapters, not public
  protocols.
- A missing GPU, NVIDIA decoder, inference plugin, engine, catalog, or runner
  is a readiness or execution failure. There is no CPU inference fallback.
- Derived replay artifacts inherit source classification and labels.
- Every successful product has one canonical `result_uri` and one content link.
  Current stored Task products must match their Task, pipeline and content link;
  authorize before decoding them and return a redacted diagnostic for corruption.

## Build And Test

- Public consumers select `default-features = false, features = ["contract"]`.
  Keep its dependencies free of MCP, Store, Rerun and runtime execution. `runtime`
  supplies domain execution; `mcp` adds hosted transport and is the default.
  Qualify the contract tests through an independent consumer as well as the service.

- `cargo check -p veoveo-stream-mcp`
- `cargo test -p veoveo-stream-mcp --all-targets`
- `cargo xtask image build --target stream-mcp`
- `cargo xtask smoke stream-gpu --installation <installation-target.json> --pipeline-id <installed-object-detection-pipeline> --producer-key-secret <installed-recording-producer-secret>`
- Local processes pass `--live-app servers/stream-mcp/assets/live.html` alongside
  their runtime configuration. Restart after HTML edits. Image assembly supplies
  the App through a separate declared asset context, preserving Rust compilation.

The Rust executable uses the shared Bookworm control artifact target. The C++ runner
lives in `gst-runner/` and builds separately inside the exact DeepStream
image. GPU acceptance requires NVIDIA Container Toolkit and a model engine
compiled for the deployment GPU.

## Contract Compliance

Contract revision: 3

- C01: met
- C02: met — each tool returns a typed canonical result_uri and one matching resource link; current Task products are validated after SQL authorization
- C03: met
- C04: met — run and session collections use authorized cursor pages of 100; discovery does not enumerate these records
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
- C17: met — both gateway registrations declare revision 3
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met — one authorized shared Task watch supplies run updates; the listener composes live-owner updates with current session checks and reconnect reconciliation
- C28: met — static resource discovery declares no list-change capability; session and task changes update contents
- C29: met
- C30: met
- C31: pending — installed Discover and list readiness qualification is pending
- C32: pending — knowledge-source publication belongs to Phase 7
