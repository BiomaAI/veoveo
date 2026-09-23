# Speech Instructions

Follow the root instructions and this component's DESIGN.md. Keep source authority,
Task lifecycle and publication in Rust. The inference worker accepts only private
bounded requests and never receives browser credentials or arbitrary URLs.

Run inference on NVIDIA CUDA and fail readiness when hardware execution is unavailable.
Provisional transcript snapshots replace earlier snapshots. They never submit chat
messages, trigger agents or establish immutable final output.

Keep microphone audio ephemeral. Stop and cancel have distinct meanings: stop flushes
the final result, while cancel discards the session. A lost connection cannot become
an automatic message send or inference replay.

GPU smoke and lifecycle assertions belong in the native Rust harness. Downloaded
fixtures require fixed hashes and recorded provenance. Runtime evidence must include
the actual GPU, model revision and package pins.
