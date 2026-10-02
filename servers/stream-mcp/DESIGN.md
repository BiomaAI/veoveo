# Stream MCP Design

This document is the canonical design and operational contract for
`stream-mcp`. Stream owns admitted live and replay media graphs. Perception is
one typed pipeline profile within that domain; it is not a server identity.

Stream is distinct from Media MCP. Media owns provider-submitted generation
jobs with webhook completion. Stream runs installation-local GStreamer graphs
over encoded sensor streams and governed recordings.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with direct live-session tools, durable recording runs, resources, templates, prompts, completions, subscriptions, and notifications. |
| MCP Apps SEP-1865 / `ext-apps` | Version `2026-01-26`; `ui://stream/live.html` is a self-contained App using canonical Stream tools and resources. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Pipeline profiles, RTP ingress, live sessions, detections, encoded preview chunks, recording selections, and artifact results use closed typed shapes. |
| Stream replay `veoveo.stream-results/v1` | The contract library owns the version enum, complete result model and portable validation shared by producers and consumers. |
| Stream resource addresses | `StreamResource` owns the route vocabulary. Distinct catalog IDs and UUIDv7 run/session IDs compose through foundational URI builders. Collection continuations use distinct typed JSON/base64url cursors. |
| Stream Artifact addresses | `StreamArtifactUri` wraps the Artifact contract's presented address with the fixed `stream` scheme and a typed Artifact occurrence ID. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; recording replay is durable, cancellable, resumable from governed identity, and returns its terminal payload through `tasks/get`. A live session is direct bounded work, not an indefinitely running task. |
| [GStreamer 1.0](https://gstreamer.freedesktop.org/documentation/) | Operator-admitted native launch graphs are private installation configuration. Clients select stable pipeline IDs and never submit launch text. |
| Rust 1.98.1 and GNU ELF | The Linux amd64 control executable compiles on Bookworm independently of the DeepStream C++ runner; their private JSON process boundary is unchanged. |
| [NVIDIA DeepStream 9.1](https://docs.nvidia.com/metropolis/deepstream/9.1/text/DS_Release_notes.html) and TensorRT | NVIDIA NVDEC, `nvstreammux`, `nvinfer`, and optional `nvtracker` execute the perception profile. Triton is a build-stage dependency only. |
| [RTP 2.0](https://www.rfc-editor.org/rfc/rfc3550) and [RTP payload format for H.264](https://www.rfc-editor.org/rfc/rfc6184) | Live ingress accepts one admitted RTP/H.264 UDP endpoint with a dynamic payload type and a 90 kHz clock. |
| H.264/AVC Annex B and RFC 6381 | Encoded access units use Annex B byte-stream alignment. Each live pipeline declares the exact `avc1.PPCCLL` decoder profile exposed to its App. |
| [WebCodecs](https://www.w3.org/TR/webcodecs/) and Media Capabilities | The App decodes the existing H.264 access units with `VideoDecoder`; Media Capabilities must report the exact stream as supported and smooth. The App identifies whether the browser reports power-efficient or software H.264 decode. |
| [Rerun 0.38.1](https://rerun.io/docs/) RRD and `VideoStream` | Recording replay consumes authorized H.264 `VideoStream` ranges. Derived detections are published as typed JSON and immutable RRD annotations. |

## Library Features

Clients import `veoveo-stream-mcp` with `default-features = false` and
`features = ["contract"]`. The public `contract` module owns the domain request,
response and result models. Video selection and source identity come from the
[recorded-video library](../../platform/recordings/video/DESIGN.md#library-features)
through its contract feature. Selections, run/analysis views and results retain the
Recording owner’s `RecordingUri` type, imported with only its contract feature enabled.
Wire decoding and prompt arguments use that owner’s UUIDv7 and URI admission. Artifact
metadata comes from the Artifact contract.
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
Portable replay-result validation also checks that the source snapshot belongs to
the result Recording; Reason grounding reuses that check.
Runtime source access still requires current authorization.
`StreamArtifactUri` is available through `contract`. It delegates parsing and building
to the Artifact owner's `ArtifactUri`, preserves admitted wire spelling and implements
the foundational `ResourceAddress` trait. The contract-only `uris` module builds
catalog, run and live-session addresses from their owning IDs. `PipelineId` and
`ModelId` admit lowercase catalog names of at most 128 ASCII characters; `RunId`
and `SessionId` admit canonical RFC UUIDv7 values. Run and session cursor types
validate their collection, version and position before a reader queries state.
The resource handler parses one `StreamResource` and dispatches its typed address.
Run reads and subscription admission select caller-owned Tasks in SQL. Private
live-session reads apply the session manager's ownership rules before materializing
the in-memory view. Catalog, run and live-session responses store each identity once
and derive their addresses. Their decoders reject disagreement among wire IDs and addresses. Run
builders reject a product from another Task or pipeline. Pipeline builders require a
model for perception profiles and exclude models from pass-through profiles.

Public contract tests compare every exported schema with the captured wire profile.
Run those tests through an independent Cargo consumer to check dependency isolation;
a workspace build can unify runtime features. Native runner fixtures exercise process
and validation behavior. GPU and installed acceptance use the owning workload checks.

## MCP Setup And Compliance

`StreamTaskKind` declares durable operation names in the contract-only library through
`TaskTypeDefinition`. Resource reads, collection pages and Task result subscriptions
select those names with `OwnerTaskQuery`. SQL excludes other operation types before
request or result decoding, including updates and Store reconnect baselines.

The MCP feature implements `McpServerContract` with Stream-owned resource types and
an empty `StreamScope` vocabulary. Gateway operation policy and current owner, context
and label checks authorize access. Startup builds `McpServerSetup` before Store access
or recovery. Discovery serves its checked descriptors and validates catalog addresses.
Template expansion tests compare every declaration with the owning builder. Both gateway
registrations and the embedded manual declare contract revision 3.

The server accepts requested Task updates and mutable run/session resource addresses.
Static catalogs accept no list-change subscriptions. Live sessions have a process-local
GPU owner; their notifications use its local hub. `RunResource` implements the foundational
`TaskResourceAddress` trait only for run and run-results addresses. The request listener
combines these resources and explicit Task IDs into one authorized shared Task watch.
Its initial snapshot and Store connection reconciliation supply current run state across
server instances and event-retention gaps.

The listener attaches to the live-session hub before admission and validates session
access before each delivery. A fresh listener or an overflowing local buffer reconciles
only the requested live-session resources. Run updates never come from this hub. Each
request accepts at most 256 resource addresses and 256 distinct Tasks, owns its SDK sink,
and releases both sources when cancelled. A live owner process replacement removes its
sessions; callers must start new sessions on the replacement. This live lifecycle does
not change the persistence or cross-instance delivery of recording runs.

### Contract Compliance

C02 tool schemas and result delivery use a canonical top-level `result_uri`.
The installed GPU replay checks product reads and completion across distinct replicas.
C27 uses the shared Task source for runs and the admitted GPU owner for live sessions.
The replica probe checks run invalidations, completed-state recovery and cancellation.
The [flight harness](../../testing/flight-smoke/DESIGN.md) qualifies live-owner delivery
through the public Gateway. It checks initial session, result and preview invalidations,
advancing GPU inference and preview counters, reconnected baselines and cancellation.
Discover/list readiness (C31) and knowledge-source publication (C32) require the work recorded in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md). Checked setup verifies
API and declaration consistency; it does not establish these runtime guarantees.

## Ownership Boundary

The public contract is provider-neutral:

- Stream owns `stream://` resources and `ui://stream/live.html`.
- A pipeline ID selects an operator-reviewed graph and typed profile.
- Perception profiles declare the operation, model, inference configuration,
  and optional tracker.
- Native GStreamer launch strings, element names, TensorRT paths, and tracker
  paths remain private deployment configuration.
- Recording Hub owns durable recording identity, ingest, retention, freezing,
  and sealing.
- Each authoritative simulation owns its operator cameras, shared RTX render
  products, NVENC bitstreams, and WebRTC presentation. Stream consumes an
  admitted encoded source without becoming simulation authority.

A GStreamer graph is deployment-code-equivalent. Changing it requires the same
review and rollout discipline as changing a container image. MCP clients cannot
provide fragments, properties, element factories, filesystem paths, or model
paths.

The live-session wire types live in `src/contract/live.rs`. The server and focused
flight acceptance client import those types through the Stream library. The client
selects only `contract`, which excludes service execution and its dependencies.

## Live Data Path

```text
encoded camera producer
        |
        | RTP/H.264, 90 kHz timestamps
        v
udpsrc -> jitter buffer -> depay -> h264parse -> encoded tee
                                                |          |
                                                |          +-> bounded MCP App preview
                                                |          |
                                                |          +-> optional non-blocking
                                                |              Recording forwarder route
                                                v
                                             NVDEC
                                                |
                                         nvstreammux
                                                |
                                 nvinfer -> optional nvtracker
                                                |
                                                v
                                  typed live result events
                                                |
                         stream://session/{id}/results
```

`start_live_session` reserves one admitted pipeline and starts its native
runner. The runner connects to a server-owned Unix socket before accepting
media. This socket is a private process protocol and is never exposed from the
pod.

Inference results and encoded access units are independent event variants.
Rust validates every detection, H.264 byte bound, Annex B prefix, and
decode-order sequence before retaining it. An encoded chunk's timestamp is its
H.264 presentation timestamp for WebCodecs. AVC frame reordering can make that
timestamp move backward in decode sequence, so timestamp monotonicity is not an
admission rule. Live detection indices are monotonic decode-order identities
assigned after NVDEC; Recording analysis retains its governed source-timeline
indices. Result and preview rings are bounded. Overflow drops the oldest retained
App history; it never blocks the live pipeline.

The reference RTP graph sets `rtpjitterbuffer mode=none` to derive presentation time
from the sender's 90 kHz RTP clock. Packet arrival time does not redefine that media
clock when a simulator stalls and catches up. The jitter buffer still reorders packets
and applies its 50 ms latency and packet-loss bounds. GStreamer's default clock-slaving
mode can clamp distinct frames to one presentation timestamp during catch-up;
`h264parse` can then emit an access unit with no timestamp. The runner rejects that
unit instead of inventing a timestamp. The selected mode follows GStreamer's
[RTP timestamp profile](https://gstreamer.freedesktop.org/documentation/rtpmanager/rtpjitterbuffer.html#RTPJitterBufferMode).

The UAV publisher's [native GPU regression](../../showcase/uav-sim/runtime/tests_gpu/test_stream_rtp.py)
sends steady and stalled/catch-up RTP through the production NVDEC/TensorRT runner.
It qualifies the example graph without a cluster, preserves model/image caches and
checks both inference progress and unique preview timestamps. Helm configuration
checks require the same timestamp mode in the rendered deployment. Installed public
resource delivery and composed flight use the separate installation harness.

A native probe failure posts a private application message to the pipeline bus.
The runner exits immediately with the typed cause, and Stream reaps it before
releasing the admitted UDP port. A failed probe cannot leave a session marked
running while silently suppressing all later frames.

The live path does not resolve a recording, wait for Recording Hub
acknowledgement, or create a task snapshot. A frame that has just arrived can
produce a detection immediately. The session exposes receipt time so
acceptance can measure result freshness.

An admitted live graph may declare an optional recording output. Stream sends
the already parsed H.264 access units to a bounded worker queue, and that
worker publishes a Rerun `VideoStream` to a pod-local Recording forwarder.
Inference never waits for the forwarder or Hub. Queue exhaustion or forwarder
failure changes the recording-output lifecycle to `failed` while the live
session continues. The session resource exposes the recording key, routing
state, forwarded-frame count, and error. Recording Hub remains the durable
authority after it accepts those bytes.

One pipeline has at most one active live session because its admitted UDP port
is exclusive. The creating principal controls session shutdown. Read access is
shared with authorized viewers in the same tenant and Work Context when the
session's data labels are a subset of the viewer's labels. Cross-context,
cross-tenant, and under-labeled reads fail closed. Stopping a session
terminates its native runner and retains the bounded result history for
inspection. Invalid native events and unexpected event-channel closure also
terminate and reap the runner before the session enters `failed`; a failed
runner cannot retain the pipeline's exclusive UDP port.

## Stream MCP App

`ui://stream/live.html` is self-contained and network-free. The host discovers
it from the server's MCP resources and grants only its linked
`start_live_session` and `stop_live_session` tools.

The runtime image packages the document at `/opt/veoveo/stream/assets/live.html`.
Startup loads a bounded immutable `AppHtml` snapshot. A local process can select
`--live-app servers/stream-mcp/assets/live.html` alongside its normal configuration;
restart it after an HTML edit. The image planner puts this file in the assembly
context, keeping presentation bytes outside the Rust compiler action.

The App reads:

```text
stream://pipelines
stream://sessions
stream://sessions{?cursor}
stream://session/{session_id}
stream://session/{session_id}/results
stream://session/{session_id}/preview
```

Preview data is the admitted H.264 access unit stream copied after parsing. It
is not a JPEG fallback and does not trigger a second encode. WebCodecs draws
decoded frames while a separate overlay canvas renders typed bounding boxes.
The App selects `prefer-hardware` when Media Capabilities reports the exact
stream as power efficient. When the exact stream is supported and smooth but
not power efficient, it selects `prefer-software` and identifies software
H.264 decode in the UI. This browser-only decode exception does not change the
hardware GPU requirement for Stream processing or visual acceptance.
The App waits for a retained keyframe before decoding and discards already
observed sequence numbers. Refreshes are serialized because MCP resource
notifications and the bounded refresh timer may arrive together. A sequence
gap or asynchronous decoder failure returns the decoder to its keyframe gate;
the next retained random-access unit reconstructs decoder state without
restarting the Stream session. Its session selector switches among live
streams visible to the operator's Work Context without changing their
pipelines or transport. This permits a logged-in human to inspect a stream
started by an authorized automation principal without granting that human
ownership of the pipeline.

An RTP publisher creates a new random SSRC, sequence origin, and timestamp
origin whenever its process starts. The admitted jitter buffer treats a large
sequence discontinuity as a new source epoch within one second and starts on
two consecutive packets. A simulator pod replacement therefore resumes an
existing Stream session without retaining the old source clock or waiting for
the jitter buffer's 60-second default dropout window.

The platform Helm chart renders that same qualified jitter-buffer contract as
the source catalog. A chart-local launch graph is not a second profile and must
not omit the bounded dropout, misorder, or fast-start settings. The simulator's
live RTP publisher is an independent consumer of native NVENC access units;
Recording reconnects do not restart its source epoch.

The preview resource is meant for an operator view, not bulk media
distribution. It keeps the MCP boundary portable across Console and external
MCP Apps hosts. A future high-fanout transport would remain an internal adapter
behind the same session resource contract.

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
and releases cache reservations. Compiler changes require the native hardware replay
gate with the exact candidate executable and App. The gate checks NVIDIA processing,
published artifacts, installed process isolation, and temporary cache cleanup.

The build combines a shared Rust scratch artifact with a separately compiled C++
runner. Only the C++ action uses the DeepStream SDK. Its input mount contains the
runner directory, which lets a Rust edit reuse the native result. Runtime images
continue to require the NVIDIA GPU resource and the admitted DeepStream plugins.

## Recording Replay

`AnalysisResults` declares `StreamResultsSchema::V1`. Its `validate` method checks
selection shape, strictly ordered frames inside the requested range, a processed-frame
count that covers returned frames, and every detection. Detections require a supported
class ID, a nonblank label of at most 256 bytes, finite probabilities in 0–1 and finite
positive box sizes with nonnegative origins. The producer also checks boxes against
its private input dimensions. Contract-only readers run the same portable checks;
the result schema does not authenticate a producer or establish source access.

The supported v1 wire value and fields are unchanged. Unknown schema versions and
partial result documents fail admission. Cross-server consumers import this owning
model instead of maintaining a subset decoder. Reason applies its own video-selection
and citation limits after Stream validation.

`run_recording` accepts a canonical
`recording://recordings/{uuidv7}` video selection and an admitted pipeline ID.
The durable task reauthorizes the recording under its stored owner, captures
the complete acknowledged parts visible at task start, and materializes one
bounded source range. Filesystem paths and submitted gateway bearer tokens are never durable
task input.

The replay extractor finds decoder-reentrant preroll, remuxes H.264 into MP4
without re-encoding, and carries the original Rerun index separately. The
native runner reconstructs source time as:

```text
original Rerun index = decode_start_index + GStreamer buffer PTS
```

Later Recording Hub batches do not enter a running replay. Replay is for
retrospective or reproducible analysis; it is not the implementation of live
Stream.

A deployment may select Stream without Recording. Its admitted catalog omits
recording graphs, while live pipelines and the App remain available. Reason
continues to require Recording because its contract analyzes durable video
ranges.

## Pipeline Catalog

Each pipeline declares:

- a stable ID, title, and description;
- one typed profile: `pass_through` or `perception`;
- an optional recording-replay graph;
- an optional live graph with dimensions, frame rate, expected bit rate, RTP
  ingress, RFC 6381 codec, and encoded-output element;
- an optional operator-owned recording route with a loopback proxy,
  application ID, entity path, timeline, and bounded queue.

A perception profile binds one site-approved TensorRT engine. Implemented
operations are object detection and object detection with tracking.
Segmentation and pose remain rejected until their typed result contracts and
native metadata validation exist.

A pass-through live profile needs no model and may retain or transform an
encoded stream before preview or recording output. Recording replay currently
requires a perception profile because its durable result contract publishes
typed detections and annotations.

Catalog validation rejects unknown models, duplicate IDs or live UDP ports,
relative runtime paths, invalid tracker dimensions, missing named elements,
unsupported operations, NUL-containing launch text, and launch text larger
than 64 KiB. Startup also verifies every referenced model and configuration
file.

## Native Runner

The image uses exact DeepStream 9.1 multi-architecture manifests:

- `nvcr.io/nvidia/deepstream:9.1-triton-multiarch` supplies build headers;
- `nvcr.io/nvidia/deepstream:9.1-samples-multiarch` is the deployed runtime.

The Rust server serializes a closed request document. The C++ runner parses the
admitted launch graph with fatal-error handling, resolves exact named elements,
and overrides the security-sensitive properties from typed configuration.
Native DeepStream metadata crosses the process boundary only after bounded
conversion to the repository-owned event schemas.

Recording replay writes one bounded JSON response file atomically. Live mode
writes newline-delimited typed events to one Unix socket. Native stdout is
redirected to diagnostics so it cannot become a second data protocol. Event
validation failure closes the native graph process before publishing the typed
session failure.

The pod requests an NVIDIA GPU. NVDEC, inference, and tracking have no CPU
fallback. Missing plugins, model engines, driver capabilities, runner binary,
or GPU access are deployment failures.

The canonical chart requests 8 GiB and limits Stream to 24 GiB of host memory.
The admitted DeepStream 9.1 traffic-detection graph crossed the former 16 GiB
limit during live UAV acceptance even when no replay graph overlapped it. The
request reserves realistic node capacity while the limit covers TensorRT,
GStreamer, encoded-preview retention, and bounded result state for one admitted
graph. Operators may raise both values for a heavier catalog, but a production
profile must not reduce them below the measured canonical graph.

## MCP Surface

Tools:

- `start_live_session`
- `stop_live_session`
- `run_recording`

Each successful tool returns one typed `result_uri` and one matching resource link
beside a short status. Replay links to `stream://run/{run_id}/results`. Starting and
stopping a live session link to `stream://session/{session_id}`; live results and preview
addresses remain additional structured fields. Artifact metadata stays in structured
content. Tool errors return no product.

Task reads, completion subscriptions and synchronous replay completion authorize the
current caller in SQL before decoding stored output. The decoder requires the current
product shape and checks the run identity, admitted pipeline and canonical content link.
Run resources use the same checks. Invalid stored products fail with a diagnostic that
omits the stored payload. The tool output fields use one current format; installation
updates coordinate the server and its consumers.

Prompts:

- `stream-start-live-session`
- `stream-run-recording`

Canonical resources:

```text
ui://stream/live.html
stream://pipelines
stream://pipeline/{pipeline_id}
stream://models
stream://model/{model_id}
stream://sessions
stream://session/{session_id}
stream://session/{session_id}/results
stream://session/{session_id}/preview
stream://runs
stream://runs{?cursor}
stream://run/{run_id}
stream://run/{run_id}/results
stream://artifact/{artifact_id}
```

Discovery lists collection roots and item templates without enumerating runs or
sessions. These records do not change the discovery surface, so Stream declares no
resource-list-change capability. Run pages contain `runs`, `limit: 100`, and an
optional `next_cursor`. The Store applies task ownership, tenant, profile, labels,
and task type before the page limit. Pages order creation time and Task ID ascending.

Session pages contain `sessions`, `limit: 100`, and an optional `next_cursor`.
The process-local session index orders UUIDv7 IDs newest first and checks Work Context
membership and labels before materializing at most 100 session views. Live sessions
belong to their running GPU process; pagination does not make them persistent.
Versioned cursors identify their collection, and every page reapplies current caller
authority. A cursor grants no access. Completions filter before limiting results to
100 and report `hasMore` without an unbounded count.

The Live Monitor reads one session page at a time. Newer and Older controls navigate
its visited pages; starting a session returns to the first page. A navigation
generation prevents an earlier response from replacing the selected page.
Start stays disabled until the initial session page loads, while a start request is
pending, and when that page already contains a running session for the selected
pipeline. Pipeline changes recompute this state. The browser acceptance client waits
for the controls to become ready before starting a session.

Completed recording runs publish typed JSON results, an immutable RRD
annotation layer, and an optional remuxed source clip through the Artifact
plane. Derived artifacts inherit source classification and labels. Large
artifacts use governed Artifact downloads rather than a server-local file
route. The JSON results carry the complete immutable recording-source snapshot.
Each artifact descriptor carries only that snapshot's SHA-256 digest and the
essential run identity, which keeps Artifact control-plane metadata bounded
regardless of the number of acknowledged recording parts.

## GPU Deployment

The Kubernetes pod requests one `nvidia.com/gpu` and uses the configured
NVIDIA runtime class. UDP port `9000` is admitted only from pods labeled
`veoveo.ai/stream-producer: "true"`. External producers use their own
installation-specific pipeline and NetworkPolicy configuration.

TensorRT engines are compiled for the deployment GPU during the model-cache
initialization step. NVIDIA driver libraries injected by the Container Toolkit
take precedence over bundled compatibility libraries.

## Testing Strategy

CPU-independent contract tests cover catalog closure, URI parsing, typed
runner requests, native result validation, the self-contained App surface, and
RTP packetization.

The GPU smoke uses the deployed Stream image and an actual H.264 source. It
must prove:

- RTP ingress produces results before any recording replay begins;
- NVDEC, DeepStream, and TensorRT process non-zero frames;
- the result freshness bound is met;
- the App decodes actual H.264 and displays typed overlays;
- the App's hardware or software H.264 label agrees with Media Capabilities
  for the exact codec, dimensions, bitrate, and frame rate;
- recording replay still publishes governed artifacts when Recording is
  selected.

The UAV showcase starts the live Stream session before flight, sends the
already encoded nadir-camera access units to Stream before logging them to
Rerun, and retains independent Recording and authoritative live-camera evidence.
After fresh live inference and the authenticated App capture succeed, the
acceptance stops that live session before starting the independent replay
graph. This keeps each admitted DeepStream/TensorRT workload bounded without
weakening the proof that newly arrived frames are processed live.
Visual acceptance uses headed Chrome with hardware-backed WebGPU or WebGL. The
harness probes both APIs when available and fails only when neither reaches
hardware.

## Deliberate Limits

- The first live ingress is RTP/H.264 over UDP with a 90 kHz clock.
- App preview is a bounded MCP resource, not a public broadcast endpoint.
- The first native result profile is detection with optional tracking.
- Pass-through profiles are represented but do not yet emit a public
  transformation result.
- Operator GStreamer configuration is trusted deployment code, never
  untrusted request input.
