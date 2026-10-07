# Recording Hub

## Standards And Protocols

Hub implements the authenticated Recording ingest protobuf profile declared in
`docs/RECORDING_INGEST.md`. Durable payloads use the pinned Rerun RRD profile.
SurrealDB holds stream acceptance and materialization checkpoints. Local filesystem
journals use synchronized writes and atomic publication on the same filesystem.
`veoveo.ai/recording-journal-quarantine/v1` is an internal JSON recovery receipt,
owned by Hub; it is not a producer protocol or an accepted recording batch.
The `sensor-sim --stack` manifest is Hub-owned JSON for deterministic ingest fixtures.
`veoveo.ai/recording-ingest-diagnostics/v2` identifies Hub's flattened camelCase
JSON counters. Authenticated ingest uses the `2026-09-23` protobuf profile in its
`v1` package and media type; internal ingest routes use version 1.

## Sensor Manifests

The stack loader rejects unknown fields in the root, each sensor variant, coordinates,
track patterns and waves before it creates a runtime or generator. A private tagged
wire enum admits each complete flat sensor object; serialization preserves that flat
shape. Required fields must be present, while omitted or null `durationS` selects
an unbounded run. Sensor IDs pass the same nonempty ASCII path-component validation
through both construction and decoding. Loader errors identify the manifest path.

## Recording Identity

Hub imports [`veoveo-recording-contract`](../contract/DESIGN.md) to construct the
public Recording URI returned by ingest. Store IDs convert to its checked RFC UUIDv7
identity before the shared builder emits the protobuf string. Hub has no dependency
on the Recording MCP server. The domain crate also supplies the MCP server’s public
contract, keeping producer and reader address admission identical.
Hub also consumes its `RecordingProducerScope`: ingest checks the caller's granted
names against `Ingest`, and Artifact publication requests `Publish` through the
typed producer OAuth client. MCP sealing uses the separate `RecordingScope` enum.

## Immutable Layer Publication

Hub carries distinct dataset/recording identities into RRD normalization and typed
SHA-256 values into layer staging and Artifact streaming requests. The layer UUID maps
directly to the Artifact occurrence UUID. Publication admits the returned neutral
Artifact URI, expected occurrence and length, and absence of a download location before
catalog commitment. ArtifactMetadata supplies no declared content digest; the streaming
request's checked digest and Artifact service verification establish content integrity.
Hub does not infer a digest from opaque metadata. Producer Rerun application and recording
names keep their upstream profile.

## Archive Materialization

Hub applies Rerun's `OBJECT_STORE` chunk-compaction profile to a complete archive
shard. It writes the result through the RRD encoder directly to a staged file, then
finishes the footer, synchronizes the file, and publishes it by rename. The archive
test reopens the footer and checks that compaction reduced chunk count. Live playback
uses the separate `LIVE` profile to keep updates responsive.

The native `h264_video_extracts_across_restart_segment_boundary` control checks
retained encoded samples across segment restart, decoder-reentrant keyframe selection
and MP4 sample tables, including a six-second gap. It performs no video decoding.
Mandatory NVDEC with hardware-surface validation and headed hardware playback require
separate GPU qualification; both gates are pending.

## Terminal Journal Recovery

Startup replays accepted duplicate batches even when their stream is finished.
A journal at or beyond a terminal stream's authoritative `next_sequence` was never
accepted. Hub preserves it under `.ingest-journal/.quarantine/<tenant>/<stream>/`
and writes a receipt containing the terminal state, cutoff, sequence, exact journal
hash and byte length. A hard link avoids copying the payload. Both files and the
directory are synchronized before the original replay candidate is removed.
Interrupted publication resumes only when existing bytes and the receipt match.
Conflicts or failed durable publication retain the original and stop recovery.

Quarantine does not reopen a stream, advance a checkpoint, publish content, or
release storage. It remains charged to the spool's available-space floor. Hub logs
the stream and sequence for operator recovery; operators retain the files until an
explicit retention or export decision. Subsequent startup skips the private
quarantine directory. Accepted journals and mutable capture-layer recovery retain
the strict replay and ordering rules in `docs/RECORDINGS.md`.

The filesystem boundary tests cover interrupted publication, byte and receipt
conflicts, symlink rejection, accepted duplicate selection and terminal cutoff
selection. The installed recovery must also verify the exact bytes against the
receipt and the unchanged durable stream checkpoint.

## JSON Adapter Profiles

Spooler configuration and flat sensor manifests use closed camelCase members.
Track and wave discriminants use snake_case. Sensor reports use the same controlled
member profile; these unversioned development JSON models admit one spelling.
Producer Rerun application names, encoded protobuf/RRD batches and native synchronized
forwarder/Hub recovery records keep their profiles. A renamed config member refuses
before the stack loader constructs a runtime or emits a sample.
