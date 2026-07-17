# Governed recordings

Recording ingest begins at a producer-local forwarder. Native Rerun gRPC stays
on `127.0.0.1:9876`; the forwarder journals bounded batches and sends the
authenticated protobuf protocol through the gateway. Recording Hub receives
only gateway-issued internal assertions on its ClusterIP API at port 9878.
Neither Kubernetes Services nor public ingress expose its loopback Rerun
receiver.

Recording Hub fsyncs each validated batch journal before advancing its
SurrealDB checkpoint, then materializes ordered RRD segments under
`{dataset}/{day}/{recording}.rrd`. It rolls segments before the artifact-plane
upload ceiling and never truncates an existing file. A restart creates an
`.rN` sibling. Startup reconciles journal checkpoints, decodes and hashes every
segment, repairs crash-safe footer-less files, and fails closed on corruption
or a catalog mismatch.

`recording-mcp` is the governed control plane. It exposes catalog, recording,
and segment resources; prompt and completion support; resource subscriptions;
bounded temporal queries; and synchronous idempotent sealing. Sealing requires
`admin:manage`, validates each frozen segment again, creates immutable governed
artifact occurrences for every segment and a JSON manifest, stages those
occurrence identities, then changes the recording and its segments to `sealed`
while publishing the durable outbox event in the same SurrealDB transaction.

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

Encoded camera streams use the canonical H.264 `VideoStream` profile documented
in [`servers/perception-mcp/DESIGN.md`](../servers/perception-mcp/DESIGN.md).
Frozen or sealed RRD segments are the only Perception source. Video readers
merge every authorized segment of the logical recording before seeking,
because decoder state and a requested sample may be in different physical
files.
