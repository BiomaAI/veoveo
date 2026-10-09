# Private Computer Host

The Rust launcher owns private Docker, provider and storage process management.
The [Computers design](../DESIGN.md#qualification-limits) records installed qualification.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.ai/computer-host/v1` | Closed installation-owned JSON configuration; one provider UUID/namespace and a digest-pinned local image catalog |
| Docker Engine 29.8.0, API 1.53 and volume-plugin API v1 | Dedicated private Unix socket, overlay2 on ext4, local retained-volume plugin; no host daemon access |
| OpenShell `0.1.2` | Unmodified official GNU gateway/supervisor and static musl sandbox; in-process Docker driver; private TLS gRPC and selected SSH transport |
| OIDC Discovery 1.0, OAuth 2.0 and JWT/JWKS | HTTPS installation issuer, exact audience and explicit worker roles; mTLS user promotion disabled; mandatory Sandbox JWT |
| TLS 1.3, X.509 DER/PEM and `veoveo.ai/computer-storage/v1` | Separate provider server, worker client and supervisor client roots; parsed certificate admission through x509-parser `0.18.1`; retained storage keeps independent worker trust |
| OCI images | Exact manifest references from one installation registry, HTTPS or explicitly declared development HTTP |
| Linux namespaces, cgroup v2, signals and ext4 | One privileged compute container with owned mount/network/PID/cgroup namespaces, finite CPU/RAM ceilings and persistent local ext4 data |
| Kubernetes Secret projection | Read-only fixed-name files, confined resolution inside the mount and bounded copies into root-owned regular files; no user-controlled paths |

## Ownership And Topology

The launcher coordinates Docker, retained storage and the private provider inside one
privileged container. These processes share the mount namespace, which gives Docker
the allocator's mounted home paths without an external propagation dependency. Their
startup and shutdown order is part of retained-storage availability. The separately
replicated Computers MCP/control service keeps domain policy, grants and lifecycle
operations; it has no privileged mounts or daemon socket.

The `computer-host` Bake image composes the independent storage and provider targets.
It adds the exact Docker 29.8.0 static executable closure used in native qualification,
its matching DinD initializer and a small Rust launcher. The Host inherits the
storage image's Debian trixie runtime. The provider target packages official release
executables and verifies archive and executable hashes. It uses no provider compiler
or custom solver build. Gateway and supervisor require the GNU glibc closure;
the injected sandbox is the official static musl executable. The supervisor companion
uses its unchanged official image. The manifest and binary inventory travel with the
Host. Image construction alone does not qualify its ABI or mounted runtime.

The host requires its own container network and mount namespaces. Kubernetes must use
`hostNetwork: false`, no host PID sharing, one replica and a replacement rollout. The
DinD initializer must never run against the installation's host network namespace.
It prepares only this container's cgroups and mount tree. The retained data volume is
mounted at `/var/lib/veoveo-computers`; the launcher owns its private `state` directory.
That volume must be backed by persistent ext4. Overlay, tmpfs and remote storage fail
admission. Docker data, the storage journal and provider SQLite state survive container
replacement. `/run/veoveo-computers` is disposable private runtime state.

The image starts `veoveo-computer-host init` as PID 1. Before creating runtime
threads, Rust creates private mount and cgroup namespaces rooted at the current
container cgroup. It makes mounts private, mounts cgroup v2 in that namespace and
requires finite positive CPU, memory and PID maximums at that root. Upstream DinD then moves the launcher into
`/init` and enables nested controllers. Docker's `/docker` children therefore count
toward the host ceiling, including containers retained across host replacement.
Direct `run` without this bootstrap fails. The launcher never configures the node's
daemon or changes a sibling cgroup. See the kernel's
[cgroup namespace rules](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html#namespace).

The chart defaults to zero CPU/RAM requests for both Computers services and keeps
their maximums. Guest templates set maximums without memory reservations or pinned
CPUs. Idle guests consume their actual working set; resource availability is not
guaranteed in advance. Installation operators own the aggregate ceiling and pressure
policy. A lower limit does not rewrite per-Computer template limits.
The container runtime must supply the aggregate `pids.max` before Host init; Docker's
`--pids-limit` provides this in native qualification. Host init and direct runtime
admission reject `max`, zero, malformed or unavailable PID limits before starting
DinD or its services. Host does not write cgroup limits. A Kubernetes Pod ancestor
limit alone does not establish a finite limit at the private container namespace
root; installations must qualify their supported container-runtime configuration.

Storage discovers loop devices added after container creation and creates only their
verified block nodes inside this private `/dev`. Native qualification removes those
nodes before allocation, then simulates an interrupted Ready publication and verifies
that recovery preserves the backing inode and bytes before the retained restart test.

This is a trusted single-host container profile. Privileged host root and general
Docker administration remain operator authority. This profile does not claim VM-grade
hostile-tenant isolation. GPU-capable Computer templates require their separate
resource and hardware qualification; this development template runs terminal tools.

## Configuration And Trust

`veoveo-computer-host init --config <file>` bootstraps the container, then `run`
reads at most 64 KiB, rejects unknown fields
and validates identities, capacity, image digests and private networks before startup.
Configuration provides `providerId`, `namespace`, `defaultImage`, `supervisorImage`,
`images`, `templates`,
`reserveBytes`, `registry`, `bridgeAddress` and `networkPool`. The schema value is
`veoveo.ai/computer-host/v1`. `providerId` decodes through the Computers contract’s
`ProviderInstanceId`; the launcher imports that lightweight owner contract directly. Templates use the allocator's fingerprint/capacity shape.
The bridge address is a private `.1` address on a /24; the sandbox pool is a distinct
private /16 base. The installation must choose ranges that do not overlap its pod,
service, physical or routed networks. Docker owns these ranges within its namespace.

Registry `authority` is one explicit DNS/IPv4 host with optional port. Transport is
`https` or `development_http`; the latter adds only that authority to the private
daemon's insecure-registry list. The current profile preloads an unauthenticated
installation registry using image digests. Registry credential injection and custom
CA roots are not yet admitted profiles. Startup checks the private image inventory and
pulls only missing catalog entries, with a five-minute deadline per image. The provider
uses `image_pull_policy=Never`. The installation selects the qualified supervisor
image and includes its digest in the catalog. Before starting the provider, the
launcher inspects that reference through the private engine with a two-second
deadline and a 64 KiB response limit. Shared image admission requires the exact
RepoDigest and official config digest from the included release manifest. It
accepts unchanged installation mirrors without private labels on the upstream image. The native fixture uses the same
admission and compares the companion's local image ID with the admitted image ID;
Docker's local image ID differs from the registry manifest digest.
The official supervisor image contains only the supervisor executable. The Host
supplies the separately released sandbox for workload injection. An offline
installation includes this artifact closure and an accessible qualified OIDC issuer.

The host mounts its operator-owned trust Secret at
`/etc/veoveo/computers/host-trust`. The provider server root is `provider-ca.pem`.
Worker and supervisor client roots are `worker-client-ca.pem` and
`supervisor-client-ca.pem`. Each role supplies one valid self-signed X.509 CA;
the launcher rejects shared certificate identities or public keys across these
roles and the separate `storage-ca.pem`. The launcher builds
`provider-client-ca.pem` from only the worker and supervisor client roots and passes
that bundle to the provider listener's `--tls-client-ca` option.

The remaining fixed inputs are `provider-server.pem`, `provider-server-key.pem`,
`guest.pem`, `guest-key.pem`, `jwt-key.pem`, `jwt-public.pem`, `jwt-kid`,
`storage-server.pem` and `storage-server-key.pem`. The provider server certificate
must be signed by the server root with ServerAuth usage. The guest certificate must
be signed by the supervisor root with ClientAuth usage; `guest_tls_ca` supplies the
server root for the supervisor's TLS connection. The Host receives no worker private
key. Storage keeps its separate worker trust and server key pair.

ConfigMap and Secret inputs follow their managed revision links. The launcher checks
file ownership, write permissions and the 64 KiB bound on the opened regular-file
handle, which keeps one revision through an atomic projection update. Trust input
links must resolve inside their mounted root. Private keys exclude group/world
access. Internal state and locks reject final symlinks. Copies under the private
runtime directory have mode 0600.

Configuration requires `providerAuthentication`: HTTPS `issuer`, typed canonical HTTPS `resource`,
`rolesClaim`, nonempty `adminRole` and `userRole`, and `jwksTtlSecs` in 1..3600.
The Host derives upstream OIDC `audience` from that resource. The installation
provides a restricted private-key worker registration with the selected roles and
short-lived access tokens. The Host never mounts the worker signing key. The provider uses its system trust store
when `issuer-ca.pem` is absent from the mounted Host trust directory. An operator may
supply that fixed file as one complete, valid self-signed public CA certificate.
The launcher admits it through the same confined projection and file checks, copies
it into private runtime trust and sets only the provider child's `SSL_CERT_FILE` to
that copy. A present invalid or inaccessible file refuses startup. Each startup uses
the current mounted input; removing it restores system trust even when an older copy
exists. Other child environment variables stay cleared. With chart NetworkPolicy enabled,
`computers.host.issuerEgress` declares explicit IPv4 CIDRs with prefixes 1..32
for issuer/JWKS HTTPS on TCP 443. Missing destinations refuse configured rendering;
the chart infers no issuer IP or reference-installation network. Computers MCP worker
discovery/token HTTPS egress uses the separate `networkPolicy.externalEgressCidrs`
installation declaration; Host egress does not admit that worker path. Worker credential acquisition and
refresh belong to the runtime edge; the Host receives no worker OAuth secret.

Provider configuration disables mTLS certificate-to-user promotion and anonymous
user access. The supervisor keeps its complete TLS certificate/key/CA bundle and
mandatory Sandbox JWT. Stock client certificate issuer/subject values confer no user
role. Storage uses its separate worker CA. Provider server certificates cover the
worker endpoint and `host.openshell.internal`; storage certificates cover its worker
endpoint. Sandbox JWT signing material is stable installation-owned Secret state.
Key rotation requires a separately qualified transition.

The launcher validates both server TLS key/certificate pairs before spawning children.
A malformed trust input identifies its
fixed filename in the diagnostic without printing its contents.

## Process Lifecycle And Retention

A root-owned file lock excludes overlapping launchers on the same state. Existing
storage metadata is checked for the configured provider/namespace before child startup.
On first enrollment, Docker starts without a plugin registration, opens its API, and
allows storage to record its engine identity. The helper then listens and its plugin is
registered. On restart, registration is installed before Docker starts, and storage
reopens its journal without waiting for Docker's API. That lets the daemon restore
volume metadata. All physical storage operations still verify the original engine.

Docker listens only on the private Unix socket. The provider listens on port 8805 with
TLS and OIDC worker authentication; supervisor JWT authenticates its own methods. Inside the private Host
namespace the supervisor connects to `127.0.0.1:8805`; the provider server
certificate includes that IP SAN. Standalone native fixtures use their inspected
private bridge address and its IP SAN as a separate route profile. Storage listens on port 8806 with
its dedicated worker mTLS. The launcher waits for bounded process readiness and the
declared image preload before starting the provider. `health` verifies the launcher
readiness marker, private Docker API and local provider/plugin listeners. It establishes
process availability; the control service performs authenticated provider/storage
readiness and retained lifecycle checks.

SIGTERM/SIGINT or a child exit removes readiness and stops the provider, Docker, then
storage. The helper remains available for daemon volume callbacks. Each stop has a
finite grace period and then kills the owned process group. The container needs at
least 45 seconds of termination grace. Shutdown never purges Docker or retained files.
A failed or interrupted lifecycle remains governed by the Computers operation fence;
process availability alone cannot settle its domain outcome.

The private state directory remains mode 0700. Docker owns traversal permissions in
its own data root below that boundary; restart accepts its root-owned, non-writable
permissions without resetting them. Computer logs use Docker's local driver with three
10 MiB files per container. A provider or host image update is an explicit maintenance
boundary for this topology and requires the retained drain/restart qualification. A
control-service or Console update does not replace the compute host.

Qualification must cover a new volume, retained container replacement, daemon/provider
failure, unchanged Docker identity, preserved file content and denied stale writers.
The selected topology must prove loop devices, namespace teardown and restoration,
guest transport and the exact composite image ABI. Installed template maintenance,
backup/restore, host-loss durability and encryption/key ownership retain their separate
release gates in the Computers plan.

`tests/native_host.rs` uses an explicit replacement profile. `Restart` recreates the
entire Host container from the immutable `VEOVEO_COMPUTERS_HOST_IMAGE`.
`ImageUpgrade` resolves that source and `VEOVEO_COMPUTERS_HOST_REPLACEMENT_IMAGE`
to distinct immutable local image IDs. Both inputs use the stock release profile;
the restart case does not establish an image upgrade. The
Computer image comes from the locally admitted RepoDigest in
`VEOVEO_COMPUTERS_HOST_TEST_IMAGE`; the matched supervisor uses
`VEOVEO_COMPUTERS_NATIVE_SUPERVISOR_IMAGE`. The required
`VEOVEO_COMPUTERS_HOST_PULL_REGISTRY` selects one DNS/IPv4 authority reachable
inside the private Host. The fixture replaces only the authority in both image
references and preserves repository paths and manifest digests. Before creating
fixture state, it admits the local RepoDigests and ImageIDs, including the official
supervisor manifest and config identities, and reads each mapped manifest with redirects and proxies
disabled. Each read has a five-second deadline and a 64 KiB body limit. The body
SHA-256 and Docker-Content-Digest must equal the selected manifest digest; the
runnable OCI/Docker schema-2 manifest config digest must equal the local ImageID.
Missing manifests, image indexes and mismatched identities refuse admission. Ops
stages the selected immutable manifests before qualification. The retained template
fingerprint and Host configuration use the mapped pull references; inputs preserve
both local and pull identities. The fixture creates separate provider server,
worker client, supervisor client and storage roots. Its private test issuer binds
the inspected Docker bridge gateway, uses that IP in its HTTPS certificate SAN
and issues a fresh private client credential. The worker adds only the issuer CA;
the Host child receives the mounted issuer CA through `SSL_CERT_FILE` with TLS
verification enabled. Read-only provider probes require authorized bearer access
and reject certificate-only users, invalid signatures, mismatched issuer or
audience, expired tokens and unrecognized roles. The fixture preloads a Computer from the explicit local registry and runs
the production runtime and allocator clients. A Computer writes a retained file under
UID 10001, stops, and resumes after replacement of the entire compute container and
its mount/network namespaces. The test requires the same Docker UUID and provider
resource, a new process identity and the original bytes. It also proves guest TLS
authentication cannot acquire provider user authority. The fixture verifies that the
installation bridge identity is unchanged, removes its own containers, verifies loop
detachment and deletes only its owned retained state and trust. Root-owned fixture
configuration uses the same read-only file permission contract as mounted configuration.
The fixture records the explicit profile, both Host image IDs and replacement
duration in `host-restart.json` or `host-upgrade.json`. `inputs.json` preserves the exact template URI/fingerprint,
home capacity and both host image IDs after fixture cleanup. The template registry
must be reachable from the private bridge namespace. A publication endpoint bound
only to the host's loopback cannot be substituted with the host bridge's gateway;
use the registry's inspected address and port on that bridge.

The replacement fixture selects the workload from its retained writer CID and
requires exactly one supervisor with the same provider namespace, resource ID
and name. It verifies the supervisor's configured preload image. Both child
cgroups must descend from the Host root. The workload has a two-CPU, 2 GiB and
256-PID maximum. Stock OpenShell sets no separate supervisor CPU, memory or PID
ceiling; Docker inspection admits its null PID limit and the kernel child maxima
are unlimited. The aggregate Host has a one-CPU, 6 GiB and 1024-PID maximum that
includes both children and their descendants. The fixture records that inherited
budget before parsing child inspection, checks the current process population,
and requires the admitted pair to account for every running nested container. Memory
protection, CPU shares and pinned CPU sets provide no reservations. The two
containers have separate mount, PID, IPC and cgroup namespaces. The workload has
no network attachment; the supervisor shares only the private compute Host's
network namespace and runs as UID/GID 65534 without added capabilities. Its
root is read-only and no-new-privileges is required. Its host has a one-CPU maximum while the guest
allows two. A three-second guest workload must increase the host's throttling counter.
This qualifies aggregate enforcement without exhausting installation RAM.
Its retained template remains the 512 MiB native host profile;
the service's separate template-transition fixture qualifies exact installation
template capacities. The fixture checks that the selected image leaves the retained
target absent before allocation. Bytes seeded by the external allocator must survive
first Create/Start and Host replacement. Restart and image upgrade establish their
selected forward replacement case; rollback to older readers after new storage
journal operations have been admitted requires separate qualification.

Each active Computer adds one stock supervisor container with CPU, memory, PID
and log/tmpfs costs. Workload template ceilings do not constrain that supervisor
independently. The Host aggregate ceiling covers all nested processes; template
admission does not reserve capacity. Operators must budget their combined working sets.

Source checks qualify configuration and fixture refusal rules. Official
artifact/ELF closure, workload ceilings, aggregate resource enforcement and retained
composite Host replacement require the exact candidate images and ordinary owned native fixture.
Installed trust, rollout and host-loss recovery require their installation checks.
A source or compiler pass establishes none of those runtime outcomes.

Policy checkpoints use version 2 and its current provider-policy fingerprint.
Host/provider updates require a coordinated drain of version-1 operations before
the matched binaries start. Unsupported checkpoint versions fail admission; no
rolling overlap or rollback to a version-1 reader is admitted. The retained Host
fixture uses the explicitly selected image replacement within the matched version-2 profile and preserves
the backing inode, home bytes and provider resource identity.
