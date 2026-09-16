# Gateway Task Routes

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP 2026-07-28 Tasks extension | Opaque canonical Task identifiers projected from upstream CreateTaskResult |
| Veoveo invocation authority | Stable tenant, actual actor, Work Context, profile and invocation provenance; every use checks current policy and retained data labels |
| SurrealDB 3.2.4 | Durable mapping from 0037, shared Task reference from 0041, optional version-1 ownership metadata from 0081 |
| Veoveo `gtr_` route identifiers | 32 random bytes encoded as base64url without padding; identifiers convey no access authority |

`task_routes.rs` owns the mapping between an upstream Task and its public gateway
identifier. The source tuple is server, opaque upstream ID, owner and profile. Each
mapping also retains its tenant, Work Context, authorization digest and optional
shared-runtime Task link. Those fields must match before an existing route is reused.

An upstream tool retry can return its original Task. Gateway projection returns that
Task's existing public route, including across replicas and fresh authentication.
It preserves the original creation time and expiry. A concurrent CREATE or an
uncertain database response triggers one exact source lookup; this lookup never
redispatches the upstream tool. The unique source index arbitrates competing writers.
An expired mapping is not recreated or extended by a retry.

Reads, updates, cancellations and listeners match durable ownership before checking
current profile exposure and policy. Version-1 ownership retains the principal key,
kind, issuer, subject, invocation mode, initiator, delegation identity and admitted
data labels. The route separately binds tenant, Work Context and profile. Current
labels must contain the retained labels. A new sign-in, an unrelated scope change,
or a Work Context policy revision does not change ownership. Removing required
current permissions still denies access. Every upstream request carries the current
caller authority; the owning server enforces its current domain and named grants.
Creation authority never becomes a credential for later access.

The full invocation fingerprint remains immutable admission evidence and an exact
retry constraint. Discovery, continuation and subscription cache keys retain their
full authority fingerprints. Their isolation must not be replaced with the durable
ownership comparison.

## Persisted Version Transition

Migration 0081 adds optional ownership metadata without rewriting routes or Tasks.
New gateway writers always populate version 1. For an existing route without that
metadata, the gateway reads only ownership fields from its exact shared-runtime
Task reference and verifies tenant, owner, Work Context, profile and source server.
This gateway-owned version-0 reader preserves existing opaque IDs and original TTLs.
It supports Tasks written during an overlapping rollout as well as earlier Tasks.
No provider request, tool redispatch or data mutation participates in recovery.

An old external route has no trusted shared-runtime ownership record. It keeps its
original exact invocation constraint until expiry; current credentials cannot invent
the missing provenance. This limitation applies only to routes created by old
writers. New external routes have the same version-1 recovery as hosted Tasks.
Remove the version-0 reader after old writers have retired and no unexpired route
lacks ownership metadata. Old gateway binaries can read the additive schema, but
rollback restores their stricter permission-bundle comparison. During overlap a
request reaching an old replica can still encounter that recovery limitation.

Qualification exercises refreshed scopes and policy revisions, required-scope and
role removal, retained-label removal, wrong tenant/context/profile/actor/issuer,
changed initiator/delegation, current policy revocation and removed Task exposure.
The isolated store also qualifies the version-0 first-party reader, old external
constraints and rejection of a substituted source server. No existing Task ID,
invocation digest, creation time or expiry changes during reads.

The isolated store tests cover twelve competing projections, retained expiry,
changed Work Context and authority, source-link substitution, expired mappings,
independent servers and overflow-safe retention input. Public Computers lifecycle
acceptance found the original unconditional CREATE defect. Release 157 repeated the
original Stop and Start requests with fresh authentication and returned both original
canonical IDs. Neither effect repeated. Subsequent grant revocation denied the retry
at the owning Computers service.
