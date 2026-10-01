# Knowledge Source Extension Design

This document is the normative contract for `ai.veoveo/knowledge-source`, the MCP
extension through which a server publishes resources as knowledge. A server that
declares the extension tells its consumers four things about each declared
collection: how to enumerate it, how long a read stays fresh, how changes arrive,
and who may read each record. Every read then carries a typed observation that
names the revision, content digest, modification time, and access descriptor of
what the caller received.

Agents, the Console, and the knowledge service consume the same resources that any
MCP client reads. The extension adds metadata. It adds no methods, and it never
replaces a server's resources with a second copy. The
[knowledge sharing design](../../docs/KNOWLEDGE.md) describes how the audit log records
knowledge reads and the `knowledge-mcp` catalog and index built on this extension.

The crate `veoveo-mcp-knowledge-extension` in this directory owns the typed
models, the capability declaration, the read and search helpers, and the shared
implementation of the well-known docs collection. The
[implementation plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md) tracks its
delivery.

## Status

Typed models, MCP negotiation and conditional-read helpers, document collection
paging, and compile-time document hashing are implemented. Rust hosted servers
declare their docs collections and share authenticated reads. Python and Node
document indexes use the same page shape. Their extension negotiation, gateway
audit integration, kernel provenance, and installed conformance are in progress
under Phase 6 of the implementation plan. No deployed server declares the extension yet.

The `contract` feature builds with default features disabled and depends only on
foundational Veoveo types, serialization, timestamps, and hashing. The `mcp`
feature adds the pinned Rust MCP SDK. Neither feature depends on MCP core or any
domain server. Audit and Store can therefore share observations without a runtime
dependency on MCP. The macro package hashes embedded document bytes during Rust
compilation and emits `include_str!` to track the document as a build input.

## Standards And Protocols

| Standard or protocol | Profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/draft) `2026-07-28` | Resources, resource templates, completion, resource links, `annotations.lastModified`, and `subscriptions/listen`, under the [hosted-server contract](../contract/DESIGN.md) |
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
| `access` | `work-context`: each observation carries a Work Context access descriptor. `profile`: any caller whose profile exposes the server may read every member |
| `indexing` | `content`: the knowledge service may index returned text. `metadata`: it may index titles and observation fields only. `none`: it catalogs the collection without indexing it |

The template's own `title`, `description`, and `mimeType` describe the collection
for display, and the extension does not repeat them. Stewardship, authoritative
scope, and approval belong to the installation control plane, so a server's
declaration never names them.

A member is bounded text or JSON that the governed agent read adapter admits.
Binary content is never a member. A record that owns bytes returns an
`artifact://` resource link in its JSON body.

## Observations

A read of a collection member returns ordinary contents. When the request declares
the extension, the result also carries one observation in its `_meta`. The member's
text item carries `annotations.lastModified` whenever the domain records a
modification time.

```json
{
  "contents": [{
    "uri": "time://events/0199b0f5-7b1e-7cc4-9a3d-5c1f7e0b2a91",
    "mimeType": "application/json",
    "text": "{…}",
    "annotations": { "lastModified": "2026-09-25T14:02:11Z" }
  }],
  "_meta": {
    "ai.veoveo/knowledge-observation": {
      "collection": "time.events",
      "revision": "7",
      "contentSha256": "4b1f…e09c",
      "observedAt": "2026-09-26T21:40:03Z",
      "modifiedBy": { "kind": "principal", "id": "0199…" },
      "access": {
        "tenant": "0199…",
        "workContext": "0199…",
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
| `revision` | yes | Opaque strong validator. It changes whenever the returned text or the access descriptor changes. Immutable members may use their content digest |
| `contentSha256` | yes | Lowercase hex SHA-256 of the exact UTF-8 bytes of the returned text |
| `observedAt` | yes | Server time when the domain produced this read |
| `modifiedAt` | when recorded | Time at which the domain last modified the current revision |
| `modifiedBy` | when recorded | Principal ID that produced the current revision, in the platform's shared human and service principal namespace |
| `access` | for `work-context` collections | Tenant, Work Context, owner, direct grants when the domain stores them, and data labels |
| `external` | for connector projections | `system`, `nativeId`, and optional `url` of the record in an external system of record, plus `mirroredAt` when the connector served a stored copy |

A domain server fills observations from its own records. The caller never supplies
provenance. An observation describes the read that produced it and grants no
authority.

The owner and direct grants use the foundational `AccessSubject` variants
`principal` and `group`. Modification attribution accepts a principal only.
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

## Search Tools

A server may publish search over its collections as an ordinary tool. The tool's
`_meta["ai.veoveo/knowledge-source"]` declares `{ "role": "search",
"collections": [...] }`. Its structured output lists results with `uri`, optional
`title`, an optional `snippet` of at most 320 characters, and an optional `score`. Its content carries one
`resource_link` per result. A search tool returns only resources the caller may
read. Knowledge content reaches a consumer through `resources/read`.

## Change Signals

A `listen` collection makes each member URI and its `enumerate` collection URI
subscribable through `subscriptions/listen`. Its events come from Store LIVE queries
with change-feed recovery through the shared `SubscriptionHub`, which satisfies
contract rule C27 across restarts and replicas. A process-local broadcast alone does not qualify.
A notification names the changed URI, or requests reconciliation, and carries no
content.

## Well-Known Docs Collection

Every server declares the `{slug}.docs` collection over `{scheme}://docs/{doc_id}`:

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
| K07 | MUST | For `listen` collections, make members and the collection subscribable, and emit changes from a restart-safe source. |
| K08 | MUST | Return resource links from declared search tools, restricted to readable resources. |
| K09 | MUST | Fill observations from domain records only, and keep caller input out of provenance. |
| K10 | MUST | Change `revision`, and signal the change for `listen` collections, whenever the returned text or the member's access descriptor changes. |

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/models.rs` | `CollectionDescriptor`, `Freshness`, `ChangeSignal`, `AccessModel`, `IndexingMode`, `Observation`, `AccessDescriptor`, `ExternalRecord`, and `SearchDeclaration` |
| `src/server.rs` | capability declaration, descriptor attachment, observation attachment, and conditional-read evaluation |
| `src/client.rs` | client capability declaration and typed observation parsing |
| `src/docs.rs` | protocol-independent `{slug}.docs` descriptors, typed document addresses and stable pages, consumed by `veoveo_mcp_contract::docs` |
| `src/identity.rs` | collection, document, revision and external-record identities |
| `macros/src/lib.rs` | compile-time document embedding and SHA-256, using the workspace's existing hashing implementation |
