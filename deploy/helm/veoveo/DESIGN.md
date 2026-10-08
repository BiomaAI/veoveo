# Veoveo Installation Chart

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.ai/module-selection/v1` and `veoveo.ai/module-plan/v1` | Checked camelCase JSON; optional selection, decimal-string generation, nonsecret credential revision and composition-generated lane/runtime bindings |
| Helm v2 chart format and JSON Schema draft-07 | Closed installation values, rendered Kubernetes resources and immutable image references |
| Kubernetes apps/v1 and core/v1 | Deployments, Services, ConfigMaps and references to installation-owned Secrets |
| Kubernetes admissionregistration.k8s.io/v1 and CEL | Fail-closed managed kernel and controller resource validation; requires Kubernetes 1.30 or newer |
| Kubernetes networking.k8s.io/v1 | Namespace-isolated managed ingress/egress and fixed destination admission |
| Rerun Data Protocol `rerun.cloud.v1alpha1` | Read-only Redap route on a separate Ingress with native HTTP/2 gRPC to the recording service; browser gRPC-Web uses the same path |
| SurrealDB 3.3.0 | One RocksDB node, digest-pinned image, `/ready` for traffic admission and `/health` for process liveness |
| OCI image digests | Veoveo image ownership and production digest enforcement through the shared chart helpers |
| vLLM 0.31.0 pooling and OpenAI Embeddings API subset | Private GPU embedding runtime with an installation API key, local Qwen3 checkpoint and priority scheduling |
| Amazon S3 API | Private Artifact object storage through RustFS 1.0.0 or an installation-owned compatible store; multipart write, object metadata and ranged reads are exercised by the Artifact client |
| OTLP gRPC and HTTP; OpenTelemetry Collector 0.161.0 | Optional telemetry receiver with installation-owned pipeline configuration; the checked-in receiver, batch processor and debug exporter profile is qualified with an OTLP/HTTP log |
| `veoveo.ai/computers-service/v3` | Private Computers JSON configuration; the typed service validates the selected capacity and trust before store mutation |
| `veoveo.ai/computer-host/v1` | Private compute-container configuration; dedicated daemon, provider and retained ext4 storage |
| RFC 9562 UUIDv8 | Deterministic identity for explicitly unconfigured Computers; configured provider identity remains an installation input |

## Module Preparation And Publication

`moduleInstallation.planJson` contains the complete gateway-generated module-plan v1
JSON document. The chart preserves its bytes in an immutable ConfigMap and consumes
its lane commands and runtime bindings. The composition binary owns module names and
dependencies. The chart checks enabled hosts against those bindings and never turns on
a workload because its schema prerequisite was selected.

Artifact Service receives the same plan path, composition, installation generation,
credential revision and runtime username as other database-authenticated runtimes. Its
plan ConfigMap mounts read-only, and the Deployment template changes when the plan
generation or bytes change. Startup admission checks these values before Artifact
object-store effects, repository and audit startup, upload recovery, and HTTP binding.

Frames receives that installation plan through the same read-only ConfigMap and
runtime identity inputs. Its startup checks the authenticated Store account and
prepared compiled lanes before Task recovery or HTTP binding; the chart does not run
schema preparation from the server.

Ordinary preparation, per-lane migration and control-plane publication Jobs may start
concurrently. Gateway commands wait for database readiness and the matching completed
preparation proof; publication also waits for selected lanes. The existing
`installation-bootstrap` Job runs `installation-prepare` to provision the database
and generation-fenced runtime credentials. Per-owner Jobs install the selected
current schemas; a separate runtime-authenticated Job publishes the control plane.
Runtime credentials serve
publication and gateway traffic. Root migration credentials enter only preparation
and lane Jobs, apart from the database's own root provisioning input.

The selection file owns installation generation and `credentialRevision`. Generation
is independent of Helm release counters. Changed preparation inputs require a higher
generation; the same generation with a different identity fails. Job names hash their
complete immutable specs. Repeated unchanged renders preserve object identities.
Credential revision updates each controlled database client pod template, including
the agent manager's desired managed-workload configuration. Managed-agent replacement
uses the manager's lease and pod drain checks; static YAML alone cannot establish that
all running agents accepted a rotated account.

## Hosted Server Probes

Artifact, DuckDB, Frames, Map, Media and Timeseries use their mounted `/readyz`
for Kubernetes readiness and gateway health checks. Their `/healthz` routes serve
liveness independently of Store availability. Each Kubernetes HTTP probe sends a
Host value admitted by the server. The owning server defines its dependency checks.

## Embedding Runtime

The `embedding-runtime` component renders the `embedding` Deployment and Service,
its model-cache PVC, a checkpoint ConfigMap and a NetworkPolicy. The `full` preset
includes it. Custom selections may select it independently of MCP servers. The
deployment contract requires an `embedding` GPU placement when that component is
selected; the chart supports the same NVIDIA device-plugin and DRA allocations as
the other GPU workloads.

The official vLLM image uses the same digest as Reason's base image. Its startup check
refuses an unavailable CUDA device. The pooling runner serves the pinned Qwen3
Embedding 0.6B checkpoint with priority scheduling. Installations set the fraction of
GPU memory under `embedding.engine.gpuMemoryUtilization`; the reference chart uses
0.45 as a candidate budget for its 24 GiB device. That value does not qualify the
full-context cache or capacity with the installation's co-resident GPU workloads.
Startup profiling and the selected production workload must establish both.

`embedding.engine.precision` declares `bfloat16` or `float16` and defaults to
`bfloat16`. The Deployment passes that value explicitly as vLLM's `--dtype`.
Installations must match this effective engine precision to the selected qualified
embedding runtime bundle. Each precision requires its own hardware comparison,
ranking, scheduling, capacity and production Knowledge workload qualification.

The Deployment starts without profiler instrumentation. The
[verification guide](../../../platform/runtimes/embedding/verification/README.md)
describes an isolated local diagnostic process for vLLM 0.31's maintained Proton
graph-attribution profiler. Its `/start_profile` and `/stop_profile` routes bypass
API-key middleware, so the diagnostic process uses a no-network container and
loopback-only requests. The production chart does not enable profiling. The v0.31
image pin awaits GPU qualification.

An init container verifies every entry in `embedding-checkpoint` before starting the
server. The packaged manifest is copied from the runtime's `checkpoint.sha256`; the
native Helm check requires byte equality. Checkpoints are installation-staged under
`/models/qwen3-embedding-0.6b-97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3`.
The model mount is read-only, and `HF_HUB_OFFLINE=1` prevents runtime acquisition.
The 4 GiB PVC request describes model storage, without reserving host free disk space.
Recreate updates drain the previous instance before its replacement starts.

`embedding.apiKeySecret` selects an installation-owned Secret and key. The chart
passes that key to vLLM without putting it in values, ConfigMaps or command arguments.
Only platform consumers of the embedding client receive the same Secret reference.
The dedicated ingress policy permits port 8000 from pods in the same namespace,
excluding `computer-host`. It renders even when general network policies are disabled.
General internal, DNS and external-egress policies exclude Embedding to preserve that
isolation. The service has no external route and needs no outbound connection.

`cargo test -p veoveo-deployment-smoke --test embedding_helm` checks rendered objects,
pins, source-manifest agreement, both GPU allocation modes and rejected CPU or unpinned
configuration. Installed network denial and GPU measurements are separate acceptance.

## Knowledge Service

The `knowledge` server renders one CPU Deployment and a Service on port 8800.
The full preset selects it; custom selection requires Gateway, Store and the embedding
runtime. Recreate updates drain the indexing worker before replacement. The service
stores its catalog and index in Store and requests no persistent or ephemeral storage.

`knowledge.existingConfigMap` supplies one JSON `IndexingConfig` per tenant. The
`indexingConfigKeys` list names 1–128 distinct JSON filenames under the read-only
configuration mount. The installation hashes the public ConfigMap's `data` object as
sorted compact JSON with its trailing newline; that SHA-256 becomes
`knowledge.configurationRevision` and enters the Pod template. The installation owns
validation of the file contents against the [service configuration](../../../servers/knowledge-mcp/DESIGN.md#indexing-configuration-and-lifecycle).

`knowledge.existingSigningSecret` contains the private signing keys and optional CA
files referenced by those configurations. It mounts only in Knowledge, read-only with
mode 0440; the pod's fsGroup permits UID 10001 to read it. The chart renders no Secret
contents. The embedding API key references the shared runtime's installation Secret.
The generated embedding-space document binds its model identity to the qualified
checkpoint and vLLM image digest. A changed public configuration or embedding space
changes the Pod template. Key rotation is read on each new machine connection.

Startup and liveness use `/knowledge/healthz`; readiness uses `/knowledge/readyz`.
An initial index build or reconnection can remove readiness while HTTP liveness stays
healthy. Termination allows 45 seconds for the service's worker and HTTP drain.
The [native chart suite](../../../testing/deployment-smoke/tests/knowledge_helm.rs)
checks rendered objects and refused configurations. Image publication, machine-client
provisioning and installed acceptance remain in the [consolidated plan](../../../docs/CONTRACT_CONSISTENCY_PLAN.md).

## Audit Service

Gateway rendering requires an explicit `gateway.auditRetentionDays`. The gateway also
loads the `seed-b64` entry from `gateway.auditSigningSecret`; both replicas use this
installation-owned Ed25519 seed. The chart stores no private key in values or ConfigMaps.
Gateway readiness includes the sealer's active or standby lease state.
`gateway.auditExport.destinations` renders a public ConfigMap and a Pod checksum.
S3 credentials reference the selected Secret's `object-store-access-key` and
`object-store-secret-key` entries. OTLP bearer authentication optionally references
`token` in `otlpBearerSecret`. The exporter requires receipts from both destinations
when both are selected. The Bioma reference selects the bundled bucket with prefix
`audit` and Object Lock disabled. Destination changes require a coordinated gateway
drain and restart; an existing receipt applies only to its original configuration.

Termination allows 120 seconds for the gateway and 90 seconds for Artifact and Speech.
Each host bounds HTTP draining at 30 seconds and drains its audit writer after producers
stop. Gateway then drains committed records through its sealer and releases the lease.

## Object Store Runtime

The bundled RustFS process uses four Tokio workers through
`objectStore.rustfs.runtimeWorkerThreads`. The setting accepts integers of at least
two. Its worker count is independent of the container's CPU quota: the one-CPU
default must still schedule S3 requests and health probes while storage maintenance
runs. Installations can raise the worker count and CPU allocation together for their
workload.

The readiness probe calls `/health/ready`, which checks storage and IAM readiness
before Kubernetes admits traffic. Liveness calls `/health` to detect an unresponsive
process. These endpoints follow the pinned RustFS 1.0.0
[health implementation](https://github.com/rustfs/rustfs/blob/1.0.0/rustfs/src/server/health.rs).
`deployment-smoke helm-config` checks the rendered worker setting, schema rejection
and separate probe paths. The installed Artifact multipart harness qualifies storage
operations independently of probe responses.

## Object Store Version Transition

The chart owns the RustFS image and the single-replica StatefulSet. It pins the
stable 1.0.0 OCI index digest. The installation owns its PVC, credentials and
stored objects. The single replica stops before its replacement starts; there is
no mixed-version serving window or second writer on the ReadWriteOnce volume.

An installation moving from 1.0.0-rc.3 must qualify a same-volume sequence with
its pinned source and target images before updating the image: write a multipart
object on the source, restart on the target, and check its metadata, full body and
ranged bytes. A reverse restart on the source must check an object written by the
target before the source image can be treated as a rollback. The installation
keeps the existing PVC and exact old image digest until the target has passed
read and write checks against the installed store. If reverse compatibility is
absent, recovery uses a restored volume or a corrected target image; changing
the tag alone is not a rollback of a migrated format. The RC image leaves support
after each installation has passed this transition and retained-data checks.

## Redap Ingress

The chart places `/rerun.cloud.v1alpha1.RerunCloudService` on a dedicated Ingress.
Traefik uses the `recording-mcp` Service's `serversscheme: h2c` annotation to speak
HTTP/2 to the Rust gRPC listener. Other ingress controllers must configure their
gRPC upstream protocol through `ingress.redapAnnotations` and qualify native gRPC
and browser gRPC-Web independently. A Cloudflare Tunnel public-hostname route does
not carry native gRPC; Bioma's direct k3d ingress or service route is the native
client qualification path. The [Rerun client guide](../../../docs/RERUN_RECORDINGS.md)
describes grants and token renewal.

## Workspace Models

Chat models execute in the gateway. `gateway.agents.models` supplies approved model
connections through `VEOVEO_AGENT_MODELS`. Connections bind provider destinations,
registered secret references, tenant/context access, required scopes and execution
ceilings. The gateway validates them against its activated catalog before serving.
Definitions and immutable executable revisions live in the platform store.

`gateway.agents.modelSecrets` maps names beginning `VEOVEO_AGENT_MODEL_` to exact keys
in installation-owned Kubernetes Secrets. Only the gateway receives those keys.
Routine definition edits and publication change no Pod template. Installation model
changes still require a gateway rollout and explicit revision adoption when executable
connection settings change. `consoleBff.workspace` retains the separate browser edge's
OAuth client, resource and requested scopes.

The model configuration is qualified by `cargo test -p veoveo-deployment-smoke
--test workspace_helm`, which requires Helm and GNU timeout. It checks the real
rendered environments and rejects inline keys, reserved variables and out-of-bounds
model ceilings. Provider execution and public browser acceptance remain separate checks.

## Computers Control

The `full` and `foundation` presets include `computers`. A custom partial
installation selects it through `mcpServers` and must also select `gateway` and
`platform-store`. The deployment contract expands the same selections and includes
the `computers-mcp` image. Compute-host images belong to the selected capacity
topology; unconfigured control does not require a privileged compute host.

The chart runs two unprivileged Computers replicas by default. It mounts no host
socket or retained home. The service reads database-scoped store credentials and
internal assertion trust through the existing Secret references. Its database health
probe controls traffic admission. Provider availability remains a domain state,
which keeps the collection and unrelated services usable during a capacity outage.
The probe does not restart a worker when the store is unavailable.

`computerCapacity: unconfigured` renders the complete public configuration in
`computers-configuration`, including the canonical public origin and
`computers.access` limits. It reports Setup Required. The unconfigured provider UUID
is the first 128 bits of SHA-256 over
`veoveo.ai/computers-unconfigured/v1/{installationId}/{namespace}/{releaseName}`,
with its version nibble set to 8 and variant nibble set to `a`. It identifies only
this unconfigured control installation. Repeated renders preserve the exact
configuration and Pod template. Configured capacity supplies its own stable identity.

`computerCapacity: openshell-docker` requires `computers.existingConfigMap`,
`computers.configurationRevision` and `computers.existingTrustSecret`. The ConfigMap contains the closed
`computers.json` service document. Its SHA-256 is the configuration revision that
enters the Pod template. The installation owner verifies those bytes and publishes
the public ConfigMap through its own reconciliation path. Dedicated worker
certificates and keys live in the existing Secret, mounted read-only under
`/etc/veoveo/computers/trust`. Configured JSON is authoritative for the access policy;
`computers.access` only supplies the generated unconfigured document. The worker Secret also contains
the command encryption keys referenced by `execution.keys`; its mode 0440 permits
worker fsGroup reads. Configured capacity requires the Artifact service and its
storage dependencies for command output publication. Unconfigured control needs no
command key. See the service design for the coordinated v2 migration and key overlap.

The document listens on `0.0.0.0:8804`, admits Host `computers-mcp:8804`, and uses
the installation's exact public origin. The gateway registers
`http://computers-mcp:8804/computers/mcp` and health URL
`http://computers-mcp:8804/computers/readyz` through its ordinary catalog.
The chart supplies no provider administrator credentials or guest trust to Console.

External configuration fields in unconfigured mode are rejected. Missing configured
references fail rendering; malformed files, templates and certificate material fail
service startup before writes. The former unreleased `computers.capacityMode` field
is replaced by `computerCapacity`, shared with the typed deployment selection.

## Configured Compute Host

Configured capacity also deploys one [private compute host](../../../platform/computers/host/DESIGN.md).
`computers.host.existingConfigMap` supplies `host.json`, and its exact SHA-256 enters
`computers.host.configurationRevision`. `computers.host.existingTrustSecret` provides
only the host-side fixed trust files. Worker private keys remain in the control
service's separate Secret. Host trust is projected read-only with mode 0400.

The host uses its own network, PID and mount namespaces, root privileges and a
private Docker socket inside the container. It receives no installation host socket,
hostPath volume, shared service-account token or external mount propagation. Its
Service exposes only provider/storage mTLS ports 8805 and 8806 inside the cluster.
The deployment uses one replica and Recreate, with 60 seconds for ordered shutdown.
An image/configuration update is a retained maintenance boundary. The control service
and Console retain their independent rollout behavior.

The default PVC is `computer-host-data`, with ReadWriteOnce access and
`helm.sh/resource-policy: keep`. An installation may supply
`computers.host.persistence.existingClaim`. The backing filesystem must be persistent
ext4; a PVC capacity field alone does not prove that filesystem or enforce allocation
quotas. The storage helper creates sparse homes with fixed maximums and checks its
free-space floor before new allocations. That floor does not reserve blocks or
constrain existing writers. Both Computers services default to explicit zero CPU/RAM
requests with finite limits. The private host's cgroup namespace includes nested
Computers under its aggregate ceiling. Storage/node placement belongs to the installation. The runtime
directory uses a bounded memory-backed volume and retains no key material after Pod
replacement. Routine chart removal cannot purge retained Computer data.

When NetworkPolicy is enabled, the compute host is excluded from general installation
traffic and external-egress rules. Its ingress admits only Computers workers on the
two mTLS ports. `computers.host.registryEgress` supplies exact CIDR/port entries for
the declared installation registry; DNS uses the existing namespace-bound policy.
Configured capacity requires those registry rules when policy is enabled. The guest's
own sandbox egress policy remains an independent enforcement boundary.

The typed deployment selection uses the same `computerCapacity` values. Configured
capacity requires Computers control and contributes `computer-host` and
`computer-template` to the exact/offline image closure. Unconfigured core control
requires neither image. Image digests in the supplied JSON catalogs must match the
installation's qualified image records. Kubernetes topology, retained maintenance,
gateway registration and public/offline acceptance remain separate release gates.

## Configuration Qualification

`cargo test -p veoveo-deployment-smoke --test computers_helm` renders both core
presets twice, checks stable Pod/configuration identities and verifies the privileged
boundary. It also renders configured capacity and rejects missing trust/configuration
and missing store dependencies. These are real Helm configuration checks. They do
not establish provider execution or installed acceptance.
The public ingress sends exact `/_ws_tunnel` to the Console BFF for the stock CLI
adapter. The Computer-prefixed pairing and tunnel routes use the existing `/console`
prefix. Both paths share the application's narrow grant enforcement; neither exposes
the private provider listener. This change requires coordinated BFF, gateway and
Computers application images and the additive CLI-ledger migration.

Configured Computers also requires the `computers` Artifact audience. Helm validates
that the caller's internally signed service assertion can reach capability issuance;
the Artifact service continues to evaluate caller and Work Context authority. Both
the chart defaults and Bioma values declare this audience.

Configured service v3 capacity includes an explicit file-qualified default template.
The Computers worker and its JSON ConfigMap must be upgraded together. Existing retained
Computer IDs and homes are preserved; their template transitions are installation-owned
and require the ordinary explicit environment-update operation.

## Managed Kernels

`gateway.agents.templates` is the shared approved template catalog. A nonempty
catalog requires `agent-runtime-support`. The chart starts one lifecycle manager in
`agentManager.namespace`, which must differ from the main installation namespace.
The gateway receives template configuration and no Kubernetes write credential.

The installation owner supplies the manager namespace with an identical public
gateway ConfigMap, database-scoped credentials, approved model credentials, immutable
template ConfigMaps and any private-registry pull credentials. Values hold references.
The manager's public configuration includes the same model and template objects sent
to the gateway. Templates refer to digest-pinned kernel images. Their data digest is
checked against the immutable ConfigMap before creating a workload.

A dedicated Role permits Deployment and owned credential reconciliation, PVC creation
and native Pod watching. It grants no PVC deletion, Pod creation, exec, Secret listing,
RBAC mutation or writes in the main namespace. Admission policies also constrain the
controller's resource names and immutable credentials, and enforce the actual Pod
image, entrypoint, service account, security context, resources, storage and Secret
references. The chart consumes the public model and template DTOs with their camelCase
field names. Each admitted model branch binds its configured URL and model ID to the
same template credential reference, Secret name and key. Unselected models and
foreign credential references produce no branch; a template with no eligible binding
produces a false predicate. Empty credential bindings fail values-schema admission.
The Pod policy requires every environment name to occur once before matching
credential destinations. The environment length cannot exceed the installed list
of literal, field and Secret variable names, which bounds the pairwise comparison.
Its CEL `all`, `filter` and `size` guard covers ordinary
variables and model credentials together; duplicate names cannot supply one value
for admission and another for the kernel.
The same workload DTO supplies resource quantities, configuration and database
references, storage capacity and installation-defined parameter names.

The manager cannot replace its own privileged Deployment. Kernel service
accounts have no API token or RoleBinding. The namespace enforces Restricted Pod
Security at the qualified Kubernetes v1.36 profile.

Deleting managed Deployments permit finalizer updates when their complete specification,
labels, annotations and owner references remain unchanged. Kubernetes must finish
foreground garbage collection even after the installation retires an image. Changing
the executable specification or ownership still requires ordinary admission. The native
manager admission test completes foreground deletion under a policy that rejects all
executable images, and rejects specification and ownership edits during that deletion.

NetworkPolicy always isolates the managed namespace, independently of the main
chart's optional network policy switch. Kernels may reach the gateway, store, DNS
and explicit `agentManager.modelEgress` destinations. Only the manager receives
`kubernetesApiEgress`. These destination entries use installation-approved CIDRs and
ports, including the actual API endpoint after service translation where required by
the CNI. Standard NetworkPolicy has no DNS-name matching. Operators must admit the
model service's exact destination ranges; broad private-network entries would weaken
this boundary. Private MCP workers receive no direct managed-kernel ingress rule.

Namespace retention protects archived memory during chart removal. PVCs have no
Deployment owner reference and the manager never deletes them. Cross-namespace
adoption of an existing PVC requires a storage-owner transfer of the same PV; an
ordinary instance retry cannot replace missing retained memory.

Managed namespace network policies apply even when the main installation disables its
network policies. Additional gateway and store ingress rules render only when the main
namespace uses isolation. Otherwise, an ingress-only rule would unexpectedly isolate
existing services and reject their ordinary callers.
