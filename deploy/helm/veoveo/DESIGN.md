# Veoveo Installation Chart

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Helm v2 chart format and JSON Schema draft-07 | Closed installation values, rendered Kubernetes resources and immutable image references |
| Kubernetes apps/v1 and core/v1 | Deployments, Services, ConfigMaps and references to installation-owned Secrets |
| Kubernetes admissionregistration.k8s.io/v1 and CEL | Fail-closed managed kernel and controller resource validation; requires Kubernetes 1.30 or newer |
| Kubernetes networking.k8s.io/v1 | Namespace-isolated managed ingress/egress and fixed destination admission |
| Rerun Data Protocol `rerun.cloud.v1alpha1` | Read-only Redap route on a separate Ingress with native HTTP/2 gRPC to the recording service; browser gRPC-Web uses the same path |
| OCI image digests | Veoveo image ownership and production digest enforcement through the shared chart helpers |
| Amazon S3 API | Private Artifact object storage through RustFS 1.0.0 or an installation-owned compatible store; multipart write, object metadata and ranged reads are exercised by the Artifact client |
| OTLP gRPC and HTTP; OpenTelemetry Collector 0.161.0 | Optional telemetry receiver with installation-owned pipeline configuration; the checked-in receiver, batch processor and debug exporter profile is qualified with an OTLP/HTTP log |
| `veoveo.io/computers-service/v3` | Private Computers JSON configuration; the typed service validates the selected capacity and trust before store mutation |
| `veoveo.io/computer-host/v1` | Private compute-container configuration; dedicated daemon, provider and retained ext4 storage |
| RFC 9562 UUIDv8 | Deterministic identity for explicitly unconfigured Computers; configured provider identity remains an installation input |

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
`veoveo.io/computers-unconfigured/v1/{installationId}/{namespace}/{releaseName}`,
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
`http://computers-mcp:8804/computers/healthz` through its ordinary catalog.
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
references. The manager cannot replace its own privileged Deployment. Kernel service
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
