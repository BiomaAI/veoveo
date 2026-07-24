# Perception MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Provider neutral local perception: bounded, durable inference tasks (object
detection, optional tracking) over frozen or sealed Recording Hub video.
Production execution uses NVIDIA DeepStream and TensorRT, and NVIDIA names
never appear in its public MCP identities.

## Invariants

- Owns the `perception://` scheme: pipelines, models, analyses, results, and
  artifacts.
- A task first authorizes the canonical `recording://recordings/{uuidv7}`
  identity, re-resolves it inside the durable task, and reads only frozen or
  sealed segments. It never persists a filesystem path or bearer token.
- The ingest profile is pinned: H.264 Annex B `VideoStream` samples,
  nanosecond timelines, no B-frames, sparse keyframe markers, and
  decoder-reentrant IDRs. Other codecs and frame series timelines are
  rejected. Extraction remuxes without re-encoding; the original Rerun index
  is `decode_start_index + DeepStream buffer PTS`.
- The DeepStream runner is a process boundary local to one task, never an MCP
  or network protocol. One runner process per task; the server defaults to
  one active job.
- Derived artifacts inherit the source recording's classification and labels.
  Large bytes use the governed artifact download path, never inline content
  and never a second HTTP file route.
- There is no CPU inference fallback. A missing GPU, engine, catalog, tracker
  config, or runner is a readiness failure.

## Build And Test

- `cargo check -p veoveo-perception-mcp`
- `cargo test -p veoveo-perception-mcp` — crate tests run without a GPU.
- The GPU smoke requires an NVIDIA driver compatible with DeepStream 9,
  NVIDIA Container Toolkit, the device plugin, NGC login (`docker login
  nvcr.io`) for the base images, and a TensorRT engine built for the
  deployment GPU plus catalog and nvinfer config mounts.
- The C++ runner lives in `deepstream-runner/` and builds inside the two
  stage Docker image.

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
