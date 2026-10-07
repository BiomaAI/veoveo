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

Individual Reason address wrappers derive `ResourceAddress` from owner route
declarations. Their typed constructors, accessors and inline string schemas preserve
the existing public profile. The composed resource enum delegates exact building to
these wrappers and keeps collection cursors and optional-domain dispatch local.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with task-only reasoning, resources and templates, typed structured results, notifications, and usage records. |
| [`ai.veoveo/knowledge-source`](../../mcp/knowledge-extension/DESIGN.md) | Completed analysis and result summaries, cursor enumeration, conditional reads and request-scoped invalidations; observations use RFC 3339 times and SHA-256 content and access revisions. |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | The server-owned `ui://reason/analyses.html` application exposes pipelines, models, durable analyses, and results. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Video selection, reasoning request, model and pipeline catalog, event, grounding, provenance, and artifact contracts. |
| RFC 3986 and RFC 6570 | Concrete resource addresses use the shared URI component parser and builder; discovery templates expand to the same typed routes. Reason accepts one spelling for each address and rejects duplicate or unsupported query parameters. |
| RFC 9562 UUIDv7 | Analysis identities use lowercase hyphenated UUIDv7 values with the RFC UUID variant, backed by native Task identities. |
| Reason analyses cursor version 2 | URL-safe, unpadded base64 wraps collection-bound JSON with creation time and analysis identity. The input limit is 1024 bytes. |
| Reason runner `veoveo.ai/reason-runner-request/v4` and `veoveo.ai/reason-runner-response/v2` | Private Rust/Python JSON file protocol with closed fields, explicit markers and bounded answers. |
| Reason annotations `veoveo.ai/reason-annotations/v2` | Controlled JSON provenance embedded in a Rerun TextDocument; RRD binary framing uses the pinned Rerun profile. |
| Reason terminal result `veoveo.ai/reason-analysis/v2` | Public structured completion with one canonical result URI; the reader accepts this current profile only. |
| Stream replay `veoveo.ai/stream-results/v2` | Grounding consumes the complete result through Stream's contract-only library and its validation API. |
| Reason grounding `veoveo.ai/reason-grounding/v2` | Selected frame indices, labels and track IDs passed to the runner. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; every reasoning invocation is a durable, cancellable task whose terminal payload is returned by `tasks/get`. |
| [Rerun 0.38.1](https://rerun.io/docs/) RRD and `VideoStream` | Frozen or sealed sources and task-start snapshots of complete acknowledged ingest parts preserve exact time; derived semantic events are published as RRD annotations. |
| H.264/AVC Annex B | The source profile matches Stream: no B-frames and decoder-reentrant IDRs marked in the Rerun stream. |
| ISO Base Media File Format / MP4 | A bounded source range is remuxed without re-encoding for the task-local decoder and world-model runner. |
| Typed JSON process protocol | One schema-controlled request and response per isolated runner process. This boundary is private and does not replace MCP. |
| OAuth bearer and signed JWT identity | Source recording, grounding artifacts, results, and derived artifacts retain gateway-resolved Work Context authority and labels. |
| [vLLM 0.31.0](https://github.com/vllm-project/vllm/releases/tag/v0.31.0) | Official CUDA 13.0 runtime, pinned by OCI index digest. The image supplies the matched Torch and Transformers stack. |
| Hugging Face checkpoint | A site-supplied, revision- and digest-pinned checkpoint in native Transformers layout. |
| [PyNvVideoCodec 2.2.2](https://pypi.org/project/pynvvideocodec/2.2.2/), NVDEC, CUDA, and DLPack | Internal image-input adapter: NVDEC exports device RGB surfaces, Torch owns resized CUDA observations, and Transformers produces CUDA pixel patches. PyAV 18.1.0 reads container metadata only. |
| vLLM precomputed image embeddings | Internal Qwen3-VL profile with base and deepstack features. The offline engine and its single worker share the runner process; embeddings never enter the RPC tensor serializer. |

The current shared image pin is vLLM 0.31.0; its official CUDA image has not yet
completed Reason qualification. The 0.31 CUDA dependency profile still pins Torch
2.13.0 and PyNvVideoCodec 2.0.4 for vLLM's own video loader. Reason uses its separate
NVDEC-to-CUDA RGB adapter and passes precomputed embeddings to vLLM; it does
not invoke that loader. The 2.2.2 override supports device-memory frame
ownership and DLPack transfer. Each runtime update must qualify CUDA decode,
the in-process embedding path, and a complete Reason task. Remove the override
when vLLM supports the required codec release or the adapter qualifies against
vLLM's own pin.

Reason imports the recorded-video owner's camelCase selection and source snapshot
without a parallel wire representation. The source digest hashes that owner's
ordered current JSON. A result embedding the changed snapshot carries
`veoveo.ai/reason-results/v2`; its decoder checks that marker. The private runner uses
`veoveo.ai/reason-runner-request/v4` and `veoveo.ai/reason-runner-response/v2`, with
closed camelCase fields. Grounding uses `veoveo.ai/reason-grounding/v2`.


## Owned JSON And Native Adapters

Public requests, results, Findings, catalog views, Artifact provenance, configuration
and private runner messages use camelCase fields and reject undeclared keys. Tagged
answer and Task variants use snake_case values. MCP prompt arguments and URI template
variables use their declared snake_case names. Task status uses the shared lightweight
TaskStatus vocabulary. Current authority and mutable progress stay with the services.

The AnalysisCursor v2 base64url envelope contains a JSON object with createdAt and
taskId position fields. The FindingCursor v1 tuple has no renamed field bytes. Decoders
require their collection and version before using positions.

The result resource first selects an authorized current Task, reads its bound Artifact
occurrence, and admits the complete ReasoningResults. The selected request, recording,
range, pipeline, Task, model, source digest, Finding, summary and Artifact provenance
must agree before the resource returns the original bytes. Summary-only knowledge
reads continue to use current metadata and grants without downloading result bytes.

ReasonUsageMetadata describes the sole Reason usage producer. Publication converts
that closed value into the platform's open metadata object; native usage columns keep
their Store profile. Reason adds no usage read endpoint.

The model adapter's structured events use external track_ids. The adapter admits that
profile and translates it to owned trackIds fields. vLLM keyword arguments and native
GPU buffers keep their external spellings and representation. Python peer fields have
one admitted wire spelling on both validation paths; unknown and mixed keys fail
before inference or response-file publication.


## Library Features

Clients import `veoveo-reason-mcp` with `default-features = false` and
`features = ["contract"]`. The optional `knowledge` feature exposes
`FindingCollection::descriptor` without service, database or transport dependencies.
The public `contract` module owns the domain request,
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
source-snapshot digest. The Video owner supplies immutable source builders with typed
Recording IDs and SHA-256 values. The executor requires the snapshot to match the
selected Recording before dispatch. Artifact descriptors carry the typed Recording
identity and serialize their digest as bare lowercase hex.
Runtime source access still requires current authorization.
The `uris` module is available through `contract`. Its builders require the ID type
for each route. `ReasonResource` implements the foundational `ResourceAddress` trait
and parses the supported routes before dispatch. `ReasonScope` implements
`ScopeDefinition` with an empty vocabulary: Reason declares no additional domain
scopes. Gateway operation policy and Task ownership supply authorization.

Public contract tests compare every exported schema with the captured wire profile.
Run those tests through an independent Cargo consumer to check dependency isolation;
a workspace build can unify runtime features. Native runner fixtures exercise process
and validation behavior. GPU and installed acceptance use the owning workload checks.

## Reusable Findings

Reason declares `reason.analyses` and `reason.results` through the
[`ai.veoveo/knowledge-source`](../../mcp/knowledge-extension/DESIGN.md) extension.
The collections expose completed analyses and findings derived from their stored
results. `FindingSummary` is the public contract; the analyses collection carries
the submitted question or task, and the results collection carries answer excerpts
or event summaries. Reading these resources performs no inference.

| Collection | Enumeration | Member |
|---|---|---|
| `reason.analyses` | `reason://knowledge/analyses{?cursor}` | `reason://knowledge/analyses/{analysis_id}` |
| `reason.results` | `reason://knowledge/results{?cursor}` | `reason://knowledge/results/{analysis_id}` |

`FindingResource` builds and parses these addresses. A `FindingCursor` carries its
collection, creation time and typed analysis identity; it cannot be paired with a
different collection. Pages contain up to 100 `items` and an optional `nextCursor`.
The SQL query selects successful Tasks and readable result Artifacts before output
decoding or pagination. ID completion uses the same predicate. Visibility follows
the result Artifact's current grants, selected Work Context, labels and retention.
Task controls and the existing `reason://analysis` resources require Task ownership.

Members include the recording identity and range, source-snapshot digest, pipeline,
model, recorded model digest, prompt revision, decode policy and confidence basis.
The full result link uses the Artifact owner's URI builder. A summary occupies at
most 64 KiB. Answer excerpts stop at a UTF-8 boundary within 4096 bytes. Event
summaries include at most eight events and declare the total and any omissions;
shortened descriptions also declare truncation. Publication captures checked `FindingData`
in the successful Task output before writing any Artifacts. Its 60 KiB limit leaves
space for member addresses and timestamps. Summary reads use that retained content
and never download the full result Artifact; result size does not constrain discovery.

Publication and retrieval share `ReasonArtifactMetadata` and its typed provenance
variants. A member read checks the retained finding against stored Artifact
provenance, then rechecks both Task admission and Artifact metadata after service I/O.
An access change during the read requires a retry. Artifact's lightweight
`knowledge` feature owns conversion of its service snapshot into the access descriptor.
Reason limits that descriptor's retention to the Task's retention as well.

Revisions hash the returned summary and its access descriptor. Conditional reads
perform the same admission and source checks before comparing revisions. Modification
time comes from the stored completed Task; no modifier is inferred from ownership or
Artifact creation provenance. Both collections advertise a five-minute maximum age.

One Store observer per process receives native LIVE events and change-feed recovery
signals for Tasks and Artifact access. Each listener compares SQL fingerprints of
its admitted rows, including members beyond the first page. Hidden rows cannot change
that fingerprint. A listener accepts at most 32 finding roots or members, emits an
initial invalidation after observation starts, and reconciles after Store reconnects.
Grant, Artifact, Task and token deadlines wake a listener without idle polling.
When a previously admitted member loses access, the listener invalidates its member
and collection before ending the request. Initial subscription admission rejects an
inaccessible member. The gateway supplies signed caller authority; the source checks
its expiry and current Artifact/Task access throughout the request.
Resource reads and fingerprint collection have a 60-second deadline; SQL observation
has a 10-second timeout, and notification delivery has a 10-second deadline.

## Data path

The Rust executable comes from the shared Rust 1.99.0 Bookworm control compiler.
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

## Published Result Admission

`ReasoningResultsBuilder` produces an immutable `Checked` result. Construction
and decoding agree on the result tag, source selector and snapshot, task/answer
kind, positive observed-frame count, event ranges and ordering, and portable answer fields. The executor retains
its configured response budgets and verifies cited tracks against the admitted
grounding input. Portable decoding does not establish grounding membership.

The terminal envelope binds its analysis, finding and result provenance. Summary
ranges and event totals agree with the finding; annotation descriptors identify
that result Artifact and the same recording and snapshot. Optional clip provenance
also agrees with the finding's entity, timeline and decode start. The producer
calculates observed-frame and elapsed summaries from its admitted full result.
Those values cannot be reconstructed from an unseen Artifact during decoding.

This coordinated admission cut requires producers and retained-result consumers
to upgrade together. Contradictory products are rejected without rewriting their
payloads. Each current format tag selects one closed shape. Enum ordinals, signed timeline
indices and the shared recorded-video snapshot hash profile agree across the producers
and receivers. The shared Workbench
keeps its normal MCP envelope admission; Reason adds no owner renderer.

Results, annotations and a source clip each name a distinct Artifact occurrence;
their different provenance roles cannot describe the same immutable object.

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
result also states `confidenceBasis: model_reported`, which distinguishes
reasoning output from a Stream perception result's calibrated detector
confidences. Same engine, same input, same prompt revision, and greedy decoding
must produce the same result.

A request may reference grounding through Stream's `StreamArtifactUri` builder and
parser. The server fetches that Artifact with the caller's authority at submission
and checks both the reported size and returned bytes against its configured limit.
It decodes the complete Stream replay result and runs the producer-owned validation.
The recording, entity and timeline must match the requested video, and the replay
range must cover the requested range. Admission caps the complete result at 100,000
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

The response envelope, answer variants, events and index ranges reject undeclared
fields. Descriptions and answers carry free-form text; event detection converts model
output into the declared event shape before publication. The private request and
response snapshot in `testdata/private-protocol.schema.json` uses the shared
[producer/consumer checks](../../testing/python/DESIGN.md). Those checks cover the
declared shapes and bounds; task, source-range and grounding relationships are
validated by the executor.

## MCP surface

The gateway mounts the server at `/reason/mcp` and exposes:

- tools: `analyze_recording`;
- resources and templates for pipelines, models, analyses, results, and
  derived artifacts;
- prompts: `reason_analyze_recording`, `reason_answer_question`;
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
registrations declare contract revision 4.

`resources/list` publishes collection roots, the embedded documents and the fixed
pipeline and model catalogs. Analysis and result identities use templates and the
analysis collection. This discovery list is immutable for the running catalog;
Task changes do not advertise list-change notifications. Reading `reason://analyses` returns an object with `analyses`,
`limit: 100` and an optional opaque `nextCursor`. Continue through
`reason://analyses?cursor={nextCursor}`. The Store filters tenant, principal,
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

### Reusable Finding Queries

The library's `knowledge::readable_findings` selects completed analyses whose result
Artifacts the caller can read. `FindingSelection` distinguishes exact analysis identity
from a creation-time and analysis-ID page position. Pages contain at most 101 rows,
including one lookahead. The query combines successful Task status and retention with
Artifact's shared SQL admission predicate before ordering, limits and decoding.
The result occurrence's stored provenance must name the same analysis and identify a
Reason result. The reader validates the Task/output identity and returns only the
analysis, pipeline, model and result Artifact identities with stored timestamps.
Other output Artifacts require their own access checks.

`ArtifactReadScope` supplies tenant, subjects, clearance and selected context through
bound parameters. Source reads use the current result grants even when the reader is
an indexing service. The Task runtime continues to authorize Task control by owner.
Hosted analysis and result routes use the Task admission described above.

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

`AnalyzeRecordingOutput` publishes `veoveo.ai/reason-analysis/v2`. Its `resultUri`
identifies `reason://analysis/{analysis_id}/results`, whose authorized reader returns
the immutable reasoning result. The adjacent content says `Analysis completed.`
and links that resource once. Structured content carries the analysis address, model,
pipeline, completion metrics, required `FindingData` and Artifact descriptors. The
result builder derives the product address from the same analysis identity as the parent address. Pipeline and model
addresses derive from the checked finding; decoding rejects mismatched identities.

Task reads and subscriptions authorize through the shared SQL owner read before
decoding the request or output. A completed result must match the native Task ID and
the pipeline in its request. Resource views use the same current output type. Unknown
versions, missing schema markers, obsolete fields and inconsistent identities fail
validation. Explicit tool errors keep their no-product result.

The producer constructs terminal success through one result builder. Authorized reads
and subscription reconnects deliver the stored current result without rewriting it.
Task recovery reuses the current request, validated grounding subset and issued
capabilities. There are no readers or migrations for historical result formats.
The finding requirement ships through a coordinated installation drain: stop Reason
Task admission and indexing, settle or cancel running analyses, discard disposable
Reason Tasks and their derived knowledge indexes, then install producers and consumers
together before reopening admission. Old Task outputs fail validation. Full result
Artifacts keep their existing format and access policy.

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
static annotation, so events appear in the console viewer beside Stream
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
- grounding subset extraction from a perception result

The GPU smoke is a Rust scenario over the production service boundaries,
mirroring the perception smoke: real H.264 ingress through Recording Hub,
catalog resolution to a governed recording identity, a durable reasoning
task with a fixed prompt, and typed result plus Rerun annotation publication
through the shared artifact plane. It asserts result structure and retained
invocation provenance rather than exact generated text. The scenario runs
only on a deployment whose checkpoint is present in the model cache.

The installed Knowledge harness consumes an existing completed analysis through the
public gateway. Its read-only case compares both summaries with `FindingData`, checks
conditional reads, searches for the supplied domain query and compares each retrieved
revision with a fresh source observation. Repeating it after source and indexer
restarts checks retained result consumption without rerunning inference. This focused
query does not establish corpus-wide retrieval quality.

`tests/gateway_source_conformance.rs` runs K01–K08 through the gateway over both finding
collections and documentation. It reads the selected result summary to derive the
result Artifact identity, then uses Artifact's owner-supplied grant driver to change
access without running inference. Each finding collection must notify its member and
enumeration subscribers, preserve its changed observation across a Reason Deployment
restart, and deliver a second change after reconnecting. Temporary grants are removed
after a failed probe or an uncertain response. The
[installed harness contract](../../testing/installed/DESIGN.md) defines inputs and
commands. This source check does not establish GPU execution or recipient search access.

The access case requires an initially denied reviewer and an Artifact administrator.
It grants the reviewer read access only to the result Artifact, checks source and
search visibility, and verifies that Task control and annotations stay inaccessible.
Revocation must change the source revisions and remove indexed visibility. The test
waits on catalog notifications for at most 45 seconds per index transition and removes
its temporary grant after a failed assertion or an uncertain dispatch response.

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

## Identity Declaration Mechanics

Pipeline and model identities use `Id` with Reason-owned catalog-name admission. Serde decodes validated Strings and schemas retain the inline unconstrained String profile. Analysis Task identity and typed resource admission remain separate owner implementations.

## Value Admission

Finding data and summaries retain their Wire fields through `Checked`. Owner checks enforce recorded task, answer, address, provenance and size relationships; reduced task views keep their explicit domain projections.

## Cursor Admission

Analysis continuation uses a private OpaqueCursor with its owner base64url collection envelope and admitted aliases. Finding continuation keeps its on-demand tuple encoding and canonical equality through an ordinary owner codec; the nominal collection and analysis position stay typed.

## Persistence Query Placement

Complete persistence statements live in `queries/`, grouped by source responsibility.
Completion selects one static statement for each admitted Task or Artifact field.
Native query fixtures live in `tests/queries/`; finite corruption cases select complete
statements and preserve SQL admission before decoding. Runtime values use bindings.

## Task Lookup Persistence

The schema-only `schema` feature declares `reason_analysis`; Gateway composition
registers its fresh owner lane. The hosted runtime requires `task_lookup` contributions
for analysis creation and terminal settlement. Creation stores the typed Task link,
tenant and requested pipeline. A successful settlement validates the complete MCP
output against its Task and pipeline, then validates the Results publication receipt
against the finding's recording, model, prompt, task kind and source digest. Both
mutations commit inside the kernel Task transaction. Explicit MCP tool errors retain successful Task completion with a distinct no-product
settlement. Failure and cancellation retain explicit terminal settlements without output links.

The lookup declares the finding and Results provenance fields. Nominal driver
adapters decode them through the checked public contracts. `AnalysisResultRecord`
pairs the original MCP result with its admitted analysis interpretation. Native
decoding repeats product and canonical content admission through the owner validator
and rejects native database values inside JSON. Encoding writes the original result
without replacing admitted nulls, metadata or extensions with a normalized projection.
SQL compares the whole receipt without nested paths into its open extension material.
Settlement and hydration check Task, pipeline, finding and publication relationships.

Finding selection applies tenant, keyset and search predicates to the owner table.
Tasks' versioned lifecycle export supplies current success, timestamps, retention
and whole-result equality without returning a Task payload. Artifacts' versioned
read export applies current grants, clearance, classification, context and expiry
before the page limit. Reason compares the entire admitted current metadata value
against its typed publication receipt. Selected rows then undergo complete output,
lookup identity and copied-field checks; visible corruption is an error. Artifact
sharing can admit another Task owner's finding without granting Task control.

Observation listens to the Reason lookup through its owner-declared one-day
changefeed as well as kernel Task and Artifact changes. Fingerprints cover the
complete admitted collection and its integrity material. Task retention, Artifact
retention and future grant expiries determine the next recheck deadline.
