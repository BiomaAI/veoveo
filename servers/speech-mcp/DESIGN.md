# Speech MCP

Status: inference boundary under implementation. Public MCP, browser and deployment
acceptance remain tracked in [the delivery plan](../../docs/SPEECH_PLAN.md).

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28` | Planned public transcription, official Tasks and authorized transcript resources using the shared runtime |
| Veoveo Artifact plane | Planned source read capabilities and idempotent transcript publication |
| `veoveo.speech-worker/v1` | Private Unix socket protocol: one bounded JSON request followed by length-prefixed little-endian float32 mono PCM for live input; NDJSON transcript snapshots in response |
| Photon | Moondream `2.4.1`, Kestrel `0.8.1`, NVIDIA CUDA only; Parakeet Ultra weights SHA-256 `c9608f36d0ab956c14bfcc525479b0746b3b42a56f6949ec85c14eb7466717dc` |
| JSON / WebVTT | Typed transcripts with word/segment times in seconds and caption export |

## Inference Boundary

The persistent worker loads one model and admits a bounded number of simultaneous
requests. CUDA execution and an actual warmup must succeed before the socket opens.
The parent creates a private workspace and owns worker termination. A filesystem
source must resolve inside that workspace. Network URLs and arbitrary filesystem
paths are rejected. Live PCM is bounded by frame size, sample rate, elapsed duration
and per-frame receive time. A zero-length frame finishes the input. Disconnection
cancels inference and releases capacity.

The worker is an internal implementation detail. It has no HTTP listener, credentials,
Artifact authority or durable Task store. Its socket is mode 0600. The parent owns
audio cleanup, request admission and typed response validation. Provider exceptions
become closed error codes; they cannot expose file paths or captured audio to users.

Results contain source-language text and timestamps. Parakeet does not report a
language code. No speaker identity, translation or confidence value is fabricated.
Provisional snapshots may revise previous text. Text and segment limits apply before
publication and before delivery to a browser.

## Packaging

The worker's Python environment has an exact lock separate from the Rust compilation
inputs. Its GPU runtime and model cache belong to the Speech image. This release
boundary permits model updates without rebuilding the gateway and browser edge.
The model remains loaded between requests; idle residency does not initiate inference.

The latest model repository revision on September 22, 2026 is
`4cd0e9998a84defad3b47e6a698b699c3d3de274`. Its configuration, tokenizer and weight
objects are identical to Photon `0.8.1`'s pinned revision
`510e6f5a1c4619f39c72b083c091476935734e65`. The packaged adapter uses that qualified
immutable pin and records the weight digest. Attribution: Moondream Parakeet Ultra,
derived from NVIDIA Parakeet TDT 0.6B v3, CC-BY-4.0.

## Contract Compliance

This first checkpoint implements only the private inference boundary. MCP discovery,
tools, Tasks, resources, prompts, completions, subscriptions, well-known docs and
registration are pending. This component must not be registered as a hosted MCP
server before those surfaces and their qualification are complete.
