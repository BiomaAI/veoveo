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
failure removes partial output. `video.rs` owns encoded access-unit inspection.

## Verification

`cargo test -p veoveo-rrd --lib` exercises the file contracts. Hub integration tests cover
video selection across a producer restart and verify the resulting MP4 sample table.
These shared operations are exported only from `veoveo_rrd`.
