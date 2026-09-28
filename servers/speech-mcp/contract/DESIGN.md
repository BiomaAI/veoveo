# Speech Contract

## Standards And Protocols

These Rust Serde and Schemars types define the Speech subset of MCP 2026-07-28,
JSON Schema 2020-12 and the Veoveo native dictation projection. Transcript JSON uses
`veoveo.speech-transcript/v1`; captions use WebVTT. Raw microphone chunks are mono
little-endian float32 PCM. They are an authenticated input transport, not an Artifact
byte-download route.

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
