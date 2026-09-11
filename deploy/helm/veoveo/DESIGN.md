# Veoveo Installation Chart

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Helm v2 chart format and JSON Schema draft-07 | Closed installation values, rendered Kubernetes resources and immutable image references |
| Kubernetes apps/v1 and core/v1 | Deployments, Services, ConfigMaps and references to installation-owned Secrets |
| OCI image digests | Veoveo image ownership and production digest enforcement through the shared chart helpers |
| `veoveo.io/computers-service/v3` | Private Computers JSON configuration; the typed service validates the selected capacity and trust before store mutation |
| `veoveo.io/computer-host/v1` | Private compute-container configuration; dedicated daemon, provider and retained ext4 storage |
| RFC 9562 UUIDv8 | Deterministic identity for explicitly unconfigured Computers; configured provider identity remains an installation input |

## Computers Control

The `full` and `extension-foundation` presets include `computers`. A custom partial
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
quotas. The storage helper preallocates each admitted home and enforces its configured
free-space reserve. Storage/node placement belongs to the installation. The runtime
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
