# Recording ingest

Recording ingest is one authenticated data-plane contract across public, local-network,
and Kubernetes routes. A producer sends native Rerun traffic to a loopback forwarder.
The forwarder persists bounded batches locally, obtains an OAuth client-credentials token,
and uploads versioned protobuf envelopes to `/ingest/recordings/v1`.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| OAuth 2.0 client credentials and `private_key_jwt` | machine producer authentication at the installation gateway |
| Protocol Buffers | versioned Recording ingest envelopes with the repository-owned media type |
| Rerun RRD 0.35.0 | complete bounded recording and Blueprint store payloads |
| SHA-256 | immutable batch and Blueprint content identity |
| `veoveo.io/recording-ingest-diagnostics/v1` | aggregate authenticated-ingest process counters without tenant or stream identity |

The OAuth protected resource is installation-specific. A representative installation uses
`https://platform.example/ingest/recordings` and the `recording:ingest` scope. Public and
split-horizon local DNS select different network routes to the same gateway resource.
Network location never changes producer authority.

## Protocol

`platform/recordings/protocol` owns version `2026-08-06` of the wire schema and media
type. A batch declares its monotonic sequence, exact Rerun 0.35.0 RRD encoding release,
message count, payload bytes, and SHA-256 digest. Stream creation is idempotent under the
producer's `source_stream_id`.
Repeating an accepted sequence with the same digest succeeds without another append.
A different digest conflicts, and a gap returns the next expected sequence.

The public operations are discovery, open or resume, status, append, Blueprint
publication, and finish. They do not expose raw recording bytes or proxy Rerun read operations.
Each accepted batch is materialized as an immutable ordered part of the current
writing segment before its append response advances the materialized
checkpoint. Segment byte and age limits merge those complete parts into one
ordinary immutable RRD segment and open the next writing segment. Finishing
freezes the remaining parts and closes the ordered stream. Sealing and artifact
publication remain governed Recording MCP operations.

A producer registration separately authorizes Blueprint publication and sets
byte, message, and retained-revision limits. One publication contains exactly
one Rerun Blueprint store for the recording application, contains at least one
`SetStoreInfo`, and ends with one active/default activation command. The
forwarder associates it only when exactly one local recording for that
application is active. Hub stores the Blueprint under its distinct store
identity and the same tenant, recording, producer, Work Context, and invocation
authority. It never relabels presentation metadata as recording data.

Revisions start at one and advance without gaps. Repeating one revision with
the same store identity, digest, byte length, and message count is idempotent;
different content at that revision conflicts. The queue persists Blueprint
revisions beside recording batches and removes either only after its own
durable acknowledgment. Invalid or unauthorized presentation metadata is
rejected with a typed diagnostic while accepted recording batches continue.
Discovery publishes both Blueprint byte and message ceilings. The forwarder
applies those ceilings while collecting decoded messages, before a complete
RRD is retained or queued, and Hub applies the producer-specific limits again.

## Authentication and policy

Machine producers use OAuth `client_credentials` with `private_key_jwt`. The active
gateway control plane binds each producer to one tenant, dataset, application allowlist,
classification, labels, retention policy, and quota set. Payloads cannot select those
values. The gateway authenticates the external token, evaluates policy, records durable
audit evidence for stream lifecycle operations and every authentication or policy denial,
and issues a short-lived internal assertion addressed to Recording Hub. Successful batch
appends do not create general operator audit events. Their accepted-batch evidence is the
immutable ingest ledger entry containing producer, stream, sequence, digest, byte count,
message count, and timestamp.

Producer public keys belong in installation-controlled JWKS material. The producer keeps
its private key. An installation can therefore issue tokens and ingest on its local
network while disconnected from the Internet.

## Durability

The forwarder recovers and uploads one queued batch at a time, which keeps memory
bounded by the negotiated batch limit even when its disk queue is full. It
removes a local batch only after Recording Hub reports a durable checkpoint.
Incomplete Blueprint collection is bounded by the negotiated byte and message
limits. Both native receivers apply one aggregate byte ceiling and admit at
most 32 incomplete stores; the loopback Hub may lower that count through the
configured revision budget.
Persisted enqueue and upload cursors make queue work constant with respect to
backlog depth. Reconciliation inventories the directory once at startup, then
normal appends and acknowledgements update the cached byte count and the next
ordered path directly.

Recording Hub first validates the complete Rerun payload, then writes a
deterministic batch journal file through fsync and an atomic no-clobber link. A
SurrealDB transaction records the batch digest and advances the stream
checkpoint only after that file exists durably. Startup reconciliation
completes a transaction interrupted after publication. Concurrent publication
accepts identical bytes but cannot replace an existing sequence or Blueprint
revision with different bytes.

The serialized materializer retains the last authorized open-stream checkpoint
and active writing-segment identity in memory. Every request still carries
current Gateway authorization and is checked against the immutable producer and
ownership binding. The transaction compares revision and sequence before it
commits; a process restart or conflict rehydrates both checkpoints from
SurrealDB. Successful commits project the exact written fields locally and
avoid redundant database and catalog readbacks on the live path.

Every storage-backed Hub operation has an eight-second deadline. Expiration
returns the retryable `storage_unavailable` protocol error and drops the active
future, which releases the ordered materialization lock without acknowledging
the producer batch. A retry remains idempotent whether the interrupted database
transaction committed or rolled back. Kubernetes readiness and liveness probes
execute a bounded SurrealDB query through the Hub process. An open TCP listener
does not qualify an ingest replica as healthy.

Authenticated ingest exposes bounded aggregate diagnostics at the cluster-internal
`/internal/recording-ingest/v1/diagnostics` route. The same gateway assertion required by
the write routes protects this projection. It reports unique accepted batches, messages,
and bytes; duplicate append requests; batches and bytes waiting for materialization; and
the last completed append time. Every successful append emits the same fields as one
structured event. The projection contains no tenant, producer, stream, recording, token,
or filesystem identity. Direct loopback Rerun counters remain separate because they do
not prove authenticated forwarder acceptance.

Producer rate and byte quotas use fixed UTC minute and UTC day windows. One
deterministic counter record per active window is updated atomically with the
batch ledger and stream checkpoint. The first append in a window seeds that
counter from the indexed ledger, which preserves an in-progress window across
an upgrade or Hub restart. Later appends perform constant-size updates rather
than aggregating the producer's retained batch history. Expired counter records
are removed when the next window is created.

### Open quota-boundary defect

An acceptance run observed one append at an exact UTC minute transition fail with
`quota_checkpoint` not containing the batch acceptance time. The uploader retained the
durable batch and succeeded on its 250 ms retry, so the event did not lose recording
data or delay simulation. The request path currently chooses a quota checkpoint before
journal materialization, while the store validates that checkpoint against a separate
later clock read. Crossing the window boundary between those operations makes the
otherwise valid checkpoint stale.

The correction must derive the acceptance timestamp and quota checkpoint from one
transaction input, then use that timestamp for the ledger entry and both quota-window
checks. A deterministic test must hold an append across the minute and day boundaries,
prove one successful commit without transport retry, and retain same-window quota
atomicity. This defect remains open; a successful retry is durability evidence, not a
substitute for the boundary correction.

One ordered materializer converts a journal batch into an immutable sequence
part beneath one cataloged writing segment before the append completes. A batch
journal file is eligible for removal only after its sequence part exists
durably. Startup replays any durable journal entry left between those steps.
Segment rollover re-encodes the ordered parts into one complete RRD through
fsync and atomic rename before freezing the catalog row. This ordering provides
at-least-once transport with append-once stored batches; it does not claim
network-level exactly-once delivery.

The writing row is also the live playback identity. Recording MCP decodes new
parts in sequence. A late viewer's bounded bootstrap is first compacted with
Rerun 0.35's live-viewer profile, which removes repeated store metadata and
merges adjacent one-row chunks before they reach the browser. Newly durable
batches then remain complete RRD frames on the persistent, versioned stream so
their publication adds no interval timer. A rollover ends that response; Console then
opens the successor response on the same Rerun JavaScript channel. Batch
boundaries never appear as catalog segments or independent recordings.

The writing row also supports bounded live queries and analysis. Recording MCP
captures only complete materialized parts whose append response has advanced
the durable checkpoint. A Stream replay or Reason task copies that ordered source
snapshot before decode, so rollover may remove the Hub's parts directory
without changing the task input. The snapshot provenance records segment
identity, part sequence, byte length, and SHA-256 without exposing paths.

## Network routes

The external and local-network origin uses HTTPS. Public and split-horizon DNS
may route the same canonical hostname differently, but every route presents a
certificate for that hostname and preserves the public resource identity.
Kubernetes forwarders also use canonical HTTPS unless an internal gateway
service presents a certificate valid for the same origin.

The Helm workloads set `--gateway-transport-url` to the private gateway
ClusterIP route. This changes only the socket destination. Discovery results,
OAuth issuer, client-assertion audience, protected resource, streams endpoint,
and HTTP Host remain bound to the canonical public origin. NetworkPolicy
confines that internal HTTP hop; service-mesh mTLS protects it when the
installation enables the mesh.

Native Rerun gRPC is loopback-only at the forwarder. Recording Hub exposes an internal
HTTP service to the gateway and has no NodePort or public raw proxy. A firewall or
NetworkPolicy narrows reachability, but it never replaces OAuth authorization.

## Producer forwarder

`recording-forwarder` listens on `127.0.0.1:9876` by default. A Rerun SDK connects to
`rerun+http://127.0.0.1:9876/proxy`, while the forwarder discovers and uploads through
the canonical gateway origin. Its queue directory must be persistent. The process
applies disk backpressure once that queue reaches its configured byte limit.
Rerun gRPC transmits SDK chunks without a flush marker. The forwarder therefore
keeps their monotonic source-generation identity and closes a durable batch when
that source span reaches 750 milliseconds. Every H.264 access unit begins a new
batch, every IDR remains an exact GoP boundary, 4,096 messages bound one batch,
and the gateway's advertised byte limit splits any larger encoded result. No
wall-clock batch task exists. This keeps serial durable appends below the
producer rate without weakening ordered checkpoints.
Late native viewers receive the live tail before retained proxy history. They
can inspect earlier buffered data after current camera and telemetry messages
are flowing, without making the producer wait for catch-up playback.

Discovery, OAuth, and ingest requests share one bounded deadline. A timeout
defers the durable queue entry and releases the uploader for retry; shutdown
cancels an in-flight request before draining the remaining queue. No failed
network request can wedge the producer, its queue, or pod termination.

The stream-byte quota bounds one ingest generation rather than one logical
recording. When a generation reaches that limit, the forwarder closes it in
continuation mode and resumes the same application and recording keys with a
new source stream. Local batches retain their durable queue order while remote
sequences restart from one. Playback and governance continue to address one
recording across every generation.

A producer slot that intentionally owns one recording per application passes
`--finish-superseded-recordings`, and its gateway registration names that application in
`single_recording_application_ids`. The first `SetStoreInfo` for a new recording identity
durably orders older local queues ahead of the new stream. Their accepted batches and
Blueprint revisions drain before completion. When the new stream opens, Recording Hub
also freezes and finalizes any older live recording for that producer and application.
This authoritative transition repairs an abandoned stream even when its former local
queue no longer exists. Reopening the same recording identity is idempotent and does not
close it. Applications that support concurrent recordings remain outside the configured
set.

The producer registration supplies a JWKS public key. The matching private key stays on
the producer as a PEM file and is selected by key ID and algorithm. A producer
uses these canonical settings:

```sh
recording-forwarder \
  --gateway-url https://platform.example/ \
  --protected-resource https://platform.example/ingest/recordings \
  --client-id recording-producer \
  --key-id recording-producer-2026 \
  --private-key-pem-file /run/secrets/recording-producer.pem \
  --queue-dir /var/lib/veoveo-recording-forwarder
```

Kubernetes native sidecars use the same command with
`--gateway-transport-url http://mcp-gateway:8788/`. Each sidecar mounts only its
producer key and a dedicated persistent queue. Kubernetes starts the forwarder
before the producer container and terminates it after the producer exits.

The same command works on the local network when split-horizon DNS resolves
`platform.example` to the LAN ingress. The certificate and OAuth resource identity remain
unchanged.

## Acceptance

The Rust smoke harness starts an isolated SurrealDB with Recording Hub and the gateway.
The producer forwarder client executes the complete contract against those services. The
harness confirms discovery and private-key JWT token issuance. It retries a native RRD
batch, resumes at the durable checkpoint, finishes the stream, then inspects the merged
immutable segment digest.

```sh
cargo xtask smoke recording-ingest
```
