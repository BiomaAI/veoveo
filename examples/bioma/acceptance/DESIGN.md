# Bioma Installation Acceptance

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo configuration | Repository-owned typed gateway and deployment contracts, validated against the Bioma installation |
| SurrealDB 3.2.4 | Native Rust SDK, parameterized SurrealQL transactions and the platform-store schema; recovery exports use native SQL values |
| Kubernetes | Existing Deployment, Secret, PersistentVolume and PersistentVolumeClaim APIs; retained local-path storage is installation-owned |
| Pilot migration | Private `bioma-pilot-cutover/v1` evidence and four explicit adoption records; this is an installation procedure, not a public API |
| Archive recovery | POSIX tar contents and file metadata, checked by the Rust recovery test |

## Ownership

This crate validates the Bioma reference installation. Generic agent authoring and
runtime lifecycle remain in the gateway, platform store and agent manager. No Bioma
identity or migration procedure enters those components.

`pilot_cutover` performs the one-time ownership transfer of the four existing UAV
pilots. Its input binds each retained runtime and OAuth principal to the published
managed definition. The transaction creates only lifecycle records and their audit
events. Runtime records, grants, wakes, Tasks and memory stay in place.

## Cutover And Recovery

The old UAV Helm release is suspended and its four pilot workloads are drained
before taking the targeted export. Each physical volume must use `Retain`.
`pilot_recovery` restores each archive into a disposable directory on the same node
and compares contents, permissions, ownership, timestamps and links.

The record rehearsal reads the installation through native database credentials,
exports only the referenced records and vehicle grants, then restores them into an
isolated database. It exercises atomic adoption and rejects a retry after a managed
generation has advanced. Private SQL and the reviewed adoption plan are created only
after this rehearsal succeeds. They stay in the installation's private output directory.

Transfer each retained volume to its recorded managed claim and copy the same signing
key into its immutable managed Secret. Confirm source workloads have no Pods before
releasing an old claim. A missing retained volume or key requires operator recovery;
the procedure must never substitute empty storage or a new identity.

Adopt the pilots paused. Remove the four static OAuth registrations and per-pilot
Helm resources, then resume through the normal management API. The manager now owns
each instance's workload; Helm installs the reviewed template and simulator.

Transfer checks require the old release to remain suspended with its pilot Deployments
drained. Installed checks instead require reconciliation to be active and the old
Deployments absent. Both compare retained physical volumes and signing keys with the
frozen migration evidence. The installed check permits the currently approved kernel
image, verifies the template digest and keeps every other resource and identity binding
equal to the original plan.

Before new managed writes, coordinated recovery may restore the frozen configuration
and records. After resume, use forward repair that preserves new work. The retained
exports do not authorize overwriting subsequent edits, grants or memory.

## Qualification

Ordinary composition tests run with `cargo test -p veoveo-bioma-acceptance`.
Installed migration tests are explicitly ignored by default. The operator supplies
the private export directory and database connection through environment variables;
credentials are never written to the source tree or printed as command arguments.
Record each installed command through `cargo xtask test-report run` before committing
its implementation and evidence.
