# Artifact Service

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Internal HTTP | JSON metadata and capability control; streamed GET/HEAD downloads with the existing single-range profile |
| Gateway identity | Forwarded, verified short-lived internal assertion for ordinary operations and capability issuance/revocation |
| Veoveo Artifact read delegation | Repository-owned internal API, opaque UUIDv7 capability and task identities, bearer secret confined to task-read routes |
| Persistence | Typed platform Store records and ordered SurrealQL migration `0049`; native delegation acceptance uses SurrealDB 3.2.4 |
| Content and credential identity | SHA-256 for immutable blobs and domain-separated secret hashes |

This service implements the internal Artifact plane. `servers/artifact-mcp` owns its
public MCP projection. The shared request types live in `mcp/contract`; the HTTP
client lives in `platform/artifacts/client`.

## Resumable Upload Contract

The public upload contract is defined in
`mcp/contract/src/artifact_service/upload.rs`; its implementation is tracked in
[`ARTIFACT_UPLOAD_PLAN.md`](../../../docs/ARTIFACT_UPLOAD_PLAN.md). The typed contract
requires an explicit installation quota and transfer policy, validates MIME admission,
and negotiates part size within the S3 multipart profile. The `artifact_upload` gateway
action has no MCP method. Public routes remain disabled until durable storage and
gateway authorization are activated.

## Task Read Delegation

A durable task can retain a bounded read capability instead of retaining a submitted
gateway bearer. Issuance requires a currently valid gateway identity whose Work
Context policy revision matches the current Store record. The capability captures
that identity's tenant, actor, server, profile, memberships and label clearance.
These signed claims are the authority ceiling for the capability lifetime. Identity
provider group changes require revocation or expiry; the service does not refresh
identity-provider claims in the background.

The Store persists only the domain-separated secret hash. The issued secret is a
credential and must remain in protected task state. It authenticates only the read
routes below and carries no write, share, enumeration or capability-issuance power.
Every read requires the capability ID, secret and exact task ID. Possession of those
values delegates the read; the task ID is a binding, not a second authenticator.

| Operation | Internal route | Credential |
|---|---|---|
| Issue | `POST /artifact-read-capabilities` | Gateway assertion |
| Current scope | `GET /artifact-read-capabilities/{capability}?task_id=…` | Task read secret |
| Revoke | `DELETE /artifact-read-capabilities/{capability}` | Issuing tenant and actor's gateway assertion |
| Metadata | `GET /artifact-read-capabilities/{capability}/artifacts/{artifact}/meta?task_id=…` | Task read secret |
| Bytes | `GET` or `HEAD /artifact-read-capabilities/{capability}/artifacts/{artifact}/download?task_id=…` | Task read secret |

Issuance sets an explicit deadline within 24 hours, a distinct-occurrence count of at
most 10,000, and a positive byte limit within signed 64-bit storage. The ledger
charges an occurrence's full byte length once, including metadata admission.
Repeated metadata, downloads and ranges reuse that reservation. Concurrent service
instances share atomic quota accounting. These limits bound the input set; they do
not cap repeated network transfer of the same admitted bytes.

Every metadata or download request reloads the occurrence and evaluates its current
grants, labels, tenant and retention state. A prior admission grants no access by
itself. Both initial admission and retries require an unexpired, unrevoked capability
and an unchanged Work Context policy. The latter compares a database-computed digest
of the policy revision, membership rules and output policy. Titles and timestamps
are excluded, allowing configuration reconciliation that preserves policy content.
Creation compares that digest again transactionally to reject a concurrent change.

`service/read_capability.rs` owns policy and audit behavior. The focused ledger trait
and its Surreal adapter own durable delegation; `platform/store/src/artifact_reads`
owns schema-aware queries and quota updates. The client selects either an existing
gateway caller or a task capability through `ArtifactReadAuthority`; it never signs
a gateway identity.

## Verification And Activation

Unit and native HTTP tests cover current grant revocation, tenant and label denials,
task/secret mismatch, expiry, owner revocation, quotas and credential separation.
The ignored native test
`artifact_read_delegation_survives_service_recreation_and_enforces_native_atomic_state`
starts and reaps an isolated SurrealDB 3.2.4 process. Set `VEOVEO_SURREAL_BINARY` to
that exact executable. It recreates service instances, races byte admissions, checks
persisted authority fields, and distinguishes timestamp updates from policy changes.

The SQL formatter `@surrealdb/surql-fmt` has no stable release at this checkpoint.
Its latest published `0.1.0-beta.2` formats the query files but corrupts dotted field
paths in this migration. The migration therefore retains manually checked field
paths and passes the native 3.2.4 parser and database application test.

Stream and Reason persist the capability in protected task state and supply it to
the recording reader. The scope endpoint checks current validity without admitting
an occurrence. The reader uses that verified actor and byte ceiling before reading
catalog state or copying live parts that have no Artifact occurrence yet. It does
not convert that scope into a gateway identity. Hardware workload acceptance remains
required before admitting the common compiler images.
