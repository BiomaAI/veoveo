# Veoveo Installation Chart

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Helm v2 chart format and JSON Schema draft-07 | Closed installation values, rendered Kubernetes resources and immutable image references |
| Kubernetes apps/v1 and core/v1 | Deployments, Services, ConfigMaps and references to installation-owned Secrets |
| OCI image digests | Veoveo image ownership and production digest enforcement through the shared chart helpers |
| `veoveo.io/computers-service/v1` | Private Computers JSON configuration; the typed service validates the selected capacity and trust before store mutation |
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

`computers.capacityMode: unconfigured` renders the complete public configuration in
`computers-configuration`, including the canonical public origin and
`computers.access` limits. It reports Setup Required. The unconfigured provider UUID
is the first 128 bits of SHA-256 over
`veoveo.io/computers-unconfigured/v1/{installationId}/{namespace}/{releaseName}`,
with its version nibble set to 8 and variant nibble set to `a`. It identifies only
this unconfigured control installation. Repeated renders preserve the exact
configuration and Pod template. Configured capacity supplies its own stable identity.

`computers.capacityMode: openshell-docker` requires `existingConfigMap`,
`configurationRevision` and `existingTrustSecret`. The ConfigMap contains the closed
`computers.json` service document. Its SHA-256 is the configuration revision that
enters the Pod template. The installation owner verifies those bytes and publishes
the public ConfigMap through its own reconciliation path. Dedicated worker
certificates and keys live in the existing Secret, mounted read-only under
`/etc/veoveo/computers/trust`. Configured JSON is authoritative for the access policy;
`computers.access` only supplies the generated unconfigured document.

The document listens on `0.0.0.0:8804`, admits Host `computers-mcp:8804`, and uses
the installation's exact public origin. The gateway registers
`http://computers-mcp:8804/computers/mcp` and health URL
`http://computers-mcp:8804/computers/healthz` through its ordinary catalog.
The chart supplies no provider administrator credentials or guest trust to Console.

External configuration fields in unconfigured mode are rejected. Missing configured
references fail rendering; malformed files, templates and certificate material fail
service startup before writes. The private compute host, retained maintenance,
installation-owned gateway registration and full installed/offline qualification
remain separate delivery gates.

## Configuration Qualification

`cargo test -p veoveo-deployment-smoke --test computers_helm` renders both core
presets twice, checks stable Pod/configuration identities and verifies the privileged
boundary. It also renders configured capacity and rejects missing trust/configuration
and missing store dependencies. These are real Helm configuration checks. They do
not establish provider execution or installed acceptance.
