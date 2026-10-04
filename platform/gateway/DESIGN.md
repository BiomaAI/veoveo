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
| Administrative HTTP | RFC 9110 request methods and response status; gateway-authenticated proxy routes with bounded request and response bodies |

## Ownership

The reusable `veoveo-mcp-gateway` library owns forwarding mechanics. The
[`veoveo-gateway-composition`](composition/DESIGN.md) package owns the `gateway`
executable, owner bindings, installation commands and image packaging.


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

Upstream MCP errors preserve their protocol code, message and domain data. A final
non-success HTTP response without a valid MCP error carries its typed status through
the gateway handler. HTTP 4xx and 5xx retain their status; an unexpected redirect
becomes 502 without forwarding its location or credentials. Connection failures map
to MCP internal errors. Authentication challenges keep their dedicated SDK path.

`mcp/http_response.rs` separates a protocol error from an HTTP rejection until the
handler finishes. Only a returned HTTP rejection sets the request's response status.
Source errors absorbed by catalog aggregation cannot change a successful partial list.
The middleware waits for the profile's JSON response before writing HTTP headers;
request-stream failures after subscription acknowledgement follow stream termination.
Audit classification uses the protocol error while retaining the transport status for
HTTP delivery. Neither messages nor domain error data select an HTTP status.

## SDK Transport Profile

The workspace pins stable RMCP 3.5.0 with the maintained fork revision
`917e7914c93975fc1eddbff8792f1e2de933bc17`. The upstream base is release
[`rmcp-v3.5.0`](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.5.0).
Rig selects the same SDK revision through `4125e888edc7609413e9e9092a67ddbaf59422b5`,
so native agent clients and the gateway share one protocol type graph.
The fork retains exact Task subscription filters and exposes HTTP rejection status
and body through `StreamableHttpError::HttpResponse`. Valid MCP error envelopes keep
the SDK's protocol-error path. Sessionless legacy discovery follows its existing
explicit adapter profile.

The gateway owns qualification of the HTTP patch: plain-text 413, malformed error
bodies, preserved MCP errors, rejected redirects, isolated discovery failures,
concurrent requests and no dispatch retry after rejection. SDK native suites cover
its model, protocol, header, Task and subscription profiles; gateway and shared-host
consumer suites qualify the repository pin. Installed qualification must also repeat
the Computers oversized-request case. The patch can retire when stable upstream
exposes equivalent typed HTTP rejection and exact Task filtering and those suites
pass against the replacement. The gateway then removes its fork pin in the same
qualified dependency change.

## Upstream Discovery Recovery

Every request-scoped client discovers the selected upstream before forwarding a domain
request. Discovery has a ten-second deadline per attempt. A transport connection or
response-body failure permits one fresh SDK discovery attempt with the same admitted
identity and HTTP trust configuration. Authorization challenges, malformed responses
and MCP application errors fail immediately. The gateway keeps its shared HTTP pool.

Discovery recovery dispatches no domain mutation. After discovery, the existing
idempotent request path may reconnect once when its connection fails. A typed
HTTP rejection is definitive and does not retry. Tool mutations
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
Knowledge coordinator in the [consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md#knowledge-service).

## Administrative HTTP Routes

The authenticated `/admin/{profile}/servers/{server}/{path}` proxy resolves the
upstream admin route beside the registered MCP endpoint. An upstream at `/mcp`
serves `/admin/{path}`; one at `/frames/mcp` serves `/frames/admin/{path}`.
The public mount does not determine the internal route. The URL library appends
validated path segments and preserves the request query. An endpoint that does
not end in `/mcp` fails with a configuration diagnostic. Profile policy and audit
admission still run before forwarding.

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

`CatalogAdmission` supplies installation-owned validation after the generic control
plane checks. Catalog constructors and `GatewayControlStore` require an explicit
`GatewayCatalogAdmission`; unbound loads and publication refuse the capability.
The store repeats admission when decoding a persisted revision. A catalog carries
its binding through clones, and `GatewayCatalogHandle::replace` rejects a different
binding before changing the generation or notifying readers. Reloads reuse the
current binding rather than constructing another validator.

Recording's `gateway` feature implements producer-scope admission in its owning
library. It requires `RecordingProducerScope::Ingest` and preserves additional
installation scopes for normal policy evaluation. The shared control-plane DTO still
contains `recording_ingest_resources` and related producer fields. Their owner-wire
extraction is required before this configuration supports independent catalog
sections; the current port does not change those wire fields.

`OAuthClientResolver` returns a generic effective registration and optional
`OAuthClientAuthority`. The reusable library handles token scope/resource checks
and delegates membership, service roles, current-token binding and action admission
to the registration's owner. `GatewayState` binds one resolver and refuses OAuth
resolution when unbound. Composition chooses the Agents resolver for durable managed
registrations. The explicit static-only resolver admits catalog registrations and
rejects managed claims; it supports installations that exclude durable registrations.

The Agents adapter reads current registration and template authority for each
resolution. Its implementation and native policy cases live under
[`agents/runtime/src/gateway`](../../agents/runtime/src/gateway/DESIGN.md).
The existing `ManagedAgentToken` JWT claim still appears in the generic token-binding
API and verified token envelope. Independent authority implementations can provide
policy, but a new module-specific token binding needs owner-wire extraction and
consumer transfer before activation. The authority port preserves the claim's wire form.

The generic upstream pool owns HTTP clients and checked TLS construction.
`UpstreamClientKey` exposes the catalog revision while keeping the TLS fingerprint
opaque. Computers' owner adapter supplies its separate HTTP/1.1 upgrade pool using
that key and builder. Both paths retain ten-second connection establishment, no
total response timeout, disabled redirects, declared CA roots and typed mTLS secret
purposes. The Computers pool retires clients from older catalog revisions.

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
[consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md#unified-audit-log).

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

## Cursor Admission

Catalog continuation uses an owner codec over typed item keys and the existing version-string/surface JSON envelope. Surface binding and the 16 KiB input ceiling remain in the gateway. Shared cursor mechanics do not enumerate protocol catalog surfaces.


## Installation Command Composition

The gateway binary registers all 17 schema-only owner exports. `module-plan` produces
the generic checked plan offline. Commands compare the supplied plan with those exports
and the expected image binding, generation and credential revision before connecting.
The plan producer adds no owner vocabulary to the foundational module crate.

`installation-prepare` authenticates as root, creates an absent namespace/database,
claims the preparation generation, applies the existing mixed catalogs and completes
runtime-account provisioning. It publishes no control plane. `module-migrate` runs one
selected lane after database history proves its dependencies; waits are capped at 300
seconds. `module-status` reads the selected history without mutation.

`control-plane-publish` uses database-scoped runtime authentication after health, mixed
catalog, preparation and selected-lane checks. Its revision/audit transaction checks
the preparation key again. Audit attribution names the authenticated database account;
`applied_by` records the separately validated operator attribution. Identical active
control data returns the existing validated revision after a transaction reads the
preparation marker and active revision together. Serving applies the same readiness
checks before opening the HTTP listener.

Installations advance their explicit generation when preparation identity changes and
drain writers for an upgrade until mixed runtime/schema overlap is qualified. Empty
owner lanes do not disguise the mixed legacy schema as an owner migration. Rendered Jobs
and installed fresh-start/upgrade acceptance are tracked in the active contract plan.

## Transport-Free Gateway Values

The transport-free [Gateway Contract](contract/DESIGN.md) owns App dependency DTOs
and discovery failure values shared with MCP and browser schema consumers. Hosted
gateway behavior uses these values without making their consumers enable runtime.

## HTTP Module Composition

The [HTTP module design](src/http/DESIGN.md) owns typed contexts, registered raw
profile capture, deferred optional factories, native MCP discovery and task-scope
cleanup. Domain routers reside in their owner crates. The composition executable
supplies their configurations and retains the cleanup supervisor until shutdown.
