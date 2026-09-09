# Computers Core Capability Plan

Status: the user approved Computers as a core Veoveo capability on 2026-09-09.
This document proposes the implementation and release sequence for review;
runtime implementation and deployment are not started by this planning change.
The product decision is recorded in `ARCHITECTURE_DECISIONS.md`.

## Standards And Protocols

| Boundary | Planned supported profile |
|---|---|
| Veoveo identity and Work Context governance | Existing canonical tenant/principal mapping, gateway policy, session families, and scoped attachment authority |
| HTTP and JSON | RFC 9110 control operations; closed Rust serde/schemars DTOs, JSON Schema 2020-12, and generated Console types |
| Shared Tasks and transactional outbox | Existing durable Task identities, leases, cancellation, recovery, retention, and replayable installation events |
| Provider completion | Authenticated, idempotent webhooks persisted before acknowledgment; use the shared webhook-wait recovery boundary |
| OpenShell provider adapter | Generated protobuf/gRPC over mTLS. Native watches are internal adapter inputs; they do not replace Veoveo's webhook completion contract |
| Browser terminal | RFC 6455 binary transport; typed internal terminal-v2 controls, absolute authorization deadlines, and explicit ReplayComplete metadata |
| Stock OpenShell CLI | Reviewed baseline 0.0.116; existing-SSO pairing and restricted HTTP/2/gRPC over WebSocket, including the stock client's root tunnel route |
| SSH | Authorized access to the bound Computer; browser canonical-main attachment and ordinary SSH sessions have separate tested semantics |
| Retained storage | Reviewed `veoveo.io/computer-storage/v1` allocation boundary over mTLS, with bounded frames and exact identity/capacity validation |
| Release and installation | Existing OCI runtime/publication digest distinction, Helm packaging, installation-owned GitOps and trust references; deployment v7 remains the disposable-development contract |
| Visual acceptance | Headed browser with a proved hardware WebGPU or WebGL path; GPU workloads retain their existing hardware and placement requirements |

OpenShell 0.0.116 is the latest published release verified during review on
2026-09-09. The supplied reference needs additional gateway and supervisor fixes.
Implementation must recheck authoritative releases and pin every selected artifact
exactly. No new dependency pin is introduced by this plan.

## Product And Installation Contract

Computers ships in the standard Veoveo release with its native API, Console page,
schema migrations, service images, installation package, diagnostics, and acceptance.
The capability has no optional-product enable flag. Access remains policy-controlled.
The installation supplies its compute capacity, storage budget, admitted templates,
provider connection, and trust material. Standard-install preflight verifies that
closure; a normal installation cannot silently omit the capability's dependencies.

Admission can be paused for maintenance. Capacity exhaustion and provider failure
produce visible Computers states. Computers readiness participates in installation
qualification, while unrelated gateway routes and services keep their own readiness.
Packaging includes the disconnected-installation artifact closure; local retained
operation cannot depend on a Veoveo-hosted service or an Internet image pull.

The first supported user profile provides one personal Computer per canonical owner.
An authorized human can create it, use an admitted development image, reconnect to
its running terminal, and Stop/Start over the same retained home. Stopped Computers
continue to reserve storage. Coding tools inside a personal Computer use their own
authorized credentials and network policy. Console login is not coding-tool login.

Personal Computers carry no provider administration, host container socket,
database administrator credential, cluster credential, image-signing authority, or
implicit production-promotion grant. Ordinary editing and bounded local tests run
inside the Computer. A separate authorized build worker performs image publication
and release operations against an exact committed revision.

Shared Computers, arbitrary user-chosen images, GPU passthrough, remote-editor
certification, and unattended factory jobs are later profiles. CPU development work
does not authorize CPU substitutes for rendering or GPU acceptance. Such work uses
an admitted hardware-GPU worker or reports that the required capability is absent.
The factory plan continues to govern disposable author/verifier jobs separately.

## User Journey

1. The signed-in user opens the native Computers page. It shows their Computer,
   admitted image and resources, storage retention, capacity, and permitted actions.
2. Create admits one durable operation. Navigation and reload recover the same
   Task. Uncertain responses retain the original request identity.
3. Connect opens the terminal when server-issued action flags permit it. Detaching
   or navigating away preserves the canonical process and background work.
4. Returning reconnects only when the user left the terminal connected. Explicit
   Disconnect stays disconnected. Replay drains before input and terminal replies
   are enabled.
5. Stop visibly settles through its Task. Start creates a new process over the
   same home, repositories, coding-tool state, and useful build caches.
6. Connect from CLI presents credential-free commands and an existing-SSO pairing
   flow. Ordinary access-token renewal preserves authorized work. Logout, revocation,
   or the absolute connection deadline closes attachment authority promptly.
7. Provider or storage trouble presents the known outcome and permitted recovery.
   Unknown remote effects remain locked to their original operation.

## Proposed Component Ownership

These are planned paths; component DESIGN.md and AGENTS.md files are added when
their implementations land. Each design names its supported protocols near the top.

| Owner | Planned path and responsibility |
|---|---|
| Public contracts | `platform/computers/contract/`: lifecycle, snapshot, action flags, terminal controls, CLI bootstrap/grants, typed errors, schema export |
| Domain | `platform/computers/`: canonical ownership, admission, retained templates, operation state, attachment leases, Tasks and store transactions |
| Service | `platform/computers/service/`: thin binary plus focused modules for lifecycle workers, internal routes, webhook settlement, and terminal execution |
| Provider runtime | `platform/runtimes/computers/`: exact OpenShell profile, generated clients, provider operation correlation, terminal and storage adapters |
| Storage service | `platform/computers/storage/`: maintained allocation/restore implementation, bounded capacity, single-writer enforcement and operational diagnostics |
| Gateway | Existing gateway modules: authenticate and authorize public/native operations; forward narrow internal authority to Computers |
| Console BFF | Focused `computers/` modules: existing session/CSRF integration, browser terminal and stock CLI edge transport |
| Console | `apps/console/web/src/computers/` and native view: generated types, controller, terminal state, status and recovery UI |
| Deployment | Existing component catalog, image targets and Helm packages: complete owned inputs, trust mounts, provider/storage dependencies and standard Computers closure |
| Acceptance | Focused Rust smoke harness: native fault tests, installed browser/CLI flows, replica routing, storage and release evidence |

The service owns lifecycle execution independently of gateway replicas. This limits
gateway build dependencies and prevents a Console/gateway rollout from restarting
Computers workers. Terminal relays remain bounded streams; gateways do not grow a
second domain controller. Artifact file movement uses the existing governed Artifact
service rather than another public upload system.

## Delivery Sequence

### 1. Close The Contracts And Reproduce The Provider Profile

Review the supplied source as port units against current main. Do not merge its
unrelated downstream history. Inventory omitted Task helpers, policy actions,
migration registration, router wiring, manifests, and build inputs before estimating
the port. Keep a dependency list with a concrete owner and acceptance for each item.

Publish three short decisions beside the relevant code: provider completion/recovery,
retained-home exclusion, and renewable CLI attachment. Generate every public DTO,
including CLI bootstrap, from the canonical exporter. Remove downstream names and
obsolete configuration fields when porting; assign migrations in our own ledger.

Reproduce the six OpenShell patches in their actual dependency graph. The gateway
and supervisor branch after the common third patch; they are distinct qualified
artifacts. Choose upstream-released equivalents when available, otherwise maintain
an explicit Veoveo provider patch set with exact source, license and artifact
provenance. Admission validates the complete supported tuple, including replay.

First support the reviewed Linux/amd64 Docker-backed retained profile. The Veoveo
control plane remains Kubernetes/Helm-installed. Document the compute host boundary
and package its provider/storage prerequisites; a Docker socket stays inside that
trusted boundary. Other OpenShell drivers require independent qualification.

Exit: reproducible provider artifacts and passing native retained-start/replay probes;
agreed contracts for unknown outcomes, storage ownership, and CLI renewal. This work
resolves the highest-risk assumptions before building the full UI.

### 2. Implement Durable Ownership And Lifecycle

Reuse canonical identity and existing Tasks. Persist Computer identity, selected
provider instance, retained home, immutable template, operation identity, worker
lease, and terminal attachments as separate typed concepts. Preserve owner checks
through list, Task reads, lifecycle operations, terminal grants, and CLI bootstrap.

Carry an indeterminate remote outcome explicitly. Fix the reference's conversion of
LifecycleUnknown/WatchFailed into ordinary terminal failure. A failed completion
stream cannot free the operation lock or permit another dispatch. Task publication
and acknowledgment must retain the original operation until its effect is settled.

Preserve the repository's webhook-only completion rule. The qualified provider
profile must correlate accepted operations and deliver authenticated completion
events through a durable outbox. A native-watch adapter is acceptable only when
its source supplies replay/checkpoint guarantees sufficient to survive disconnection;
otherwise add provider-side event journaling. Persist inbound events before replying,
deduplicate redelivery, and reject stale process/operation epochs. No recovery path
queries provider status to establish completion. A missing event exposes an unresolved
operation that needs event redelivery or documented maintenance recovery.

Reuse webhook-wait Task recovery after dispatch. Claims and bounded preparation may
resume from durable checkpoints; they cannot repeat an unknown remote effect. Keep
cancellation pending until dispatched work and home-writer authority are reconciled.

Exit: isolated real-store tests pass concurrent admission, changed-input retries,
cross-owner denial, worker lease loss, cancellation, duplicate/out-of-order webhooks,
lost acknowledgments, and service restart without redispatch or early lock release.

### 3. Ship The Retained-Home Service

Provide the actual allocator implementation missing from the archive. Qualify a
Linux filesystem-backed reference allocator with physically enforced capacity and
explicit reservation accounting. Select its filesystem/volume mechanism in the
provider/storage prototype; a byte limit in a DTO is not capacity enforcement.

Prepare initializes exactly one new home. Restore opens the same complete allocation
and refuses to seed, format, resize, or substitute another home. Bind allocation to
Computer identity and immutable template/storage fingerprints. Enforce owner UID/GID,
real ENOSPC behavior, restart restoration, and exclusive writer access. Store leases
alone cannot prove a stopped provider process: the allocator/provider boundary must
fence the old writer before granting another.

Account for active, stopped, and incomplete allocations. Bound logs and temporary
storage separately. Admit new allocations only when physical capacity and the free
space floor permit them. Publish a preservation and recovery runbook; routine cache
cleanup never removes homes or user state.

Exit: real storage tests cover failed preparation, duplicate frames, capacity
exhaustion, allocator and host restart, identity mismatch, and contested writers.
Actual files and useful compiler caches survive Stop/Start and restart restoration.
Host restarts, disk exhaustion and destructive fault injection use disposable
provider/storage fixtures with the candidate artifacts and containment profile.

### 4. Deliver The Browser Development Loop

Port native navigation, generated API types and the Computers controller. Use shared
SSE invalidations and canonical snapshots/Task results. Expose action flags from the
server. Show capacity, maintenance, disconnected, denied and unresolved-operation
states without requiring the user to understand provider internals.

Implement the terminal state machine separately from lifecycle UI. Preserve binary
bytes and explicit replay completion. Disable user input and automatic terminal
answerbacks while replay callbacks drain. Renew attachment authority before expiry;
an old callback cannot reconnect after explicit Disconnect, logout, or owner change.
Subscribe to revocation before baseline authorization and fail closed if its event
stream is lost. Bound slow readers/writers independently of authorization deadlines.

Exit: real headed hardware-browser acceptance proves edit/test, same-process
reattachment, replay after a query-emitting TUI, authorization renewal, navigation,
reload, intentional Disconnect, Stop/Start, keyboard use, and narrow recovery UI.

### 5. Make Stock CLI Sessions Usable For Real Work

Keep the admitted CLI unmodified. Preserve existing-SSO pairing, explicit code
confirmation, loopback callback validation, credential-free bootstrap, both tunnel
routes, and the narrow gRPC/SSH forwarding facade. Reject arbitrary methods, targets,
Computers, origins and forged session headers.

Replace the reference's static access-token-bound relay ticket with a reviewed
renewable attachment design. The proposed basis is an opaque, durable pairing grant
bound to owner, Computer, session family, policy and an absolute installation limit.
Short-lived internal authority renews only while that grant and its underlying
authority remain valid. Provider session renewal must preserve the established SSH
transport; prove that capability early and include a provider fix if required.
Expiry and revocation remain enforced. Merely extending a stale JWT is not renewal.

Store connection authority outside replica memory and share sealing-key rotation
through installation trust. CreateSshSession and ForwardTcp may land on different
replicas. A replica loss permits explicit reconnect; it does not imply that an
existing TCP connection can migrate without interruption.

Exit: real stock CLI through ingress, alternating BFF/gateway replicas, healthy
bursts and blocked writers; sustained work crosses actual token renewals without
fresh pairing. Revocation and absolute expiry close access promptly. An interrupted
ordinary SSH exec is reported honestly and is never rerun automatically.

### 6. Support Template Changes And Retained-Instance Maintenance

Changing the default template affects new Computers. Existing Computers, queued
operations and retries continue to use their recorded template and home. Retain
the exact referenced images and configuration; missing retained inputs block dispatch.

Implement replacement as a durable maintenance Task. Drain controllers and provider
work, checkpoint the exact source/target instance, fence the source writer, attach
the retained home, qualify policy and terminal behavior, then adopt the target.
Rollback first proves that the target writer stopped. Uncertain maintenance retains
its lock and checkpoints across worker restart.

Exit: cancellation, lost replies, restart at each checkpoint, default rollover,
upgrade and rollback preserve user data and never create concurrent home writers.

### 7. Package Computers As Core And Deploy The Bioma Configuration

Ship canonical service, provider and allocator image recipes; migrations, standard
Helm resources, configuration validation, trust references, ingress routes and
offline artifact closure. Declare ownership and complete inputs in existing build
and deployment contracts. Split independently updated deployment units only with
explicit ownership, including any required migration of existing objects.

Remove the reference feature-enable switch. Keep operational admission/maintenance
settings. Register core Console/API surfaces and installation diagnostics. Preflight
fails on missing required provider/storage closure before writes. Provider outages
make Computers unavailable without taking unrelated services out of readiness.

First qualify a clean installation with its own origin, identity, image, policy and
capacity. Then publish qualified immutable artifacts, update the Bioma-owned GitOps
configuration, and reconcile **Veoveo at veoveo.bioma.ai**. Verify actual running
digests and configuration. Record the complete candidate closure and installed results.

Exit: both clean installation and Bioma configuration pass the release matrix below.

## Build And Deployment Iteration Plan

Develop Console through its Vite/BFF loop. Use focused Rust package checks and one
stable checkout/target configuration per builder. Generate OpenShell clients offline
from hash-verified protocol inputs. Keep protocol/provider dependencies in the owning
runtime/service so a Console layout edit does not compile them.

Retain separate images and cache identities for Computers service, allocator,
OpenShell gateway, supervisor, and development templates. Rebuild only affected
artifacts. A provider patch should not rebuild Console; a new default development
image should not restart existing Computers. Publish once and deploy the exact
qualified digests. Use component-scoped receipts to prove which workloads changed.

| Representative change | Acceptance target |
|---|---|
| Console source edit | Vite refresh with no Rust build, image publication, or Pod restart |
| Console production assets | Reuse the existing BFF binary and all Computers/provider artifacts |
| Computers implementation | Focused package tests; rebuild and roll out only its affected service units |
| Provider repair | Build the affected provider branch, qualify its exact tuple, and preserve running retained instances until maintenance |
| New template default | Validate and publish configuration/image closure; retain old Computers' selected images and caches |
| No-op deployment | Reuse unchanged units and produce no rollout |

Record dependency/source preparation, compilation, linking, image export, push,
qualification, reconciliation and readiness separately. Initial targets are a warm
focused source check below 30 seconds and a warm component build/qualification/rollout
below two minutes, excluding provider rebuilds and sustained acceptance. These are
targets to measure, not performance claims. Report misses and their input/cache cause.

Use short configurable authorization lifetimes for routine fault suites, then run
one sustained public browser/CLI qualification across real access-token renewals.
Keep required functional acceptance in the release path. Record additional capacity
and throughput experiments separately so they do not become indefinite blockers.
Extend `BUILD_DEPLOY_ITERATION_AUDIT.md` with measured costs and avoidable retries.

## Release Acceptance Matrix

| Area | Required installed evidence |
|---|---|
| Core delivery | Standard release/Helm/offline closure includes Computers; clean install needs installation configuration only |
| Ownership | Two real principals, including similar display names, cannot read or control each other's Computer, Tasks, terminal or CLI grants |
| Replicas | At least two gateway and BFF replicas; cross-replica admission, pairing/session use, revocation and restart recovery |
| Lifecycle | Same-request retries preserve identity; unknown effects stay locked; webhook loss/redelivery, lease loss and cancellation preserve exact operation authority |
| Browser | Headed hardware GPU proof, native page, keyboard/narrow UI, real editing/testing, safe replay, reload/navigation, renewal and intentional Disconnect |
| CLI | Stock client on a clean machine through real ingress; sustained session renewal, root tunnel, restricted forwarding, bounded stalls and revocation |
| Retention | Files, repository state, coding-tool state and useful caches survive Stop/Start; new process identity is observed |
| Storage | Real ENOSPC, incomplete allocation, single writer, service/host restart and documented restoration; no silent reseed |
| Maintenance | Immutable template rollover, worker/controller drain, replacement checkpoints and rollback with preserved homes |
| Isolation | Admitted containment and egress; personal shell cannot obtain host sockets, provider administration or build/release authority |
| Runtime health | Actual source/configuration/image closure, Computer capability readiness and continued unrelated service availability during a provider fault |
| Iteration | Representative changed-component and no-op runs record cache use, build timings and actual changed workload inventories |

All smoke lifecycle, assertions, retries and cleanup stay in Rust. Record affected
checks with `cargo xtask test-report run`, inspect `show`, and commit passing current
evidence with each build-input change. Each implementation checkpoint is a coherent
commit; qualification receipts identify mocked, native and installed evidence separately.

The capability is complete when the full supported personal browser and CLI journey,
retained storage and recovery, core installation packaging, and Bioma deployment pass.
A browser-first milestone is useful progress; it does not finish the Computers goal.

## Reviewed Handoff

Source package: `veoveo-openshell-handoff-2026-09-09.zip`, SHA-256
`b330a4016a25d182e206421c4eb019a1cd2fc0ae94e40b7d2056dcd654cba061`.
The package selects downstream source revision
`396a71200470043a5e7b2f6c3a855166c63e52e1` against Veoveo baseline
`11f59d55b487a4fa856f73cb31a498c7fb7e6d30`. Its listed checksums passed review.
Its selected source is not a buildable checkout. Reported private runtime results
are useful context and do not substitute for the candidate's acceptance evidence.

Authoritative upstream baseline:
[OpenShell 0.0.116](https://github.com/NVIDIA/OpenShell/releases/tag/v0.0.116).
