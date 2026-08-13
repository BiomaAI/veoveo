# Recording MCP design

`recording-mcp` is the governed catalog and read boundary for Recording Hub
data. The repository-wide ingest, storage, and playback contract is normative
in [`docs/RECORDINGS.md`](../../docs/RECORDINGS.md).

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP for discovery, bounded queries, resources, templates, subscriptions, notifications, and artifact publication. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Recording query, manifest, subscription, and structured-result contracts. |
| [Rerun 0.35.0](https://rerun.io/docs/) RRD and Rerun Data Protocol | Immutable frozen and sealed shards are layers of one recording-scoped dataset segment. The public service implements the viewer's read subset of the `rerun.cloud.v1alpha1.RerunCloudService` protocol over HTTP/2 or gRPC-Web. It does not claim the catalog, mutation, table, task, or maintenance profiles. |
| Rerun 0.35.0 WebViewer `LogChannel` | Live playback uses the public `WebViewer.open_channel` and `LogChannel.send_rrd` API. Each send contains one complete independently decodable RRD byte array, as required by the pinned JavaScript SDK. |
| [Fetch Standard](https://fetch.spec.whatwg.org/) and Veoveo framed RRD stream v2 | Console performs one authenticated same-origin GET. The response media type is `application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=2`; each frame is an unsigned four-byte big-endian length followed by one complete RRD. The required `x-veoveo-rerun-live-start` header selects `bootstrap` for an empty Rerun channel or `resume-head` for a channel that already holds the bounded bootstrap. This is an internal browser adapter, not a public recording protocol. |
| Veoveo recording ingest | Version `2026-08-06`; authenticated protobuf batches and distinct Blueprint publications carry native Rerun stores from a producer-local forwarder through the gateway to Recording Hub, with policy-scoped single-recording replacement. |
| Veoveo recording playback manifest | Version `veoveo.io/recording-playback/v8`; one finite producer Blueprint, one stable Redap archive URI, one optional recording-scoped `rerun_rrd_channel_v2` source, a catalog revision, and recording-scoped access material. |
| [JSON Web Token](https://www.rfc-editor.org/rfc/rfc7519) | Rerun-compatible HS256 read tokens carry the standard Redap audience and an exact installation hostname. A server-side session binds each token subject to one recording and one authorized Veoveo actor. |
| H.264 Annex B in Rerun `VideoStream` | Producers store decoder-reentrant access units and original timeline indices. Archive materialization derives canonical sparse keyframe markers from the encoded bytes. The internal live-view adapter omits keyframe columns because the pinned viewer derives sync samples from H.264 and requires dense sample chunks. |
| SHA-256 | Frozen shard and artifact manifests bind immutable bytes to a digest. |

The MCP surface owns recording discovery, bounded queries, subscriptions, and
artifact publication. The authenticated manifest and bounded live RRD stream routes
sit beside the MCP endpoint because neither is an MCP content block. Gateway
policy and audit target `recording://recordings/{id}` before either route
reaches this server.

Archive playback uses Rerun's native lazy data path. `recording-mcp` derives one
in-memory Redap catalog for an authorized recording, assigns its catalog UUIDv7
as the exact Rerun dataset id, and registers every immutable shard as a named
layer of the producer's recording segment. Registration verifies that Rerun
reports the expected segment id for every layer. The durable Veoveo catalog and
RRD shards remain authoritative; the derived Redap catalog is bounded cache
state and is rebuilt from them after eviction or restart.

The manifest returns one stable canonical `rerun://` HTTPS dataset-segment URI rather than a
URL per shard. Its revision is a deterministic digest over ordered layer names,
content digests, and lengths. New frozen shards append layers to the existing
dataset. Replacement, removal, or identity drift rebuilds the derived catalog
and changes the revision. Rerun reads footer manifests and fetches only the
chunks required by the active view; no browser path opens every shard or
constructs a whole-recording RRD.

The public Redap path is recording-scoped, read-only, and intentionally smaller
than a general Rerun catalog. A five-minute server session binds the token
subject to the authorized actor and recording. The standard Rerun read token is
host-limited and expires after 30 minutes, while active Console renewal keeps
the server session alive and rotates the token before its renewal window.
`FindEntries` enumerates only the one dataset in that session's isolated
derived catalog, which lets native Rerun source navigation resolve without
exposing another recording. Writes, registration, tables, tasks, and
maintenance are denied. The BFF never stores playback session state and never
proxies archive bytes.

Live playback is a Rerun-native message projection over the recording's current
writing shard. It emits store information and static context, retains a bounded
row-ID history window, then follows newly durable data. The bounded bootstrap
is compacted once with Rerun's `live` optimization profile before delivery.
Static context belongs to the governed recording. An authenticated ingest
connection may roll over, reopen, or cross a date partition without losing
codec declarations, camera calibration, or other static Rerun state.
This preserves its rows and H.264 groups of pictures while preventing the
browser from indexing the producer's many one-row SDK chunks during its first
interactive frames. Every outgoing message is rewritten to the same dataset
and segment identity used by Redap, then encoded with Rerun's native LZ4
protobuf transport. A typed catalog subscription advances the same response to
the next writing shard after rollover. It never replays a shard already sent to
that receiver.

The live projection removes every `VideoStream:is_keyframe` column after
compaction. Rerun 0.35 discovers H.264 sync samples from the access-unit bytes,
while its viewer cache indexes `VideoStream:sample` as a dense physical column.
Omitting the sparse marker prevents messages from separate live batches from
being compacted into a chunk whose sample component has fewer values than rows.
This adapter does not alter the durable ingest parts. Archive materialization
derives and validates its canonical keyframe markers from the same encoded
bytes.

WebViewer opens one `LogChannel` for the selected live recording. Console fetches
the recording-scoped stream through the HttpOnly session and parses only enough
bytes to recover each complete framed RRD. It passes that array directly to
`LogChannel.send_rrd`. The channel remains open when the HTTP transport
reconnects, which preserves the producer Blueprint and viewer state. A reconnect
starts at the current durable head and never replays the bootstrap into the same
channel. Recording history retains data committed during the transport gap. The BFF
streams the response without buffering. No access token enters a URL or a
browser-readable cookie. Catalog SSE events refresh recording metadata, while
the recording-scoped stream follows segment rollover within its existing
response. History remains the lazy archive dataset and is the only mode that
renews a Redap credential.

## Validation

The focused headed-browser gates retain the authoritative simulator and require
a hardware-backed WebGL or WebGPU adapter. Live acceptance follows the active
recording for two minutes and proves zero source lag plus changing H.264 camera
content. Archive acceptance requires the exclusive recording-scoped Redap read
subset, the producer Blueprint, and a nonblank archived camera frame:

```sh
cargo xtask smoke uav-recording-browser-verify \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222

cargo xtask smoke uav-recording-archive-browser-verify \
  --recording-id <ready-recording-uuidv7> \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222
```

Rerun 0.35 persists standalone-viewer state unconditionally. Before starting an
embedded viewer, Console clears the pinned Rerun state keys. A previously opened
Redap server therefore cannot restore catalog queries or watch traffic into Live
mode. The governed producer Blueprint is opened again as the presentation
authority.
There is no manifest status polling. Filesystem events wake the projection when
the active file or acknowledged part directory changes; idle playback does not
scan on an interval.

Governed query and analysis plans include complete acknowledged ingest parts
from the current writing shard. An analysis consumer captures one ordered
snapshot, copies its live parts into bounded task-local storage, and verifies
each copy against its captured byte length and SHA-256 identity. Frozen and
sealed sources remain zero-copy. The writer publishes each part through an
atomic UUIDv7 staging file; readers exclude that exact staging identity until
its rename commits the canonical sequence-named part. Other unexpected entries
still invalidate the snapshot. Source provenance contains recording,
segment, and part identities without filesystem paths. Hub may replace the
parts directory with its frozen shard during capture. A missing part or an
uncovered writing segment restarts the complete authorized read-plan capture;
one snapshot never combines paths from two catalog views.

`contract.rs` owns playback manifest v8. `service.rs` resolves an authorized
playback plan from durable identities, while `service/read.rs` owns governed
analysis snapshots. `playback.rs` owns session authorization, stable identity,
derived catalogs, and the scoped Redap service. Its `redap` Cargo feature is
required by the server binary but excluded from library consumers, which keeps
recording analysis and smoke builds out of the DataFusion-backed server graph.
`live_playback.rs` owns the bounded follow projection. `live_stream.rs` owns the
authorized framed RRD transport and advances one WebViewer channel from segment
to segment through the typed store change stream.
`bin/server.rs` owns HTTP and gRPC-Web composition.
