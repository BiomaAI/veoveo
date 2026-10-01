# Gateway MCP Forwarding

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP | The repository's hosted MCP 2026-07-28 profile, with typed protocol envelopes, resource discovery, tools, Tasks and subscriptions; the [server contract](../../mcp/contract/DESIGN.md) defines requirements |
| MCP Apps | Extension `io.modelcontextprotocol/ui`, release 2026-01-26; `_meta.ui.resourceUri` links a tool to its App document |
| Knowledge reads | `ai.veoveo/knowledge-source`; SHA-256 content binding, opaque revision validators and observation metadata from the [extension contract](../../mcp/knowledge-extension/DESIGN.md) |
| Resource addresses | Repository-owned server schemes and `ui://` App routes validated through foundational URI types; installation manifests select identity or server-owned projection |
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
