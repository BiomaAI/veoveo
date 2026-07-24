# Perception MCP Design

This document is the canonical design and operational contract for the
`perception-mcp` crate.

`perception-mcp` is Veoveo's provider-neutral local perception domain. Its
production execution implementation uses NVIDIA DeepStream and TensorRT, but
NVIDIA names do not appear in its public MCP identities.

This is deliberately not part of `media-mcp`. Media owns provider-submitted
generation and other conversational, webhook-completed jobs. Perception owns
bounded inference over recorded sensor data already inside the installation.
It uses no LLM, NIM, NeMo Agent Toolkit, Triton server, transcription service,
or provider API.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with task-only analysis, resources and templates, structured results, notifications, and usage records. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Video selection, pipeline, detection, tracking, provenance, and artifact contracts. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`; analysis is durable, cancellable, resumable from governed recording identity, and result-addressable. |
| Veoveo recording ingest | Version `2026-07-21`; authenticated protobuf batches carry producer Rerun messages into Recording Hub before perception begins. |
| [Rerun](https://rerun.io/docs/) RRD and `VideoStream` | Frozen and sealed source recordings, exact timeline indices, and derived annotation recordings. |
| H.264/AVC Annex B | One access unit per Rerun sample, strictly increasing indices, no B-frames, sparse keyframe markers, and decoder-reentrant IDRs with SPS/PPS. |
| ISO Base Media File Format / MP4 | Selected Annex B samples are remuxed without re-encoding. The runner receives a zero-based presentation timeline plus the original Rerun decode-start index. |
| GStreamer / NVIDIA DeepStream metadata | Process-local GPU runner boundary for decode, batching, inference, and optional tracking. It is not exposed as an MCP or public network protocol. |
| OAuth bearer and signed JWT identity | Recording and artifact access retain gateway-resolved tenant, Work Context, classification, labels, and caller authority. |

## Data path

```text
camera / sensor producer
        |
        | Rerun VideoStream: H.264 Annex B samples + timestamps + keyframes
        v
recording-forwarder:9876/proxy  (producer loopback only)
        |
        | authenticated versioned protobuf batches
        v
gateway -> recording-hub -> frozen .rrd -> sealed artifacts
                                |
                                | authorized logical-recording read plan
                                v
                         perception task
                                |
                         keyframe scan + no-transcode MP4 remux
                                v
                     nvv4l2decoder -> nvstreammux -> nvinfer
                                                -> optional nvtracker
                                                                  |
                                                                  v
                   typed JSON + derived RRD annotations + artifacts
```

The Rerun receiver is a producer-local transport, not durable storage or an
authorization boundary. The forwarder authenticates to the gateway and sends
versioned protobuf batches. Recording Hub materializes each accepted batch as
an immutable RRD segment and catalogs it in SurrealDB. A logical Rerun recording
is identified by application ID plus recording ID and may span multiple
segments.

Perception first authorizes the canonical
`recording://recordings/{uuidv7}` identity against its tenant and labels. A
durable task then re-resolves that identity and reads only frozen or sealed
segments; it never persists a filesystem path or bearer token. All authorized
physical segments are loaded as one logical Rerun store before video is
selected, so an IDR in one segment can decode a requested P-frame in the next.
There is no live-proxy read mode. Perception tasks have one resumable source
contract and operate only on frozen or sealed segments.

Video ranges use inclusive timeline bounds. A decoder-reentrant keyframe may
precede the requested start as preroll or fall just after it when the requested
start lies between discrete camera samples. The decode start must remain at or
before the requested end, and Perception publishes only frames inside the
requested range.

## Canonical video ingest profile

The current production runner accepts one profile:

- Rerun `VideoStream` samples using H.264 Annex B access units.
- A duration or timestamp timeline whose raw values are nanoseconds.
- Strictly increasing sample indices, with one sample at each index.
- No B-frames, so decode and presentation order are identical.
- Sparse `is_keyframe=true` markers on decoder-reentrant keyframes. An absent
  marker means the sample is not a keyframe.
- Decoder-reentrant IDRs at the desired seek interval. Each reentrant IDR must
  make SPS/PPS available so a clip can begin there without earlier stream
  state.

This matches Rerun's current streaming-video model: encoded samples are stored
in `VideoStream`, H.264 samples use Annex B, B-frames are unsupported, and an
MP4 can be produced by remuxing without re-encoding. See Rerun's
[video reference](https://rerun.io/docs/concepts/logging-and-ingestion/video)
and [video query guide](https://rerun.io/docs/howto/query-and-transform/query_videos).

The repository's executable ingest example is the real cross-segment test in
`platform/recordings/hub/tests/spool_roundtrip.rs`. Its producer shape is:

```rust
stream.set_duration_secs("sensor_time", timestamp_seconds);
let mut video = VideoStream::new(VideoCodec::H264).with_sample(annex_b_access_unit);
if is_idr {
    video = video.with_is_keyframe(true);
}
stream.log("/world/camera/front", &video)?;
```

The test sends valid H.264 through the Hub's loopback Rerun receiver, restarts
the spooler, requests a P-frame from the second physical segment, finds its IDR
in the first segment, remuxes both samples, and decodes the resulting MP4 when
FFmpeg is available. The authenticated gateway ingest smoke separately proves
the production forwarder-to-Hub boundary.

## Extraction and original time

The extractor queries `sample`, `codec`, and `is_keyframe` together on the
selected entity and timeline. It scans backward from the first requested sample
to the nearest decoder-reentrant keyframe, enforces configured sample and byte
ceilings, and remuxes the retained H.264 access units into MP4 with a 1 GHz media
timescale. It does not decode or re-encode on the CPU.

MP4 presentation time starts at zero for decoder preroll. The typed runner
request separately carries `decode_start_index`; DeepStream returns each frame's
buffer PTS and the runner reconstructs the exact Rerun index as:

```text
original Rerun index = decode_start_index + DeepStream buffer PTS
```

This preserves a timestamp timeline even when the requested range begins after
the keyframe or crosses an RRD segment boundary. Derived RRD annotations use the
explicit source timeline kind rather than guessing from its name.

## NVIDIA execution boundary

The deployable image is a two-stage DeepStream 9 build:

- `nvcr.io/nvidia/deepstream:9.0-triton-multiarch` supplies the C++ development
  headers only during the build.
- `nvcr.io/nvidia/deepstream:9.0-samples-multiarch` is the final runtime base.
- The C++ runner builds `filesrc -> qtdemux -> h264parse -> nvv4l2decoder ->
  nvstreammux -> nvinfer -> [nvtracker] -> fakesink`.
- `gst-nvinfer` loads a site-approved TensorRT engine and returns native
  `NvDsBatchMeta`, `NvDsFrameMeta`, and `NvDsObjectMeta` as a bounded typed JSON
  response.
- The optional tracker uses NVIDIA's low-level multi-object tracker library and
  an explicitly mounted tracker YAML.

Container execution requires one coherent host driver library set. The native
driver libraries injected by NVIDIA Container Toolkit take precedence over the
CUDA forward-compatibility libraries bundled in the DeepStream image. GeForce
GPUs do not support the latter path. A successful `nvidia-smi` call therefore
does not replace an acceptance test that decodes video and executes inference.

The deployed process does not start Triton. NVIDIA documents the `triton` image
as its development image and the `samples` image as the runtime containing
DeepStream libraries and GStreamer plugins; see the
[DeepStream container guide](https://docs.nvidia.com/metropolis/deepstream/dev-guide/text/DS_docker_containers.html).

One runner process is created for each analysis task. That gives task-level
timeouts, cancellation, filesystem isolation, and a small crash boundary. It is
optimized for local bounded analysis, not a permanently attached live camera
pipeline. Multiple concurrent tasks should be limited to the GPU capacity of
the deployment. The server therefore defaults to one active perception job;
additional durable tasks remain queued while lease heartbeats continue.

## MCP surface

The gateway mounts the server at `/perception/mcp` and exposes:

- tools: `analyze_recording`, `extract_clip`;
- resources and templates for pipelines, models, analyses, results, and derived
  artifacts;
- prompts: `perception-analyze-recording`, `perception-extract-clip`;
- completions for pipeline, model, analysis, and artifact identities;
- final durable tasks, task subscription, cancellation, and result retrieval;
- resource subscriptions and list-changed/update notifications;
- typed structured tool content and canonical `perception://` resource links.

Canonical resources include:

```text
perception://pipelines
perception://pipeline/{pipeline_id}
perception://models
perception://model/{model_id}
perception://analyses
perception://analysis/{task_id}
perception://analysis/{task_id}/results
perception://artifact/{artifact_id}
```

Analysis publishes three immutable occurrences through the shared artifact
plane: typed JSON results, a Rerun annotation layer, and optionally the remuxed
source clip. Derived artifacts inherit the source recording's classification
and labels. Large bytes are never returned inline or exposed from a second HTTP
file route. Runner JSON is capped at 256 MiB by default, and an MCP resource read
is capped at 16 MiB before the artifact body is fetched; larger occurrences use
the governed artifact download path.

## GPU image and Kubernetes deployment

The Kubernetes node needs an NVIDIA driver compatible with DeepStream 9, NVIDIA
Container Toolkit, and the NVIDIA device plugin. The pod requests one
`nvidia.com/gpu`; a missing GPU is a scheduling or readiness failure, never a CPU
fallback. Log in to NGC before building the image:

```bash
docker login nvcr.io
```

The production installation uses two read-only mount roots:

```text
/opt/veoveo/perception/config/
  catalog.json
  primary-detector.txt
  tracker.yml                 # only for tracking pipelines
/opt/veoveo/perception/models/
  primary-detector.engine
  labels.txt                  # when referenced by the nvinfer config
```

Start from `configs/perception/catalog.example.json`, then set:

```dotenv
PERCEPTION_CONFIG_DIR=/opt/veoveo/perception/config
PERCEPTION_MODEL_DIR=/opt/veoveo/perception/models
```

The server fails readiness when the catalog, TensorRT engine, nvinfer config,
tracker config, or C++ runner is missing. There is no CPU inference fallback.

TensorRT engines should be built for the deployment GPU and the TensorRT
runtime shipped with DeepStream 9. By default, serialized engines are specific
to their TensorRT build version and GPU type; NVIDIA documents optional version
and hardware compatibility modes with possible performance tradeoffs in the
[TensorRT engine compatibility guide](https://docs.nvidia.com/deeplearning/tensorrt/latest/inference-library/engine-compatibility.html).

## Testing Strategy

Implemented crate tests cover:

- catalog validation and the repository catalog example
- canonical perception resource identities
- typed runner request construction and source-index preservation
- rejection of invalid or out-of-bounds detections

The implemented GPU smoke is a Rust scenario over the production service
boundaries. It covers:

- real H.264 `VideoStream` ingress through Recording Hub
- catalog resolution to a governed UUIDv7 recording identity
- internal authentication and final MCP task execution
- NVDEC, DeepStream, and TensorRT execution against a site-built engine
- typed result, Rerun annotation, and source-clip publication through the
  shared artifact plane

The fixture must produce decoded frames and at least one valid detection. Its
exact detection count is diagnostic rather than a protocol contract. The smoke
harness owns process lifecycle and cleanup; the Justfile remains a short human
dispatch surface.

## Deliberate limits

- The first production contract is H.264 `VideoStream`; it does not silently
  accept `AssetVideo`, JPEG frame series, AV1, H.265, or sequence timelines.
- The catalog accepts TensorRT engines only. ONNX-to-engine compilation is a
  deployment build step, not request-time behavior.
- The runner supports object detection and object detection plus tracking.
  Segmentation and pose require new typed result contracts before they can be
  enabled.
- Recent proxy replay is bounded convenience over an already cataloged
  recording, not the durable source of truth.
- macOS can build and test the Rust protocol/extraction layers but cannot run
  the NVIDIA container. End-to-end GPU validation runs on the Ubuntu target.
