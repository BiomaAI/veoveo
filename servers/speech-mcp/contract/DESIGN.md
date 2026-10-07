# Speech Contract

## Standards And Protocols

These Rust Serde and Schemars types define the Speech subset of MCP 2026-07-28,
JSON Schema 2020-12 and the Veoveo native dictation projection. Transcript JSON uses
`veoveo.ai/speech-transcript/v2` with camelCase JSON fields; controlled enum values
use snake_case. Captions use WebVTT. Raw microphone chunks are mono
little-endian float32 PCM. They are an authenticated input transport, not an Artifact
byte-download route.

RFC 9562 identities use lowercase hyphenated UUIDs: transcription Tasks require
version 7, and private dictation accepts browser version 4 and native version 7.
Speech declares its resource routes with `ResourceAddress`; parsing and typed
construction use the same foundational component descriptors. URI schemas preserve
the transcription v7 and dictation v4/v7 patterns. Public
addresses accept no query, fragment, encoded alias or additional path segment.

## Ownership

The parent Speech design owns semantics, authority and limits. This crate contains
only public shapes and validation. Gateway and browser edge depend on this crate
without linking the inference runtime or acquiring its image inputs.
Artifact identities and metadata come from the domain-owned
[`veoveo-artifact-contract`](../../../platform/artifacts/contract/DESIGN.md).
This crate has no MCP transport, database, or asynchronous runtime dependency.

`task_kind.rs` owns `SpeechTaskKind`, the checked transcription Task operation. Service
admission obtains its name through `veoveo-types::TaskTypeDefinition`. Shared Task
infrastructure imports no Speech types.

`identity.rs` owns distinct `TranscriptionId` and `DictationSessionId` values.
`TranscriptionId::try_from(TaskId)` checks the native UUIDv7 profile without a text
round trip; `task_id()` supplies its native runtime identity.
`resources.rs` owns all Speech resource families and document names. Its
`TranscriptionUri` and `DictationUri` constructors require the matching identity type;
their JSON representation is a string. `SpeechScope` declares no additional domain
scopes. Gateway operation policy and current Task/source/session authority authorize
each request.

The service retains typed transcription IDs through authorization, execution and
publication, converting at the shared Task runtime adapter. MCP reads and subscriptions
parse the same resource variants. Dictation state, native HTTP, gateway policy targets
and the browser edge carry `DictationSessionId`. A snapshot constructor derives its
URI from its private identity, and decoding rejects mismatched session/URI pairs.
Generated Workspace schemas and types consume this contract.

## Value Admission

DictationSnapshot implements `Check` for its private session-ID/result-address agreement. Construction and decoding reuse that admission. Its public progress fields retain their existing runtime mutation API, so this model uses an explicit representation exception instead of immutable whole-model Checked storage. Progress updates cannot mutate either identity field.

Transcript construction and decoding share interval, ordering, finite duration and
size admission, including the 0.1-second rounding allowance. The recording ceiling
is portable; the smaller dictation ceiling is checked by the session service.
Immutable transcript documents admit their owned schema tag and bare lowercase
SHA-256 source digest. Transcription outputs admit distinct JSON and WebVTT
occurrences, Speech presentation, no download locations and finite duration.
Retained reads additionally bind the source and result Task to the selected request. Each output occurrence's
Speech attribution admits exactly `sourceArtifactUri`, `sourceSha256`, `model` and
`modelRevision`. Construction and retained decoding reject additional fields in
that owner projection, including retired names alongside current fields. The generic
Artifact metadata map stays open for other owners. Speech also checks that both
occurrences agree on attribution and identify the selected source.

Speech maps transcript subscriptions through the shared Task resource watch. It
authorizes source access before subscribing and again before every notification,
including resource-only updates. Independent dictation state is not a Task watch.

Hosted transcript listeners deliver through the maintained subscription context and filter-enforcing sink. Each native Task update rechecks current source access before resource or Task notifications. Context cancellation and a dropped client subscription end the listener and release its native observation stream; reconnect establishes a newly authorized baseline.

## Wire Naming And Format Admission

`TranscribeRequest` admits `artifactUri`; the prompt argument `artifact_uri` keeps
its identifier profile. Dictation controls and receipts admit `sampleRate`,
`resultUri`, `nextSequence` and `maxDurationSeconds`. Transcript products admit
`durationSeconds`, `sourceArtifactUri`, `sourceSha256` and `modelRevision` where
those fields occur. Typed constructors and decoding keep the same ID, interval,
source attribution and resource agreement checks. Unknown or retired field names
fail decoding; no alias selects a second spelling.

`TRANSCRIPT_SCHEMA` supplies the supported document tag. Its decoder rejects
unsupported formats with a coordinated-upgrade diagnostic. JSON schema generation
uses the same owner field spelling as serialization and admission. Task kind names,
resource templates and identity values keep their own grammar. The parent design
owns the required installation drain and the private worker protocol.
