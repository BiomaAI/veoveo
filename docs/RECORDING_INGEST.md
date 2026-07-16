# Recording ingest

Recording ingest is one authenticated data-plane contract across public, local-network,
and Kubernetes routes. A producer sends native Rerun traffic to a loopback forwarder.
The forwarder persists bounded batches locally, obtains an OAuth client-credentials token,
and uploads versioned protobuf envelopes to `/ingest/recordings/v1`.

The OAuth protected resource is installation-specific. Bioma uses
`https://veoveo.bioma.ai/ingest/recordings` and the `recording:ingest` scope. Public and
split-horizon local DNS select different network routes to the same gateway resource.
Network location never changes producer authority.

## Protocol

`platform/recordings/protocol` owns the wire schema and media type. A batch declares its
monotonic sequence, exact Rerun RRD encoding release, message count, payload bytes, and
SHA-256 digest. Stream creation is idempotent under the producer's `source_stream_id`.
Repeating an accepted sequence with the same digest succeeds without another append.
A different digest conflicts, and a gap returns the next expected sequence.

The public operations are discovery, open or resume, status, append, and finish. They do
not expose raw recording bytes or proxy Rerun read operations. Finishing drains every
durable batch into immutable RRD segments. Sealing and artifact publication remain
governed Recording MCP operations.

## Authentication and policy

Machine producers use OAuth `client_credentials` with `private_key_jwt`. The active
gateway control plane binds each producer to one tenant, dataset, application allowlist,
classification, labels, retention policy, and quota set. Payloads cannot select those
values. The gateway authenticates the external token, evaluates policy, records audit
evidence, and issues a short-lived internal assertion addressed to Recording Hub.

Producer public keys belong in installation-controlled JWKS material. The producer keeps
its private key. An installation can therefore issue tokens and ingest on its local
network while disconnected from the Internet.

## Durability

The forwarder removes a local batch only after Record Hub reports a durable checkpoint.
Record Hub first validates the complete Rerun payload, then writes a deterministic batch
journal file through fsync and atomic rename. A SurrealDB transaction records the batch
digest and advances the stream checkpoint only after that file exists durably. Startup
reconciliation completes a transaction interrupted after rename.

One ordered materializer converts journal batches into immutable RRD segments. Open
queries include the unmaterialized journal tail. A batch journal file is eligible for
removal only after a cataloged segment covers its sequence. This ordering provides
at-least-once transport with append-once stored batches; it does not claim network-level
exactly-once delivery.

## Network routes

The external and local-network origin uses HTTPS. Bioma's public DNS reaches Cloudflare
and its local DNS resolves the same `veoveo.bioma.ai` name to the installation's LAN
Traefik address. Traefik presents a certificate for that canonical name in both cases.
Kubernetes forwarders can use the internal gateway service when the installation's
network policy or service mesh protects that hop.

Native Rerun gRPC is loopback-only at the forwarder. Recording Hub exposes an internal
HTTP service to the gateway and has no NodePort or public raw proxy. A firewall or
NetworkPolicy narrows reachability, but it never replaces OAuth authorization.

