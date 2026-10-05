# Artifact Service

## Standards And Protocols

Artifact ledger targets use the domain contract's `ArtifactLedgerAddress` builder.
Access-request and read-capability denials commit an individual record before
returning. Authenticated records carry the verified actor; an invalid capability
records the requested object without claiming the capability's issuing actor.

| Boundary | Supported profile |
|---|---|
| Internal HTTP | JSON metadata and capability control; streamed GET/HEAD downloads with the existing single-range profile |
| HTTP Content-Disposition, [RFC 6266](https://www.rfc-editor.org/rfc/rfc6266.html), and [RFC 8187](https://www.rfc-editor.org/rfc/rfc8187.html) | Attachment delivery with the authorized occurrence's UTF-8 `filename*`; full, single-range and HEAD responses share the same presentation |
| Gateway identity | Forwarded, verified short-lived internal assertion for ordinary operations and capability issuance/revocation |
| Upload identity | Dedicated `artifact-upload` EdDSA assertion includes the checked control-plane SHA-256 and Work Context digest; ordinary forwarded server tokens do not authorize uploads |
| Veoveo Artifact read delegation | Repository-owned internal API, opaque UUIDv7 capability and task identities, bearer secret confined to task-read routes |
| Persistence | Typed platform Store records and the current Artifacts owner lane; native durability acceptance uses SurrealDB 3.3.0 |
| Content and credential identity | SHA-256 for immutable blobs and domain-separated secret hashes |
| Local blob storage | `object_store` 0.14.1 filesystem profile; opaque files with HTTP delivery headers supplied by the Artifact service, without unsupported object attributes |
| S3 multipart adapter | `object_store` 0.14.1 low-level `MultipartStore`, one-based public parts mapped to zero-based adapter indices; private provider handles and receipts |
| S3 reconciliation | General-purpose S3 `ListMultipartUploads` and `ListParts`, SigV4 through the storage SDK; bounded XML decoding with `quick-xml` 0.42.0 |
| Veoveo upload ledger functions | `fn::artifact_upload_profile_digest` and `fn::artifact_upload_authority_matches` bind transactions to current profile and Work Context policy |

This service implements the internal Artifact plane. `servers/artifact-mcp` owns its
public MCP projection. Occurrence identity and metadata come from the lightweight
[`platform/artifacts/contract`](../contract/DESIGN.md) library. The shared request
types and access evaluator live in `mcp/contract`; the HTTP client lives in
`platform/artifacts/client`.

Resolution and streaming client methods accept the domain's `ArtifactUri`. HTTP query
decoding validates external addresses before service resolution. The occurrence comes
from that parsed address; authorization still uses the current caller and ledger.
Metadata derives its ID from the URI and rejects mismatched pairs on wire decoding.
The domain's [identity and address profile](../contract/DESIGN.md#address-and-identity-profile)
declares supported spellings and identity agreement.

Occurrence publication and repository reads construct Artifact provenance from the
checked invocation authority. The domain contract preserves the flat metadata wire
profile while using `InvocationProvenance` internally. Stored authority reconstruction
rejects contradictory or incomplete attribution before producing metadata; it never
infers a missing initiator or delegation identity. The model's
[attribution wire profile](../contract/DESIGN.md#attribution-wire-profile) specifies
the supported mode and identity combinations.

## Artifact Metadata And Discovery

`ArtifactReadAuthority` carries the verified tenant and principal, group memberships,
clearance and selected Work Context into exact metadata reads and `ArtifactListQuery`.
Store's `ArtifactReadScope` converts those typed identities at the database boundary. The occurrence query applies
tenant, classification and labels, retention deadline and either a live subject grant
or the selected context before decoding rows and applying the page limit. Indexed
context and stored invocation context must agree. Every group role can confer read
access, so read discovery needs the group identities; higher access levels still use
the shared evaluator.

Exact reads and pages use the same parameterized SQL predicate. One database statement
selects the admitted occurrence and its blob, tenant and grant records. The repository
rejects disagreement between native record links and stored identity or authority.
`ArtifactReadScope::ADMISSION` exposes that predicate for subqueries over
`artifact_occurrence`; `bind` supplies its typed caller parameters. Domain readers can
join records to readable outputs before pagination while Artifact keeps ownership of
tenant, clearance, context, grant and retention rules.

`ArtifactPlane::metadata_snapshot` and authenticated `GET /artifacts/{id}/snapshot`
return a domain-owned `ArtifactMetadataSnapshot`. The snapshot contains neutral
metadata, read subjects with their expiry, and the stored metadata update time. These
read subjects let an authorized consumer preserve source access when indexing metadata.
Grant mutation and administrative grant listing retain their existing authorization.
The snapshot carries no plane download location or bearer share link. The metadata
update time describes metadata changes; it does not claim who last modified a record
or when grants changed. Index revisions must include the observed access state.

Ordinary `head` uses this admitted read. An absent, expired or inaccessible occurrence
returns `NotFound` and commits a denied inspection audit record without decoding its
payload. The shared pure evaluator takes an explicit evaluation time and rejects expired
grants. A matching selected Work Context can independently confer access.

Pages read one extra admitted occurrence and expose a continuation only when that row
exists. Selected aggregates are checked with `access::decide` before delivery. A policy
disagreement or concurrent revocation fails the page and permits a fresh read. The
service does not scan and discard denied candidates in Rust.

`service/tests/discovery.rs` qualifies this path with an owned SurrealDB 3.3.0 process.
It covers direct and group grants, selected-context access, foreign tenants, clearance,
expired retention and grants, exact metadata reads, snapshot release/grant changes,
and malformed denied rows ahead of the visible page. HTTP tests exercise snapshot
authentication and client round trips. Contract tests check snapshot identity and wire
admission; access tests cover direct and group grant expiry at the exact deadline.
Run it with `VEOVEO_SURREAL_BINARY` set to the qualified executable and
`cargo test -p veoveo-artifact-service --lib discovery -- --include-ignored`.

## Blob Backend Profiles

Deleting an occurrence cascades its `share_link` children. SurrealDB also removes its
grant graph edges. A failed deletion transaction preserves the occurrence and both
kinds of child. Grant replacement and explicit revocation keep their policy and audit
transactions. Blob storage has an independent lifetime because multiple occurrences
can reference the same bytes; physical cleanup belongs to the storage owner.
Occurrence, blob and access-request changefeeds retain prior rows for Console's tenant
filter. Grant and share feeds retain the occurrence parent for deletion notifications.
The shared decoder reads a grant's graph `in` endpoint and a share's `artifact` field.
Native Store qualification exercises their creation and deletion on the full schema.

S3 and memory stores receive private object attributes for cache policy, content
presentation and disposition. The explicitly selected filesystem profile omits those
attributes because its upstream adapter rejects them. This selection happens before
I/O; an unsupported write never triggers a backend fallback. Governed HTTP delivery
supplies its response headers independently. Filesystem tests write complete and
verified streamed objects, reopen the store and check bytes and range reads.
Filesystem storage does not admit the resumable S3 upload profile.

`http/disposition.rs` encodes the authorized occurrence's filename for HTTP download.
The name belongs to the occurrence because identical blob bytes may have different
names. UTF-8 extended parameters preserve spaces and non-ASCII names; percent encoding
keeps quotes, delimiters and control bytes out of the header structure. Existing write
admission requires a bounded basename. An unnamed occurrence carries attachment
disposition without a filename. Gateway and Console transport preserve this header;
the download path never needs a second metadata request.
The internal `x-artifact-put` descriptor is decoded as UTF-8 JSON bytes. HTTP's ASCII
header accessor cannot decode non-ASCII names; JSON decoding also rejects malformed
UTF-8 before write admission. The transport test publishes a Unicode filename through
the ordinary client and verifies it on full, range and HEAD download responses.

## Audit

`service/audit.rs` builds closed Artifact activities and checked targets. Grant outcomes
include the typed recipient and permission; release outcomes carry the release enum.
Access requests, read/write capabilities and shares use private platform addresses built
from their distinct IDs. These ledger addresses do not advertise MCP resources.

Ordinary calls derive actor, authority, request and trace identity from the verified
internal assertion. Upload admission persists that context; finalization freezes the
completion caller's context before the worker publishes the occurrence. Task capabilities
retain their issuing authority. Public share downloads identify the validated share link
as a capability actor without recording its secret or claiming gateway authority.

Every denial requires an acknowledged audit write. Downloads record the first allowed
request per actor, artifact and five-minute UTC window. A replica caches at most 4,096
window acknowledgements and coalesces concurrent first requests. Failed commits leave
the window retryable. Store claims the window and writes its record in one transaction,
which also deduplicates requests after eviction or on another replica. Policy is evaluated
for every download before the cache is consulted.

Mutation completions enter the shared retry queue. Capability issuance requires its
record before exposing the capability. Upload publication appends its completion in the
occurrence transaction. The owning process must drain the shared writer on shutdown.

## Resumable Upload Contract

The public upload contract is defined in
`mcp/contract/src/artifact_service/upload.rs`. The typed contract
requires an explicit installation quota and transfer policy, validates MIME admission,
and negotiates part size within the S3 multipart profile. The `artifact_upload` gateway
action has no MCP method. Public routes require both durable S3 storage and an
explicit profile upload policy.

`http/uploads.rs` owns the internal `/artifact-uploads` route group. Its upload-only
verifier checks the signed assertion before consuming JSON or part bytes. JSON control
bodies have a 16 KiB bound; part bodies use the negotiated layout and streaming counter.
The S3 service starts durable recovery at boot. Memory and filesystem installations
do not mount resumable upload routes.

The Gateway exposes `/artifacts/{profile}/upload-policy` and
`/artifacts/{profile}/uploads`, with upload-ID status/cancellation, numbered part PUT,
and completion subroutes. It audits `artifact_upload` against the Artifact server,
requires the upload scope and contributor membership, and compares its loaded
configuration identity with Store before signing an assertion. Policy discovery
returns a readable disabled explanation when authenticated callers lack upload access.
The service supplies Store with an `ArtifactUploadOwner` containing typed tenant,
actor, profile, Work Context, issuer and subject values. Status, part admission,
completion and cancellation select all six identities in SQL before decoding the
upload record. Stored tenant, principal and Work Context links must agree with those
identities. Current policy validation follows ownership selection. Trusted background
recovery has a separate unscoped inspection path.
Only selected content and part headers pass through the proxy. Internal assertions,
storage handles, and redirect locations are never accepted from public callers.

The Console BFF exposes `/console/api/artifact-uploads`, with `/policy` discovery and
the same session subroutes. The cookie session selects the Gateway profile and current
bearer token; existing CSRF middleware protects every mutation. Both proxies stream
request bodies. Artifact downloads now use the Gateway's streaming HTTP client with
connection and idle limits, removing the authentication client's ten-second total
deadline from file transfers.

Console snapshot sizes resolve blobs referenced by the selected artifact occurrences.
The blob query has no independent window that could discard their metadata. Live
replay loads an older referenced blob when a new occurrence reuses it outside the
initial snapshot. Missing metadata becomes an explicit null size; zero is reserved
for an actual empty object. Negative sizes and integers beyond the browser's exact
range are rejected before serialization.

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

Native validation uses SurrealDB 3.3.0. SQL formatting uses
`@surrealdb/surql-fmt@0.1.0-beta.2`, the latest published formatter; upstream has no
stable formatter release. The formatter does not supply execution evidence.

The Artifacts owner lane stores upload admission, part descriptors, recovery leases, and
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
across replicas. The fresh Artifacts lane creates `artifact_upload_memory:global`
with zero inflight bytes. Recorded lane replay preserves the existing counter. Unknown-length streams reserve another bounded part window before
exceeding their current reservation. Failed requests release transfer budget while
preserving the immutable descriptor for a matching retry.

Finalization freezes a complete ordered manifest in Store before touching S3 completion.
Session leases carry a generation; takeover invalidates prior workers. The publication
transaction checks current authority and the active verification lease, then commits
the immutable blob mapping, governed occurrence and grants, unified completion audit,
and durable receipt fields together. The audit append runs inside the existing publication
transaction. Matching publication replay retains the same
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

## Task Output Labels

`service/write_capability.rs` issues and redeems the existing bounded Task-write
capability. Issuance accepts `required_data_labels`, an optional set of at most 256
labels that every output must retain. Unknown issuance fields are rejected. A domain
can add the labels inherited from its inputs; it cannot change the output owner or
initial grants through this field. The caller's clearance must cover the complete
required set and the governing Work Context output labels.

The service stores the required labels inside the capability's existing output-policy
record. Its classification also becomes a mandatory label. Redemption unions that
record into every artifact, including uploads with omitted labels or a different
presentation classification. The original caller assertion remains unchanged. No new
Store table or migration is needed. Task identity, bearer scope, byte/count ceilings
and idempotent publication retain their existing enforcement.

Rust and Python clients expose the same optional constraint. Existing producers
request no additional labels; their Work Context policy still applies. Computers
will supply its retained-data floor when its command output path is integrated.
Native tests use the shared pinned disposable Store fixture and separate service
instances to qualify persistence and replay without requiring an installed service.

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
starts and reaps an isolated SurrealDB 3.3.0 process. Set `VEOVEO_SURREAL_BINARY` to
that exact executable. It recreates service instances, races byte admissions, checks
persisted authority fields, and distinguishes timestamp updates from policy changes.

The SQL formatter `@surrealdb/surql-fmt` has no stable release at this checkpoint.
Its latest published `0.1.0-beta.2` formats the query files but corrupts dotted field
paths in this migration. The migration therefore retains manually checked field
paths and passes the native 3.3.0 parser and database application test.

Stream and Reason persist the capability in protected task state and supply it to
the recording reader. The scope endpoint checks current validity without admitting
an occurrence. The reader uses that verified actor and byte ceiling before reading
catalog state or copying live parts that have no Artifact occurrence yet. It does
not convert that scope into a gateway identity. Hardware workload acceptance remains
required before admitting the common compiler images.

## Upload Qualification Limits

Installed resumable upload and download acceptance covers 10 GiB with interruption,
session recovery and independently verified bytes. Qualification at 100 GiB requires
a suitably provisioned installation. Compare native multipart throughput, independent
storage and public ingress/uplink behavior, measuring memory, concurrency, verification
traffic, retries and cleanup latency.

A second interactive identity and a browser Work Context switch need installed
qualification. Service and queue-controller tests cover their authorization boundaries.
An embedded MCP App host picker is separate work: intersect its explicit App grant
with installation policy, bind the App URI and grant revision to trusted authority,
and return only the receipt to the App. Bytes stay in the host origin. The
[upload client design](../../../apps/console/web/src/uploads/DESIGN.md) owns that UI.

## Native Query Fixture Placement

Artifact service native fixtures include complete statements from `tests/queries/`,
grouped by source responsibility. Runtime test values remain bound. Store owns the
production Artifact persistence statements and their transaction admission.
