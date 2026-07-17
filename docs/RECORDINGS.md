# Governed recordings

The Recording Hub is the installation's append-only Rerun ingest plane. Its
gRPC proxy is private to the installation network. Producers write Rerun log
messages to the Kubernetes service `recording-hub:9876`; that endpoint is
not routed through public ingress.

The spooler partitions files as `{dataset}/{day}/{recording}.rrd`, fsyncs live
data, rolls segments before the artifact-plane upload ceiling, and never
truncates an existing file. A restart creates an `.rN` sibling. On startup it
decodes and hashes every segment, reconciles crash-safe footer-less files, and
fails closed on corruption or a mismatch with the SurrealDB catalog. A native
publisher becomes `ready` after the configured idle grace closes its final
segment. The authenticated batch protocol marks the recording ready when its
finish request has drained. A recovered row without either completion boundary
is `interrupted`; new data resumes it as `live`.

`recording-mcp` is the governed control plane. It exposes catalog, recording,
and segment resources; prompt and completion support; resource subscriptions;
bounded temporal queries; and synchronous idempotent sealing. Sealing requires
`admin:manage`, validates each frozen segment again, creates immutable governed
artifact occurrences for every segment and a JSON manifest, stages those
occurrence identities, then changes the recording and its segments to `sealed`
while publishing the durable outbox event in the same SurrealDB transaction.
`started_at` is the first cataloged producer message, `ended_at` is the capture
boundary, and `sealed_at` records later publication. These timestamps are not
interchangeable.

The recording server also owns authenticated HTTP playback routes beside its MCP
surface. The gateway applies the same recording resource policy and audit path,
then issues a short-lived internal assertion. The Console BFF retains the user
session and streams the authorized segment bytes with byte-range support. The
browser receives ordered same-origin URLs, never filesystem paths, gateway
bearers, or object-store credentials. The Console loads the matching Rerun
0.34.1 WASM viewer only after a recording is selected.

Recording UUIDv7 values and artifact UUIDv7 values are occurrence identities.
Filesystem paths are always tenant-internal implementation details and are not
returned by MCP. Classification is descriptive. Non-empty labels enforce
clearance; an `unclassified` recording with no labels is visible within its
tenant. Public or authorized artifact sharing is handled only through
`artifact-mcp` after sealing.

Runtime services authenticate to SurrealDB with database-scoped credentials.
Only the installation bootstrap migrates schema with root credentials. The
recording workload is intentionally one persistent spooler replica; SurrealDB
HA and a distributed recording filesystem are outside the current contract.

Encoded camera streams use the canonical H.264 `VideoStream` profile documented in
[`servers/perception-mcp/DESIGN.md`](../servers/perception-mcp/DESIGN.md). The proxy can
provide a bounded recent replay, while frozen/sealed RRD segments remain the durable and
governed source. Video readers merge every authorized segment of the logical recording
before seeking, because decoder state and a requested sample may be in different
physical files.
