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

The extension crate, Rust, Python and Node document adapters, gateway read auditing,
and kernel provenance are implemented. Store implements approval fingerprints,
generation-specific indexes, fenced member replacement and SQL-selected candidate
pages through the [knowledge storage contract](../platform/store/src/knowledge/DESIGN.md).
Installed qualification is in progress.
The `knowledge-mcp` service has not been implemented. The
[implementation plan](PLATFORM_FOUNDATIONS_PLAN.md) sequences delivery. When the
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
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) and [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html) | Revalidation with strong validators and freshness lifetimes, applied through the extension's conditional reads |
| SurrealDB 3.3 | Catalog and index records in the platform store; `FULLTEXT` BM25 and `HNSW` vector indexes |
| [Embedding runtime](../platform/runtimes/embedding/DESIGN.md) | Shared vLLM service for `Qwen/Qwen3-Embedding-0.6B`, reached through `veoveo-embedding-client` |
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
The gateway authorizes those reads exactly as it does for any other caller. It
audits them as an approved indexing client: one record per collection and five-minute
window, as [the audit design](AUDIT.md#event-selection) specifies, because the chunk
records already hold each member's revision.

## Catalog

| Resource | Content |
|---|---|
| `knowledge://sources{?cursor}` | Paged `DataService` entries: server slug, contract revision, and declared collections |
| `knowledge://source/{server}` | One source with its collections and approval state |
| `knowledge://collection/{collection}` | One `Dataset`: descriptor, approval, index generation, member count, newest observation, and last change event |
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

A change notification marks the member's chunks stale. The service then re-reads the
member and replaces its chunks with ones keyed by the new revision. A lost event
stream triggers reconciliation by conditional reads over the collection's pages. A
`not_found` or unavailable read leaves the chunks marked stale; the service removes
them only after a change event, a newer revision, or a definitive deletion from the
owning server. The chunk records are the service's whole account of what it cached
and from which revision.

A change to a member's access descriptor is a change like any other: the owning
server changes the revision and signals it (rule K10), and the service re-reads the
member and replaces its chunks. For `revalidate` collections, an access change reaches
the index within `maxAgeSeconds`.

The chunker splits text along its structure, at Markdown headings and top-level JSON
fields, and caps each chunk by characters well below the model's context. The cap and
overlap are chunker settings that the index generation records and the evaluation set
qualifies.

Chunks carry text, member URI, revision, collection, and the observation's access
descriptor. Each chunk has a BM25 `FULLTEXT` entry and an `HNSW` entry with cosine
distance in its index generation's vector index, whose dimension is the embedding
space's: 1024 for `Qwen3-Embedding-0.6B`. Search runs both indexes and fuses their rankings with
reciprocal rank fusion.

### Embedding

The service embeds through the shared [embedding runtime](../platform/runtimes/embedding/DESIGN.md)
with `veoveo-embedding-client`. Documents embed with `embed_documents`. Queries embed
with `embed_query` and the task `Given a web search query, retrieve relevant passages
that answer the query`, unless qualification selects another. Search queries run at
interactive priority and indexing at bulk priority, so a rebuild does not delay
searches. `knowledge-mcp` itself needs no GPU.

An index generation records the embedding space the runtime reports (model,
revision, dimension, and vLLM image digest), the query task, and the chunker version.
A change to any of them builds a new generation beside the active one. The active
pointer moves when the new generation covers every approved collection, and vectors
from different spaces never share an index. The knowledge evaluation set is the
workload the runtime's model selection uses.

The [knowledge domain contract](../platform/knowledge/contract/DESIGN.md) owns generation
identities and specifications below both Store and the service. Store creates a
separate chunk table and vector index for each generation, checks current approval
fingerprints before writes and activation, and reclaims retired generations explicitly.

## Search

`search` is a direct tool. Its input takes a query, optional collections and entity
kinds, and a result limit of at most 20. Its output lists results with member URI,
title, a snippet of at most 320 characters, the fused score, and a freshness summary: revision,
`lastModified`, `observedAt`, and whether the cached revision has outlived its
collection's freshness. Its content carries one `resource_link` per result.

Before returning a result, the service applies the caller's effective access to the
chunk's access descriptor: same tenant, Work Context membership or a grant with
`read`, and clearance for every data label. The search query narrows candidates inside
SurrealDB by tenant, the caller's Work Contexts and grant subjects, and the caller's
clearance labels, and it fetches more candidates than the result limit because these
filters apply after the vector search. The service then decides each remaining
candidate with `veoveo_mcp_contract::access::decide`, the predicate the Artifact service
uses, and fetches more when too few pass. `profile` collections require only that the
caller's profile exposes the source server. A result the caller may not read never
appears, including its title and snippet.

A caller who needs current content reads the member URI. That read goes to the
owning server, which applies its own authorization and returns a fresh observation.

## Embed Tool

`embed` is a direct tool for callers outside the platform namespace: agent kernels,
which run in the `veoveo-agents` namespace, and external MCP hosts. It takes up to 32
texts and a mode, `document` or `query` with a task, and returns vectors with their
embedding space. The gateway authorizes and audits it like any other tool, and an
agent's episode budget counts it. Platform services call the runtime directly instead.

## Knowledge Reads In The Audit Log

Veoveo keeps [one audit log](AUDIT.md). A read of a declared collection is an ordinary
audit record for `resources/read`, and the record carries the typed observation the owning
server returned: collection, revision, `contentSha256`, `modifiedAt`, `modifiedBy`,
and whether the read returned content or confirmed an unchanged revision. Denied
and failed reads record an outcome and reason without claiming source provenance.
The record already names the actor, delegating principal, managed
agent, Work Context, profile, and trace. "What did this answer rely on?" is therefore
an audit query by trace or agent episode.

The gateway declares the extension on each upstream read of a declaring server and
forwards the observation only to callers that declared the extension themselves. For
these reads it commits the audit record after the upstream response arrives and before
it returns the result, so no caller receives content whose revision the log lacks.
Denied reads are audited before any upstream call, as every denial is. The record holds
digests and identities, never member content, and follows the audit log's retention.
The audit observation excludes external navigation URLs, which can carry signed
credentials; external system and native record identities remain available for lookup.
Reads by the knowledge service itself use the indexing window described in
[Sources And Approval](#sources-and-approval).

## Agent Context

The governed agent read adapter declares the extension on its reads and keeps each
observation beside the text it admits.
The model receives one compact provenance line per item: collection, revision,
`modifiedAt`, and `observedAt`. It can therefore state how current its evidence
is. The adapter's byte and read budgets count those lines.

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/catalog/` | gateway discovery, control-plane approval, and DCAT-shaped resources |
| `src/index/` | enumeration, change subscriptions, reconciliation, chunking, and index generations |
| `src/embed.rs` | index and search use of `veoveo-embedding-client`, and the `embed` tool |
| `src/search.rs` | hybrid query, rank fusion, effective-access filtering, and result links |
| `platform/store/src/knowledge.rs` | typed catalog, chunk, and index-generation records |
| `platform/gateway/src/mcp/resources.rs` | observation attached to the read's audit record, and indexing-client windows |
| `agents/kernel/src/resource.rs` | observation retention and provenance lines in model context |

## Verification

- Contract rules K01 through K10 pass against this server's `knowledge.docs`
  collection.
- Unit and store tests prove that a caller never receives a result outside its
  effective access, including title and snippet, and that a restricted caller still
  receives a full page when enough readable results exist.
- A test revokes a grant and shows the member's results disappear after the change
  signal.
- Gateway tests prove that a declared read's audit record carries its observation and
  outcome, that it commits before the result returns, and that indexing reads produce
  one record per collection window.
- Change-event tests prove invalidation, re-read, and reconciliation after a lost
  stream.
- Index tests prove that a change of embedding space builds a new generation and never
  mixes vectors from two spaces.
- An `embed` tool test proves the gateway authorizes and audits each call and that the
  episode budget counts it.
- The throughput measurement records chunks per second for each Phase 7 collection and
  for a full rebuild, with searches running concurrently.
- The evaluation set measures recall at 10 for the qualified chunk settings, and
  each index generation records its result.
