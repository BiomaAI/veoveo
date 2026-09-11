# Computers Provider Runtime

Status: native lifecycle, retained terminal and renewable runtime leases are qualified
in isolated fixtures. The owning service composes browser grants with this adapter;
public ingress and installed qualification remain in
[Computers](../../../docs/COMPUTERS_PLAN.md).

## Standards And Protocols

| Boundary | Selected profile |
|---|---|
| OpenShell `0.0.116` protobuf/gRPC | Vendored protocol verified by `protocol/SHA256SUMS`; internal generated client/server types over mTLS |
| OpenShell retained Docker provider | Candidate gateway `0.0.117-veoveo.2` and supervisor `0.0.117-dev.5+gea0c605`; exact patch graph in `provider-patches/manifest.json`; native installation qualification pending |
| SSH and terminal metadata | Byte-preserving canonical-main attachment with private `openshell.terminal.v1` ReplayComplete metadata; explicit history fence |
| `veoveo.io/computer-storage/v1` | Bounded mTLS prepare/restore/handoff adapter generated from Veoveo-owned `protocol/storage.json`; exact provider, operation, source and target identity; native allocator qualification in its owning component |
| Protocol Buffers canonical encoding and SHA-256 | Template and immutable binding fingerprints with cross-language fixtures |
| Internal lifecycle checkpoint JSON version 1 | Closed validated operation/provider/binding identity and pre-dispatch process epoch; serialized for the owning durable Task |
| Internal attachment lease | Monotonic authority staleness at most 30 seconds, renewal interval at most ten seconds; admission credentials are distinct from an established connection's authority |
| Rust/Tonic/Prost | Qualified workspace Tonic `0.14.6` and Prost `0.14.4`; new generator Tonic-Prost-Build `0.14.6`, Russh `0.63.3`, Typify `0.8.0` |
| Docker volume-plugin API v1; Engine HTTP API `1.53` | Selected volume methods and registered-container enumeration; the worker fixture composes the production allocator with the native provider |

Upstream stable versions were checked through crates.io and the NVIDIA GitHub
release API on 2026-09-09. Workspace `tokio-rustls` remains on its qualified
`0.26.4` pin; advancing it to `0.26.5` is independent of this adapter port.
OpenShell `0.0.116` remains the latest published release at this checkpoint.
The provider patches implement retained restart, process groups, owned terminals,
exit-event correlation, log bounds, explicit replay completion, and Docker volume
NoCopy. They require
their own qualification and upstream/removal tracking before release.

Russh enables the existing Ring crypto backend and omits its unused RSA/compression
defaults. Canonical-main pins an Ed25519 host identity. The stable Russh release
requires the prerelease `ssh-key` version recorded in Cargo.lock; this transitive
constraint remains part of provider qualification until upstream ships a compatible
stable key crate. Typify's procedural macro is disabled because this crate uses only
its build-time type generator. Generated schema regex validation requires Regress
`0.12.0`, verified from crates.io at the same checkpoint.

Attachment transport uses the existing qualified Tower `0.5.3` pin and exact
Hyper-Util `0.1.20` Tokio I/O adapter. The latter matches the upstream
[latest stable release](https://github.com/hyperium/hyper-util/releases/tag/v0.1.20),
verified on 2026-09-09. No transitive package versions changed with this direct use.

## Ownership

`client` handles provider transport and lifecycle observation. Gateway configuration
can validate its referenced TLS material without connecting; only the existing pinned
handshake constructs an admitted runtime. This separates invalid installation inputs
from temporary provider unavailability. `models`, `binding`,
`canonical` and `policy_json` validate the admitted template and exact identity.
`terminal` and `terminal_output` own the byte stream and replay boundary. `execution`
owns bounded command streams. `allocation` and `storage` bind retained volumes;
`policy_continuity` checks retained-instance replacement authority.
`lease` enforces local authority deadlines and revocation. `forward_tunnel` owns the
bounded SSH-only CLI bridge and its independent closure task.

The gateway and BFF must not depend on this crate. The Computers worker applies
canonical authorization and shared durable Task transactions before invoking it.
The adapter returns typed outcomes without secrets or provider text in errors.

The `retained_template` Cargo example computes installation fingerprints through the
same canonical encoder used by admission. Its arguments are the pinned image, CPU
count, memory/home/temporary sizes in MiB and provider-policy JSON path. It only
prints the validated fingerprint; it does not create capacity or issue credentials.

Readiness checks the configured provider workspace as well as the provider version
and driver. The workspace must exist, echo its exact name and be active. A missing
or terminating workspace cannot advertise available capacity. The packaged host
currently enrolls the provider's `default` workspace; additional workspaces require
explicit provider provisioning before workers select them.

## Retained Allocation Identity

The allocation client pins a non-nil provider UUID alongside its template fingerprint
and capacity. Ready must echo that provider. Prepare and Restore also carry the
Computer UUID and admitted instance UUID; every reply must echo all identities exactly.
The initial instance UUID equals the Computer UUID. A replacement uses its distinct
durable instance UUID while preserving the original Computer's volume name.

The private v1 protocol is installed for retained allocation. Missing, duplicate, null
and malformed identities fail decoding. A reply for another valid provider or instance also fails.
The local TLS fixture exercises initial and replacement bindings; it establishes
transport integrity, not physical handoff authority.

Create sends the full binding in both gateway object metadata and sandbox-template
labels. OpenShell forwards template labels to Docker separately from object metadata.
The same Computer/template/instance identity therefore reaches the registered-writer
check without depending on gateway-only labels. These per-instance labels do not
change the installation template fingerprint or the stable retained-volume name.

Restore is an idempotent reopen of the currently admitted allocation and instance.
It cannot seed missing storage, change the admitted writer, resize a home or authorize
template replacement. Handoff uses a separate generated request with a durable
operation UUID, exact source/target bindings and the source provider resource ID.
Its reply echoes every field and the target's admitted capacity. The client rejects
cross-Computer targets and a target without a distinct replacement instance.
The production [allocator](../../computers/storage/DESIGN.md) persists those bindings,
records the physical container and requires verified loop detachment before admitting
a new writer. Its Docker engine identity and provider namespace match the recorded
storage host. Native helper qualification does not establish installed provider or
worker maintenance integration.

The additional `abandon` operation carries a durable operation UUID and exact
source/target bindings without a claimed resource ID. It is restricted to a home whose
journal has never admitted a physical writer. The allocator supplies independent
engine, consumer and filesystem fencing; the client cannot manufacture that proof.
Replies echo every identity and capacity. An older helper rejects this operation,
which requires qualification before maintenance admission. Retiring storage admission
does not settle an uncertain provider request or release domain capacity.

## Native Storage Boundary Probe

`retained_writer` matches the admitted binding against the selected subset of Docker's
registered-container response. It requires the recorded engine UUID, one full container
ID, provider namespace, OpenShell management/name labels and complete Computer/template/
instance labels. The provider UUID is bound to that engine and namespace by the host
configuration. The matcher cannot change an admission or establish a physical handoff.

The shared native daemon fixture owns its network namespace. It validates the created
container's network mode before starting dockerd and checks namespace separation and
host bridge identity. Host-network dockerd is prohibited even with bridge/firewall
flags disabled: daemon initialization can still alter the host bridge. Provider
fixtures temporarily relay the existing localhost registry through a private Unix
socket into the isolated namespace. The exact manifest pull preserves image identity;
the relay disappears before provider work starts and is not a product service.

`tests/native_volume_plugin.rs` exercises that matcher using actual Docker containers
and a disposable directory. A stopped container continues to reserve the volume until
removal. Unmount notifications do not release access. The probe checks nested `docker
cp`, competing writers, Stop/Start and explicit replacement after source removal.
A late old instance remains denied even when it is the sole registered consumer.
The fixture explicitly changes its in-memory admission after verifying removal; it
does not establish a durable allocator, quota enforcement or backup safety.

The selected producer must use `volume-nocopy`. Docker otherwise populates a volume
before the new container enters its registry, which prevents container enumeration
from identifying that caller. The provider's seventh patch passes `no_copy` through
to Docker, and retained templates require it with the `home` subpath. This changes
the template fingerprint. Production binding must also include the provider,
template and admitted instance; a Computer label alone is insufficient for handoff.
The [Docker volume protocol](https://docs.docker.com/engine/extend/plugins_volume/) and
[Moby mount implementation](https://github.com/moby/moby/blob/6bc6209/daemon/volume/mounts/mounts.go)
show why nested mount IDs are not unique operations. Plugin RPC retries make simple
reference-count decrement unsafe after a lost Unmount reply.

Plugin registration has daemon-wide lifetime. The maintained fixture owns a separate
daemon with no network, bridge or published API port. It imports the existing Computer
image archive and verifies its exact image ID. The exact fixture image is
`docker.io/library/docker:29.8.0-dind@sha256:77759fdec1efef224ba7110ef7b5b3c6af6164ffaef5441d3beba059bde8b857`.
[Moby 29.8.0](https://github.com/moby/moby/releases/tag/docker-v29.8.0) was the latest stable
release on 2026-09-09; the manifest was resolved from the official Docker image registry.
This fixture pin does not upgrade the installation's Docker engine. It uses the image's
DinD wrapper for cgroup v2 nesting and confines its data root and plugin sockets to
fixture-owned directories. Removing the daemon also discards its plugin client cache.

Axum and Reqwest reuse the qualified workspace versions as development dependencies.
The Docker plugin request decoder accepts bounded JSON bytes because the actual daemon
does not supply the ordinary JSON Content-Type expected by Axum's JSON extractor.

## Completion And Recovery

Healthy lifecycle observation uses native WatchSandbox. `wait_for_lifecycle` takes
the persisted checkpoint, the checked dispatch response and a remaining budget.
Synchronous responses and watch events use the same epoch checks as recovery. The
watch performs no status reads and lasts at most 180 seconds or the smaller caller
budget. An old Ready process cannot complete Start. The supplied event/log tails
are best effort and lag warnings do not establish complete history. Transport loss
returns uncertainty and never proves a failed mutation. `recovery` implements one
bounded, identity-correlated reconciliation read under
[CE-01](../../../docs/CONTRACT_EVOLUTION.md#ce-01-provider-completion-follows-qualified-semantics).

Create uses deterministic binding identity. Start/Stop must preserve the selected
provider instance and run epoch. A fresh snapshot proves only the state it contains;
it cannot certify an arbitrary past command. The owning durable operation remains
fenced until its effect can be settled. Cancellation is distinct from stopping a
provider process, and a terminal transport deadline is distinct from Computer lifetime.

## Renewable Attachment Enforcement

Every terminal and CLI adapter receives an `AttachmentLease`. The owning authority task
retains `LeaseAuthority` and captures a monotonic timestamp before each authoritative
grant/policy read. Its read latency consumes the maximum 30-second authority window.
The owner renews at most every ten seconds and caps the window by remaining grant life
and installation clock allowances. A missed update expires locally; loss of the
authority task closes its leases immediately. Delayed checks cannot overwrite a newer
check or revive revoked or expired access. Wall-clock time is a display projection.

`Terminal` exposes its checked resource and process identities for the owning service
to compare against the durable grant before forwarding bytes. Its narrow `TerminalInput`
handle lets that service drive input separately from output; dropping the terminal
still closes every input handle through the same worker and attachment lease.

Terminal and CLI pumps select authority closure independently of both I/O directions.
They drop provider transport before cleanup, reject buffered output after revocation,
and retain the main process. CLI forwarding validates the exact sandbox, SSH service,
SSH target and bounded data frames before sending them upstream. It admits no generic
TCP target. Cancellation during provider credential issuance drains the bounded reply
and revokes an orphaned credential.

Each attachment owns a separate mTLS HTTP/2 connection and an OS socket shutdown guard.
Dropping a gRPC response alone left an upstream connection open when a fixture provider
stopped consuming input. The guard closes that connection independently of HTTP/2 flow
control, without copying the byte stream or affecting shared lifecycle RPCs. Reconnection
of that transport is rejected; a broken attachment requires fresh authorized admission.

The provider SSH credential admits a connection. Its expiry does not shorten an
established connection that still holds renewed Veoveo authority. Provider RPC admission
and SSH setup retain finite deadlines; the live stream has no fixed gRPC timeout.
The lease mechanism does not authorize a principal itself. Durable browser grants and
current policy/family renewal belong to `platform/computers`; `servers/computers-mcp`
composes their authority with this runtime and shared revocation wakes. Public Console
and CLI ingress qualification remain delivery work.

The native renewal fixture exchanges terminal data after repeated two-second leases
and the provider's three-second admission credential have expired. The same shell
remains attached. Explicit revocation denies further input/output, and fresh authority
reattaches to the same native process. Local real-mTLS fixtures cover bidirectional
backpressure, buffered output denial, late mint cleanup and non-revivable deadlines.

## Execution Argument Boundary

The private execution adapter preserves an explicit argument vector, including empty
non-program arguments and embedded newlines. It admits up to 1,024 arguments, matching
the selected provider's count bound, while retaining a 32 KiB aggregate UTF-8 byte
limit. The first argument remains a canonical absolute executable path. NUL bytes,
invalid working directories and unbounded timeout/output remain rejected.

This replaces the unnecessarily restrictive 32-argument and nonempty-value limits.
Local mTLS RPC fixtures prove exact argument delivery, including quoting, Unicode
and empty values. They do not qualify process cancellation or public agent execution.
The current gateway still logs command previews; callers of this private build-worker
path must not place credentials in command arguments.
A working directory selects where a program starts; it does not confine arbitrary
program access to that directory. Computer isolation remains the authority boundary.

`execute_request` uses the private [guest launcher](../../computers/execution/DESIGN.md).
Only its fixed absolute command and fixed retained-home working directory reach the
provider's command preview. Explicit argv, relative launch directory, environment
overrides and finite binary stdin travel in a bounded length-prefixed stdin frame.
No request values enter the provider Start message's environment or argv. The runtime
retains its input stream until the native exit; transport EOF is not frame completion. The
caller must supply the exact Ready resource/process observation recorded for its
command. Before sending the native Start frame, the runtime compares the current
resource and process to that observation. A changed run, stopped expectation or
inconsistent exit state rejects execution without sending request bytes. The final
observation still has to match the selected process. This check complements the
domain's execution slot; it does not establish a provider-side compare-and-swap
against privileged out-of-band lifecycle changes.

The native fixture proves exact 100,000-byte input and output, empty/quoted arguments,
multiline environment, confined directory selection and enabled command-log privacy.
After a timeout, it requires ExecutionUnknown, then Stop and a new process epoch. A
detached descendant stops writing and its retained file survives restart. Public Task
cancellation must compose this Computer-wide Stop boundary and explain its scope.
This adapter never settles a Task or grants an actor execution authority.

## Verification And Delivery Gaps

Before dispatch the owner persists a LifecycleCheckpoint with the operation UUID,
installation-owned provider UUID, complete binding, and the previous resource/run
identity for Start or Stop. The runtime's configured provider UUID must match before
any recovery I/O. A recovery attempt reads at most once and expires within ten seconds
or the smaller remaining budget. The owning Task charges and persists its recovery
budget before calling; no per-process loop resets the budget after a restart.

Start requires a new nonempty process identity on the same sandbox. Stop requires
the same recorded process and sandbox. Create requires the exact deterministic full
binding and a ready process. A missing resource, inconsistent state, or observer error
does not authorize redispatch. Pending snapshots preserve the original operation.
This evidence establishes the declared lifecycle state, not the result of an arbitrary
command. Domain integration and native fault qualification remain delivery gaps.

The focused crate fixtures cover real local mTLS, generated provider RPCs, policy
validation, binding mismatches, bounded command/terminal streams, and replay. A fixture
provider is not native OpenShell execution evidence. Production admission additionally
requires retained storage, physical writer fencing, provider restart, command outcome
correlation, revocation under backpressure, and the supported browser/stock CLI journey.

Source provenance is the reviewed September 9 handoff with SHA-256
`b330a4016a25d182e206421c4eb019a1cd2fc0ae94e40b7d2056dcd654cba061`.
The imported selected source excludes the downstream domain/gateway integration.
The runtime is adapted independently against Veoveo main; provider patches retain
their original upstream base and declared branch/tree identities.

## Native Fixture

`tests/native_lifecycle.rs` starts the exact patched gateway and supervisor against
an isolated Docker namespace. It generates mTLS credentials and a separate Ed25519
provider JWT signer with a finite lifetime. The fixture uses a digest-pinned Computer
image, disables host bind mounts, and removes its own containers, network, credentials
and database on completion. Its private output directory retains diagnostics.

Worker and guest transport certificates are distinct. The provider's mTLS user
allowlist admits only the worker common name. A guest certificate alone must reach
TLS and fail ListSandboxes with Unauthenticated; the real supervisor continues with
its scoped sandbox JWT. This negative gate runs before every native provider fixture.
The former fixture's shared certificate is retired from the selected profile.

Run this ignored integration test explicitly with `VEOVEO_COMPUTERS_NATIVE_GATEWAY`,
`VEOVEO_COMPUTERS_NATIVE_SUPERVISOR`, and `VEOVEO_COMPUTERS_NATIVE_OUTPUT` set to absolute
paths and `VEOVEO_COMPUTERS_NATIVE_IMAGE` set to an available image digest. The command
is `cargo test --locked --offline -p veoveo-computers-runtime --test native_lifecycle
-- --ignored --nocapture`. Record it through the repository evidence recorder.

The test proves native creation, an explicit replay boundary, retained shell-local
state on reattachment, numeric UID 10001, and a new process identity after Stop/Start.
Reconciliation observes the expected stopped and restarted epochs without dispatch.
This fixture uses the container writable layer. It does not qualify retained external
volumes, host restart, storage quotas, renewable access, stock CLI or public ingress.

The stock CLI fixture uses the verified `0.0.116` binary supplied by
`VEOVEO_COMPUTERS_NATIVE_CLI`. It registers an isolated mTLS gateway with a three-second
SSH session TTL. The unmodified client keeps the same shell and exchanges input/output
after that admission credential expires. The pinned provider validates SSH credentials
when admitting a tunnel; it does not expire an established bridge with that credential.
Veoveo must enforce its own renewable connection authority and close both directions on
expiry or revocation. This direct native probe establishes the transport prerequisite;
it does not qualify platform renewal, browser pairing or public ingress. Run the same
`native_lifecycle` target with the additional CLI environment variable to include it.

`tests/native_retention.rs` adds isolated privileged block-device setup to the native
fixture. It preallocates a 512 MiB ext4 image and uses Docker's local block-volume
mounting path with `nodev,nosuid`. Actual user writes encounter ENOSPC; the existing
file survives. Writes outside the home are denied. The fixture removes its source
containers, unmounts the volume and verifies loop detachment before taking a block
backup. It restores that backup into a distinct provider resource/process while
preserving the Computer UUID, file contents and UID. No installed home is inspected
or changed. This establishes the filesystem mechanism; it is not the implementation
or qualification of a production allocator or automatic writer-handoff protocol.

Docker's local driver can mount one volume into multiple containers. Its name alone
cannot prove exclusive ownership. A production handoff must fence the old physical
writer before admitting a replacement and retain uncertainty after lost dispatch.
The volume-plugin protocol also permits repeated Mount/Unmount calls using the same
consumer ID, including `docker cp`; a naive ID set cannot establish final release.
The current host uses ext4 without project quotas, so an ordinary directory-backed
volume cannot establish the required capacity bound. The fixed-size block profile
uses the maintained [Docker local volume](https://docs.docker.com/engine/storage/volumes/)
and Linux ext4/loop facilities. Production allocation and fencing remain delivery work.

Start and Stop accept the durably recorded source observation. Before sending a
mutation, Start checks the stopped resource and process; Stop checks the running
resource and process. A stale source cannot mutate a newer run. Domain fencing
excludes competing lifecycle dispatches during this read/submission boundary.
