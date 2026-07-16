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
`examples/bioma/lan-values.yaml` enables this route while preserving the public resource
identity. Kubernetes forwarders also use canonical HTTPS unless an internal gateway
service presents a certificate valid for that same origin.

Native Rerun gRPC is loopback-only at the forwarder. Recording Hub exposes an internal
HTTP service to the gateway and has no NodePort or public raw proxy. A firewall or
NetworkPolicy narrows reachability, but it never replaces OAuth authorization.

## Producer forwarder

`recording-forwarder` listens on `127.0.0.1:9876` by default. A Rerun SDK connects to
`rerun+http://127.0.0.1:9876/proxy`, while the forwarder discovers and uploads through
the canonical gateway origin. Its queue directory must be persistent. The process
applies disk backpressure once that queue reaches its configured byte limit.

The producer registration supplies a JWKS public key. The matching private key stays on
the producer as a PEM file and is selected by key ID and algorithm. A Bioma
producer uses these canonical settings:

```sh
recording-forwarder \
  --gateway-url https://veoveo.bioma.ai/ \
  --protected-resource https://veoveo.bioma.ai/ingest/recordings \
  --client-id recording-producer \
  --key-id recording-producer-2026 \
  --private-key-pem-file /run/secrets/recording-producer.pem \
  --queue-dir /var/lib/veoveo-recording-forwarder
```

The same command works on the local network when split-horizon DNS resolves
`veoveo.bioma.ai` to the LAN ingress. The certificate and OAuth resource identity remain
unchanged.

## Acceptance

The Rust smoke harness starts an isolated SurrealDB with Recording Hub and the gateway.
The producer forwarder client executes the complete contract against those services. The
harness confirms discovery and private-key JWT token issuance. It retries a native RRD
batch, resumes at the durable checkpoint, finishes the stream, then inspects the immutable
segment digest.

```sh
cargo build -p veoveo-mcp-conformance --bin conformance \
  -p veoveo-mcp-gateway --bin gateway \
  -p veoveo-recording-hub --bin spooler
cargo run -p veoveo-smoke --bin smoke -- recording-ingest
```
