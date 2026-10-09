# Speech MCP Server — Agent Manual

## Purpose

Governed recording transcription and private human dictation.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `SpeechMcp`
  implements `DomainServer` with every read `DomainRead::no_store`, `SpeechListener`
  is its `DurableListener`, and the dictation routes are authenticated routes.
  `/speech/healthz` reports the inference worker, so a dead worker restarts the pod.
  `server::hosted_server` builds the server for the binary and native qualification.
  Do not add a `ServerHandler`, router, host check or authentication middleware here.
Follow the root instructions and this component's DESIGN.md. Keep source authority,
Task lifecycle and publication in Rust. The inference worker accepts only private
bounded requests and never receives browser credentials or arbitrary URLs.

Run inference on NVIDIA CUDA and fail readiness when hardware execution is unavailable.
Provisional transcript snapshots replace earlier snapshots. They never submit chat
messages, trigger agents or establish immutable final output.

Keep microphone audio ephemeral. Stop and cancel have distinct meanings: stop flushes
the final result, while cancel discards the session. A lost connection cannot become
an automatic message send or inference replay.

## Public Library

Consumers select `default-features = false, features = ["contract"]`. Keep the
contract feature free of MCP integration, service, provider and asynchronous runtime
dependencies. Execution uses `runtime`; hosted endpoints and binaries require `mcp`.
Qualify the isolated consumer and both runtime feature configurations when changing
these gates. Public types keep their existing domain owners.
Use `TranscriptionId` and `DictationSessionId` through application calls and construct
addresses with the matching owner types. Hosted declarations belong to `server/setup.rs`;
startup and discovery consume the shared checked setup.

## Build And Test

`cargo test -p veoveo-speech-mcp --lib` checks the public runtime adapters.
`cargo test -p veoveo-speech-mcp --test native_gpu -- --ignored` requires
the locked runner, cached model and NVIDIA CUDA.

GPU smoke and lifecycle assertions belong in the native Rust harness. Downloaded
fixtures require fixed hashes and recorded provenance. Runtime evidence must include
the actual GPU, model revision and package pins.

The existing `native_tasks` target also owns the ignored installed raw Task cases
`installed::raw_transcription_completion_through_public_gateway` and
`installed::raw_transcription_cancellation_through_public_gateway`.
Their `VEOVEO_SPEECH_TASK_INPUT` fixture selects normal private OAuth token-file
credentials, a governed recording and independent expectations. Dispatch creates
one real CUDA transcription Task. Cancellation requires a current unfinished Task;
early completion fails that profile without replay. Use the maintained owner wrapper
and retain actual SDK handles plus original close futures through registered cleanup.
These cases do not qualify process restart recovery. The owner
[scenario declarations](smoke/scenarios.json) dispatch the same target.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met — native hosted certification and domain qualification; see DESIGN.md.
- C02: met — native hosted certification and domain qualification; see DESIGN.md.
- C03: met — native hosted certification and domain qualification; see DESIGN.md.
- C04: met — static resources declare JSON or Markdown MIME types; transcript artifact reads preserve the published MIME type
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
- C31: met — installed catalog certification and headed Workspace CUDA transcription/dictation pass.
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
