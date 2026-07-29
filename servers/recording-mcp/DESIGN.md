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
| [Fetch Standard](https://fetch.spec.whatwg.org/) credentials mode | Rerun's HTTP RRD receiver uses `omit`. The Console's internal adapter changes that mode to `same-origin` only for the exact canonical bounded-live route, allowing its HttpOnly Console session to authorize the stream without a bearer URL. This adapter is not a public recording protocol. |
| Veoveo recording ingest | Version `2026-07-24`; authenticated protobuf batches carry native Rerun messages from a producer-local forwarder through the gateway to Recording Hub. |
| Veoveo recording playback manifest | Version `veoveo.io/recording-playback/v2`; one stable Redap archive URI, one optional bounded live RRD source, a catalog revision, and recording-scoped access material. |
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
Dataset enumeration, writes, registration, tables, tasks, and maintenance are
denied. The BFF never stores playback session state and never proxies archive
bytes.

Live playback is a generated stream over the current writing shard. It emits
store information and static context, retains a bounded row-ID history window,
then follows newly durable data. Every outgoing message is rewritten to the
same dataset and segment identity used by Redap. The live URL is bound to one
writing shard identity and ends at rollover. Rerun 0.35 deliberately omits
credentials for HTTP RRD fetches. Console wraps the browser Fetch boundary while
the viewer is mounted and upgrades only this exact same-origin route to
`same-origin` credentials. The unchanged URL remains the receiver identity, the
HttpOnly Console session reaches the BFF policy boundary, and no access token
enters a URL or browser-readable cookie. Unrelated Fetch inputs pass through
without constructing replacement `Request` objects, preserving Redap's
one-use streaming bodies. While the recording remains live,
Console refreshes the manifest every five seconds. Rollover first refreshes the
archive catalog, then opens the successor live source before detaching the prior
receiver. The viewer instance and operator state remain intact until the
generic Redap fallback token rotates. Rotation creates a new viewer credential
context because Rerun's mutable credential API is specific to Rerun Cloud OAuth.

Governed query and analysis plans include complete acknowledged ingest parts
from the current writing shard. An analysis consumer captures one ordered
snapshot, copies its live parts into bounded task-local storage, and verifies
each copy against its captured byte length and SHA-256 identity. Frozen and
sealed sources remain zero-copy. Source provenance contains recording,
segment, and part identities without filesystem paths.

`contract.rs` owns playback manifest v2. `service.rs` resolves an authorized
playback plan from durable identities, while `service/read.rs` owns governed
analysis snapshots. `playback.rs` owns session authorization, stable identity,
derived catalogs, and the scoped Redap service. Its `redap` Cargo feature is
required by the server binary but excluded from library consumers, which keeps
recording analysis and smoke builds out of the DataFusion-backed server graph.
`live_playback.rs` owns the bounded follow projection. `bin/server.rs` owns HTTP
and gRPC-Web composition.
