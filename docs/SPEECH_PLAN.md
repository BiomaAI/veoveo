# Speech Delivery Plan

Status: delivered September 22, 2026 at `https://veoveo.bioma.ai/workspace/`.
Private dictation and governed recording transcription passed installed acceptance.
The qualification limits below remain explicit follow-up work.

## Standards And Protocols

| Boundary | Selected profile |
|---|---|
| MCP `2026-07-28` | Governed transcription tools, official durable Tasks, resources, prompts, completions and request-scoped subscriptions through the existing Rust runtime |
| Veoveo Artifact plane | Authorized source reads, bounded Task capabilities, immutable transcript publication and current-authority downloads |
| Veoveo browser HTTP and SSE | Same-origin cookie/CSRF admission, private dictation state and reactive transcript snapshots; audio transport is separate from MCP JSON |
| Browser Media Capture and Web Audio | Explicit microphone permission and bounded mono PCM capture; no implicit browser speech-recognition provider |
| Photon / Parakeet Ultra | Private inference adapter, Moondream `2.4.1`, explicit NVIDIA CUDA execution; source-language transcription and word timestamps |
| JSON and WebVTT | Typed timestamped transcript and downloadable captions; no claim of speaker identification or translation |

## Product Boundary

Workspace gains a microphone beside its existing message composer. A person starts
capture, watches the private draft develop, stops, reviews the text and sends it
through the ordinary message path. The existing recipient selection and chat owner
policy determine which agents respond. A partial transcript never starts an agent.
Navigation, lost authority and explicit cancellation stop capture. Dictation audio
is ephemeral unless a person explicitly saves a recording.

An uploaded audio or video Artifact can be transcribed from Workspace. Its operation
appears in private Activity with cancellation, recovery and a final transcript link.
The service extracts only the audio stream from video. It does not render video.
Timestamps support transcript playback and caption export. Sharing into a chat is an
explicit action and does not grant access to a restricted source or transcript.

Meeting capture, speaker attribution and spoken agent replies are later work. The
first delivery introduces one Speech service and reuses the current Artifact, Task,
policy and browser infrastructure.

## Ownership And Implementation

`servers/speech-mcp` owns the public speech domain and its private inference worker.
Rust owns protocol admission, durable Task state, authority, cancellation, source
limits and publication. A persistent private Python worker owns the qualified Photon
CUDA runtime. No model is loaded per utterance. The service has an independent image
because its GPU dependencies and capacity differ from the gateway and browser edge.

Workspace owns microphone controls and review-before-send. The browser edge fixes
the Workspace profile and protects native speech requests with its existing session
and CSRF checks. The gateway applies current installed policy before dispatch. Native
browser projections reuse the domain operations and do not create another Task store.
Operator configuration belongs to the Speech MCP surface and installation manifest.

Recorded transcription is finite durable work. Dictation is a short-lived bounded
session with observable state, explicit completion and cancellation. Audio frames
travel through an authenticated data path rather than JSON tool arguments. Transcript
updates replace previous provisional snapshots; completed output is immutable.

## Delivery Sequence

1. Qualify the current Photon package and exact model revision on the available GPU.
   Record cold initialization, warm inference, timestamps and live snapshot behavior.
2. Implement typed Speech contracts and the bounded persistent inference worker.
3. Add durable recorded transcription with governed source reads, transcript
   publication, cancellation, authorized resources and notifications.
4. Add private Workspace dictation and recording transcription UI through the
   existing authentication and operation-receipt boundaries.
5. Package and register Speech, admit its GPU resources, and stage only affected
   services. Activate through Bioma's normal immutable GitOps configuration.
6. Run native authority/recovery tests and headed hardware browser acceptance on
   veoveo.bioma.ai. Record exact deployed images and remaining qualification limits.

## Acceptance

- Real speech produces accurate English and Spanish transcripts on NVIDIA hardware.
  Fixture expectations, elapsed time and observed execution device are recorded.
- Dictation stays private, preserves typed draft edits, and requires an explicit Send.
  It does not change agent selection or create model calls while idle.
- Stop flushes a final transcript. Cancel and navigation stop the microphone and
  terminate inference. Disconnection cannot silently send or repeat a message.
- Uploaded recordings produce a durable Task and governed timestamped output.
  Reload observes the same operation. Cancellation and process recovery settle
  truthfully without duplicate transcript publication.
- A different actor cannot read a private session or Task. Current revocation stops
  access. Derived artifacts retain source classification and data labels.
- Oversize audio, excessive duration, unavailable capacity and absent GPU fail with
  bounded actionable errors. No CPU inference fallback is admitted.
- Build and deployment records identify affected inputs, cache reuse, phase timings
  and any avoidable churn in `docs/DEVELOPMENT_ITERATION.md`.

## Initial Qualification Findings

The workstation has an NVIDIA RTX 4090. Photon's published hardware matrix does not
yet list Parakeet on that card, so an actual CUDA run is required. The published B200
throughput benchmark uses 128 concurrent requests and is not interactive latency
evidence for this installation.

Moondream documents live preview after four seconds of audio, followed by replacement
snapshots every two seconds. The UX must distinguish listening from available text
and allow revisions to provisional words. Qualification will measure actual behavior.

Moondream `2.4.1` and its exact Kestrel `0.8.1` dependency were verified from PyPI on
September 22. The upstream adapter pins Ultra revision
`510e6f5a1c4619f39c72b083c091476935734e65`; weight provenance will be recorded with
the packaged runtime. Model weights carry CC-BY-4.0 attribution.

## September 22 Implementation Checkpoint

The persistent worker passed actual CUDA qualification on the RTX 4090, including
English and Spanish word timestamps, live finalization, no-GPU startup denial,
workspace path confinement, capacity and disconnect cancellation. Private dictation
also passed actor, Work Context and browser-session isolation checks, repeated-start
and duplicate-chunk checks, final flush and discarded cancellation.

The Rust domain and Workspace UI are implemented. Recording Tasks publish JSON and
WebVTT through the real Artifact service and shared store. The native test verifies
sensitive-source admission, output label inheritance, private Task access and
cross-connection observation. A second runtime's cancellation is tested separately.
The browser build and cookie/CSRF projection test passed at this initial checkpoint.
The installed delivery below supersedes its packaging, conformance and recovery gaps.

Artifacts follow existing Work Context access policy. Private Activity refers to the
operation and Task details; it does not redefine Artifact permissions. This distinction
is preserved in the UI and qualification fixtures.


## Installed Delivery And Qualification

Veoveo Speech is deployed with the Bioma installation configuration. In Workspace,
open a chat and choose **Dictate** to produce an editable draft. **Transcribe a
recording** accepts a completed upload or an authorized Artifact link. **My activity**
retains the Task and opens its timestamped transcript, source playback and JSON/WebVTT
downloads. Activity is private; Artifact access follows the current Work Context policy.

The installed revision `3736331b2fc384466ba0f77d1e6b99d324bfaa14` converged through
GitOps with Speech, gateway and browser edge ready. Its immutable image closure is:

| Component | Runtime digest |
|---|---|
| Speech | `sha256:2bfc200da2781a8f555d4ad01c7f2ed6ed1a26dcd1e21d5b92651d0417804696` |
| Gateway | `sha256:949b60df34f70f28bba5c03465bd7b4c3b04ca5f2fb681c28cdd43d58d5214be` |
| Browser edge | `sha256:7d2c59b8ea674cf9c90d03b1e0847317e5cacd4acc3125da2123b4f297909a3f` |

The native Rust browser harness passed against that installation at
`2026-09-23T03:36:03Z`. Evidence and inspected screenshots are under
`output/acceptance/speech/01a0cc55-5295-7c30-b546-52a084ff27da/`. Headed Chrome used
NVIDIA RTX 4090 WebGL. WebGPU was also probed and reported SwiftShader; it was not
accepted as hardware evidence. Synthetic microphone audio passed through the real
AudioWorklet, authenticated browser edge, gateway and CUDA worker.

The run verified draft preservation, a deliberately delayed 2.5-second audio request,
Stop, discarded cancellation and explicit Send without invoking an unselected agent.
It uploaded a distinct recording and observed that exact Task after reload. Timestamp
playback and both downloads passed, including the transcript's source identity. The
7.435-second recording appeared in the viewer 3,469 ms after Transcribe, including the
page reload. This is one interactive observation, not a concurrency benchmark.

Native CUDA tests qualified English and Spanish timestamps, GPU absence denial, path
confinement, capacity, actor/Work Context/session isolation, output-label inheritance,
cross-runtime cancellation and recovery after publication with identical Artifact IDs.
The hosted native boundary passed 26 conformance checks. Workspace has 18 passing unit
tests, including PCM ordering, partial Stop flush and bounded streaming JSON without
Content-Length. The installation and recording URI declarations passed Helm checks.

Installed testing corrected the Speech authentication prefix, source Artifact URI
projection, an outbox query that delayed Task observation, dictation's sensitivity to
HTTP latency and transcript previews behind HTTP compression. Build changes and exact
measurements are recorded in [Development Iteration](DEVELOPMENT_ITERATION.md#speech-iteration-measurements-after-the-corrections).

## Remaining Qualification

- Physical microphone devices, permission prompts and browser/OS combinations need
  separate device qualification; the completed browser run supplied fixture audio.
- Longer recordings, video-container diversity, full capacity, sustained dictation
  and multi-client latency need performance runs. Current evidence does not certify
  the two-hour limit at installation scale.
- Installed catalog readiness qualification for C31 remains declared pending in the
  server documents. The native conformance result does not close that broader gate.
- Cold/offline packaging and full release provenance remain separate qualification.
  The installed images are immutable development-stage images, not release-qualified.

These limits do not prevent using the deployed dictation and recording workflows.
Speaker attribution, translation, meeting capture and spoken replies remain later
product work rather than implied capabilities of this delivery.
