# Knowledge Sharing

Veoveo servers share knowledge through MCP resources. Each server stays the system
of record for its domain and publishes collections through the
[`ai.veoveo/knowledge-source`](../mcp/knowledge-extension/DESIGN.md) extension. The
gateway records what each caller observed. The `knowledge-mcp` server catalogs every
declared collection and indexes the approved ones for search.

`knowledge-mcp` answers two questions for agents and people: where does the
knowledge about a subject live, and how current is what Veoveo has seen of it.
Search returns links to the owning server's resources, and every result states the
freshness of the cached revision behind it.

## Status

Designed. Neither the extension crate nor `knowledge-mcp` exists. The
[implementation plan](KNOWLEDGE_AND_IDENTIFIERS_PLAN.md) sequences delivery. When the
`servers/knowledge-mcp` crate is created, the service sections of this document move
into its `DESIGN.md`, and this document keeps the cross-component flow.

```text
crate       veoveo-knowledge-mcp
folder      servers/knowledge-mcp
slug        knowledge
URI scheme  knowledge
MCP         /knowledge/mcp
admin REST  /knowledge/admin
health      /knowledge/healthz
```

## Standards And Protocols

| Standard or protocol | Profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/draft) `2026-07-28` | Hosted-server profile under [contract revision 3](../mcp/contract/DESIGN.md): tools, resources, templates, completion, `subscriptions/listen`, and official Tasks |
| `ai.veoveo/knowledge-source` | Consumed as a client on every source read; declared as a server for the `knowledge.docs` collection |
| [W3C DCAT 3](https://www.w3.org/TR/vocab-dcat-3/) | Catalog shape: the installation catalog is a `dcat:Catalog`, each source server a `dcat:DataService`, each collection a `dcat:Dataset`. Resources return JSON with DCAT-aligned field names, not RDF |
| [W3C PROV-O](https://www.w3.org/TR/prov-o/) | Ledger model: observations are `prov:Entity`, reads and index builds are `prov:Activity`, principals and services are `prov:Agent`, and `prov:actedOnBehalfOf`, `prov:used`, `prov:wasDerivedFrom`, and `prov:wasInvalidatedBy` name the relations. Export as PROV-O JSON-LD is future work |
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) and [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html) | Revalidation with strong validators and freshness lifetimes, applied through the extension's conditional reads |
| SurrealDB 3.2.4 | Catalog, index, and ledger records in the platform store; `FULLTEXT` BM25 and `HNSW` vector indexes |
| fastembed `7.1.0` with candle `0.11.0` | `Qwen/Qwen3-Embedding-0.6B` through the `qwen3` and `cuda` features, with default features disabled |
| `ort` `2.0.0-rc.13` | Compiled only because fastembed requires it; this service loads no ONNX model. The pre-release pin record is below |
| NVIDIA CUDA | Embedding runs on a hardware GPU and fails closed without one |
| [JSON Schema 2020-12](https://json-schema.org/draft/2020-12/) | Generated schemas for every tool input, output, and resource body |

## Sources And Approval

The service discovers candidate sources from the gateway catalog: every server in
its profile whose `server/discover` result declares `ai.veoveo/knowledge-source`.
Discovery alone does not admit a source.

The control-plane document approves each collection with a typed knowledge entry.
The entry names the collection, whether the service may index it, the groups that
steward it, the subjects for which it is authoritative, and the data labels it may
contain. An installation can therefore catalog a collection without indexing it,
or refuse a collection whose labels exceed the service's clearance.

The service reads sources through the gateway as its own registered machine client.
Control-plane policy grants that client read access to approved collections only.
The gateway authorizes, audits, and ledgers those reads exactly as it does for any
other caller.

## Catalog

| Resource | Content |
|---|---|
| `knowledge://sources{?cursor}` | Paged `DataService` entries: server slug, contract revision, and declared collections |
| `knowledge://source/{server}` | One source with its collections and approval state |
| `knowledge://collection/{collection}` | One `Dataset`: descriptor, approval, index generation, member count, newest observation, and last change event |
| `knowledge://observation/{observation_id}` | One observation with its reads and derivations, paged |
| `knowledge://docs`, `knowledge://docs/{doc_id}`, `knowledge://contract` | Well-known surface |

Collection resources are subscribable, and the service notifies them as indexing
progresses.

## Index

The index is a cache over approved `content` and `metadata` collections. For each
collection the service enumerates members through the declared `enumerate` pages,
reads each member with the extension declared, and stores chunks keyed by member URI
and revision. `listen` collections then stay current through `subscriptions/listen`.
`revalidate` collections are revalidated after their `maxAgeSeconds`. Immutable
members are read once per revision.

A change notification marks the member stale in the same transaction that records
the invalidation. The service then re-reads the member, replaces its chunks, and
records the new observation. A lost event stream triggers reconciliation by
conditional reads over the collection's pages.

Chunks carry text, member URI, revision, collection, and the observation's access
descriptor. Each chunk has a BM25 `FULLTEXT` entry and a 1024-dimension `HNSW`
entry with cosine distance. Search runs both indexes and fuses their rankings with
reciprocal rank fusion.

### Embedding

The service embeds with `Qwen/Qwen3-Embedding-0.6B` on candle's CUDA backend.
Documents embed without an instruction. Queries embed with one fixed retrieval
instruction, recorded in the index generation. Chunk length and overlap are index
generation settings, qualified on an evaluation set drawn from the first adopting
collections.

The container requests `nvidia.com/gpu` and opens the CUDA device before it reports
ready. A missing device, driver capability, or CUDA context is a startup failure.
The service has no CPU embedding path.

Model files are pinned by Hugging Face revision and SHA-256 and packaged into the
image. The service never downloads a model at runtime. fastembed's default features
stay disabled, which also removes the build-time ONNX Runtime download.

An index generation records the model ID, revision, file digests, dimension, query
instruction, and chunker version. A change to any of them builds a new generation
beside the active one. The active pointer moves when the new generation covers every
approved collection, and vectors from different generations never share an index.

### `ort` pre-release pin

fastembed `7.1.0` depends unconditionally on `ort` `=2.0.0-rc.13`, and `ort` has
published no stable 2.x release. This service compiles `ort` without its download
feature and executes no ONNX Runtime session. The pin follows fastembed exactly.
Removal happens when fastembed moves to a stable `ort` release or makes it
optional, whichever comes first. The owner reviews the pin with each fastembed
release.

## Search

`search` is a direct tool. Its input takes a query, optional collections and entity
kinds, and a result limit of at most 20. Its output lists results with member URI,
title, a snippet of at most 320 characters, the fused score, and a freshness summary: revision,
`lastModified`, `observedAt`, and whether the cached revision has outlived its
collection's freshness. Its content carries one `resource_link` per result.

Before returning a result, the service applies the caller's effective access to the
chunk's access descriptor: same tenant, Work Context membership or a grant with
`read`, and clearance for every data label. It evaluates the same predicate that the
Artifact store applies. `profile` collections require only that the caller's profile
exposes the source server. A result the caller may not read never appears, including
its title and snippet.

A caller who needs current content reads the member URI. That read goes to the
owning server, which applies its own authorization and returns a fresh observation.

## Ledger

The ledger records what Veoveo observed and who read it. It lives in the platform
store and is append-only. A correction adds a record that supersedes an earlier one.

| Record | PROV role | Contents | Writer |
|---|---|---|---|
| `knowledge_observation` | `Entity` | Member URI, collection, revision, `contentSha256`, `lastModified`, `modifiedBy`, access descriptor, and first `observedAt`; unique on URI, revision, and digest | Gateway |
| `knowledge_read` | `Activity` | Observation, actor, delegating principal, managed-agent instance and episode when present, Work Context, request ID, `traceparent`, time, and outcome: `full`, `not_modified`, `denied`, `not_found`, or `unavailable` | Gateway |
| `knowledge_derivation` | `Activity` | Index generation, the chunks it produced, and the observations it used | Knowledge service |
| `knowledge_invalidation` | `wasInvalidatedBy` | Superseded observation, cause (change event, revalidation, or deletion), and time | Knowledge service |

The gateway writes observation and read records on the resource-read path for every
collection that declares the extension, whether the caller is an agent, the Console,
an external host, or this service. It declares the extension on each upstream read of
a declaring server, records the observation, and forwards the observation only to
callers that declared the extension themselves. It commits the records before it
returns the result, so an answer never cites a read the ledger lacks. Authority fields come from the
gateway's verified request context. The records hold digests and identities, never
member content.

A `not_found` or `unavailable` outcome leaves the prior observation unresolved. The
service invalidates an observation only on a change event, a newer revision, or a
definitive deletion from the owning server.

Ledger retention follows the classification of the observed member. Deleting a
member removes its chunks and keeps its observations and reads until retention
expires.

## Agent Context

The governed agent read adapter keeps each observation beside the text it admits.
The model receives one compact provenance line per item: collection, revision,
`lastModified`, and `observedAt`. It can therefore state how current its evidence
is. The adapter's byte and read budgets count those lines.

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/catalog/` | gateway discovery, control-plane approval, and DCAT-shaped resources |
| `src/index/` | enumeration, change subscriptions, reconciliation, chunking, and index generations |
| `src/embed/` | CUDA device admission and the Qwen3 embedding model |
| `src/search.rs` | hybrid query, rank fusion, effective-access filtering, and result links |
| `src/ledger.rs` | derivation and invalidation records |
| `platform/store/src/knowledge.rs` | typed catalog, chunk, generation, and ledger records |
| `platform/gateway/src/mcp/resources.rs` | observation and read records on the resource-read path |
| `agents/kernel/src/resource.rs` | observation retention and provenance lines in model context |

## Verification

- Contract rules K01 through K10 pass against this server's `knowledge.docs`
  collection.
- Unit and store tests prove that a caller never receives a result outside its
  effective access, including title and snippet.
- Store tests prove observation deduplication, supersession, read outcomes, and
  commit ordering on the gateway read path.
- Change-event tests prove invalidation, re-read, and reconciliation after a lost
  stream.
- The embedding test runs on a hardware GPU. It proves that startup fails without
  a CUDA device and that the loaded model matches its pinned digests.
- The evaluation set measures recall at 10 for the qualified chunk settings, and
  each index generation records its result.
