# Artifact Service

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Internal HTTP | JSON metadata and capability control; streamed GET/HEAD downloads with the existing single-range profile |
| Gateway identity | Forwarded, verified short-lived internal assertion for ordinary operations and capability issuance/revocation |
| Upload identity | Dedicated `artifact-upload` EdDSA assertion includes the checked control-plane SHA-256 and Work Context digest; ordinary forwarded server tokens do not authorize uploads |
| Veoveo Artifact read delegation | Repository-owned internal API, opaque UUIDv7 capability and task identities, bearer secret confined to task-read routes |
| Persistence | Typed platform Store records and ordered SurrealQL migrations through `0050`; native durability acceptance uses SurrealDB 3.2.4 |
| Content and credential identity | SHA-256 for immutable blobs and domain-separated secret hashes |
| S3 multipart adapter | `object_store` 0.14.1 low-level `MultipartStore`, one-based public parts mapped to zero-based adapter indices; private provider handles and receipts |
| S3 reconciliation | General-purpose S3 `ListMultipartUploads` and `ListParts`, SigV4 through the storage SDK; bounded XML decoding with `quick-xml` 0.42.0 |
| Veoveo upload ledger functions | `fn::artifact_upload_profile_digest` and `fn::artifact_upload_authority_matches` bind transactions to current profile and Work Context policy |

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

`GatewayProfile.artifact_upload` holds the explicit typed policy. An absent policy
disables uploads. Part deadlines are at most one hour; Store reserves a short margin
after the request deadline before reclaiming its lease. The upload assertion binds
the gateway's policy decision to the active control-plane identity and current Work
Context. A stale assertion cannot admit bytes while a replica refreshes configuration.

The focused `uploads/` modules orchestrate admission, transfer, public projections,
and recovery. A dropped request schedules release of its part lease; process failure
leaves the lease for durable recovery. Two local background workers bound simultaneous
whole-object verification streams. Session lease renewal checks terminal state and
current authority while verification proceeds. S3 enumeration reconciles uncertain
create and part acknowledgements; integrated HTTP acceptance precedes public activation.

Blob registration is immutable by tenant and whole-file SHA-256. The shared Store
transaction inserts a new mapping or retains the existing mapping, including its
object key and creation time. A length conflict rolls back publication. Concurrent
transaction retries preserve this rule for ordinary writes and recording publication
as well as uploads. The retained mapping returned by Store is authoritative; upload
cleanup may remove only its unreferenced losing object.

Native validation uses SurrealDB 3.2.4. SQL formatting uses
`@surrealdb/surql-fmt@0.1.0-beta.2`, the latest published formatter; upstream has no
stable formatter release. The formatter does not supply execution evidence.

Migration `0050` stores upload admission, part descriptors, recovery leases, and
tenant/global transfer accounting. Admission serializes through the tenant usage row;
it includes retained cleanup bytes and committed tenant storage in quota decisions.
Request replay is scoped to tenant, actor, profile, and Work Context. Native admission
tests reserve 10 GiB without transferring bytes; transfer acceptance remains separate.
Current profile/policy and Work Context digests bind admission to its governing rules.
The repository-owned `fn::artifact_upload_profile_digest` and
`fn::artifact_upload_authority_matches` functions run inside state transactions.

Part admission fixes number, byte length, and SHA-256 before reading a body. A leased
generation owns its acknowledgement; stale generations cannot replace a receipt.
Tenant counters and a shared installation memory row bound simultaneous payloads
across replicas. Unknown-length streams reserve another bounded part window before
exceeding their current reservation. Failed requests release transfer budget while
preserving the immutable descriptor for a matching retry.

Finalization freezes a complete ordered manifest in Store before touching S3 completion.
Session leases carry a generation; takeover invalidates prior workers. The publication
transaction checks current authority and the active verification lease, then commits
the immutable blob mapping, governed occurrence and grants, completion audit, outbox,
and durable receipt fields together. Matching publication replay retains the same
occurrence and completion timestamp. Ordinary writes and upload publication share the
typed content builder and SQL registration fragment.

Cancellation and failure convert reserved quota into retained cleanup debt. They keep
active request leases because bytes may still be in flight. Cleanup becomes eligible
only after those leases have stopped, and the unique object has no retained blob
mapping. Physical deletion precedes the transaction that releases cleanup debt.
Duplicate publication charges its losing object until deletion succeeds. A bounded
keyset recovery scan finds interrupted finalization and pending cleanup across replicas.
Native lifecycle tests qualify these transactions separately from physical S3 cleanup.

The S3 adapter reconstructs uploads from the ledger's private handle and ordered part
receipts. It materializes one bounded part, coalesces tiny frames into 64 KiB blocks,
and retains larger byte chunks without another payload copy. HTTP/socket and chunk
metadata add bounded overhead to the in-flight payload budget. Whole-file identity
comes from one separately bounded sequential object read after completion. A HEAD at
the upload's unique object key recovers a lost storage completion acknowledgement;
it never substitutes for the whole-file hash check. In-memory adapter tests establish
these mechanics, while real S3 and multi-GB acceptance remain rollout requirements.
Initialization removes abandoned handles at the session's exact unique key before
creating a replacement. Finalization reconciles ordered provider receipts against
accepted part numbers and sizes, while whole-file verification remains mandatory.
Enumeration uses 64-entry pages, at most 1 MiB of XML per response, and the SDK's SigV4
signer. It never deletes the completed object while removing unfinished handles.
`quick-xml` 0.42.0 is the latest stable release verified from its
[upstream crate documentation](https://docs.rs/quick-xml/0.42.0/quick_xml/).
The native `s3_upload` integration target owns a port-forward to the selected installed
RustFS service and an isolated object prefix. Credentials stay in process memory.
Set `VEOVEO_UPLOAD_S3_CONTEXT` to qualify lost acknowledgements, restored handles,
part enumeration, completion replay, exact SHA-256, and physical cleanup.
S3 connections retain the SDK's connection timeout and use a 30-second read-idle
timeout. Whole-object streams have no total request timeout; transfer handlers bound
each part independently. The SDK's default 30-second total timeout would otherwise
turn transfer duration into an implicit maximum file size.

The beta formatter corrupts nested schema field paths and nested conditional updates.
Keep these statements in the established SurrealQL format and validate them with the
pinned native database executable. Rejected formatter output supplies no execution
evidence.

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
