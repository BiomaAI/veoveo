# Veoveo Helm installation

This chart installs one autonomous enterprise Veoveo instance. Tenant ids are
internal isolation boundaries; the chart has no connection to a vendor control
plane. The platform store is exactly one SurrealDB 3.2.1 process backed by a
RocksDB PVC. Database HA is out of scope. Back up the SurrealDB and object-store
volumes according to the installation recovery objectives.

`global.installationId` is the stable cross-chart identity for the installation.
Separately installed extension releases use the same
`veoveo.ai/installation` label while retaining their own
`app.kubernetes.io/instance`. NetworkPolicy never requires an extension release to
impersonate this chart's Helm release. `global.production=true` requires an immutable
digest for every rendered Veoveo-owned image.

`installationPreset` owns the first-party deployment graph. `full` selects the
supported complete surface, `extension-foundation` selects the platform foundation
with Artifact MCP, Frames MCP, and Recording MCP, and `custom` consumes the typed
`components` and `mcpServers` arrays. The chart owns the concrete image, port, probe,
argument, storage, and GPU definitions for every first-party server under
`definitions/domain-services.yaml`; an installation selects server identities instead
of reproducing internal workload records.

The typed components distinguish `recording-data-plane`, `gpu-renderer`, and
`simulation-runtime-support` from hosted MCP servers. Selecting `simulation-view`
requires Frames MCP and both simulation components. The Rust profile resolver also
accounts for independently owned GPU workloads and rejects two exclusive one-GPU
requests on one ordinary GPU.

The Rust deployment resolver applies the same dependency graph before rendering. A
selected hosted server requires the gateway. Artifact-backed servers require the
platform store, object store, and artifact service. Stream can run admitted live
graphs without Recording; Stream replay and Reason use Recording. Gateway composition
requirements for Artifact, Frames, Map, Media,
Recording, and RRD fail when their corresponding runtime is absent.

The recording workload is one pod with Recording Hub and the governed MCP
server sharing `recording.persistence`. The `recording-hub` ClusterIP carries
only the authenticated gateway API on port 9878. Hub's native Rerun receiver
binds to container loopback and has no Service, NodePort, or Ingress.

Application charts use `recording-forwarder` sidecars. SUMO and UAV Simulation
send native Rerun traffic to their pod's loopback receiver. Each forwarder keeps
a persistent bounded queue, authenticates with `private_key_jwt`, and sends the
versioned protobuf protocol to the gateway. The producer chart's
`recordingForwarder.gatewayTransportUrl` selects the internal gateway route
without changing the public OAuth issuer, protected resource, token audience,
or Host identity.

Stream has no recording route by default. Setting
`stream.recordingOutput.enabled=true` adds that standard forwarder as a native
sidecar, admits its loopback output in the private Stream catalog, and requires
the `recording-data-plane` component. Live graph execution never waits for this
route. Its session resource reports forwarding, draining, and failure.

`recording.idleTimeoutSeconds` closes a loopback-native Hub capture after its
final message; the default is 15 seconds. The gateway authorizes the playback
manifest and the Console BFF authenticates bounded live delivery.
`recording-mcp` reads the shared PVC without exposing it, derives one
recording-scoped Redap dataset, and serves its read-only gRPC-Web path at the
installation's existing public origin. The chart routes only
`/rerun.cloud.v1alpha1.RerunCloudService` directly to that server. A
host-limited token and server-side recording session protect the route; the
general Rerun catalog and mutation methods are unavailable.

Simulation View uses a separate provider-neutral MCP pod and one hardware-GPU pod. The
GPU pod contains the Isaac/RTX renderer and the mTLS pose-ingress sidecar. Those
containers exchange complete latest-pose snapshots through a memory-backed volume;
the renderer never gains network egress. A read-only governed-artifact volume supplies
content-addressed USD, USDZ, GLB, glTF, and texture inputs. The renderer requests
exactly one `nvidia.com/gpu` and starts only through `simulationView.runtimeClassName`.
The NVIDIA device plugin owns device visibility for every GPU container. The chart
does not set `NVIDIA_VISIBLE_DEVICES`; the plugin supplies the UUID selected by each
container's `nvidia.com/gpu` allocation. Workloads retain their required
`NVIDIA_DRIVER_CAPABILITIES`.

An exclusive multi-GPU node can prove that boundary before profile acceptance. Supply
one digest-pinned CUDA-capable image already admitted by the installation:

```sh
cargo xtask smoke gpu-allocation-verify \
  --context <kube-context> \
  --node <two-gpu-node> \
  --image <registry/image@sha256:digest>
```

The Rust smoke harness schedules two simultaneous one-GPU pods on that node. Each pod
must see one allocated UUID, and the UUIDs must differ. Time-sliced allocation is not
accepted as isolation evidence.

`simulationView.signaling`, `simulationView.media`, and
`simulationView.poseIngress` select the installation-owned exposure. Signaling accepts
Ingress, ClusterIP, NodePort, or LoadBalancer. Media is a bounded UDP port collection,
while pose ingress is one TLS 1.3 port authenticated by client certificates. The
installation supplies the exact public signaling URL, media host and IP, pose PKI
Secret, port allocation, and any admitted producer CIDRs. In-cluster producers carry
the `veoveo.ai/simulation-view-pose-producer: "true"` pod label. Readiness requires the
named RTX render product, NVENC, and a visible non-stale hardware frame.

Every MCP workload has one active pod and uses `Recreate`. This includes the
gateway MCP endpoint, domain servers, GPU servers, and the stdio bridge that
owns its child process. The chart does not expose replica or rollout controls
for those workloads. Sessions, subscriptions, notifications, and task links
remain attached to one process. Artifact byte delivery and the Console BFF are
outside the MCP boundary and keep independent replica settings.

`duckdb-mcp` has a persistent `ReadWriteOnce` workspace. It provides
owner-scoped mutable analytical databases and arbitrary sandboxed SQL, so it
also has a single-writer storage boundary. Its task, identity, policy, and
audit state still lives in SurrealDB; the PVC stores only the DuckDB database
files.

`map-mcp` has a persistent `ReadWriteOnce` volume. SurrealDB holds its
canonical catalog, while the volume retains the
tenant-scoped DuckDB Spatial projection and activated Valhalla routing builds.
Release activation serializes projection changes within that process.

`optimization-mcp` runs as a Rust control container beside the pinned NVIDIA
cuOpt 26.06 executor. The executor alone requests one `nvidia.com/gpu`; both
containers share a bounded Unix-socket volume, and the control container retains
prepared governed problems on its `ReadWriteOnce` workspace. Startup, readiness,
and liveness require the exact executor protocol, a CUDA 13.2-capable driver,
and one visible hardware GPU. The pod uses the `nvidia` RuntimeClass and has no
CPU solver or GPU-optional deployment mode.

`serverBootstrap` delivers installation-time domain configuration to any MCP
server component, keyed by domain-service name. Each entry renders a
`{name}-bootstrap` ConfigMap mounted at the canonical
`/etc/veoveo/bootstrap/catalog.json` and passed via `--bootstrap-catalog`.
The document is a generic envelope (`server`, `tenantKey`, `payload`); the
payload schema is owned by the server crate, rejects unknown fields, and can
be checked before install with the server binary's `bootstrap-validate` verb.
Application is idempotent at startup (Map applies create-only: existing
sources and mobility-profile versions are skipped). Bootstrap never performs
governed operations such as downloading, validating, or activating releases.

`clusterInspection.enabled` gives the console BFF a namespaced read-only Role
for Kubernetes inventory. The Role lists workloads, pods, services, ingress,
persistent volume claims, network policies, disruption budgets, and ConfigMaps.
It grants no access to Secrets and no mutation verbs. The console BFF requires a
successful gateway `AdminRead` authorization for each inventory request. When
NetworkPolicy is enabled, put the Kubernetes API endpoint ranges in
`clusterInspection.kubernetesApiCidrs` to permit HTTPS from the console BFF.

### Console BFF outbound routing and trust

`consoleBff.oauthResource` is the public OAuth protected-resource identity. It remains
the `resource` used during authorization and token operations, including audience and
scope validation. `consoleBff.mcpTransportUrl` is the network endpoint used by the
Console Apps MCP client. An in-cluster deployment normally selects
`http://mcp-gateway:8788/mcp/<profile>` while keeping the public origin in
`oauthResource`. The BFF sends the public deployment authority as the gateway `Host`
header. Both URLs must select the same exact profile. A blank `mcpTransportUrl`
preserves the previous behavior by using `oauthResource` for transport.

An installation may add public CA roots to every Console BFF outbound HTTPS client:

```yaml
consoleBff:
  oauthResource: https://veoveo.example/mcp/operator
  mcpTransportUrl: http://mcp-gateway:8788/mcp/operator
  outboundCa:
    existingConfigMap: corporate-ca
    key: ca.pem
```

The ConfigMap owns a PEM bundle and must exist before the Deployment starts. Kubernetes
fails the mount when the ConfigMap or key is absent. The BFF fails startup when the
mounted file is unreadable, empty, or invalid. These roots augment the standard trust
store and the projected Kubernetes API root; certificate verification remains enabled.
Changing the ConfigMap contents requires a Console BFF rollout because clients load the
bundle at startup. A deployment/v3 installation places these values in a file selected
through the platform release's `installationValues` array.

`time-mcp` runs as one temporal authority process with a persistent
`ReadWriteOnce` volume for staged and active TZDB and leap-second products.
SurrealDB retains the release catalog, active authority pointers, calendars,
mission epochs, clock policy, events, and durable Task API state. Authority
activation remains serialized within the process.

`view-mcp` runs as one stateful offscreen renderer and requests one
`nvidia.com/gpu`. Install NVIDIA GPU Operator or NVIDIA Container Toolkit,
provide an `nvidia` RuntimeClass, and put `google-maps-api-key` in the
installation secret. Readiness fails unless Bevy selects an NVIDIA Vulkan
hardware adapter; the image does not install a Mesa Vulkan software ICD. Its
non-overlapping replacement also preserves the exclusive GPU allocation.

The operator must create these resources before installation:

- `surrealdb.adminExistingSecret`: `username` and `password` for bootstrap only.
- `surrealdb.runtimeExistingSecret`: database-level `username` and `password`.
- `global.existingSecret`: gateway signing keys, internal JWKS, console session
  key, provider credentials, object-store credentials, the gateway refresh
  delivery key under `refresh-delivery-key-b64`, and a distinct 32-byte
  base64 playback key under `recording-playback-token-key`. Simulation View
  also reads `simulation-view-renderer-control-token` and
  `simulation-view-pose-control-token`.
- `simulationView.poseIngress.existingTlsSecret`: DER server certificate under
  `certificateKey`, PKCS#8 DER private key under `privateKeyKey`, and DER producer
  trust anchor under `clientCaKey`.
- `gateway.existingControlPlaneConfigMap`: the typed gateway JSON under
  `gateway.controlPlaneKey`, plus any file-backed JWKS or CA documents referenced
  by that JSON.
- `telemetry.existingConfigMap`: the collector configuration under
  `telemetry.configKey`, including the enterprise SIEM/export destination.

The chart mounts the complete gateway ConfigMap at `/etc/veoveo/gateway` in both
the bootstrap Job and the running gateway. File references in the control plane
must resolve beneath that directory. This keeps revision validation and runtime
authentication on the same immutable input set.

The control plane must define a Work Context for every tenant in active use.
Each OAuth client selects a default context and a direct, delegated, or automated
invocation mode. The gateway resolves context membership from configured
principal, group, role, and OAuth-client selectors, then signs the authority used
by tasks, recordings, agents, and artifact outputs. The neutral enterprise model
and identity-provider mapping guidance are in
[`../../../docs/WORK_CONTEXT_GOVERNANCE.md`](../../../docs/WORK_CONTEXT_GOVERNANCE.md).

Each Helm revision runs installation bootstrap against the mounted control
plane. Bootstrap validates the seed and publishes a new immutable database
revision when its hash differs from the active revision. This is also the
gateway schema upgrade path: an older active payload does not need to satisfy
the new schema before the current seed replaces it. A matching hash still
requires the stored active revision to pass full typed validation.

Generate `refresh-delivery-key-b64` independently from all signing and session
keys with `openssl rand -base64 32`, then store that base64 text as the Secret
value. It must decode to exactly 32 bytes. The gateway uses it only to encrypt a
successor refresh token during the short duplicate-delivery window; plaintext
successors are never persisted.

Generate `recording-playback-token-key` separately with
`openssl rand -base64 32`. It must decode to exactly 32 bytes and signs only
recording-scoped Redap read tokens. Do not reuse any other installation key.

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

The Work Context governance schema uses a coordinated hard-cut rollout. Stop
producers, preserve any externally required evidence, then clear SurrealDB,
recording data, artifact objects, and durable forwarder queues together before
installing the release. Bootstrap creates the canonical schema and materializes
the configured contexts. Browser sessions and service tokens are reissued after
the identity-provider role mapping is active.

RustFS and external S3-compatible stores are private infrastructure. Configure only
the endpoint reachable by Artifact service. Clients never address object storage;
authorized, ranged, and shared downloads stream through the installation origin from
`global.publicBaseUrl`. Set `objectStore.mode=externalS3` to use an existing private
S3-compatible service.

Anyone-with-link artifact URLs contain a bearer secret under `/s/*`. The chart
renders that path as a dedicated Ingress and defaults
`ingress.publicShareAnnotations` to the ingress-nginx
`nginx.ingress.kubernetes.io/enable-access-log: "false"` policy. For any other
IngressClass, replace that annotation with the controller's path-level access-log
disable or redaction policy and verify the rendered controller configuration
before accepting traffic. Suppress the same path in APM, WAF, and tracing
pipelines. The normal Ingress does not own `/s` and does not receive
public-share traffic. Application audit records contain the artifact identity
and outcome, never the raw link token.

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
