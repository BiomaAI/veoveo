# Governed Recorded Video

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Recording resources | Canonical UUIDv7 recording URI and typed reader authority |
| Rerun RRD | Existing repository 0.38.1 VideoStream profile |
| H.264 and MP4 | Bounded Annex B access-unit selection and remux without re-encoding |
| Source identity | Checked UUIDv7 Recording, dataset and layer IDs; ordered camelCase Serde JSON, JSON Schema 2020-12, positive byte counts and bare lowercase SHA-256 fields |

The video materializer depends on the shared Recording reader and RRD libraries.
It validates selectors and limits, obtains the governed snapshot, extracts a bounded
range with decoder-reentrant preroll, and returns the original source identity beside
its encoded clip and MP4 bytes. It owns no Recording Hub or MCP service implementation.

Consumers own hardware decoding and inference. Materialization requires an explicit
Artifact caller or task-read capability, verified by the shared reader. The source
byte bound applies before copying live parts; committed layers use the bounded
Artifact cache. The library never authorizes replay from an old spool path.
Its Rerun query uses the catalog's dataset and Recording IDs for both committed
segments and copied live parts. Source snapshots identify the original inputs before
the shared reader normalizes its live copies.

## Selector Admission

`RecordingVideoSelectionBuilder` and `IndexRangeBuilder` construct immutable
`Checked` products. JSON decoding applies the same range, absolute entity-path
and timeline checks before a selector reaches materialization. Range indices stay
signed; an ordered negative range is valid. Source snapshots serialize `recordingId`, `datasetId`, `capturedAt` and ordered
`sources`; each source uses `layerId`, `layerName`, optional `layerOrdinal`, `kind`,
optional `partSequence`, `byteLen` and bare lowercase `sha256`. Optional absent
fields are omitted. `digest_sha256` hashes the declared JSON member order and
source order. The unversioned snapshot has one admitted spelling; old and mixed
keys are refused. Native reader snapshots and SQL column names keep their
storage profile. The explicit adapter constructs the current public owner type.

The contract control constructs checked source identities, asserts an independent
ordered preimage and its digest, and captures `testdata/source-snapshot.json` through
`UPDATE_RECORDED_VIDEO_FIXTURES`. The native reader adapter compares its actual
conversion with that same captured public value. Stream and Reason import the
shared contract and check source Recording identity before replay or grounding.

This coordinated admission cut rejects selectors that previously required an
explicit validation call to detect malformed fields. Producers and readers must
upgrade together; persisted malformed selectors require correction before replay.
No reader repairs a selector or attributes an old snapshot to another recording.

## Library Features

`contract` exposes `contract::RecordingVideoSelection`, `IndexRange`,
`VideoTimelineKind` and the captured source identities. Selections use `RecordingUri`
from the [Recording domain contract](../contract/DESIGN.md). Its decoder
requires the canonical plural route and RFC UUIDv7 before materialization. It includes selector validation
and the snapshot digest. Select `default-features = false, features = ["contract"]`
for public consumers. Chrono supplies date/time values without its clock feature.
The contract has no MCP, Store, reader, Rerun, async or GPU dependency.

`runtime` is the default and includes `contract`. The `runtime` module owns
`VideoSourceLimits`, `MaterializedVideo`, source authorization,
timeline mapping and remux. Its private snapshot adapter converts the reader's typed
identities fallibly to the public snapshot and excludes local paths. Materialization
admits that snapshot and its selected Recording before extracting encoded video.
The public snapshot keeps
source order, optional field omission and JSON bytes in its SHA-256 calculation,
which returns `Sha256Digest`.

`RecordingSourceIdentityBuilder` checks layer text, positive bytes, nonnegative
Store-compatible ordinals and part-sequence presence for the source kind.
`RecordingSourceSnapshotBuilder` requires at least one source, consistent facts for
each layer, one layer ID per name, unique layer/part pairs and a byte sum that fits
`u64`. Several live parts may share one layer. The checked models expose immutable
fields and apply the same builders during JSON decoding. Their layer names are
captured catalog labels; this snapshot does not carry a layer-kind vocabulary.
The shared bare-hex adapter keeps digest fields aligned with their declared schema.

Reason and Stream consume the same contract feature; their runtime features enable
materialization. The library defines neither server's resource vocabulary. Selection
validation checks ranges, entity paths and timeline names. The runtime converts the
admitted Recording ID to a Store ID and checks current access when resolving the source.
Successful contract decoding establishes address validity, not source authorization.

## Value Admission

Recording source identities and ordered snapshots store owner-checked builder facts through `Checked`. Admission verifies layer identity, source distinctness and byte accumulation without reordering the digest inputs.
