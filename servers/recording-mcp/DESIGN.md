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
| [Fetch Standard](https://fetch.spec.whatwg.org/) credentials mode | Console fetches the canonical live-frame route with `same-origin` credentials. Its internal adapter restores that mode only for Rerun's finite Blueprint request. This adapter is not a public recording protocol. |
| Veoveo recording ingest | Version `2026-08-01`; authenticated protobuf batches and distinct Blueprint publications carry native Rerun stores from a producer-local forwarder through the gateway to Recording Hub. |
| Veoveo recording playback manifest | Version `veoveo.io/recording-playback/v4`; one finite producer Blueprint, one stable Redap archive URI, one optional `rerun_js_channel_rrd_frames` source, a catalog revision, and recording-scoped access material. |
| Veoveo Rerun live frames | Internal adapter protocol `VVRL0001`; a stream preface followed by big-endian `u32` lengths and complete RRD payloads capped at 16 MiB. It is not a public Rerun wire protocol. |
| [JSON Web Token](https://www.rfc-editor.org/rfc/rfc7519) | Rerun-compatible HS256 read tokens carry the standard Redap audience and an exact installation hostname. A server-side session binds each token subject to one recording and one authorized Veoveo actor. |
| H.264 Annex B in Rerun `VideoStream` | The governed video profile stores decoder-reentrant access units, keyframe markers, and original timeline indices inside RRD. |
| SHA-256 | Frozen shard and artifact manifests bind immutable bytes to a digest. |

The MCP surface owns recording discovery, bounded queries, subscriptions, and
artifact publication. The authenticated manifest and bounded live RRD routes
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

Live playback is a generated stream over the current writing shard. It emits
store information and static context, retains a bounded row-ID history window,
then follows newly durable data. The bounded bootstrap is compacted once with
Rerun's `live` optimization profile before delivery. This preserves its rows
and H.264 groups of pictures while preventing the browser from indexing the
producer's many one-row SDK chunks during its first interactive frames. Every
outgoing message is rewritten to the
same dataset and segment identity used by Redap. The live URL is bound to one
writing shard identity and ends at rollover. The response begins with the
`VVRL0001` preface and frames each complete RRD payload with a big-endian `u32`
length. Console validates the bounded framing and sends each payload through
one persistent Rerun `LogChannel`. Rerun classifies that `JsChannel` as live and
keeps its native Following state; Console issues no playback or cursor commands.
The exact same-origin Fetch carries the HttpOnly Console session, while no
access token enters a URL or browser-readable cookie. Natural completion of the
response triggers manifest refresh. Its successor feeds the existing channel,
preserving the WebViewer, producer layout, and operator state. History remains
the lazy archive dataset and is the only mode that renews a Redap credential.
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

`contract.rs` owns playback manifest v4. `service.rs` resolves an authorized
playback plan from durable identities, while `service/read.rs` owns governed
analysis snapshots. `playback.rs` owns session authorization, stable identity,
derived catalogs, and the scoped Redap service. Its `redap` Cargo feature is
required by the server binary but excluded from library consumers, which keeps
recording analysis and smoke builds out of the DataFusion-backed server graph.
`live_playback.rs` owns the bounded follow projection. `bin/server.rs` owns HTTP
and gRPC-Web composition.
