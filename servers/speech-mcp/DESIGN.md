# Speech MCP

Speech supplies private dictation and recording transcription. Acceptance requires
the native CUDA, recovery, hosted MCP and installed browser checks described under
[qualification limits](#qualification-limits).

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28` | Public transcription, official Tasks and authorized transcript resources using the shared runtime |
| Veoveo Artifact plane | Source read capabilities and idempotent transcript publication |
| `veoveo.ai/speech-worker/v2` | Private Unix socket protocol: one bounded JSON request followed by length-prefixed little-endian float32 mono PCM for live input; NDJSON transcript snapshots in response |
| Photon | Moondream `2.6.1`, Kestrel `0.9.1`, Torch `2.14.1`, NVIDIA CUDA only; Parakeet Ultra weights SHA-256 `c9608f36d0ab956c14bfcc525479b0746b3b42a56f6949ec85c14eb7466717dc` |
| `veoveo.ai/speech-gpu-acceptance/v2` | Native hardware-run operator report with current owned JSON keys; no downstream machine reader is declared |
| `veoveo.ai/speech-transcript/v2` / WebVTT | CamelCase JSON transcript documents with word/segment times in seconds; captions use the WebVTT text profile |

The transcript document schema declares its exact current format literal with the
foundational `format_tag` scalar naming role. Decoder admission checks the same
version before transcript retention.

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

## Wire Admission

Speech JSON uses camelCase field names and snake_case controlled enum values.
Tool and prompt names, the prompt argument `artifact_uri` and URI template variables
keep their declared identifier spellings. Native Task rows and JWT identity snapshots
keep their own storage and claim profiles. The durable request envelope keeps its
native field names; its nested tool input, Artifact capabilities and publication
products use their owning wire contracts.

The private worker advertises `veoveo.ai/speech-worker/v2`; parent readiness
admits that protocol before accepting inference work. Rust field names and the private Python request dataclass stay internal. Pydantic
wire models declare their camelCase field names directly. Both Python validation
paths reject retired snake_case spellings,
mixed spellings and unknown fields before path admission or worker capacity effects.
`provider_transcript` explicitly translates provider `duration_seconds` into owned
`durationSeconds` and emits only declared transcript fields.

Transcript construction and retained decoding require
`veoveo.ai/speech-transcript/v2`. Workspace admits that tag before previewing a
published document. Format disagreement requires a coordinated upgrade. The
installation drains writers and replaces all incompatible peers together; it starts
from empty current-format storage. This profile supplies no rolling mixed-version
support. WebVTT bytes, PCM framing, source hashes and model identities use their
existing formats. Native GPU reports use `veoveo.ai/speech-gpu-acceptance/v2` with
camelCase timing and acceptance fields; reports require a fresh hardware run.

## Packaging

The worker's Python environment has an exact lock separate from the Rust compilation
inputs. Its GPU runtime and model cache belong to the Speech image. This release
boundary permits model updates without rebuilding the gateway and browser edge.
The model remains loaded between requests; idle residency does not initiate inference.

The runtime base pins uv `0.12.23` with the selected CPython `3.14.8` profile and
Debian Trixie at OCI index
`sha256:8e88a074b0969bdc461f681727238e109438d70771828909f9ef19cfcc96c43a`.
Build-only Hatchling is pinned at `1.32.4`. Moondream `2.6.1` requires Kestrel
`0.9.1`, whose kernel and native packages are `0.7.4` and `0.1.8` respectively.
Kestrel `0.9.2`, kernels `0.7.5` and native `0.1.9` are excluded by that exact
Moondream dependency. Speech owns this compatibility exception and follows an
upstream Moondream release that supports the newer Kestrel family. Review is due
by October 14, 2026 or before the next dependency image release, whichever comes first.

`runner/src/speech_runner/cache_model.py` owns the model identity and the required
checkpoint files. Its shared admission helper resolves the immutable revision with
Hugging Face `snapshot_download`. Build-time caching permits downloading that revision;
runtime requests `local_files_only=True`. The helper selects the SDK's configured
`HF_HUB_CACHE` and requires the current model repository's exact revision snapshot.
The full-precision Ultra snapshot contains exactly the three declared files.
Admission rejects every additional entry, including directories and dangling symlinks,
before provider construction. The provider's optional `ternary.json` manifest is outside
this profile; cache selection patterns alone do not establish snapshot membership.
Required files may resolve inside that snapshot, its repository-local `blobs` directory,
or the configured Hub cache's shared `blobs` directory, which holds Xet-backed objects.
The profile grants no access to an enclosing home or temporary directory, or to the
separate unused `HF_XET_CACHE`. Admission verifies every file's byte length and SHA-256
before returning the model directory. Resolver and
filesystem errors expose only a fixed checkpoint-admission diagnostic.

The worker passes that verified directory through the released
`md.photon(MODEL, device="cuda", model_path=...)` API. Installed pods run offline
against read-only image weights. Warmup and at least 500,000,000 bytes of CUDA model
residency precede socket readiness. The image includes model attribution, and image
assembly performs no CUDA model initialization. The selected package and interpreter
profile requires both maintained native GPU controls before hardware qualification.

Helm selects Speech in the full installation. A pod requests one NVIDIA GPU,
reserves two inference slots for recordings and two for private dictation, and
admits one replica. Its six-GiB temporary volume accommodates two bounded source
files. Liveness probes the resident worker; readiness additionally checks storage.
The Bioma local node advertises eight shared GPU slots using NVIDIA device plugin
`0.20.1`, pinned by OCI digest. Time slicing provides admission slots on the existing
GPU, not memory isolation or additional hardware capacity.


The packaged model revision is `73175eb7aeb0d82f1e2a6b53b3aabc10a90bcd0b`.
Checkpoint admission requires the following immutable files:

| File | Bytes | SHA-256 |
|---|---:|---|
| `config.json` | 1153 | `e747b85e1bdfd300c8b8ac63bac8dd5221f8fe9bc275b48d06c735fcd6971b6e` |
| `tokenizer.json` | 1159960 | `bd321b096832a3f270bd3b2a88823957920f1a5c5ada71114a26ea729d0cbe91` |
| `model.safetensors` | 1255353386 | `c9608f36d0ab956c14bfcc525479b0746b3b42a56f6949ec85c14eb7466717dc` |

Attribution: Moondream Parakeet Ultra, derived from NVIDIA Parakeet TDT 0.6B v3,
CC-BY-4.0.

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

Native hosted certification exercises declared schemas, authenticated documentation,
stateless transport, Tasks and subscriptions. Owner checks exercise source/output
authority, independent-runtime cancellation and recovery after publication but before
Task settlement with identical Artifact IDs. Installed certification requires catalog
readiness under C31. Headed Workspace acceptance checks private dictation, delayed
chunk delivery, cancellation, explicit send, recording transcription and checked
transcript/caption downloads through the deployed CUDA worker. Qualification must use
the current transcript and worker formats; protocol-only checks establish no inference
or installed behavior.

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

## Task Completion Products

Transcription publication returns a typed MCP result until Task completion admission.
The completion stores the transcript's canonical `resultUri` alongside the complete
MCP envelope, preserving its single resource link.
