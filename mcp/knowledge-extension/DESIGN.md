# Knowledge Source Extension Design

This document is the normative contract for `ai.veoveo/knowledge-source`, the MCP
extension through which a server publishes resources as knowledge. A server that
declares the extension tells its consumers four things about each declared
collection: how to enumerate it, how long a read stays fresh, how changes arrive,
and who may read each record. Every read then carries a typed observation that
names the revision, content digest, modification time, and access descriptor of
what the caller received.

Adoption is optional. Servers declare collections when those resources improve
knowledge discovery or reuse. A declaration commits its owner to every applicable
extension rule; ordinary MCP resources need no knowledge metadata.

Agents, the Console, and the knowledge service consume the same resources that any
MCP client reads. The extension adds metadata. It adds no methods, and it never
replaces a server's resources with a second copy. The
[knowledge sharing design](../../docs/KNOWLEDGE.md) describes how the audit log records
knowledge reads and the `knowledge-mcp` catalog and index built on this extension.

The crate `veoveo-mcp-knowledge-extension` in this directory owns the typed
models, the capability declaration, the read and search helpers, and the shared
implementation of the well-known docs collection. The
[implementation plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md) tracks its
delivery.

## Status

Typed models, MCP negotiation and conditional-read helpers, document collection
paging, and compile-time document hashing are implemented. Rust hosted servers
declare their docs collections and share authenticated reads. Python and Node
document indexes use the same page shape. Python servers use the shared observation
adapter and package SHA-256 document manifests through the SDK's Hatch build hook.
Gateway reads validate source observations
and commit their audit records before delivery. The kernel retains observations and
provenance inside its existing byte budgets. The Node adapter negotiates observations
and checks image-build document manifests. K07/K08 execute owner-supplied probes;
domain adoption and installed conformance are in progress under the implementation plan.

The `contract` feature builds with default features disabled and depends only on
foundational Veoveo types, serialization, timestamps, and hashing. The `mcp`
feature adds the pinned Rust MCP SDK. Neither feature depends on MCP core or any
domain server. Audit and Store can therefore share observations without a runtime
dependency on MCP. The macro package hashes embedded document bytes during Rust
compilation and emits `include_str!` to track the document as a build input.

## Standards And Protocols

| Standard or protocol | Profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/draft) `2026-07-28` | Resources, resource templates, completion, resource links, resource/link `annotations.lastModified`, and `subscriptions/listen`, under the [hosted-server contract](../contract/DESIGN.md) |
| [MCP extensions](https://modelcontextprotocol.io/extensions/overview), SEP-2133 | Identifier `ai.veoveo/knowledge-source` in `capabilities.extensions` on `server/discover`, and in `_meta["io.modelcontextprotocol/clientCapabilities"].extensions` per request; the settings object is empty in this revision |
| MCP `_meta` key rules | Repository-owned keys `ai.veoveo/knowledge-source` and `ai.veoveo/knowledge-observation`; `veoveo.ai` is the owned domain |
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) HTTP semantics | Strong-validator semantics for `revision` and `ifNoneMatch`, carried in `_meta` rather than HTTP headers |
| [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html) HTTP caching | Freshness lifetime semantics for `maxAgeSeconds` |
| [RFC 8246](https://www.rfc-editor.org/rfc/rfc8246.html) immutable responses | Semantics of `immutable` freshness |
| [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) | `observedAt`, `modifiedAt`, and `annotations.lastModified` timestamps |
| SHA-256, FIPS 180-4 | `contentSha256` over the exact UTF-8 bytes of each returned text item |
| [W3C DCAT 3](https://www.w3.org/TR/vocab-dcat-3/) | Collection descriptors map to `dcat:Dataset`; this extension uses DCAT terms for alignment and does not serialize RDF |
| [JSON Schema 2020-12](https://json-schema.org/draft/2020-12/) | Closed schemas generated from the crate's Rust types for every descriptor and observation |
| Veoveo Work Context | Access descriptors use the tenant, Work Context, grant, and data-label model in [Work Context governance](../../docs/WORK_CONTEXT_GOVERNANCE.md#output-ownership-and-access) |
| `ai.veoveo/indexing-read` | Gateway request metadata selecting an approved collection and source-contract, enumeration or member intent; installation client registration establishes authority |

Veoveo owns and versions this extension outside the MCP SEP process.

## Negotiation

A server declares the extension in its `server/discover` capabilities:

```json
{ "capabilities": { "resources": { "listChanged": true },
                    "extensions": { "ai.veoveo/knowledge-source": {} } } }
```

A client that wants observations declares the same identifier in each request's
client capabilities. A server attaches `ai.veoveo/knowledge-observation` to a
read result only when the request declares the extension. Clients that do not
declare it receive ordinary resources, so the degradation needs no special path.

Compatible additions appear as optional fields or as flags in the settings object.
A breaking change, as defined by the MCP extensions overview, uses a new identifier
such as `ai.veoveo/knowledge-source-v2`.

## Collections

A collection is a set of resources with one URI template, one enumeration path,
one freshness policy, and one access model. A server declares each collection by
attaching `_meta["ai.veoveo/knowledge-source"]` to the resource template that
addresses its members:

```json
{
  "uriTemplate": "time://events/{event_id}",
  "name": "temporal-event",
  "title": "Temporal event",
  "mimeType": "application/json",
  "_meta": {
    "ai.veoveo/knowledge-source": {
      "collection": "time.events",
      "entityKind": "temporal-event",
      "enumerate": "time://events{?cursor}",
      "freshness": { "maxAgeSeconds": 300 },
      "changeSignal": "listen",
      "access": "work-context",
      "indexing": "content"
    }
  }
}
```

| Field | Meaning |
|---|---|
| `collection` | Stable identifier, `{server-slug}.{name}`, unique within the installation catalog |
| `entityKind` | Domain noun for the members, used for filtering and display |
| `enumerate` | Bounded, cursor-paged collection resource that lists member URIs in stable order, per contract rule C04 |
| `freshness` | Either `{ "immutable": true }` or `{ "maxAgeSeconds": n }` |
| `changeSignal` | `listen`: member and collection URIs are subscribable and a restart-safe source emits their changes. `immutable`: members never change after creation. `revalidate`: the server emits no changes, and consumers revalidate after `maxAgeSeconds` |
| `access` | `work-context`: each observation carries a Work Context access descriptor. `profile`: members share the current tenant and collection exposure, without per-record access restrictions |
| `requiredScopes` | Typed scope names every reader must hold, in addition to collection exposure and record access. An empty set adds no scope requirement. The source owns these names |
| `indexing` | `content`: the knowledge service may index returned text. `metadata`: it may index titles and observation fields only. `none`: it catalogs the collection without indexing it |

The template's own `title`, `description`, and `mimeType` describe the collection
for display, and the extension does not repeat them. Stewardship, authoritative
scope, and approval belong to the installation control plane, so a server's
declaration never names them.

A member is bounded text or JSON that the governed agent read adapter admits.
Binary content is never a member. A record that owns bytes returns an
`artifact://` resource link in its JSON body.

Enumeration returns one JSON text item with an `items` array and an optional
`nextCursor`. Each item has a concrete `uri`; the owner may add typed domain fields
such as a title or identifier. Pages contain at most 100 items. An absent cursor
ends traversal. A continuation cursor is nonempty, at most 4,096 UTF-8 bytes, and
never repeats during a traversal. Consumers bind it to the declared enumeration
template's `cursor` variable, or add that query parameter when the declaration is a
concrete URI. The source selects readable members before ordering and pagination.

## Indexing Requests

An installation-registered indexing client includes `_meta["ai.veoveo/indexing-read"]`
on source reads. `IndexingReadIntent` carries a typed collection ID and the
`source_contract`, `enumeration` or `member` kind. The gateway checks the machine
client's registered collection set and current installation approval. Ordinary clients
cannot acquire indexing authority by supplying this metadata. Sources apply their
ordinary authorization and observation contract; the intent requires no source handler.

`source_contract` permits the exact owning scheme's `contract` resource before a
collection has a Store registration. Catalog-only approval permits that metadata read;
enumeration, member reads and resource subscriptions require indexing approval.
Root subscriptions can omit intent metadata: the gateway selects the unique approved
registration for the exact root under current tenant and scope predicates in SQL.

`enumeration_uri` expands the declared template through the shared URI implementation.
It validates the cursor and supports a concrete enumeration URI by adding its cursor
query parameter. `is_enumeration_uri` rebuilds that address to reject additional paths
or query parameters. Member delivery requires a collection-matching observation whose
tenant and labels fit the installation approval. The gateway checks approval again
after the read. Subscription admission accepts an enumeration resource before building;
a member requires a current observation in the active index.

## Observations

A read of a collection member returns ordinary contents. When the request declares
the extension, the result also carries one observation in its `_meta`. The observation
carries `modifiedAt` whenever the domain records a modification time. Discovery
resources and resource links may also carry `annotations.lastModified`.
[MCP TextResourceContents](https://modelcontextprotocol.io/specification/draft/schema#textresourcecontents)
has no annotations field; read-time modification metadata belongs in the observation.

```json
{
  "contents": [{
    "uri": "time://events/0199b0f5-7b1e-7cc4-9a3d-5c1f7e0b2a91",
    "mimeType": "application/json",
    "text": "{…}"
  }],
  "_meta": {
    "ai.veoveo/knowledge-observation": {
      "collection": "time.events",
      "revision": "7",
      "contentSha256": "4b1f…e09c",
      "observedAt": "2026-09-26T21:40:03Z",
      "modifiedAt": "2026-09-25T14:02:11Z",
      "modifiedBy": { "kind": "principal", "id": "0199…" },
      "access": {
        "tenant": "0199…",
        "workContext": "0199…",
        "readPolicy": { "kind": "subjects" },
        "owner": { "kind": "principal", "id": "0199…" },
        "dataLabels": ["operations"]
      }
    }
  }
}
```

| Field | Required | Meaning |
|---|---|---|
| `collection` | yes | The declaring collection |
| `revision` | yes | Opaque strong validator. It changes whenever the returned text, access descriptor or modification/external metadata changes. Immutable members with no separate metadata may use their content digest |
| `contentSha256` | yes | Lowercase hex SHA-256 of the exact UTF-8 bytes of the returned text |
| `observedAt` | yes | Server time when the domain produced this read |
| `modifiedAt` | when recorded | Stored modification time of the returned content; an access-only revision need not change this timestamp |
| `modifiedBy` | when recorded | Principal ID that produced the current revision, in the platform's shared human and service principal namespace |
| `access` | for `work-context` collections | Tenant, stored Work Context, explicit read policy, owner, direct grants when the domain stores them, and data labels |
| `external` | for connector projections | `system`, `nativeId`, and optional `url` of the record in an external system of record, plus `mirroredAt` when the connector served a stored copy |

A domain server fills observations from its own records. The caller never supplies
provenance. An observation describes the read that produced it and grants no
authority.

`readPolicy` is required. The recorded Work Context identifies the source operation;
it grants membership-based read access only when the source declares that policy.
Every policy requires the same tenant, clearance for all labels and current profile
exposure of the collection.

| `readPolicy.kind` | Additional admission |
|---|---|
| `tenant` | Every caller admitted to this collection in the tenant may read |
| `subjects` | The caller matches the owner or a direct principal/group grant |
| `work-context` | The caller has read membership in the stored Work Context, or matches an owner/grant subject |
| `selected-work-context` | The caller selects the stored Work Context and has read membership there, or matches an owner/grant subject |
| `selected-work-context-members` | The caller selects the stored Work Context and has read membership there. Ownership and subject grants do not bypass membership |
| `subjects-in-context` | The caller matches an owner/grant subject and selects the stored Work Context; an optional typed `profile` additionally requires that gateway profile |

Servers select the policy from their persistence contract. Tenant-shared calendars
can declare `tenant`; private events use `subjects`. Owner-scoped Tasks whose source
also checks the selected context and profile use `subjects-in-context`. None of these
declarations expands the source's read policy. A source policy outside these forms
requires an extension of the contract before that collection can be indexed.

`owner` uses a foundational `AccessSubject`. Each `grants` entry is a typed
`ReadGrant` with `subject` and optional RFC 3339 `expiresAt`. Both principal and group
subjects are supported. A grant permits read only before its deadline. The access
descriptor's optional `expiresAt` ends every read path, including owner, tenant and
context access. Consumers evaluate these deadlines when selecting results in SQL;
cache freshness does not extend them. Deadlines describe stored policy, so passing a
deadline changes access without changing the descriptor or requiring a source event.
A stored grant addition, removal or deadline change changes the revision under K10.
Modification attribution accepts a principal only.
Collection IDs, revisions, document IDs, entity kinds, external-system IDs and
external-record IDs are distinct validated types. Builders enforce collection and
access-model agreement before a server can attach an observation. Client helpers
verify that the digest describes the returned text and that a not-modified response
matches the validator the client sent.

### Conditional reads

A consumer that holds a revision sends it with the read:

```json
{ "method": "resources/read",
  "params": { "uri": "time://events/0199…",
              "_meta": { "ai.veoveo/knowledge-source": { "ifNoneMatch": "7" } } } }
```

When the current revision matches, the server returns `contents: []` and an
observation with `"notModified": true`. The server authorizes a conditional read
exactly like a full read. A consumer therefore uses the conditional read both to
revalidate a cached copy and to confirm that the caller may still read it.

Every observation field except `observedAt` and `notModified` must match the cached
full read. `observedAt` may advance but cannot regress. Shared validation identifies
the differing field without including its value in the diagnostic.

## Search Tools

A server may publish search over its collections as an ordinary tool. The tool's
`_meta["ai.veoveo/knowledge-source"]` declares `{ "role": "search",
"collections": [...] }`. Its closed `SearchResults` structured output has a `results`
array containing at most 100 unique resource URIs. Each result carries `uri`, optional
`title`, an optional `snippet` of at most 320 characters, and an optional `score`. Its content carries one
`resource_link` per result, with the same title and snippet as the structured hit.
The shared `server::search_result` builder produces both representations from the
checked results. A search tool returns only resources the caller may read. Knowledge
content reaches a consumer through `resources/read`.

## Change Signals

A `listen` collection makes each member URI and its `enumerate` collection URI
subscribable through `subscriptions/listen`. Its events come from Store LIVE queries
with change-feed recovery through the shared `SubscriptionHub`, which satisfies
contract rule C27 across restarts and replicas. A process-local broadcast alone does not qualify.
A notification names the changed URI, or requests reconciliation, and carries no
content. The enumeration root signals changes to membership and to any member's
content or access descriptor. After registering its change receiver and completing
authorization, the source emits an initial invalidation for every accepted member
and enumeration URI. The SDK's filter acknowledgement precedes handler setup; consumers
wait for these invalidations before reading. Conformance drains initial invalidations
before mutation and separately requires the resulting member and root changes.
When the source accepts a resource-catalog filter, it also emits an initial list
invalidation after registering the inventory observer. Gateway indexing discovery
waits for this signal before caching the catalog.
A consumer can therefore subscribe before enumerating
without first discovering every member URI. Conditional consumers preserve cached
content only when the complete observation, excluding `observedAt` and `notModified`,
matches the preceding full observation. An unchanged revision with changed access
violates K10 and cannot authorize cache reuse.

## Well-Known Docs Collection

Every server adopting the extension declares the `{slug}.docs` collection over
`{scheme}://docs/{doc_id}`:

| Field | Value |
|---|---|
| `enumerate` | `{scheme}://docs` |
| `freshness` | `{ "immutable": true }` for the lifetime of the running image |
| `changeSignal` | `immutable` |
| `access` | `profile` |
| `indexing` | `content` |
| `revision` | SHA-256 of the embedded document bytes, computed at build time |

The shared crate implements this collection once for every Rust server through
`veoveo_mcp_contract::docs`, and the Python SDK implements it in
`veoveo_mcp.contract.docs` for Python servers such as `datasheet-mcp`. The Node server
`chart-mcp` implements the same declaration in its own package.
Its image runs `build-docs.mjs` and checks the resulting manifest at startup. Both
ordinary and conditional reads pass the launcher's current gateway-token check.

Python wheel builds load `sdk/python/src/veoveo_mcp/build_docs.py` as a Hatch custom
hook and set its `package` to the server's import package. The hook embeds both
documents and `_documents.json`, containing their build-time digests. Package loading
checks those digests against the original UTF-8 bytes and fails on missing or altered
content. Explicit source-tree development computes digests when loading documents;
deployed packages require the build manifest.

## Server Rules

A server that declares `ai.veoveo/knowledge-source` satisfies these rules for every
declared collection. The conformance client checks K01 through K08 against a
running server. Review enforces K09 and K10.

| ID | Level | Requirement |
|---|---|---|
| K01 | MUST | Declare the extension in `server/discover` and attach a valid descriptor to each collection's resource template. |
| K02 | MUST | Declare the `{slug}.docs` collection. |
| K03 | MUST | Serve `enumerate` as bounded, cursor-paged pages in stable order. |
| K04 | MUST | Return bounded text or JSON members, and link bytes as `artifact://` resources. |
| K05 | MUST | Attach a complete observation to every member read whose request declares the extension. |
| K06 | MUST | Answer a matching `ifNoneMatch` with empty contents and `notModified`, after the same authorization as a full read. |
| K07 | MUST | For `listen` collections, make members and the collection subscribable from a restart-safe source. Emit initial invalidations after observation starts; the enumeration root then signals every member content, access or membership change. |
| K08 | MUST | Return resource links from declared search tools, restricted to readable resources. |
| K09 | MUST | Fill observations from domain records only, and keep caller input out of provenance. |
| K10 | MUST | Change `revision`, and signal the change for `listen` collections, whenever the returned text or the member's access descriptor changes. |

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/models.rs` | `CollectionDescriptor`, `Freshness`, `ChangeSignal`, `AccessModel`, `IndexingMode`, `Observation`, `AccessDescriptor`, explicit `ReadPolicy`, `ExternalRecord`, `SearchDeclaration`, `SearchHit` and bounded `SearchResults` |
| `src/server.rs` | capability declaration, descriptor attachment, observation attachment, conditional-read evaluation and search resource-link construction |
| `src/client.rs` | client capability declaration and typed observation parsing |
| `src/docs.rs` | protocol-independent `{slug}.docs` descriptors, typed document addresses and stable pages, consumed by `veoveo_mcp_contract::docs` |
| `src/identity.rs` | collection, document, revision and external-record identities |
| `macros/src/lib.rs` | compile-time document embedding and SHA-256, using the workspace's existing hashing implementation |
