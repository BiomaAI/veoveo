# Bioma Installation Acceptance

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo configuration | Repository-owned typed gateway and deployment contracts, validated against the Bioma installation |
| SurrealDB 3.2.4 | Native Rust SDK, parameterized SurrealQL transactions and the platform-store schema; recovery exports use native SQL values |
| Kubernetes | Existing Deployment, Secret, PersistentVolume and PersistentVolumeClaim APIs; retained local-path storage is installation-owned |
| Pilot migration | Private `bioma-pilot-cutover/v1` evidence and four explicit adoption records; this is an installation procedure, not a public API |
| Definition consolidation | Private `bioma-pilot-consolidation/v1` transaction for four paused, drained instances; no compatibility adapter or public rebinding API |
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

## Definition Consolidation

`pilot_consolidation` replaces the four vehicle-specific definitions with the shared
`uav-pilot` publication. The four existing managed instances keep their identities,
public keys, signing Secrets and physical memory volumes. Vehicle control grants
continue to select each pilot’s vehicle. The template accepts only a session.

The operator rehearses the transaction against a disposable SurrealDB 3.2.4 copy of
the relevant installation records. The test rejects a stale fourth instance and a
live runtime lease, checks transaction rollback, and verifies that replay cannot
replace a subsequent generation. Exported records contain no private signing keys.

For the live cut, pause the instances through the API, wait for their episodes to
finish, then disable the old definitions. The manager removes the workloads. Wait
until no Deployment, Pod using a retained claim, or runtime lease survives. Install
the new immutable template and approve it in the gateway and manager. Publish the
shared definition through the ordinary API. The consolidation test records private
before-images and Kubernetes resource UIDs, then changes all four references in one
transaction. It advances generation and dispatch epoch and supersedes controller
claims. The retained active-generation marker prevents recreation of missing memory.

Resume through the ordinary API and verify all four instances are Ready on the
shared revision. Compare the original Secret UID, PVC UID, physical volume, public
key and principal. Archive the old definitions after verification; historical
revisions remain available to recorded episodes.

Recovery before resume uses the saved before-images to restore the old definition,
requested revision and template references under another drained transaction. Keep
generation and dispatch epoch increasing, supersede the consolidation operation and
create a fresh paused operation. Reinstall the prior approved template before
enabling the old definitions and resuming. Never restore an entire stale instance
or reduce its generation. After resume, repair forward under the usual pause/drain
procedure; memory and new work are never rolled back. This cut does not copy or
convert memory data and introduces no archive or backup service.
