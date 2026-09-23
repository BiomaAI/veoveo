# Speech MCP Server — Agent Manual

## Purpose

Governed recording transcription and private human dictation.

## Invariants

Follow the root instructions and this component's DESIGN.md. Keep source authority,
Task lifecycle and publication in Rust. The inference worker accepts only private
bounded requests and never receives browser credentials or arbitrary URLs.

Run inference on NVIDIA CUDA and fail readiness when hardware execution is unavailable.
Provisional transcript snapshots replace earlier snapshots. They never submit chat
messages, trigger agents or establish immutable final output.

Keep microphone audio ephemeral. Stop and cancel have distinct meanings: stop flushes
the final result, while cancel discards the session. A lost connection cannot become
an automatic message send or inference replay.

## Build And Test

`cargo test -p veoveo-speech-mcp --lib` checks the public runtime adapters.
`cargo test -p veoveo-speech-mcp --test native_gpu -- --ignored` requires
the locked runner, cached model and NVIDIA CUDA.

GPU smoke and lifecycle assertions belong in the native Rust harness. Downloaded
fixtures require fixed hashes and recorded provenance. Runtime evidence must include
the actual GPU, model revision and package pins.

## Contract Compliance

Contract revision: 3

- C01: met — native hosted certification and domain qualification; see DESIGN.md.
- C02: met — native hosted certification and domain qualification; see DESIGN.md.
- C03: met — native hosted certification and domain qualification; see DESIGN.md.
- C04: met — native hosted certification and domain qualification; see DESIGN.md.
- C05: met — native hosted certification and domain qualification; see DESIGN.md.
- C06: met — native hosted certification and domain qualification; see DESIGN.md.
- C07: met — native hosted certification and domain qualification; see DESIGN.md.
- C08: met — native hosted certification and domain qualification; see DESIGN.md.
- C09: met — native hosted certification and domain qualification; see DESIGN.md.
- C10: met — native hosted certification and domain qualification; see DESIGN.md.
- C11: met — native hosted certification and domain qualification; see DESIGN.md.
- C12: met — native hosted certification and domain qualification; see DESIGN.md.
- C13: met — native hosted certification and domain qualification; see DESIGN.md.
- C14: met — native hosted certification and domain qualification; see DESIGN.md.
- C15: met — native hosted certification and domain qualification; see DESIGN.md.
- C16: met — native hosted certification and domain qualification; see DESIGN.md.
- C17: met — native hosted certification and domain qualification; see DESIGN.md.
- C18: met — embedded crate documents and shared documentation projection are covered by native tests.
- C19: met — embedded crate documents and shared documentation projection are covered by native tests.
- C20: met — embedded crate documents and shared documentation projection are covered by native tests.
- C21: met — embedded crate documents and shared documentation projection are covered by native tests.
- C22: met — embedded crate documents and shared documentation projection are covered by native tests.
- C23: met — embedded crate documents and shared documentation projection are covered by native tests.
- C24: met — embedded crate documents and shared documentation projection are covered by native tests.
- C25: met — native hosted certification and domain qualification; see DESIGN.md.
- C26: met — native hosted certification and domain qualification; see DESIGN.md.
- C27: met — native hosted certification and domain qualification; see DESIGN.md.
- C28: met — native hosted certification and domain qualification; see DESIGN.md.
- C29: met — native hosted certification and domain qualification; see DESIGN.md.
- C30: met — native hosted certification and domain qualification; see DESIGN.md.
- C31: pending — installed protocol readiness qualification is in progress.
