# Bioma Installation Acceptance

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo configuration | Repository-owned typed gateway and deployment contracts, validated against the Bioma installation |
| SurrealDB 3.3.0 | Native Rust SDK, parameterized SurrealQL transactions and the platform-store schema; recovery exports use native SQL values |
| Kubernetes | Existing Deployment, Secret, PersistentVolume and PersistentVolumeClaim APIs; retained local-path storage is installation-owned |

## Ownership

This crate validates the Bioma reference installation. Generic agent authoring and
runtime lifecycle remain in the gateway, platform store and agent manager. No Bioma
identity or migration procedure enters those components.

`knowledge_config` checks that the installation's indexer can discover its approved
sources, that every registered tool call is denied, and that its membership ends at
the Operations viewer role. User profiles expose Knowledge reads independently of
the machine's indexing profile. Collection approvals belong to this installation;
server protocol declarations are compared with the generic development catalog.

`record_restore` qualifies bound single-record insertion and transactional batch
restore in disposable database fixtures. It compares complete native record values,
rejects a conflicting final record, and checks that the failed transaction leaves
no partial writes. Its SQL lives under `tests/queries/record_restore`.

## Qualification

Run `cargo test -p veoveo-bioma-acceptance` for composition and native fixture checks.
The database harness provides isolated credentials, timeouts and owned cleanup.
No private installation exports or running pilot workloads are prerequisites for
these checks.
