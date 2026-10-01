# Knowledge Service

Knowledge catalogs approved source collections and retrieves relevant source members
with their revision, freshness and resource links. The service contract below defines
its hosted surface. Hosted transport, the source subscription coordinator and packaging
are pending; the library implements generation building and hybrid retrieval.

## Standards And Protocols

| Standard or interface | Profile |
|---|---|
| MCP `2026-07-28` | Required hosted profile under [contract revision 3](../../mcp/contract/DESIGN.md); transport qualification pending |
| `ai.veoveo/knowledge-source` | [Collection declarations and observed reads](../../mcp/knowledge-extension/DESIGN.md), with 100-member enumeration pages |
| W3C DCAT 3 | Required JSON catalog shape; no RDF serialization or full DCAT conformance claim |
| SurrealDB 3.3.0 | BM25, filtered HNSW cosine search and native `search::rrf` with k=60 |
| JSON Schema 2020-12 | Schemars-generated contract models and checked request deserialization |
| [Embedding runtime](../../platform/runtimes/embedding/DESIGN.md) | Internal HTTP API through the shared client; query priority is interactive and indexing priority is bulk |
| [Veoveo resource URI profile](../../platform/types/DESIGN.md) | Owner routes through `ResourceAddress` and component builders |

## Library And Coordination

`contract` exposes Knowledge scopes, resource addresses, search models and the shared
Knowledge domain types. It excludes Store, MCP integration, HTTP and asynchronous
runtime dependencies. `runtime` adds source ingestion and retrieval.

`Indexer::build` creates an inactive generation from the complete approved collection
set. It traverses at most 10,000 pages and 100,000 unique members per collection under
a one-hour build deadline. Each enumeration call has 30 seconds; each source read,
embedding and replacement has 120 seconds. Duplicate members, repeated cursors and
failed reads prevent coverage. The coordinator must establish source listeners before
building, settle queued invalidations and activate through Store's compare-and-set.
A failed build leaves the active generation untouched and can be explicitly reclaimed.

`Indexer::refresh` fences a member before source I/O. The same operation replaces its
chunks after a verified full read. `GatewaySource` uses an authenticated gateway peer, declares the extension on each
read and verifies the returned URI, observation and digest. It never connects to the
host named by a resource URI. Unavailable and unqualified not-found responses leave
the member stale. Subscription recovery, conditional refresh and restart coordination
are tracked in the [active plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-8-knowledge-service).

The indexer accepts only the installed `structure-v1` chunker version.
The chunker preserves source-byte ranges and starts sections at Markdown headings or
top-level JSON fields. It caps both Unicode characters and embedding input bytes, and
keeps overlap within a section. Metadata indexing constructs text from a title and a
closed set of observation fields; it excludes source bodies, access subjects and external
navigation URLs. Embedding requests contain at most 32 texts and 128 KiB.

`SearchCaller::from_policy` resolves collection selections through the shared policy
evaluator and recomputes Work Context membership from one current catalog. It checks
the signed active context's tenant, membership and policy revision. Each collection
carries the owning scheme and profile URI selectors; both SQL ranking inputs enforce
them. The transport must establish current identity/session authority before calling
this constructor. Search requests can narrow
its exposed collections. They cannot supply tenant, groups, clearance or membership.
Store applies those predicates to both ranking inputs and selects one best chunk per
member before returning rows. Search begins with 128 candidates per ranking and expands
to 512, 2,048 and 8,192 when chunk duplication prevents a full result page. Exhausting
that budget returns a diagnostic that asks the caller to narrow collection selection.
Every search has a 60-second deadline.

The final access check uses the existing shared `decide` implementation through the
foundational `AccessGrant` trait. An access discrepancy aborts the response; it never
silently drops a decoded row and returns a shortened page. Source record/grant expiry
is evaluated at query time and again before returning results.

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
window, as [the audit design](../../docs/AUDIT.md#event-selection) specifies, because the chunk
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

The service embeds through the shared [embedding runtime](../../platform/runtimes/embedding/DESIGN.md)
with `veoveo-embedding-client`. Documents embed with `embed_documents`. Queries embed
with `embed_query` and the task `Given a web search query, retrieve relevant passages
that answer the query`, unless qualification selects another. Search queries run at
interactive priority and indexing at bulk priority, so a rebuild does not delay
searches. `knowledge-mcp` itself needs no GPU.

An index generation records the embedding space selected by deployment (model,
revision, dimension, and vLLM image digest), the query task, and the chunker version.
A change to any of them builds a new generation beside the active one. The active
pointer moves when the new generation covers every approved collection, and vectors
from different spaces never share an index. The knowledge evaluation set is the
workload the runtime's model selection uses.

The [knowledge domain contract](../../platform/knowledge/contract/DESIGN.md) owns generation
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
chunk's access descriptor. Every policy requires the same tenant and clearance for
every data label, and the caller must hold the collection's `requiredScopes`.
The source's explicit `readPolicy` selects tenant sharing,
owner/grant access, Work Context sharing, sharing in the selected Work Context, or
subject access constrained to the current context and optional profile. A membership-only
policy requires membership in the selected context even for the owner. Record expiry
ends every read path; an individual grant's expiry ends access through that grant.
A stored Work Context alone grants no read permission.
The search query applies these predicates and current collection exposure, including scheme/prefix/template URI selectors, inside
SurrealDB before candidate decoding, ordering and LIMIT. Vector retrieval may require
additional candidate pages to fill the result limit after database filtering.

The service uses `veoveo_mcp_contract::access::decide` for the final tenant, clearance
and subject/context decision, enabling context membership only when the source policy
allows it. It also enforces the source's selected-context and profile requirements.
`profile` collections require that the caller's profile exposes the source collection.
A result the caller may not read never appears, including its title and snippet.

A caller who needs current content reads the member URI. That read goes to the
owning server, which applies its own authorization and returns a fresh observation.

## Embed Tool

`embed` is a direct tool for callers outside the platform namespace: agent kernels,
which run in the `veoveo-agents` namespace, and external MCP hosts. It takes up to 32
texts and a mode, `document` or `query` with a task, and returns vectors with their
embedding space. The gateway authorizes and audits it like any other tool, and an
agent's episode budget counts it. Platform services call the runtime directly instead.

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/index.rs`, `src/chunk.rs` | fenced source reads, bounded enumeration, source-byte chunking and inactive generation builds |
| `src/source.rs`, `src/source/gateway.rs` | checked source pages and bodies; authenticated gateway peer adapter |
| `src/contract/` | owner scopes, typed routes, search models and shared domain re-exports |
| `src/embed.rs` | index and search use of `veoveo-embedding-client`; hosted `embed` wiring pending |
| `src/search.rs` | hybrid query, rank fusion, effective-access filtering, and result links |
| `platform/store/src/knowledge.rs` | typed catalog, chunk, and index-generation records |
| `platform/gateway/src/mcp/resource_read.rs` | observation attached to the read's audit record, and indexing-client windows |
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
