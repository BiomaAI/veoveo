# Reason MCP Design

This document is the canonical design and operational contract for the
`reason-mcp` crate.

`reason-mcp` is Veoveo's provider-neutral video reasoning domain. It answers
semantic and temporal questions about recorded sensor video: what happened in a
segment, which events occurred and when, and what a bounded prompt asks about
the footage. Its production execution implementation uses a locally deployed
multimodal world model compiled to a TensorRT-LLM engine, but NVIDIA names do
not appear in its public MCP identities.

This is deliberately not part of `perception-mcp`. Perception is bounded
deterministic inference: a site-approved detection engine whose calibrated
per-frame output is reproducible byte for byte. Reasoning output comes from a
generative model and carries model-reported rather than calibrated confidence.
Keeping the domains separate preserves perception's audit story and keeps the
DeepStream and TensorRT-LLM release trains independently upgradable. It is also
not part of `media-mcp`, because reasoning runs entirely inside the
installation. It uses no provider API, no webhook completion, no resident
inference service, and no agent framework.

## Data path

```text
recording-hub frozen/sealed segments
        |
        | authorized logical-recording read plan
        v
   reason task
        |
   keyframe scan + no-transcode MP4 remux
        v
   reason-runner: NVDEC decode -> frame sampling -> observation frames
                              -> world-model engine (TensorRT-LLM)
        |
        v
typed reasoning results + derived RRD annotations + artifacts
```

The recording authorization contract is identical to perception's. A task
first authorizes the canonical `recording://recordings/{uuidv7}` identity
against its tenant and labels, then re-resolves that identity inside the
durable task and reads only frozen or sealed segments. No filesystem path and
no bearer token is ever persisted. The canonical video ingest profile is the
one documented in `servers/perception-mcp/DESIGN.md`: H.264 Annex B
`VideoStream` samples, nanosecond duration or timestamp timelines, no
B-frames, and sparse `is_keyframe=true` markers on decoder-reentrant
keyframes.

## Reasoning contract

One tool, `analyze_recording`, accepts a video selection, a pipeline identity,
and one typed reasoning task:

- `describe_segment` produces a text description of the selected range, with
  an optional focusing prompt.
- `detect_events` produces a bounded list of typed events. Each event carries
  an inclusive source-timeline index range, a short label, and a description.
- `answer_question` produces a text answer to one question about the range.

Frame sampling is explicit. A pipeline declares its observation resolution,
and the request bounds how many observation frames the model receives. The
runner reports how many frames it actually observed.

Every result carries its audit identity: the model, the optional engine
digest from the catalog, the prompt template revision, and the decode
parameters that produced it. Decoding is greedy by default. Sampled decoding
is opt-in per request and its parameters are recorded in the result. The
result also states `confidence_basis: model_reported`, which distinguishes
reasoning output from perception's calibrated detector confidences. Same
engine, same input, same prompt revision, and greedy decoding must produce
the same result.

A request may reference grounding: the governed results artifact of a
completed perception analysis over the same recording. The server resolves
the artifact with the caller's authority at submission time, validates its
schema, extracts a bounded typed subset of detections, and embeds that
subset in the durable request. Reasoning output may then cite perception
track identities. Grounding never travels as a bearer token or a URL.

## Work Context and ownership

Reason tasks retain the gateway-resolved invocation authority at creation,
exactly as every hosted server does under the Work Context governance model.
Artifact publication stamps the context's output owner and initial grants,
and the source recording's classification and labels flow onto every derived
artifact. The server has no legacy ownership path.

## NVIDIA execution boundary

One runner process is created for each reasoning task. The server writes a
typed JSON request, invokes the configured runner binary, and reads a typed
JSON response from the path named in the request. That boundary gives
task-level timeouts, cancellation, filesystem isolation, and a small crash
boundary, and it keeps the GPU dependency out of the server process. The
runner decodes the remuxed MP4 with NVDEC, samples frames to the pipeline's
observation resolution, executes the TensorRT-LLM engine, and writes the
typed answer. Frame indices are reconstructed as
`decode_start_index + buffer PTS`, so every event lands on the original
recording timeline.

The runner binary belongs to the deployable image, not to site
configuration. The engine is the opposite: a site-supplied deployment input,
compiled for the deployment GPU with the TensorRT-LLM build shipped in the
image. Engine compilation is a deployment step, never request-time behavior.
The server validates the catalog, the engine path, the prompt template, and
the runner at startup and fails readiness when any is missing. There is no
CPU inference fallback.

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
static document, so events appear in the console viewer beside perception's
bounding boxes. Large bytes are never returned inline; oversized occurrences
use the governed artifact download path.

## GPU image and Kubernetes deployment

The Kubernetes node needs an NVIDIA driver compatible with the image's
TensorRT-LLM build, NVIDIA Container Toolkit, and the NVIDIA device plugin.
The pod requests one `nvidia.com/gpu`; a missing GPU is a scheduling or
readiness failure, never a CPU fallback. The Helm chart ships the workload
disabled by default because enablement requires two site inputs: the
deployable runner image and a site-compiled engine.

The production installation uses two read-only mount roots:

```text
/opt/veoveo/reason/config/
  catalog.json
  prompt-template.txt
/opt/veoveo/reason/models/
  world-model.engine            # site-compiled TensorRT-LLM engine
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
only on a deployment whose runner image and engine are present.

## Deliberate limits

- The production contract is H.264 `VideoStream`, identical to perception's
  ingest profile. Other codecs and frame-series timelines are rejected.
- The catalog accepts TensorRT-LLM engines only. Checkpoint-to-engine
  compilation is a deployment build step, not request-time behavior.
- Reasoning confidence is model-reported. Results are audit-stamped and
  greedy-deterministic, but they are not calibrated detector output and the
  contract never presents them as such.
- Grounding accepts the typed perception results schema only. Opaque or
  unversioned grounding payloads are rejected at submission.
- The runner binary ships with the deployable image. This repository defines
  the runner contract and the server enforces it fail-closed; a deployment
  without the runner image and a site-compiled engine keeps the workload
  disabled.
- There is no live-proxy read mode and no attachment to a live camera. Reason
  tasks operate only on frozen or sealed segments.
