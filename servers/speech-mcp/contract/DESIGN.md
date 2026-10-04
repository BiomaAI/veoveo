# Speech Contract

## Standards And Protocols

These Rust Serde and Schemars types define the Speech subset of MCP 2026-07-28,
JSON Schema 2020-12 and the Veoveo native dictation projection. Transcript JSON uses
`veoveo.speech-transcript/v1`; captions use WebVTT. Raw microphone chunks are mono
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
