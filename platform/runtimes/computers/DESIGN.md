# Computers Provider Runtime

The private adapter admits one OpenShell 0.1.2 provider profile. Consumer compilation
and owning controls qualify typed admission, identity fencing and private serialization.
Official artifact admission and native acceptance are separate release gates; public ingress
and installed acceptance belong to [Computers](../../computers/DESIGN.md#qualification-limits).

## Standards And Protocols

| Boundary | Selected profile |
|---|---|
| OpenShell `0.1.2` protobuf/gRPC | Vendored protocol verified by `protocol/SHA256SUMS`; internal generated client/server types over mTLS |
| OpenShell retained Docker provider | Unmodified released gateway/supervisor/sandbox/CLI `0.1.2`; in-process Docker driver; exact artifacts in `provider/manifest.json`; native installation qualification pending |
| OIDC Discovery 1.0, OAuth 2.0 and JWT/JWKS | Installation-supplied HTTPS issuer and dedicated private worker client; exact audience and explicit upstream roles; mTLS user promotion and anonymous user access disabled |
| `veoveo.ai/openshell-release-profile/v1` | Closed official source/artifact declaration; archive/executable hashes and gateway/supervisor OCI manifest/config identities |
| `veoveo.ai/openshell-native-profile/v2` | Closed pre-effect fixture receipt; four released executables, unchanged companion image and separately injected sandbox; 16 KiB maximum |
| SSH terminal | Stock authenticated `shell_request` with a fresh PTY and shell per attachment; byte-preserving stdout/stderr and terminal resize |
| `veoveo.ai/computer-storage/v1` | Bounded mTLS prepare/restore/handoff/abandon adapter generated from Veoveo-owned `protocol/storage.json`; exact provider, operation, source and target identity; native allocator qualification in its owning component |
| Protocol Buffers canonical encoding and SHA-256 | Template and immutable binding fingerprints with cross-language fixtures |
| Internal lifecycle checkpoint JSON version 1 | Closed validated operation/provider/binding identity and pre-dispatch process epoch; serialized for the owning durable Task |
| OpenShell gateway TOML schema 2 | Host emits supported stock gateway and Docker driver settings; native authenticated qualification is pending |
| Veoveo private policy checkpoint protobuf v2 | `protocol/maintenance.proto`; canonical Prost encoding of exact provider/source/run and selected gateway version with the generated provider configuration. Encrypted journal storage is required; this format grants no authority |
| Internal attachment lease | Monotonic authority staleness at most 30 seconds, renewal interval at most ten seconds; admission credentials are distinct from an established connection's authority |
| `veoveo.ai/computer-files/v2` | Private framed JSON header with raw binary file stream and typed SHA-256 receipt; regular files up to 64 MiB, no archive extraction, fixed guest helper command; numeric header version 2, export `maximumBytes`; execution protocol stays version 1 |
| Rust workspace toolchain, Tonic and Prost | Provider executables are official release artifacts; qualified workspace Tonic `0.14.6` and Prost `0.14.4`; generator Tonic-Prost-Build `0.14.6`, Russh `0.63.3`, Typify `0.8.0` |
| Docker volume-plugin API v1; Engine HTTP API `1.53` | Selected volume methods and registered-container enumeration; the worker fixture composes the production allocator with the native provider |

OpenShell's selected stable release is
[v0.1.2](https://github.com/NVIDIA/OpenShell/releases/tag/v0.1.2), commit
`6648bd0c290efbc41ba131ee9831ee45cd431f94`. The exact licensed protocol tree and
hashes bind the generated client to that release. The provider package declares the
official archive/executable hashes and OCI identities in
[its README](provider/README.md).

## Installation And Persistence Compatibility

Install the official gateway, supervisor companion and static musl sandbox from
the declared `0.1.2` release profile. The gateway contains the Docker driver. The
adapter checks gateway version and provider capabilities before effects. Generated requests select the checked binding's
provider name and explicit workspace; public Computer UUIDs and grants keep their
existing domain identity. Public callers cannot provide private provider selectors.

The private protobuf cut changes network policy TLS, enforcement and access from
strings to enums. Timestamp and duration fields also use the upstream well-known
messages, and launch authentication/runtime generations carry the current run's
credentials and identity. Canonical hashes cover encoded fields rather than a semantic
cross-version normalization: templates that use changed field encodings receive new
fingerprints. Existing simple-template fingerprint goldens remain stable because
those fields are absent. Installations must materialize and admit their complete
current-format template catalog before accepting work.

The encrypted maintenance payload uses PolicyCheckpoint version 2 and fingerprint
domain `veoveo-private-policy-checkpoint-v2`. Its configuration body and selected
gateway version affect the bytes and fingerprint. Decoders reject version 1, another
provider profile and unknown/noncanonical encoding. Lifecycle JSON envelope version 1
keeps its field shape and exact resource/process identity checks.

A coordinated drain is required. Settle pending lifecycle and maintenance operations
with the qualified previous workers before changing provider and readers. An unresolved
outcome preserves its Computer fence; an expired wait never authorizes another
mutation. Keep the prior binaries, encrypted journals and retained data available for
recovery. Mixed maintenance readers are unsupported. The upgrade does not convert
old private checkpoints or rewrite retained files. Retained volume names remain bound
to the public Computer UUID. A rollback requires the matched old provider/readers and
a catalog whose bindings and encrypted checkpoints they admit; new-format maintenance
must finish or remain fenced before rollback. Image/home data compatibility requires
its own declared retained-maintenance transition.

## Provider Process And Build Profile

The companion supervisor runs with its own UID/GID, all capabilities dropped,
no-new-privileges and a read-only root. It mounts read-only channel/state volumes,
never the retained home or Docker socket, and shares no PID/cgroup namespace. Host
networking refers only to the existing private compute-host/DinD namespace. The
workload runs `network=none` and enters a capability-free static musl sandbox boundary.
Installed companion gRPC uses loopback with an admitted IP SAN. Native fixtures bind
an observed private bridge gateway IP and ephemeral port with that exact IP SAN; they
do not qualify the installed host's namespace or mount topology.

Both containers receive the template CPU/memory maxima and configured PIDs maximum
separately. They have zero reservations and no pinned CPUs. The companion therefore
adds a second container ceiling, read-only staging volume and bounded log storage;
the two maxima do not imply one shared template-memory allowance. The compute host's
finite aggregate ceiling supplies pressure containment. Retained-Computer quotas
count allocations and do not promise memory reservations. Stop/retirement and cleanup
must settle all supervised resources while the storage allocator independently
establishes physical writer exclusion.

The package verifies official archive and executable hashes and consumes the pinned
upstream gateway image. It compiles no provider source or solver. The official GNU
gateway and supervisor depend on the glibc runtime; the released musl sandbox is
static. The Host's actual ELF closure and mounted runtime require native qualification.
The unchanged supervisor image and installation mirror must preserve the selected
amd64 manifest and config digests.

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
`terminal` and `terminal_output` own fresh-shell admission and the byte stream. `execution`
owns bounded command streams. `allocation` and `storage` bind retained volumes;
`policy_continuity` checks effective policy continuity after an authenticated retained
handoff. It compares the complete instance-bound template, including guest labels.
The owning durable maintenance operation fences mutations before capturing a stopped
source. Restoration accepts only the allocator's exact source/target receipt and
does not depend on the deleted provider object. Template preflight allows an image
change with identical command, static policy, resources and storage sizes. The caller
must separately qualify image data compatibility before admitting that transition.
After a lost update ticket or reply, `reconcile_replacement_policy` performs bounded
authoritative reads only. An unchanged baseline preserves the unknown update outcome.
Matching additional grants require the original maintenance provenance and Loaded
revision; coincidentally matching grants from another operation cannot settle it.
Its `checkpoint` module encodes sensitive policy for immediate journal encryption and
recovers it without rereading a retired source. It rejects cross-provider, source,
process, workspace and template identity; unknown or noncanonical encoding also fails.
The checkpoint is bounded to 1 MiB plus 4 KiB of identity overhead. Its internal
unkeyed fingerprint must remain private with the policy, because settings can contain
secrets. Returned plaintext buffers are zeroized; generated provider strings do not
provide a whole-process memory-erasure guarantee.
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
enrolls the provider's `default` workspace. Its retained volume approval profile,
`RetainedVolumeAdmission::DEFAULT`, requires `openshell.ai/sandbox-attachable=true`
and `openshell.ai/sandbox-attachable-workspace=default`. The allocator and native
physical-home fixture share these claims and require them on reuse; unrelated labels
may coexist. These operator claims approve attachment; they do not authorize a
principal or prove physical writer exclusion. Neither owner relabels an unadmitted
existing volume. The Host's instance
namespace does not identify this provider workspace. Computers MCP refuses another
workspace for its packaged retained Host backend before trust or effects. Generic
Gateway configuration and runtime calls preserve explicitly provisioned workspaces;
qualifying another retained workspace needs a matching storage profile.

## Lifecycle Correlation

`LifecycleOperationId` binds a provider Create, Start or Stop checkpoint to its
initiating operation. A lifecycle Task supplies its `TaskId` through a checked
conversion. Command and file containment use their own recorded operation nonce.
Those operations can share the lifecycle adapter without confusing a containment
operation with the Task that requested work. The provider ID occupies a separate
constructor argument. Deserialization rejects nil correlation IDs before recovery.
The private checkpoint keeps its UUID wire representation.

Allocator handoff, abandon and policy restoration take the maintenance `TaskId`.
Only their private protocol adapters convert it to the allocator's UUID field.

## Retained Allocation Identity

The allocation client pins the Computers contract’s `ProviderInstanceId` alongside its template fingerprint
and capacity. The lightweight contract carries no provider, MCP or persistence dependency.
Private protobuf and allocator adapters convert the identity to their wire UUID.
Ready must echo that provider. Prepare and Restore also carry the
Computer UUID and admitted instance UUID; every reply must echo all identities exactly.
The initial instance UUID equals the Computer UUID. A replacement uses its distinct
durable instance UUID while preserving the original Computer's volume name.
`Binding::from_instance` resolves that persisted pair without changing admission.

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

## Provider Restart

The adapter accepts stock OpenShell restart behavior. Gateway restart replaces the
supervisor and revives the guest. Terminal sessions reset; retained-volume files
persist. Veoveo observes the provider's resulting resource and process identity before
admitting another attachment or settling an operation. A disconnected stream or
missing observation preserves the original operation fence.

The provider owns its private state formats and restoration behavior. Veoveo neither
adds companion binding files nor adopts a running supervisor through a private
protocol. Native qualification must establish retained file contents, fresh terminal
attachment and domain fencing across the released provider's restart path.

## Completion And Recovery

`retirement` sends one DeleteSandbox request for an exactly observed Stopped resource
and process. The entire preflight/request has a thirty-second deadline. A changed
binding, phase or run rejects dispatch; a lost reply remains unknown. The returned
acknowledgement distinguishes accepted deletion from the provider reporting an absent
compute resource. Neither result proves physical exclusion or permits releasing a
home. The owning maintenance journal must hold the original dispatch ticket and keep
source lifecycle mutations fenced. The selected private provider profile excludes
concurrent out-of-band administration and never reuses retired instance names.

The gateway assigns a canonical UUIDv4 `provider_attachment_epoch` after admitting
template inputs. Policy capture admits that owner identity separately, then compares
every other spec field against the selected immutable template. Config reads must
carry the same epoch as their resource spec. Before/after reads and restoration
watches preserve the full same-instance epoch equality. The encrypted checkpoint
retains its source epoch in the existing authenticated config payload; recovery
requires that identity alongside the original resource and process. Source and
replacement epochs are independently admitted against their own spec/config pairs.
Only their epoch difference is normalized when comparing settings across those
instances. A missing, malformed or mismatched epoch refuses capture or restoration.
The supervisor's canonical UUIDv4 configuration registration identity is admitted
from resource status and matched against every config read in Ready and Stopped.
The gateway preserves that identity across Stop. Its authenticated checkpoint
payload retains the source identity; a replacement carries its independently
admitted registration identity. Cross-instance comparison normalizes only these
two admitted instance facts. Same-instance reads, watches and reconciliation
require unchanged identities. Restoration refusal diagnostics expose closed
stage names without provider settings, policy values or credentials.

Recovery may observe the exact original binding within its persisted budget, then
ask the allocator to prove physical writer removal and hand off the retained home.
A current not-found result alone cannot transfer storage. The command fixture now
drives the product maintenance journal and worker through source Stop, encrypted
policy capture, retirement, allocator handoff and replacement creation. Existing
command-marker bytes must survive. Policy restoration and final verification recover
the private checkpoint from the real store after the source has been deleted. Provider
update recovery uses the read-only entrypoint. The fixture's admitted transition uses
the same image on a fresh instance. The service's `native_maintenance` fixture adds
explicit installation-template image upgrade, recovery and reverse replacement.
Installed adoption retains its own acceptance requirement.

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

Each terminal attachment requests a fresh PTY and ordinary SSH shell. Ready reports
authenticated acceptance of that request; a later native exec failure closes the
attachment with an error. Reconnection starts a new shell and restores no previous
terminal output. Files on retained volumes follow the storage contract. Keyboard,
paste, IME, mouse reporting and normal terminal query responses use the same byte
stream. No provider metadata or historical-output fence delays input.

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

The native renewal fixture requires terminal data after repeated two-second leases
and the provider's three-second admission credential have expired. The same shell
stays attached. Explicit revocation must deny further input/output, and fresh
authority must open a new shell in the admitted Computer process. Stock-provider
qualification of this case is pending. Local real-mTLS fixtures cover bidirectional
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

## Regular File Transport

`file_request` carries the private `veoveo.ai/computer-files/v2` helper protocol over
the existing bounded native execution stream. The provider sees only the fixed
launcher command with `--files`. The caller holds current domain authority and an
exclusive durable fence, and selects a template qualified with this helper profile.
The runtime requires the exact admitted Ready resource and process before dispatch
and verifies that process again at completion.
The writer and packaged guest helper select the same file profile. Upgrade drains
file operations, rebuilds the helper-bearing Computer template and selects its admitted
digest before installing the v2 writer. Version 1, retired fields and mixed headers
refuse admission. Existing uncertainty fences persist.

Import sends a bounded header and exactly the declared binary body. Export streams
provisional bytes to the caller. A successful native exit must agree with a typed
helper receipt, exact byte count and SHA-256 before those bytes can be published.
The helper result has a separate 4 KiB allowance above the 64 MiB file limit.
Ordinary command-output limits remain unchanged. Paths, bodies and provider commands
never appear in public result diagnostics.

A definitive helper rejection requires its matching native exit status. Malformed
results, transport loss, incomplete input and uncertain commits retain
`ExecutionUnknown`; the caller must contain the selected Computer and cannot replay
an import. A rejected export may already have produced provisional bytes, which its
caller discards. This adapter does not issue Artifact capabilities or settle Tasks.
The native file scenario owns its provider and retained home. It passes with template
`sha256:ddaafa2630bacbc9868b210fc280fd2665cf539795c01cb034996823c8a4e4b5`:
1,000,003 binary bytes round-trip with matching SHA-256, an opaque archive remains
unextracted, overwrite and bad checksum are rejected, and a short body remains
uncertain. Stop/Start preserves committed bytes while the incomplete destination is
absent. A stale process observation cannot write a new file. Enabled provider command
logs omit the requested paths. Public Artifact handoff qualification remains in the
Computers service.

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
validation, binding mismatches, bounded command/terminal streams, and fresh-shell
admission. A fixture provider is not native OpenShell execution evidence. Production admission additionally
requires retained storage, physical writer fencing, provider restart, command outcome
correlation, revocation under backpressure, and the supported browser/stock CLI journey.

Source provenance is the reviewed September 9 handoff with SHA-256
`b330a4016a25d182e206421c4eb019a1cd2fc0ae94e40b7d2056dcd654cba061`.
The imported selected source excludes the downstream domain/gateway integration.
The runtime is adapted independently against Veoveo main; provider patches retain
their original upstream base and declared branch/tree identities.

## Native Fixture

The retained native fixtures can enroll several digest-pinned images in their private
Docker daemon. A temporary bounded relay serves bytes from the owned localhost:5001
registry under each selected image's exact DNS name and unprivileged port. The alias
and insecure-registry setting exist only inside the disposable daemon. The relay
container runs the already cached helper image by ID. No installed Docker configuration
changes and no external registry credentials cross this fixture boundary.
Cold private-daemon container creation has an explicit sixty-second setup allowance.
It logs its duration and names its owned container if the reply is lost; it never
resends Create. This setup allowance does not extend provider operation budgets.

`tests/native_lifecycle.rs` owns isolated provider lifecycle qualification. The stock
profile requires the exact official gateway and supervisor, complete supervisor TLS
and a separate gateway Sandbox JWT signer. It must use a test-only OIDC issuer in
its owning harness; an installation requires its own dedicated worker registration.
The fixture must remove its owned containers, network, credentials and database.
Diagnostics expose closed stages and error classes without provider payloads or secrets.

Stock certificate authentication cannot establish user authority. With OIDC configured,
mTLS user promotion disabled and anonymous access refused, a supervisor certificate
without a JWT must complete TLS and fail user RPCs. A current Sandbox JWT must pass
only its declared supervisor methods. Worker tokens must pass required control methods
and fail expired, wrong-issuer, wrong-audience and unauthorized-role cases. These
native security controls and stock retained lifecycle acceptance remain open.

Run this ignored integration test explicitly with `VEOVEO_COMPUTERS_NATIVE_GATEWAY`,
`VEOVEO_COMPUTERS_NATIVE_SUPERVISOR`, and `VEOVEO_COMPUTERS_NATIVE_OUTPUT` set to absolute
paths and `VEOVEO_COMPUTERS_NATIVE_IMAGE` set to an available image digest. The command
is `cargo test --locked --offline -p veoveo-computers-runtime --test native_lifecycle
-- --ignored --nocapture`. Record it through the repository evidence recorder.

The lifecycle case requires native creation, fresh terminal attachment, numeric UID 10001,
and settled observations before abrupt loss of its admitted gateway process group.
The fault sends SIGKILL once and reaps the owned group within three seconds. The
qualified gateway hosts its Docker driver in-process; the fixture does not require
a standalone driver child. Ordinary fixture cleanup keeps graceful TERM/KILL/reap.
Graceful provider shutdown intentionally stops workloads and does not promise
continuity of the running canonical process.
The replacement controller uses the original private database, trust and launch inputs.
A fresh Get supplies the replacement controller observation to the lifecycle assessor,
which opens a current watch if readiness is pending. The previous Create response
cannot establish recovery. The case admits the current process identity observed
from the replacement controller. It requires a replacement supervisor, preserved
workload container/image/home and actual retained-file reads through fresh authenticated
access. Reconnection opens an empty shell with no historical replay. Stop/Start must
then publish its current process identity while preserving admitted retained resources.
Process-group cleanup controls run locally; stock native restart qualification remains
open. This case does not qualify host reboot, storage quotas or public ingress.

The stock CLI fixture uses the verified `0.1.2` binary supplied by
`VEOVEO_COMPUTERS_NATIVE_CLI`. It uses isolated worker OIDC authentication and complete supervisor TLS with a three-second
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
