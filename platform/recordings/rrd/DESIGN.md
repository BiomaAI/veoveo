# Shared RRD Operations

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Rerun RRD and SDK types | Existing repository wire profile 0.38.1; this extraction preserves its pinned ABI |
| H.264 Annex B | Decoder-reentrant access units with SPS/PPS, IDR frames, and no B-frames |
| ISO Base Media File Format | Bounded H.264 MP4 remux through mp4 0.14.0, without decoding or encoding |
| SHA-256 | Complete immutable segment and layer byte identity |
| Veoveo spatial metadata | Published frame-kind and geofence-rule enum spellings and the 1–128 byte geofence identity profile; distinct from Map's operation vocabulary |
| Veoveo ingest parts | Ordered numeric `.rrd` files; atomic UUIDv7 staging files and the video marker are excluded |
| Apache Arrow IPC | Bounded projection of admitted RRD data |

## Ownership

This library owns file formats and typed Rerun adapters. It has no Hub lifecycle,
producer authentication, catalog mutation, or MCP transport dependency. A caller must
supply an authorized input set before invoking a file operation.

`spatial_metadata.rs` owns the recorded `FrameKind`, `GeofenceId`, and `GeofenceRule`
values. Recorded geofences use `must_stay_inside` and `must_stay_outside`; Map's current
geofence operation vocabulary has a different profile. RRD imports CRS, datum, and
ellipsoid IDs from Map's contract feature without its runtime dependencies. The wire
schemas and recorded values keep the 0.38.1 profile.

`segment.rs` verifies one complete RRD file and its unique producer recording identity.
`ingest_parts.rs` discovers complete parts in sequence order and rejects unknown names
or non-file entries. The Hub owns publication and recovery of those files.

`video_clip.rs` combines the supplied segments as one logical recording and selects a
bounded encoded range with the preceding decoder-reentrant keyframe. It remuxes existing
H.264 access units into MP4. No image is rendered, and no video is decoded or encoded by
this operation. GPU decode and inference remain the consuming runtime's responsibility.

`recording_layer.rs` normalizes and verifies canonical durable Store IDs.
`properties_layer.rs` builds deterministic properties layers. `projection.rs` owns the
Arrow implementation. It imports the sealed query and sampling vocabulary from the
[Recording domain contract](../contract/DESIGN.md). `ArrowProjectionQuery::new` parses
those selectors with the pinned Rerun types and rejects duplicate resolved entities
without opening files. Every Arrow writer requires this prepared type. During execution,
row and sample counts and the byte writer enforce the admitted limits; cancellation or
failure removes partial output. Its summary carries typed SHA-256 values and a nonzero
byte length into the Recording result builder. `video.rs` owns encoded access-unit inspection.

## Canonical Layers And Sealed Properties

Normalization accepts distinct public dataset and recording IDs, converting them to
Rerun application/recording text only when constructing the Store ID. Producer names
stay open. Byte and schema digests use the shared digest type; the schema hash inputs,
compressed encoder settings and field order are unchanged. A guarded staging file is
synced and inspected before atomic replacement. Decoder failures, multiple stores and
interruption before replacement leave source bytes intact and remove staging. A
post-rename directory-sync failure reports an uncertain installation; retry inspection
can recognize the complete installed file.

Properties consume the pure contract's checked model. Their deterministic RRD JSON
preserves timestamp spelling, omitted empty maps and bare digest values. The properties
producer writes the complete RRD 0.38.1 footer through the maintained manifest builder,
sorting its Sorbet schema fields with Arrow's ordering. Message order, compression,
JSON encoding and schema-hash inputs stay fixed. Sorting footer fields stabilizes the
whole-file hash; an arbitrary footer field order produces different whole-file bytes.
The Recording producer profile requires this deterministic footer and a fresh installation
cut. Retained files with different bytes fail admission rather than being rewritten or
normalized. The private writer records the encoder's actual chunk span and uncompressed
length, validates the complete manifest and checks the existing schema digest before
emitting its custom footer. Before reusing
an existing Writing-layer file, the builder compares its complete bytes and Store ID
with the deterministic admitted properties. New files are inspected and installed by
rename. The preparation callback admits complete expected stage facts before any final
file is reused or installed; every pre-install failure removes the owned partial. The caller still owns
current authorization, publication fencing and retention.

## Verification

`cargo test -p veoveo-rrd --lib` exercises the file contracts. Hub integration tests cover
video selection across a producer restart and verify the resulting MP4 sample table.
These shared operations are exported only from `veoveo_rrd`.

## Identity Declaration Mechanics

RRD identity declarations use `Id` with RRD-owned lexical admission. Rerun entity/frame spellings keep their existing allowance for slash and `tf#` text, while geofence IDs apply their coordinate profile. These identities keep their existing String conversion and schema declarations.
