# Reason MCP Design

This document is the canonical design and operational contract for the
`reason-mcp` crate.

`reason-mcp` is Veoveo's provider-neutral video reasoning domain. It answers
semantic and temporal questions about recorded sensor video: what happened in a
segment, which events occurred and when, and what a bounded prompt asks about
the footage. Its production execution implementation serves a locally mounted
multimodal world-model checkpoint through the vLLM runtime shipped in the
deployable image, but runtime and vendor names do not appear in its public MCP
identities.

This is deliberately not part of `stream-mcp`. A Stream perception profile is
bounded deterministic inference: a site-approved detection engine whose
calibrated per-frame output is reproducible byte for byte. Reasoning output
comes from a generative model and carries model-reported rather than calibrated
confidence. Keeping the domains separate preserves the Stream result's audit
story and keeps the DeepStream and world-model serving release trains
independently upgradable. It is also not part of `media-mcp`, because reasoning runs entirely inside the
installation. It uses no provider API, no webhook completion, no resident
inference service, and no agent framework.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with task-only reasoning, resources and templates, typed structured results, notifications, and usage records. |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | The server-owned `ui://reason/analyses.html` application exposes pipelines, models, durable analyses, and results. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Video selection, reasoning request, model and pipeline catalog, event, grounding, provenance, and artifact contracts. |
| RFC 3986 and RFC 6570 | Concrete resource addresses use the shared URI component parser and builder; discovery templates expand to the same typed routes. Reason accepts one spelling for each address and rejects duplicate or unsupported query parameters. |
| RFC 9562 UUIDv7 | Analysis identities use lowercase hyphenated UUIDv7 values with the RFC UUID variant, backed by native Task identities. |
| Reason analyses cursor version 1 | URL-safe, unpadded base64 wraps collection-bound JSON with creation time and analysis identity. The input limit is 1024 bytes. |
| Reason terminal result `veoveo.ai/reason-analysis/v1` | Public structured completion with one canonical result URI; the reader accepts this current profile only. |
| Stream replay `veoveo.stream-results/v1` | Grounding consumes the complete result through Stream's contract-only library and its validation API. |
| Reason grounding `veoveo.reason-grounding/v1` | Selected frame indices, labels and track IDs passed to the runner. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; every reasoning invocation is a durable, cancellable task whose terminal payload is returned by `tasks/get`. |
| [Rerun 0.38.1](https://rerun.io/docs/) RRD and `VideoStream` | Frozen or sealed sources and task-start snapshots of complete acknowledged ingest parts preserve exact time; derived semantic events are published as RRD annotations. |
| H.264/AVC Annex B | The source profile matches Stream: no B-frames and decoder-reentrant IDRs marked in the Rerun stream. |
| ISO Base Media File Format / MP4 | A bounded source range is remuxed without re-encoding for the task-local decoder and world-model runner. |
| Typed JSON process protocol | One schema-controlled request and response per isolated runner process. This boundary is private and does not replace MCP. |
| OAuth bearer and signed JWT identity | Source recording, grounding artifacts, results, and derived artifacts retain gateway-resolved Work Context authority and labels. |
| [vLLM 0.30.0](https://github.com/vllm-project/vllm/releases/tag/v0.30.0) | Official CUDA 13.0 runtime, pinned by OCI index digest. The image supplies the matched Torch and Transformers stack. |
| Hugging Face checkpoint | A site-supplied, revision- and digest-pinned checkpoint in native Transformers layout. |
| [PyNvVideoCodec 2.2.2](https://pypi.org/project/pynvvideocodec/2.2.2/), NVDEC, CUDA, and DLPack | Internal image-input adapter: NVDEC exports device RGB surfaces, Torch owns resized CUDA observations, and Transformers produces CUDA pixel patches. PyAV 18.1.0 reads container metadata only. |
| vLLM precomputed image embeddings | Internal Qwen3-VL profile with base and deepstack features. The offline engine and its single worker share the runner process; embeddings never enter the RPC tensor serializer. |

The qualified image uses vLLM 0.30.0, Torch 2.13.0+cu130, and
PyNvVideoCodec 2.2.2 on an RTX 4090. vLLM's package metadata pins
PyNvVideoCodec 2.0.4 for its own video loader. Reason uses its separate
NVDEC-to-CUDA RGB adapter and passes precomputed embeddings to vLLM; it does
not invoke that loader. The 2.2.2 override supports device-memory frame
ownership and DLPack transfer. Each runtime update must qualify CUDA decode,
the in-process embedding path, and a complete Reason task. Remove the override
when vLLM supports the required codec release or the adapter qualifies against
vLLM's own pin.

## Library Features

Clients import `veoveo-reason-mcp` with `default-features = false` and
`features = ["contract"]`. The public `contract` module owns the domain request,
response and result models, distinct pipeline, model and analysis identities,
resource addresses and collection cursors. Video selection and source identity come from the
[recorded-video library](../../platform/recordings/video/DESIGN.md#library-features)
through its contract feature. Selections, run/analysis views and results retain the
Recording owner’s `RecordingUri` type, imported with only its contract feature enabled.
Wire decoding and prompt arguments use that owner’s UUIDv7 and URI admission. Artifact
metadata comes from the Artifact contract.
Stream owns `StreamArtifactUri`, the replay result model and its portable validation.
Reason imports these through Stream's contract feature. Its own `grounding` module
exposes selection and subset extraction through `contract` for independent consumers.
These imports exclude MCP integration, asynchronous runtimes, database clients,
Rerun and GPU execution libraries.

The `runtime` feature adds catalog loading, artifact access, runner execution,
response validation and Rerun annotations. The `mcp` feature adds the hosted server,
HTTP authentication, Tasks and App integration. Defaults enable `mcp`, and the binary
requires it. Feature selection preserves the JSON fields, schema names and retained
source-snapshot digest. Runtime source access still requires current authorization.
The `uris` module is available through `contract`. Its builders require the ID type
for each route. `ReasonResource` implements the foundational `ResourceAddress` trait
and parses the supported routes before dispatch. `ReasonScope` implements
`ScopeDefinition` with an empty vocabulary: Reason declares no additional domain
scopes. Gateway operation policy and Task ownership supply authorization.

Public contract tests compare every exported schema with the captured wire profile.
Run those tests through an independent Cargo consumer to check dependency isolation;
a workspace build can unify runtime features. Native runner fixtures exercise process
and validation behavior. GPU and installed acceptance use the owning workload checks.

## Data path

The Rust executable comes from the shared Rust 1.98.1 Bookworm control compiler.
Its family excludes analytics feature unification. The runtime image contains no
Rust build stage. Cargo's declared asset inputs place the Python runner outside the
Rust source context. Image assembly installs hash-locked runner dependencies from
`runner/uv.lock`, then packages the source as an executable Python archive. A Python
source edit reuses the compiler output and dependency layer.

```text
recording-hub acknowledged live parts and frozen/sealed segments
        |
        | authorized task-start source snapshot
        v
   reason task
        |
   keyframe scan + no-transcode MP4 remux
        v
   reason-runner: decode -> frame sampling -> observation frames
                         -> world-model inference (vLLM)
        |
        v
typed reasoning results + derived RRD annotations + artifacts
```

The recording authorization contract is identical to Stream replay. A task
first authorizes the canonical `recording://recordings/{uuidv7}` identity
against its tenant and labels. It then re-resolves that identity and captures
the complete acknowledged parts visible at task start with prior frozen or
sealed segments. Live parts are copied into bounded task-local storage and
checked against their captured byte length and SHA-256 identity before decode.
No filesystem path or submitted gateway bearer token is persisted. The canonical video ingest
profile is the one documented in `servers/stream-mcp/DESIGN.md`.

The task obtains a bounded Artifact read capability while the gateway identity is
valid and stores it beside its output-write capability. Recovery reuses that exact
task binding. The shared reader checks current scope before catalog access and
reauthorizes every committed Artifact occurrence, including cached layers. Expired
or revoked capabilities fail the task. A persisted request missing a required field
is claimed and failed without preventing other tasks or the service from starting.

Each worker owns a persistent recording cache with an 8 GiB managed ceiling and
1 GiB of free-space reserve on its default 10 GiB claim. The canonical CLI controls
are `--catalog-cache-dir`, `--catalog-cache-managed-bytes`, and
`--catalog-cache-minimum-free-bytes`; the chart exposes them under `catalogCache`.
The recording spool remains read-only. Cancellation removes incomplete downloads
and releases cache reservations. This integration needs hardware workload acceptance
before the common compiler image can be admitted.

## Reasoning contract

One tool, `analyze_recording`, accepts a video selection, a pipeline identity,
and one typed reasoning task:

- `describe_segment` produces a text description of the selected range, with
  an optional focusing prompt.
- `detect_events` produces a bounded list of typed events. Each event carries
  an inclusive source-timeline index range, a short label, and a description.
- `answer_question` produces a text answer to one question about the range.

Frame sampling is explicit. A pipeline declares its observation resolution
and the maximum frame count proven to fit its model, prompt, and context
budget. A request may select a lower maximum. The runner applies the tighter
bound and reports how many frames it actually observed.

Every result carries its audit identity: the model, the optional engine
digest from the catalog, the prompt template revision, and the decode
parameters that produced it. Decoding is greedy by default. Sampled decoding
is opt-in per request and its parameters are recorded in the result. The
result also states `confidence_basis: model_reported`, which distinguishes
reasoning output from a Stream perception result's calibrated detector
confidences. Same engine, same input, same prompt revision, and greedy decoding
must produce the same result.

A request may reference grounding through Stream's `StreamArtifactUri` builder and
parser. The server fetches that Artifact with the caller's authority at submission
and checks both the reported size and returned bytes against its configured limit.
It decodes the complete Stream replay result and runs the producer-owned validation.
The recording, entity and timeline must match the requested video, and the replay
range must cover the requested range. Admission caps the whole document at 100,000
detections and selects only frames inside the requested range. Reason owns the
resulting subset; the runner may cite only track IDs present in that subset.

The returned Artifact metadata supplies the required data labels and classification.
Reason passes their union into Artifact write-capability issuance, whose service
requires caller clearance and persists this output label floor. All products retain
that floor in addition to source-video obligations. The task stores the validated
subset and its issued capabilities for restart recovery. It contains no submitted
gateway bearer or external fetch URL. The Artifact service owns capability persistence
and enforcement; Reason does not maintain a second admission ledger.

## Work Context and ownership

Reason tasks retain the gateway-resolved invocation authority at creation,
exactly as every hosted server does under the Work Context governance model.
Artifact publication stamps the context's output owner and initial grants,
and the source recording's classification and labels flow onto every derived
artifact. The server has no legacy ownership path.

## Execution boundary

One runner process is created for each reasoning task. The server writes a
typed JSON request, invokes the configured runner, and reads a typed JSON
response from the path named in the request. That boundary gives task-level
timeouts, cancellation, filesystem isolation, and a small crash boundary,
and it keeps the GPU dependency out of the server process. The runner
decodes the remuxed MP4, samples frames to the pipeline's observation
resolution, runs the world model through the image's vLLM runtime, and
writes the typed answer. Frame indices are reconstructed as
`decode_start_index + presentation time`, so every event lands on the
original recording timeline. The runner writes nothing to stdout; its
diagnostics go to stderr and its answer goes only to the typed response
file.

PyAV demuxes packet timestamps without decoding frames. NVDEC decodes one selected
surface at a time, and CUDA performs observation resizing and model preprocessing.
Each resized observation owns its device allocation before its decoder surface is
released. Packet timestamps must increase in decode order and the encoded dimensions
must match the bounded request.

The internal model adapter admits `Qwen3VLForConditionalGeneration`. It uses the
engine's loaded vision tower to produce base and deepstack embeddings on CUDA. The
engine runs in the same process with one worker; the adapter checks that ownership
before handing tensors to generation. Unique per-observation IDs avoid content hashing
of GPU tensors. Grid dimensions and prompt tokens remain host metadata.

The engine reserves decoder and observation storage from its configured memory budget.
Its normal vision profiling accounts for the admitted observation dimensions and frame
count. The adapter rejects software surfaces, a CPU image processor, unsupported model
architectures, and process configurations that would serialize the CUDA payload.

The runner belongs to the deployable image, not to site configuration. The
checkpoint is the opposite: a site-supplied deployment input in Hugging Face
layout, mounted read-only. Runtime optimization such as quantization is the
image's concern and never happens at request time. The server validates the
catalog, the checkpoint path, the prompt template, and the runner at startup
and fails readiness when any is missing. There is no CPU inference fallback.

The server validates every runner response before publication: the answer
kind must match the requested task, events must lie inside the requested
range in strict order, and event counts, label lengths, and response bytes
are all capped.

## MCP surface

The gateway mounts the server at `/reason/mcp` and exposes:

- tools: `analyze_recording`;
- resources and templates for pipelines, models, analyses, results, and
  derived artifacts;
- prompts: `reason-analyze-recording`, `reason-answer-question`;
- completions for pipeline, model, analysis, and artifact identities;
- final durable tasks, task subscription, cancellation, and result retrieval;
- analysis and result resource subscriptions and update notifications;
- typed structured tool content and canonical `reason://` resource links.

Canonical resources include:

```text
reason://pipelines
reason://pipeline/{pipeline_id}
reason://models
reason://model/{model_id}
reason://analyses
reason://analyses{?cursor}
reason://analysis/{task_id}
reason://analysis/{task_id}/results
reason://artifact/{artifact_id}
```

Startup assembles `McpServerSetup<ReasonContract>` before accessing the Store or
recovering Tasks. The setup checks the declared documents, capabilities, resources
and templates. Catalog descriptors also pass typed address checks. Both gateway
registrations declare contract revision 3.

`resources/list` publishes collection roots, the embedded documents and the fixed
pipeline and model catalogs. Analysis and result identities use templates and the
analysis collection. This discovery list is immutable for the running catalog;
Task changes do not advertise list-change notifications. Reading `reason://analyses` returns an object with `analyses`,
`limit: 100` and an optional opaque `next_cursor`. Continue through
`reason://analyses?cursor={next_cursor}`. The Store filters tenant, principal,
profile, data labels and task type before applying the limit, then orders by creation
time and Task ID. Every page uses the current caller's authority. A cursor is not a
snapshot: retention can remove Tasks and subsequent Tasks can appear on later pages.
The cursor version and collection identity are validated before use.

Direct analysis and result reads and subscription admission use the Task runtime's
SQL owner read. `ReasonTaskKind` declares operations in the contract-only library
through `TaskTypeDefinition`. Reads, pages and Task result subscriptions select those
names with `OwnerTaskQuery`, including updates and Store reconnect baselines.
Tenant, principal, profile, operation and data-label predicates exclude denied
rows before request or result decoding. Only analysis and result addresses admit
resource subscriptions. `AnalysisResource` implements `TaskResourceAddress` in the
isolated contract and derives the backing native Task from its typed analysis ID.
The shared `TaskResourceSubscriptions` listener combines those IDs with explicitly
requested Tasks in one SQL-authorized watch. Resource-only requests receive no Task
status payloads. Current-owner predicates apply on updates from every replica.
The Task runtime's LIVE wake and retained sequence recover gaps, with activity checks
every 15 seconds. A new database connection reconciles current authorized state even
after event history expires; an idle connection emits no timer-driven invalidations. Discovery remains
static. Requests admit at most 256 resource addresses and 256 distinct backing Tasks.

Deploy every Reason replica together to establish database-backed resource delivery.
Clients reconnect and reread their subscribed resources after replacement. Retained
Task and cursor formats require no conversion. A rollback must preserve the shared
SQL watch; a process-local notification build does not satisfy this delivery profile.

Analysis and artifact completions apply their search predicate in the Store before
reading at most 101 candidates per identity field. Artifact identities are deduplicated
across the three output fields. A response returns at most 100 values and reports
`hasMore`; it omits `total` when more candidates exist. Native isolated-store tests
qualify filtered limits, owner isolation and artifact deduplication without inference.
Install the server and collection consumers together; consumers must read the page
object and follow its continuation cursor.

### Identity Admission And Rollout

Pipeline and model identifiers accept 1–128 lowercase ASCII letters, digits and
hyphens, with an alphanumeric first character. The catalog loader and public request
decoder use the same distinct ID types. Analysis addresses accept the native UUIDv7
profile. The route parser rejects escaped aliases, extra path segments, fragments,
unsupported query parameters and noncanonical UUID spellings. Builders delegate
component encoding to the shared URI implementation.

Catalog views derive their public IDs from typed addresses. `AnalyzeRecordingOutput`
derives the results address from its analysis identity. `AnalysisView` derives both
addresses from the Task ID and admits an output only when its Task and pipeline match
the parent. Constructors establish these relationships; private wire conversions check
them when decoding the published flat JSON fields. Callers cannot mutate a nested
output through an analysis view.

Resource reads decode stored results after the SQL owner read. Missing results and
explicit tool errors have no product. A malformed successful result produces a fixed
recovery diagnostic without echoing its payload. Both analysis and results reads check
the retained output against the Task ID and the pipeline in its retained request.

The catalog and request schemas preserve their published shapes; terminal output and
its nested analysis view use the result profile below. Version-1 cursors carry the
current collection contract. Contract changes ship as coordinated hard cuts across
servers and consumers. Rebuild the disposable reference data for installation checks.

### Terminal Results And Task Recovery

`AnalyzeRecordingOutput` publishes `veoveo.ai/reason-analysis/v1`. Its `result_uri`
identifies `reason://analysis/{analysis_id}/results`, whose authorized reader returns
the immutable reasoning result document. The adjacent content says `Analysis completed.`
and links that resource once. Structured content carries the analysis address, model,
pipeline, summary and Artifact descriptors. The result builder derives the product
address from the same analysis identity as the parent address.

Task reads and subscriptions authorize through the shared SQL owner read before
decoding the request or output. A completed result must match the native Task ID and
the pipeline in its request. Resource views use the same current output type. Unknown
versions, missing schema markers, obsolete fields and inconsistent identities fail
validation. Explicit tool errors keep their no-product result.

The producer constructs terminal success through one result builder. Authorized reads
and subscription reconnects deliver the stored current result without rewriting it.
Task recovery reuses the current request, validated grounding subset and issued
capabilities. There are no readers or migrations for historical result formats.

The installed smoke imports Reason's contract-only library and checks the completion,
status text and product link. It reads that link through MCP and verifies the typed
result's model, pipeline, observed frames, event count and requested range against the
completion summary. Install producers and consumers together against current-format
state. Native control-plane fixtures cover delivery and reconnects; NVIDIA execution
requires the installed GPU checks.

Analysis publishes immutable occurrences through the shared artifact plane:
typed JSON results, a Rerun annotation layer, and optionally the remuxed
source clip. The annotation layer places each detected event on the source
timeline as a text log entry and records the full provenance block as a
static document, so events appear in the console viewer beside Stream
bounding boxes. Large bytes are never returned inline; oversized occurrences
use the governed artifact download path.

The typed JSON result carries the complete immutable recording-source snapshot
used by the analysis. Each Artifact descriptor carries only that snapshot's
SHA-256 digest and the essential analysis identity. Descriptor size therefore
remains bounded as Recording Hub acknowledges more live parts.

## GPU image and Kubernetes deployment

The Kubernetes node needs an NVIDIA driver compatible with the image's CUDA
and vLLM build, NVIDIA Container Toolkit, and the NVIDIA device plugin. The
pod requests one `nvidia.com/gpu`; a missing GPU is a scheduling or
readiness failure, never a CPU fallback. The Helm chart ships the workload
disabled by default because enablement requires one site input: the
world-model checkpoint loaded into the model cache.

The production installation uses two read-only mount roots:

```text
/opt/veoveo/reason/config/
  catalog.json
  prompt-template.txt
/opt/veoveo/reason/models/
  world-model/                  # site-supplied checkpoint, Hugging Face layout
```

Start from `configs/reason/catalog.example.json`, then set:

```dotenv
REASON_CONFIG_DIR=/opt/veoveo/reason/config
REASON_MODEL_DIR=/opt/veoveo/reason/models
```

The server defaults to one active reasoning job. A reasoning pass over a
long segment can take minutes, and serializing jobs keeps GPU memory
predictable; additional durable tasks remain queued while lease heartbeats
continue.

The model catalog also sets the vLLM GPU-memory fraction and maximum model
context. These are required, validated deployment inputs because the Reason
engine may share one physical device with other GPU workloads under a cluster
device-sharing policy. The canonical Helm profile reserves 60% of device
memory and an 8192-token context; operators size those values against the
installed checkpoint and the workloads that must remain concurrently
resident.

## Testing Strategy

Implemented crate tests cover:

- catalog validation and the repository catalog example
- canonical reason resource identities
- typed runner request construction and source-index preservation
- rejection of runner responses whose kind, order, range, or size violates
  the contract
- grounding subset extraction from a perception results document

The GPU smoke is a Rust scenario over the production service boundaries,
mirroring the perception smoke: real H.264 ingress through Recording Hub,
catalog resolution to a governed recording identity, a durable reasoning
task with a fixed prompt, and typed result plus Rerun annotation publication
through the shared artifact plane. It asserts result structure and retained
invocation provenance rather than exact generated text. The scenario runs
only on a deployment whose checkpoint is present in the model cache.

`cargo xtask smoke reason-gpu --installation <installation-target.json>` also accepts `--candidate-binary` and
`--candidate-runner`. The latter names the source root containing `reason_runner/`.
The Rust harness packages that source, launches the candidate on a private listener
inside the installed NVIDIA container, and records executable and runner-payload
digests. It verifies listener ownership, process cleanup, temporary cache removal,
and unchanged installed Pod identity and restart count. The installed catalog and
checkpoint supply the site inputs; the smoke needs no host-side configuration copy.
This candidate check does not qualify a different runtime image.

## Deliberate limits

- The production contract is H.264 `VideoStream`, identical to perception's
  ingest profile. Other codecs and frame-series timelines are rejected.
- The catalog accepts locally mounted checkpoints only. Runtime optimization
  belongs to the runner image, never to the request path.
- Reasoning confidence is model-reported. Results are audit-stamped and
  greedy-deterministic, but they are not calibrated detector output and the
  contract never presents them as such.
- Grounding accepts the typed perception results schema only. Opaque or
  unversioned grounding payloads are rejected at submission.
- The runner ships with the deployable image. This repository defines the
  runner contract and the server enforces it fail-closed; a deployment
  without the checkpoint in its model cache keeps the workload disabled.
- There is no live-proxy read mode and no attachment to a camera. Reason tasks
  can analyze just-arrived batches only after Recording Hub durably
  acknowledges them.
