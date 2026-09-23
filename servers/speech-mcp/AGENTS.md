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

Contract revision: 2

- C01: pending — hosted contract qualification is in progress; see DESIGN.md.
- C02: pending — hosted contract qualification is in progress; see DESIGN.md.
- C03: pending — hosted contract qualification is in progress; see DESIGN.md.
- C04: pending — hosted contract qualification is in progress; see DESIGN.md.
- C05: pending — hosted contract qualification is in progress; see DESIGN.md.
- C06: pending — hosted contract qualification is in progress; see DESIGN.md.
- C07: pending — hosted contract qualification is in progress; see DESIGN.md.
- C08: pending — hosted contract qualification is in progress; see DESIGN.md.
- C09: pending — hosted contract qualification is in progress; see DESIGN.md.
- C10: pending — hosted contract qualification is in progress; see DESIGN.md.
- C11: pending — hosted contract qualification is in progress; see DESIGN.md.
- C12: pending — hosted contract qualification is in progress; see DESIGN.md.
- C13: pending — hosted contract qualification is in progress; see DESIGN.md.
- C14: pending — hosted contract qualification is in progress; see DESIGN.md.
- C15: pending — hosted contract qualification is in progress; see DESIGN.md.
- C16: pending — hosted contract qualification is in progress; see DESIGN.md.
- C17: pending — hosted contract qualification is in progress; see DESIGN.md.
- C18: pending — hosted contract qualification is in progress; see DESIGN.md.
- C19: pending — hosted contract qualification is in progress; see DESIGN.md.
- C20: pending — hosted contract qualification is in progress; see DESIGN.md.
- C21: pending — hosted contract qualification is in progress; see DESIGN.md.
- C22: pending — hosted contract qualification is in progress; see DESIGN.md.
- C23: pending — hosted contract qualification is in progress; see DESIGN.md.
- C24: pending — hosted contract qualification is in progress; see DESIGN.md.
- C25: pending — hosted contract qualification is in progress; see DESIGN.md.
- C26: pending — hosted contract qualification is in progress; see DESIGN.md.
- C27: pending — hosted contract qualification is in progress; see DESIGN.md.
- C28: pending — hosted contract qualification is in progress; see DESIGN.md.
- C29: pending — hosted contract qualification is in progress; see DESIGN.md.
- C30: pending — hosted contract qualification is in progress; see DESIGN.md.
- C31: pending — installed protocol readiness qualification is in progress.
