# Gateway MCP Forwarding

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP | The repository's hosted MCP 2026-07-28 profile, with typed protocol envelopes, resource discovery, tools, Tasks and subscriptions; the [server contract](../../mcp/contract/DESIGN.md) defines requirements |
| MCP Apps | Extension `io.modelcontextprotocol/ui`, release 2026-01-26; `_meta.ui.resourceUri` links a tool to its App document |
| Knowledge reads | `ai.veoveo/knowledge-source`; SHA-256 content binding, opaque revision validators and observation metadata from the [extension contract](../../mcp/knowledge-extension/DESIGN.md) |
| Resource addresses | Repository-owned server schemes and `ui://` App routes validated through foundational URI types; installation manifests select identity or server-owned projection |
| Gateway catalog cursor | Veoveo opaque version 1: base64url JSON with a typed surface and exclusive last identity; up to 16 KiB; internal to Gateway list pagination |
| Installation authority | Typed server manifests, profile exposure and policy from the MCP gateway contract; [authentication](src/auth/DESIGN.md) resolves request identity |

## Ownership

The gateway authenticates and authorizes a request before forwarding it to a selected
server. Its catalog composes server manifests and exposed capabilities. Domain servers
own their tool schemas, result values, resource content and identifiers. The gateway
does not enumerate domain payload types or require a new producer to change core.

`mcp_support.rs` owns the transformation of protocol resource addresses when an
installation selects server-owned projection. Resource discovery, templates, embedded
resource envelope addresses and resource links use the server's registered namespace.
The MCP Apps `_meta.ui.resourceUri` field receives the corresponding App address.
`referenced_resource_schemes` preserves protocol links owned by another registered
server; installation validation checks that declaration against the registry.

## Forwarded Identity Directory

`upstream_authorized_http.rs` signs each upstream HTTP request separately. Ordinary
assertions expire within 60 seconds. A typed `subscriptions/listen` request receives
an assertion valid through the caller's existing access-token expiry, capped at 15
minutes. This larger internal bearer window lets a source keep its authorized listener
open until client credential rotation. It adds no renewal permission and cannot extend
the caller's token. Sources enforce their assertion deadline and current domain access;
client cancellation closes the request stream. Recording's separately forwarded
Artifact-read assertion keeps its 60-second limit.

After authenticating a request, the gateway synchronizes the source principal and any
distinct delegated actor into the platform directory. Store preserves security fields
on existing entries. Hosted services can therefore check both identities against
current enterprise and tenant enablement before delivering private data.

## Domain Payload Preservation

Structured tool results, resource content and unknown extension metadata keep their
upstream values. A URI-shaped string inside them does not identify a gateway routing
field. For example, a Frames world can reference an independent pose producer through
its own scheme. Changing that reference changes the world and invalidates its digest.
Payload preservation applies to every producer without a scheme allowlist.

The gateway changes only the App link within extensible metadata. CSP origins,
visibility, domain references and unknown metadata fields keep their values. Knowledge
observations follow their negotiated delivery rule below. A malformed
App link produces a protocol error. This transformation grants no resource access;
the selected profile and caller policy still govern subsequent requests.

An adapter that changes a domain's public namespace must construct coherent domain
payloads itself. The gateway cannot repair arbitrary result strings, recompute domain
digests or infer foreign-resource ownership. Installation upgrades use one gateway
behavior; no compatibility mode rewrites domain payloads.

Upstream MCP errors preserve their protocol code, message and domain data. The gateway
maps transport failures to internal errors. This distinction lets clients handle a
resource rejection without treating it as a broken connection, and lets completion
audit classify the source response. The SDK applies the negotiated MCP version's
error-code profile at each transport.

## Upstream Discovery Recovery

Every request-scoped client discovers the selected upstream before forwarding a domain
request. Discovery has a ten-second deadline per attempt. A transport connection or
response-body failure permits one fresh SDK discovery attempt with the same admitted
identity and HTTP trust configuration. Authorization challenges, malformed responses
and MCP application errors fail immediately. The gateway keeps its shared HTTP pool.

Discovery recovery dispatches no domain mutation. After discovery, the existing
idempotent request path may reconnect once when its transport fails. Tool mutations
and Task mutations do not gain a dispatch retry. The native HTTP fixture drops a
discovery connection, verifies recovery and one subsequent tool call, and checks that
repeated disconnection exhausts the retry while authorization, JSON and MCP errors do
not retry.

## Resource Read Audit

The gateway checks current resource policy before source discovery or reading. Denials
commit at admission. An admitted read requests knowledge metadata from a declaring
source and verifies the observation against the upstream member URI, returned UTF-8
bytes, selected server and conditional revision. It projects the envelope URI while
preserving the source bytes. Caller-supplied observations never reach the source.

The read handler commits one completion record before returning a result. Knowledge
records carry the audit contract's reviewed observation; ordinary resources use the
resource-read detail. A failed audit commit prevents delivery. The gateway forwards
observations only to declaring callers and preserves unrelated extension metadata.
Read results carry private cache scope and zero TTL, allowing current authorization
and audit recording on each read. The explicit upstream request path also prevents
a URI-only SDK cache from substituting for conditional authorization.

## Knowledge Indexing Clients

Server manifests carry optional typed collection approvals. An OAuth client's
`knowledge_indexing` registration identifies its allowed collections. Control-plane
validation requires a tenant-bound automated client with `private_key_jwt`, only the
client-credentials grant and one dedicated resource profile. That profile exposes
approved source resources and subscriptions. Its policy cannot allow mutations,
administration, tools, prompts or Tasks.

`mcp/knowledge_indexing.rs` applies the client restriction alongside ordinary policy.
A read supplies a collection and source-contract, enumeration or member intent.
Source-contract admission permits only the owning scheme's checked `contract` URI
under an explicit installation approval, including catalog-only approval. It works
before Store registration and never admits domain content or contract subscriptions.
For enumeration and member reads, the gateway compares the
current source approval with Store's registration before forwarding. Enumeration URIs
must match the declared page address. Member delivery requires a verified observation
from that collection within the approved tenant and label set. Rechecking approval
after the read prevents a changed registration from delivering an in-flight result.

Discovery validates a collection's owner and enumeration scheme against its source
manifest and requires the upstream server to declare the Knowledge extension.

Resource subscriptions use the same collection admission. A metadata-free root filter
selects its registration in SQL by tenant, exact enumeration root, current approval
and required scopes. Multiple matching approved collections reject the request.
Enumeration subscriptions
can precede a build; member subscriptions require a non-stale observed member in the
active generation. Indexing clients cannot subscribe to tool/prompt changes or Tasks.
Rejected subscription admission records a denial before returning. The raw Artifact
download route denies indexing clients because byte delivery cannot establish this
collection-bound observation contract.

The SDK acknowledges a subscription before its asynchronous handler starts. Indexing
resource listeners therefore wait up to 20 seconds for an initial invalidation of
every accepted URI from the upstream source. The gateway forwards those invalidations
after observation starts, preserving any intervening catalog change. Its indexing
catalog listener emits an initial list invalidation after registering the discovery
change receiver. Knowledge waits for these signals before reading catalog or member
pages. Internal catalog watchers apply the declaring source's readiness requirement
for every caller, including ordinary profiles. They wait for the source's initial list
invalidation within their ten-second opening deadline, then discard the cached
baseline before discovery. Source loss ends the stream and requires fresh admission
on reconnect.

Indexing reads currently commit ordinary per-read audit records. Five-minute collection
aggregation and installation of the indexing client are pending with the hosted
Knowledge coordinator in the [foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-8-knowledge-service).

## Qualification

`tests/audit_cli.rs` exercises the public `audit verify` command against disposable
RocksDB databases. Each case starts with sealed records, then uses the fixture's root
identity to change a record, delete a record or block, or replace a signature. A separate
case inserts a backdated record before sealing. The command must reject each case with
its corresponding integrity or clock finding. The fixture owns its Docker container,
uses generated credentials and allows 240 seconds for the complete run.

Native Task cancellation uses `/admin/{profile}/tasks/{server}/{task_id}/cancel`.
The Console supplies the Task's server and native UUIDv7 identity. Gateway policy
uses `PolicyTarget::PlatformTask`; forwarded MCP Tasks use `PolicyTarget::Task` with
an opaque route. After policy admission, the native runtime selects the caller's
principal, current profile, tenant, clearance and Work Context in SQL and repeats
that selection inside cancellation transactions. Audit records carry the native
Task identity for this API, including rejected owner selection.

Native forwarding cases cover identity projection, explicit resource links, App link
projection, independent producer references, unknown metadata, malformed App links and
profile-scoped App dependencies. Domain contracts qualify their own serialization and
digest checks. Installed acceptance compares a published Frames world with its resource
readback through the public gateway before configuring a simulator.

## Audit And Catalog Cache

Catalog admission composes generic MCP configuration checks with the Recording
adapter's producer-scope check in `src/recording.rs`. Catalog construction, revision
publication and stored-revision loading use that same admission. The adapter imports Recording's
contract-only library and requires its `Ingest` permission. Additional installation
scopes stay in the configured set and are enforced by policy. Generic MCP contracts
own no Recording scope spelling and require no domain library dependency.

Console's native inventory stream retains prior rows for tenant-scoped deletions of
principals, Tasks, Artifact blobs/occurrences/access requests, agents, wakes and
Recordings/layers. Grant and share deletions resolve their parent through the admitted
Artifact inventory. Upload deletions do not update this inventory. The complete
[Store feed policy](../../docs/TECH_DESIGN.md#changefeed-payloads-and-relationships)
lists the tables that require original rows; other gateway feeds carry identities
and committed current state.

The gateway hosts the [audit exporter](../audit/src/export/DESIGN.md) under the sealer
lease. `VEOVEO_AUDIT_EXPORT_CONFIG` selects public S3/OTLP destination configuration;
credential values come from the installation's Secret environment. Export runs beside
lease renewal and sealing. A permanent destination rejection fails audit readiness,
and retention waits for every configured destination's receipt.

Operator CLI commands write diagnostics to stderr and reserve stdout for their
documented result format. Only `serve` initializes the hosted OTLP exporters;
running an audit command inside a configured gateway Pod keeps its JSON or JSON Lines
output parseable.

Recording ingress uses the gateway-owned private resource addresses
`recording-ingest://producers/{producer}` and
`recording-ingest://producers/{producer}/streams/{stream}` in audit targets. Typed
producer and stream IDs pass through the shared URI builder. These addresses identify
internal protocol objects; they are not MCP read routes. Successful batch appends and
status polls use the recording ledger without individual audit records. Policy denials
are recorded before forwarding. Recording catalog grants audit the sorted selection's
digest and count once at admission and once on completion.

The unified audit implementation is being qualified under the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-4-unified-audit-log).

Control-plane publication compares the new Work Context definitions with the active
revision and builds typed create, update and delete records for changed definitions.
The activation transaction locks the previously read head, rejects a concurrent
revision change and commits Work Contexts together with their audit records. Request
publication uses the verified acting principal; installation bootstrap uses the
database-authenticated operator. The revision's supplied attribution cannot replace
that audit identity.
Its gateway record builder takes the admitted actor and authority. Token lifecycle
records share that contract; refresh rotation appends its record in the rotation
transaction. Tool completion retries use stable record identities and preserve the
upstream result.

Discovery entries contain admitted descriptors and their denied count. The cache key
includes the catalog generation, actor and invocation authority, OAuth client, and
managed-agent generation and dispatch epoch. A warm list uses those decisions and
writes one aggregate record. Prompt catalogs use the same cache behavior.

The four Gateway list methods page over the current admitted descriptors by identity:
tool name, resource URI, template URI or prompt name. Each cursor carries a typed last
identity and its surface; the next request returns identities strictly after that
position. New entries before the position cannot repeat a previously returned entry,
and removal of earlier entries cannot shift the page past a remaining entry. Duplicate
identities fail discovery. The cursor carries no descriptors or authority, so every
page uses the current caller admission and works across replicas.

These pages do not promise a snapshot. A newly admitted identity before the current
position appears when a client restarts enumeration after a list-change notification.
The versioned cursor accepts at most 16 KiB, rejects a different surface or malformed
identity, and reports an actionable restart diagnostic. Installation upgrades replace
numeric-offset cursors directly; clients restart interrupted enumeration without a
cursor. No persisted data or server-side cursor state requires conversion.

A dynamic catalog has an authenticated native subscription before the gateway caches
it. The gateway coalesces subscriptions for the same authority and server, with a limit
of 256 subscriptions per profile and a ten-second opening deadline. Catalog changes
invalidate the corresponding server surface. Disconnect, token expiry and a gateway
catalog replacement invalidate the associated authority's entries. Opening the next
stream precedes fresh discovery, covering notifications missed during the disconnect.
The gateway keeps no five-second expiry for admitted decisions. The private MCP TTL
sent to clients continues to describe their protocol cache, independently of the
internal subscription lifetime.

Installation capability flags must match each server's advertised catalog changes.
A static resource catalog can offer resource-specific updates without announcing list
changes. Map and Time use that profile. A server that narrows a requested catalog
subscription fails discovery because the gateway cannot safely cache those changes.
