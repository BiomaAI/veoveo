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
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; every reasoning invocation is a durable, cancellable task whose terminal payload is returned by `tasks/get`. |
| [Rerun 0.36.0](https://rerun.io/docs/) RRD and `VideoStream` | Frozen or sealed sources and task-start snapshots of complete acknowledged ingest parts preserve exact time; derived semantic events are published as RRD annotations. |
| H.264/AVC Annex B | The source profile matches Stream: no B-frames and decoder-reentrant IDRs marked in the Rerun stream. |
| ISO Base Media File Format / MP4 | A bounded source range is remuxed without re-encoding for the task-local decoder and world-model runner. |
| Typed JSON process protocol | One schema-controlled request and response per isolated runner process. This boundary is private and does not replace MCP. |
| OAuth bearer and signed JWT identity | Source recording, grounding artifacts, results, and derived artifacts retain gateway-resolved Work Context authority and labels. |
| vLLM official Muse Glimmer launch image | Hardware-GPU execution through the upstream model launch image. Upstream publishes this model profile under an unversioned tag; Veoveo pins the 2026-08-11 snapshot built from vLLM commit `99a10304dce8945119bd0b1a072297803c52a749` by OCI manifest digest. |
| Hugging Face checkpoint | A site-supplied, revision- and digest-pinned checkpoint in native Transformers layout. |

## Data path

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
No filesystem path or bearer token is persisted. The canonical video ingest
profile is the one documented in `servers/stream-mcp/DESIGN.md`.

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

A request may reference grounding: a governed
`stream://artifact/{artifact_id}` results artifact produced by a completed
Stream perception replay over the same recording. The server resolves
the artifact with the caller's authority at submission time, validates its
schema, extracts a bounded typed subset of detections, and embeds that
subset in the durable request. Reasoning output may then cite Stream track
identities. Grounding never travels as a bearer token or a URL.

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
- resource subscriptions and list-changed/update notifications;
- typed structured tool content and canonical `reason://` resource links.

Canonical resources include:

```text
reason://pipelines
reason://pipeline/{pipeline_id}
reason://models
reason://model/{model_id}
reason://analyses
reason://analysis/{task_id}
reason://analysis/{task_id}/results
reason://artifact/{artifact_id}
```

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
