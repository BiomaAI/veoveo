# Governed Recorded Video

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Recording resources | Canonical UUIDv7 recording URI and typed reader authority |
| Rerun RRD | Existing repository 0.38.1 VideoStream profile |
| H.264 and MP4 | Bounded Annex B access-unit selection and remux without re-encoding |
| Source identity | Ordered captured layer identities, Serde JSON serialization, JSON Schema 2020-12 and SHA-256 |

The video materializer depends on the shared Recording reader and RRD libraries.
It validates selectors and limits, obtains the governed snapshot, extracts a bounded
range with decoder-reentrant preroll, and returns the original source identity beside
its encoded clip and MP4 bytes. It owns no Recording Hub or MCP service implementation.

Consumers own hardware decoding and inference. Materialization requires an explicit
Artifact caller or task-read capability, verified by the shared reader. The source
byte bound applies before copying live parts; committed layers use the bounded
Artifact cache. The library never authorizes replay from an old spool path.

## Library Features

`contract` exposes `contract::RecordingVideoSelection`, `IndexRange`,
`VideoTimelineKind` and the captured source identities. Selections use `RecordingUri`
from the Recording server library with only its `contract` feature enabled. Its decoder
requires the canonical plural route and RFC UUIDv7 before materialization. It includes selector validation
and the snapshot digest. Select `default-features = false, features = ["contract"]`
for public consumers. Chrono supplies date/time values without its clock feature.
The contract has no MCP, Store, reader, Rerun, async or GPU dependency.

`runtime` is the default and includes `contract`. The `runtime` module owns
`VideoSourceLimits`, `MaterializedVideo`, source authorization,
timeline mapping and remux. Its private snapshot adapter converts the reader's typed
identities to the public snapshot and excludes local paths. The public snapshot keeps
source order, optional field omission and JSON bytes in its SHA-256 calculation.

Reason and Stream consume the same contract feature; their runtime features enable
materialization. The library defines neither server's resource vocabulary. Selection
validation checks ranges, entity paths and timeline names. The runtime converts the
admitted Recording ID to a Store ID and checks current access when resolving the source.
Successful contract decoding establishes address validity, not source authorization.
