# Speech MCP

Status: private dictation and recording transcription are deployed. Native CUDA,
recovery, hosted MCP and installed browser acceptance pass within the
[qualification limits](#qualification-limits).

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28` | Public transcription, official Tasks and authorized transcript resources using the shared runtime |
| Veoveo Artifact plane | Source read capabilities and idempotent transcript publication |
| `veoveo.speech-worker/v1` | Private Unix socket protocol: one bounded JSON request followed by length-prefixed little-endian float32 mono PCM for live input; NDJSON transcript snapshots in response |
| Photon | Moondream `2.4.1`, Kestrel `0.8.1`, NVIDIA CUDA only; Parakeet Ultra weights SHA-256 `c9608f36d0ab956c14bfcc525479b0746b3b42a56f6949ec85c14eb7466717dc` |
| JSON / WebVTT | Typed transcripts with word/segment times in seconds and caption export |

## Library Features

The server library exposes its existing public types through `contract` with
`default-features = false, features = ["contract"]`. The domain-owned
`veoveo-speech-contract` crate remains the type owner. A consumer of the server library
can use those same types without linking workers, persistence or hosted transport.
The `runtime` feature supplies execution modules, and `mcp` adds server configuration
and protocol entrypoints. The executable requires `mcp`, which is the default.

The independent [contract consumer](../../testing/fixtures/server-contract-consumer/DESIGN.md)
qualifies the isolated dependency graph. Runtime and hosted tests qualify their own
feature configurations; a contract build supplies no installed workload evidence.

The hosted adapter implements `McpServerContract` in `server/setup.rs`. It checks its
fixed resources, templates and documents before starting the inference worker. MCP
initialization and discovery use that setup. The lightweight contract owns typed
transcription and dictation identities, their resource builders and an empty scope
vocabulary. Source and session authorization remain in the application.

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

Both peers reject undeclared fields in worker requests and transcript events,
including empty tagged variants and nested words. The Python peer uses the
`runner/uv.lock` pin of Pydantic `2.13.5`. Decode diagnostics omit submitted values;
protocol-only tests import no GPU inference runtime.

Results contain source-language text and timestamps. Parakeet does not report a
language code. No speaker identity, translation or confidence value is fabricated.
Provisional snapshots may revise previous text. Text and segment limits apply before
publication and before delivery to a browser.

## Packaging

The worker's Python environment has an exact lock separate from the Rust compilation
inputs. Its GPU runtime and model cache belong to the Speech image. This release
boundary permits model updates without rebuilding the gateway and browser edge.
The model remains loaded between requests; idle residency does not initiate inference.

The runtime base pins uv `0.12.18` with CPython 3.13 and Debian Trixie at OCI
index `sha256:8891323e7ddaddc86d08c91f8af845ac5774a07d6f98e6765883030e1ad16fbc`.
Python 3.13 is the qualified native Photon wheel boundary. Build-only Hatchling
is pinned at `1.32.4`. Build-time cache admission verifies the model's SHA-256;
installed pods run offline against read-only image weights. The image includes
model attribution. No CUDA model initialization occurs during image assembly.

Helm selects Speech in the full installation. A pod requests one NVIDIA GPU,
reserves two inference slots for recordings and two for private dictation, and
admits one replica. Its six-GiB temporary volume accommodates two bounded source
files. Liveness probes the resident worker; readiness additionally checks storage.
The Bioma local node advertises eight shared GPU slots using NVIDIA device plugin
`0.20.1`, pinned by OCI digest. Time slicing provides admission slots on the existing
GPU, not memory isolation or additional hardware capacity.


The latest model repository revision on September 22, 2026 is
`4cd0e9998a84defad3b47e6a698b699c3d3de274`. Its configuration, tokenizer and weight
objects are identical to Photon `0.8.1`'s pinned revision
`510e6f5a1c4619f39c72b083c091476935734e65`. The packaged adapter uses that qualified
immutable pin and records the weight digest. Attribution: Moondream Parakeet Ultra,
derived from NVIDIA Parakeet TDT 0.6B v3, CC-BY-4.0.

## Recorded Transcription

`transcribe` accepts one canonical Artifact URI and requires MCP Tasks. Admission
validates current source access and issues bounded read and output capabilities.
The shared Task runtime persists the request, progress, lease and result. Recovery
reruns only accepted resumable work. Stable publication keys prevent duplicate
transcript and caption Artifacts. Output inherits the current source classification,
data labels and retention deadline under the Work Context output policy.

The lightweight Speech contract uses the Artifact owner's
[`ArtifactUri`](../../platform/artifacts/contract/DESIGN.md#wire-and-construction)
for source references in requests, transcript documents, and results. JSON decoding
validates the address once; admission also enforces the 256-byte source URI limit.
Speech declares its presentation scheme with the foundational `ResourceScheme`.
These contracts have no dependency on MCP or the inference runtime.

`speech://transcript/{task_id}` resolves the authorized
Task and its output metadata. Task subscriptions and transcript resource updates use
the shared durable event source. Every observation rechecks the same actor, profile,
Work Context and source access. Cancellation remains available to the owner after
source access is lost. Static catalogs do not advertise list-change notifications.
Completion returns no Artifact identifier guesses. The transcription prompt consumes
a caller-supplied governed URI.

The capabilities, document index, contract, transcript and dictation resources use
`application/json`. Embedded document bodies use `text/markdown`. Transcript artifact
reads preserve the Artifact plane's MIME type, including `application/json` and
`text/vtt`; UTF-8 text without a stored MIME type uses `text/plain`.

## Private Dictation

`start_dictation`, `finish_dictation` and `cancel_dictation` share the same application
service as native HTTP below the Speech mount. `speech://dictation/{id}` reads a
private receipt; these ephemeral receipts do not support MCP subscriptions. Chunk
acknowledgements contain the latest provisional snapshot. Recorded Task subscriptions
remain the durable observation boundary.

POST `/dictation` accepts a lowercase hyphenated RFC UUIDv4 or UUIDv7 and sample rate. PUT
`/dictation/{id}/chunks/{sequence}` accepts up to 192000 bytes of mono little-endian
float32 PCM. Sequence numbers start at zero. Repeating the last identical chunk is
safe; conflicting or out-of-order input closes inference. POST
`/dictation/{id}/finish` flushes a final snapshot. DELETE `/dictation/{id}` cancels
and discards text. GET reads its short-lived receipt. Every request carries a fresh
verified gateway assertion. The owner includes actor, profile, tenant, Work Context
and browser session family. Service principals cannot dictate.

The shared audit writer commits a session-open record before Speech exposes the
session. One terminal record contains accepted chunk counts, audio duration and the
completion reason. The session loop owns that summary, including timeout and disconnect,
and retries its stable record ID. Finish and cancel preserve their request correlation.
Rejected domain requests have individual records. Chunk success and status reads produce
no per-request audit entry. Audio and transcript text never enter audit records.

Audio remains in bounded memory. Idle sessions close after ten seconds. Input is
limited to 120 seconds, and receipts expire within 165 seconds. Runtime restart
interrupts capture; the browser keeps its last draft without resubmitting audio.
Recording and dictation concurrency have separate quotas whose sum equals worker
capacity. Timers perform cleanup and Task lease recovery, never initiate new work.
The first deployment has one GPU replica because dictation state is ephemeral.

## Contract Compliance

Native qualification passed all 26 hosted conformance checks, including declared
schemas, authenticated documentation, stateless transport, Tasks and subscriptions.
The native fixture also qualified current source/output authority, independent-runtime
cancellation, and recovery after publication but before Task settlement with identical
Artifact IDs. Installed certification covers catalog readiness under C31. The headed
Workspace harness covers private dictation, delayed chunk delivery, cancellation,
explicit send, recording transcription, Task observation after reload and checked
transcript/caption downloads through the deployed CUDA worker.

The application router is shared by the executable and native conformance fixture.
The fixture uses the actual CUDA worker, disposable database, internal assertions
and governed Artifact HTTP service. It does not mock inference or protocol handlers.

## Image Iteration

The normalized dependency parent contains the pinned CUDA Python packages and model
weights. Registry publication normalizes this parent once. Independent linked layers
supply the Rust executable, Python worker and attribution; ordinary application edits
reuse the parent by immutable digest without unpacking its filesystem. Image recipes
are runtime assets, outside the Rust compiler source context. Worker startup creates
private writable PyTorch and Triton cache directories on the temporary volume.

## Qualification Limits

The Rust worker protocol and Python models share
[schema compatibility checks](../../testing/python/DESIGN.md) through
`testdata/private-protocol.schema.json`. They cover every request and event variant,
integer widths and closed object fields. Workspace confinement, source duration,
sample-rate policy and transcript chronology require the owning behavioral tests.

Installed browser acceptance uses fixture microphone audio. Physical devices,
permission prompts and additional browser/OS combinations need device qualification.
Long recordings, video-container diversity, full capacity, sustained dictation and
concurrent-client latency need performance runs; the configured two-hour recording
limit has not been qualified at installation scale.

Cold/offline packaging and full release provenance need separate qualification; the installed development images do
not establish those release properties. Speaker attribution, translation, meeting
capture and spoken replies require separate product work.

## Identity Declaration Mechanics

Speech identity declarations use `Id` with owner UUID admission. Transcription accepts canonical RFC UUIDv7; dictation also admits canonical RFC UUIDv4 from browser generation. Their String wire behavior and declared version-specific schema patterns remain independent from native Task admission.
