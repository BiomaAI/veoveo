# Private Computer Host

Status: the composite image passes isolated native retained-container replacement.
Installed Kubernetes and public Computers journeys remain release work.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.io/computer-host/v1` | Closed installation-owned JSON configuration; one provider UUID/namespace and a digest-pinned local image catalog |
| Docker Engine 29.8.0, API 1.53 and volume-plugin API v1 | Dedicated private Unix socket, overlay2 on ext4, local retained-volume plugin; no host daemon access |
| OpenShell provider `0.0.117-veoveo.2` and supervisor `0.0.117-dev.5+gea0c605` | Exact maintained patch trees and OCI-built binaries from the provider image; private mTLS gRPC and selected SSH transport |
| TLS 1.3 and `veoveo.io/computer-storage/v1` | Separate worker trust for retained storage; provider guest certificates are denied provider user authority |
| OCI images | Exact manifest references from one installation registry, HTTPS or explicitly declared development HTTP |
| Linux namespaces, cgroups, signals and ext4 | One privileged compute container with an owned mount/network/PID namespace, bounded child lifetimes and persistent local ext4 data |
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
its matching DinD initializer and a small Rust launcher. The provider's pinned Z3
library is retained; the Debian trixie GNU runtime supplies the shared libc/C++ closure.
Package and binary inventories remain inside the image. Building this image does not
by itself qualify the resulting runtime ABI or mount behavior.

The host requires its own container network and mount namespaces. Kubernetes must use
`hostNetwork: false`, no host PID sharing, one replica and a replacement rollout. The
DinD initializer must never run against the installation's host network namespace.
It prepares only this container's cgroups and mount tree. The retained data volume is
mounted at `/var/lib/veoveo-computers`; the launcher owns its private `state` directory.
That volume must be backed by persistent ext4. Overlay, tmpfs and remote storage fail
admission. Docker data, the storage journal and provider SQLite state survive container
replacement. `/run/veoveo-computers` is disposable private runtime state.

This is a trusted single-host container profile. Privileged host root and general
Docker administration remain operator authority. This profile does not claim VM-grade
hostile-tenant isolation. GPU-capable Computer templates require their separate
resource and hardware qualification; this development template runs terminal tools.

## Configuration And Trust

`veoveo-computer-host run --config <file>` reads at most 64 KiB, rejects unknown fields
and validates identities, capacity, image digests and private networks before startup.
Configuration provides `providerId`, `namespace`, `defaultImage`, `images`, `templates`,
`reserveBytes`, `registry`, `bridgeAddress` and `networkPool`. The schema value is
`veoveo.io/computer-host/v1`. Templates use the allocator's fingerprint/capacity shape.
The bridge address is a private `.1` address on a /24; the sandbox pool is a distinct
private /16 base. The installation must choose ranges that do not overlap its pod,
service, physical or routed networks. Docker owns these ranges within its namespace.

Registry `authority` is one explicit DNS/IPv4 host with optional port. Transport is
`https` or `development_http`; the latter adds only that authority to the private
daemon's insecure-registry list. The current profile preloads an unauthenticated
installation registry using image digests. Registry credential injection and custom
CA roots are not yet admitted profiles. Startup checks the private image inventory and
pulls only missing catalog entries, with a five-minute deadline per image. The provider
uses `image_pull_policy=Never`. An offline installation must include that local registry
closure and the host image; no upstream Internet download is part of retained Start.

The host mounts its operator-owned trust Secret at
`/etc/veoveo/computers/host-trust`. Fixed inputs are `provider-ca.pem`,
`provider-server.pem`, `provider-server-key.pem`, `guest.pem`, `guest-key.pem`,
`jwt-key.pem`, `jwt-public.pem`, `jwt-kid`, `storage-ca.pem`, `storage-server.pem` and
`storage-server-key.pem`. Their canonical targets must remain inside the mount.
Inputs are regular files with no group/world write permission and a 64 KiB bound.
Copies under the private runtime directory have mode 0600. The storage loader keeps
its existing no-symlink/private-key checks. The host receives no worker private key.

Provider client admission allows only exact CN `veoveo-computers-worker`. The guest
supervisor uses a separate certificate/key with its own CN and receives no user
authority. Storage's worker CA is a separate trust root that never signs guests.
Provider server certificates must cover the worker endpoint and
`host.openshell.internal`; storage certificates must cover its worker endpoint.
JWT signing material is stable installation-owned Secret state, independent of the
container's temporary files. Key rotation requires a separately qualified transition.

The launcher validates both server TLS key/certificate pairs before spawning children
and rejects identical provider/storage CA inputs. A malformed trust input identifies its
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
mandatory worker mTLS and guest identity separation. Storage listens on port 8806 with
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

`tests/native_host.rs` resolves one candidate host image to its immutable local image
ID, then uses that same image for both container generations. It creates separate
provider/storage trust, preloads a Computer from the explicit local registry and runs
the production runtime and allocator clients. A Computer writes a retained file under
UID 10001, stops, and resumes after replacement of the entire compute container and
its mount/network namespaces. The test requires the same Docker UUID and provider
resource, a new process identity and the original bytes. It also proves guest TLS
authentication cannot acquire provider user authority. The fixture verifies that the
installation bridge identity is unchanged, removes its own containers, verifies loop
detachment and deletes only its owned retained state and trust. Root-owned fixture
configuration uses the same read-only file permission contract as mounted configuration.
