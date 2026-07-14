# Veoveo Helm installation

This chart installs one autonomous enterprise Veoveo instance. Tenant ids are
internal isolation boundaries; the chart has no connection to a vendor control
plane. The platform store is exactly one SurrealDB 3.2.0 process backed by a
RocksDB PVC. Database HA is out of scope. Back up the SurrealDB and object-store
volumes according to the installation recovery objectives.

The recording workload is one pod with an internal-only Rerun ingest container
and a governed MCP container sharing `recording.persistence`. Raw ingest is
available only through the `recording-ingest` ClusterIP service. Gateway traffic
uses `recording-mcp`; no ingress route exposes the spooler or its files.

`duckdb-mcp` is intentionally one replica with a persistent `ReadWriteOnce`
workspace. It provides owner-scoped mutable analytical databases and arbitrary
sandboxed SQL, so it has a single-writer storage boundary. Its task, identity,
policy, and audit state still lives in SurrealDB; the PVC stores only the DuckDB
database files.

`map-mcp` is also intentionally one replica with a persistent `ReadWriteOnce`
volume. SurrealDB holds its canonical catalog, while the volume retains the
tenant-scoped DuckDB Spatial projection and activated Valhalla routing builds.
Release activation serializes projection changes within that process. Scaling
Map requires an explicit projection-distribution design and is not enabled by
raising the replica count.

The operator must create these resources before installation:

- `surrealdb.adminExistingSecret`: `username` and `password` for bootstrap only.
- `surrealdb.runtimeExistingSecret`: database-level `username` and `password`.
- `global.existingSecret`: gateway signing keys, internal JWKS, console session
  key, provider credentials, object-store credentials, and the gateway refresh
  delivery key under `refresh-delivery-key-b64`.
- `gateway.existingControlPlaneConfigMap`: the typed gateway JSON under
  `gateway.controlPlaneKey`.
- `telemetry.existingConfigMap`: the collector configuration under
  `telemetry.configKey`, including the enterprise SIEM/export destination.

Generate `refresh-delivery-key-b64` independently from all signing and session
keys with `openssl rand -base64 32`, then store that base64 text as the Secret
value. It must decode to exactly 32 bytes. The gateway uses it only to encrypt a
successor refresh token during the short duplicate-delivery window; plaintext
successors are never persisted.

`gateway.refreshDeliveryWindowSeconds` defaults to `5` and accepts `1` through
`30`. If two stateless console BFF requests concurrently present the same
refresh token, the winner rotates it and a request arriving inside this window
receives the identical successor recovered from the encrypted envelope. A later
use is a replay and revokes the token family. The delivery envelope is
authenticated against the authorization server, profile, OAuth client, family,
and generation; it is never copied to logs, audit payloads, outbox events, or
console snapshots. At the deadline it is
immediately ineligible for delivery. The gateway clears it atomically if the
successor is consumed, or physically removes the expired ciphertext on the next
one-minute delivery-envelope GC pass.

For an authenticated SIEM exporter, put exporter variables in a Kubernetes
Secret and set `telemetry.credentialExistingSecret`. The collector imports that
Secret through `envFrom`; credentials never enter Helm values or the
ConfigMap. `configs/otel-collector.siem.example.yaml` is a vendor-neutral
OTLP/HTTP example using `VEOVEO_SIEM_OTLP_ENDPOINT` and
`VEOVEO_SIEM_AUTHORIZATION`.

The `installation-bootstrap` Job authenticates at root scope, creates or rotates
the database-level runtime user, applies schema migrations, and publishes the
initial gateway control revision. Every long-running workload authenticates at
database scope with the runtime Secret. Rotating either Secret is owned by the
installation operator.

For an internal RustFS store, configure `objectStore.rustfs.publicEndpoint` and
`ingress.objectStoreHost` to the same HTTPS origin. Presigned artifact downloads
must be reachable by authorized clients. Set `objectStore.mode=externalS3` to use
an existing S3-compatible service instead.

Anyone-with-link artifact URLs contain a bearer secret under `/s/*`. The chart
renders that path as a dedicated Ingress and defaults
`ingress.publicShareAnnotations` to the ingress-nginx
`nginx.ingress.kubernetes.io/enable-access-log: "false"` policy. For any other
IngressClass, replace that annotation with the controller's path-level access-log
disable or redaction policy and verify the rendered controller configuration
before accepting traffic. Suppress the same path in APM, WAF, and tracing
pipelines. The normal Ingress does not own `/s`, and the Compose Caddy route
retains `log_skip`. Application audit records contain the artifact identity and
outcome, never the raw link token.

Connected installations should provide tightly scoped
`networkPolicy.externalEgressCidrs` for the external OIDC issuer and approved
provider APIs. Offline installations leave that list empty and point the
gateway control plane at an OIDC issuer reachable inside the air-gapped network.

When `global.serviceMesh.enabled=true`, the chart emits an Istio
`PeerAuthentication` policy in `STRICT` mode for all Veoveo workloads. The
installation must have Istio sidecar injection enabled for the namespace or via
`global.serviceMesh.podAnnotations`; enabling the value without an Istio control
plane is a configuration error, not a plaintext fallback.

Apply `deploy/offline/values.offline.yaml` after importing an offline bundle to
force `imagePullPolicy: Never`.
