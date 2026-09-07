# Shared RRD Operations

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Rerun RRD and SDK types | Existing repository wire profile 0.36.3; this extraction preserves its pinned ABI |
| H.264 Annex B | Decoder-reentrant access units with SPS/PPS, IDR frames, and no B-frames |
| ISO Base Media File Format | Bounded H.264 MP4 remux through mp4 0.14.0, without decoding or encoding |
| SHA-256 | Complete immutable segment and layer byte identity |
| Veoveo ingest parts | Ordered numeric `.rrd` files; atomic UUIDv7 staging files and the video marker are excluded |
| Apache Arrow IPC | Bounded projection of admitted RRD data |

## Ownership

This library owns file formats and typed Rerun adapters. It has no Hub lifecycle,
producer authentication, catalog mutation, or MCP transport dependency. A caller must
supply an authorized input set before invoking a file operation.

`segment.rs` verifies one complete RRD file and its unique producer recording identity.
`ingest_parts.rs` discovers complete parts in sequence order and rejects unknown names
or non-file entries. The Hub owns publication and recovery of those files.

`video_clip.rs` combines the supplied segments as one logical recording and selects a
bounded encoded range with the preceding decoder-reentrant keyframe. It remuxes existing
H.264 access units into MP4. No image is rendered, and no video is decoded or encoded by
this operation. GPU decode and inference remain the consuming runtime's responsibility.

`recording_layer.rs` normalizes and verifies canonical durable Store IDs.
`properties_layer.rs` builds deterministic properties layers. `projection/` owns the
bounded Arrow projection implementation. `video.rs` owns encoded access-unit inspection.

## Verification

`cargo test -p veoveo-rrd --lib` exercises the file contracts. Hub integration tests cover
video selection across a producer restart and verify the resulting MP4 sample table.
The moved algorithms and file formats retain their existing behavior. Their canonical
Rust paths now start at `veoveo_rrd`; Hub no longer exports these shared operations.
