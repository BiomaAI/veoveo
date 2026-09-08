# Composed Flight Acceptance

## Standards And Protocols

| Boundary | Profile |
|---|---|
| MCP and authentication | Repository conformance CLI over public HTTPS, the hosted MCP 2026-07-28 profile, OAuth token exchange, exact Work Context and profile scopes |
| Flight scenario | Runtime-loaded `veoveo.uav-sim-acceptance/v11` JSON with bounded typed mission, world, video and observation parameters |
| Stream live sessions | Server-owned `servers/stream-mcp/src/contract/live.rs` wire types, compiled directly without the Stream service |
| Browser automation | Headed Chrome DevTools Protocol, hardware-backed WebGPU or WebGL, shared browser assertions owned by `testing/browser-smoke` |
| GPU workload evidence | Existing NVIDIA resource identity, NVENC source and concurrent workload assertions; software rendering is rejected |
| Evidence | Existing `veoveo.io/uav-showcase-acceptance-evidence/v4` JSON and revision-qualified captures |

## Ownership

`cargo xtask smoke` builds this harness and its conformance executable for
`uav-domain-verify`, `uav-showcase-up`, and `uav-showcase-verify`. Their command
arguments and assertions stay in Rust. `main.rs` only installs TLS and dispatches
parsed commands. `cli.rs` owns those arguments.

`domain.rs` sequences the composed workflow. Its modules separate scenario validation,
authenticated MCP calls, world and vehicle state, Stream session ownership, governed
Artifact checks, and visual flight checkpoints. The harness reads and commands a
running installation through its admitted interfaces. It does not compile or start a
platform database, task runtime, recording service, or unrelated MCP server.

Browser attachment, GPU rejection and visual assertions have one source owner in
`testing/browser-smoke/src/browser.rs`. Both focused clients compile that source.
Small process, GPU identity and token-exchange helpers likewise keep one Rust source.
No new test-support framework or upstream dependency is introduced.

## Build Acceptance

The resolved Cargo graph is tested for each focused client and for the flight client
combined with its conformance helper. The graph must exclude service implementations,
SurrealDB, DuckDB, and Rerun. A verifier edit must rebuild only the affected client;
an unchanged command must dispatch within the two-second warm budget.

A build check or `--help` timing supplies compilation evidence. It does not certify a
flight, inference run, screenshot, or visual workflow. Those require the unchanged
hardware and authenticated workload assertions against the installation.
