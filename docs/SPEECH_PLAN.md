# Speech Delivery Plan

Status: implementation started September 22, 2026. GPU qualification and installed
acceptance are required before this plan can be marked delivered.

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
