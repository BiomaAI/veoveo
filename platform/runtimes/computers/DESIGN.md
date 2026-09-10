# Computers Provider Runtime

Status: the native lifecycle and retained-terminal fixture passes. No Computer service
or installed provider is qualified by this source checkpoint. The owning domain is planned in
[Computers](../../../docs/COMPUTERS_PLAN.md).

## Standards And Protocols

| Boundary | Selected profile |
|---|---|
| OpenShell `0.0.116` protobuf/gRPC | Vendored protocol verified by `protocol/SHA256SUMS`; internal generated client/server types over mTLS |
| OpenShell retained Docker provider | Candidate gateway `0.0.117-dev.6+g32efe0b` and supervisor `0.0.117-dev.5+gea0c605`; exact patch graph in `provider-patches/manifest.json`; native installation qualification pending |
| SSH and terminal metadata | Byte-preserving canonical-main attachment with private `openshell.terminal.v1` ReplayComplete metadata; explicit history fence |
| `veoveo.io/computer-storage/v1` | Bounded mTLS allocation/restore adapter generated from `protocol/storage.json`; allocator implementation and physical volume qualification pending |
| Protocol Buffers canonical encoding and SHA-256 | Template and immutable binding fingerprints with cross-language fixtures |
| Internal lifecycle checkpoint JSON version 1 | Closed validated operation/provider/binding identity and pre-dispatch process epoch; serialized for the owning durable Task |
| Rust/Tonic/Prost | Qualified workspace Tonic `0.14.6` and Prost `0.14.4`; new generator Tonic-Prost-Build `0.14.6`, Russh `0.63.3`, Typify `0.8.0` |

Upstream stable versions were checked through crates.io and the NVIDIA GitHub
release API on 2026-09-09. Workspace `tokio-rustls` remains on its qualified
`0.26.4` pin; advancing it to `0.26.5` is independent of this adapter port.
OpenShell `0.0.116` remains the latest published release at this checkpoint.
The provider patches implement retained restart, process groups, owned terminals,
exit-event correlation, log bounds, and explicit replay completion. They require
their own qualification and upstream/removal tracking before release.

Russh enables the existing Ring crypto backend and omits its unused RSA/compression
defaults. Canonical-main pins an Ed25519 host identity. The stable Russh release
requires the prerelease `ssh-key` version recorded in Cargo.lock; this transitive
constraint remains part of provider qualification until upstream ships a compatible
stable key crate. Typify's procedural macro is disabled because this crate uses only
its build-time type generator. Generated schema regex validation requires Regress
`0.12.0`, verified from crates.io at the same checkpoint.

## Ownership

`client` handles provider transport and lifecycle observation. `models`, `binding`,
`canonical` and `policy_json` validate the admitted template and exact identity.
`terminal` and `terminal_output` own the byte stream and replay boundary. `execution`
owns bounded command streams. `allocation` and `storage` bind retained volumes;
`policy_continuity` checks retained-instance replacement authority.

The gateway and BFF must not depend on this crate. The Computers worker applies
canonical authorization and shared durable Task transactions before invoking it.
The adapter returns typed outcomes without secrets or provider text in errors.

## Completion And Recovery

Healthy lifecycle observation uses native WatchSandbox. The supplied event/log tails
are best effort and lag warnings do not establish complete history. Transport loss
returns uncertainty and never proves a failed mutation. `recovery` implements one
bounded, identity-correlated reconciliation read under
[CE-01](../../../docs/CONTRACT_EVOLUTION.md#ce-01-provider-completion-follows-qualified-semantics).

Create uses deterministic binding identity. Start/Stop must preserve the selected
provider instance and run epoch. A fresh snapshot proves only the state it contains;
it cannot certify an arbitrary past command. The owning durable operation remains
fenced until its effect can be settled. Cancellation is distinct from stopping a
provider process, and a terminal transport deadline is distinct from Computer lifetime.

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
