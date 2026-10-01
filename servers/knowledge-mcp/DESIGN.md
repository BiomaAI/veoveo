# Knowledge Service

Knowledge catalogs approved source collections and retrieves relevant source members
with their revision, freshness and resource links. The service contract below defines
its hosted surface. The HTTP adapter serves search, embedding and catalog reads through
signed gateway identities. The library coordinates source listeners and reconciliation.
The binary's authenticated discovery/reconnection wiring and packaging are pending.

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
runtime dependencies. `runtime` adds source ingestion, retrieval and the HTTP server.

`Indexer::prepare` creates an inactive generation from the complete approved collection
set; `build` also populates it. The coordinator uses `prepare` and reconciles that
set. It traverses at most 10,000 pages and 100,000 unique members per collection under
a one-hour build deadline. Each enumeration call has 30 seconds; each source read,
embedding and replacement has 120 seconds. Duplicate members, repeated cursors and
failed reads prevent coverage. The coordinator must establish source listeners before
building, settle queued invalidations and activate through Store's compare-and-set.
A failed build leaves the active generation pointer untouched and can be explicitly reclaimed.

`Coordinator::run` owns one authenticated source connection lifetime. It claims the
tenant's 30-second Store lease and renews every ten seconds, including during slow
source reads. Store-issued lease and collection tickets carry process and source epochs.
Every index mutation checks them in its transaction. Listener loss or cancellation
releases the lease, which hides mutable results; an unreachable process loses its lease
through expiry. A later run acknowledges fresh listeners and fully enumerates the
collections before serving the matching active generation again. A changed specification
builds another generation. The host must discover registrations and reconnect the
authenticated gateway client; that production wiring is pending.

The coordinator requires 1–1,024 collections and positive mutable freshness lifetimes.
It queues at most 1,024 source invalidations and starts listeners within one hour, with
30 seconds per acknowledgement. Reconciliation must settle within one hour. Changes
interrupt an ongoing traversal and advance the affected collection's epoch before
another read begins. The source adapter must invalidate the enumeration root on any
member content, access or membership change. Mutable collections also receive conditional
revalidation half a freshness lifetime after their last traversal completes. SQL hides
members whose freshness deadline passes before that traversal reaches them.

`Indexer::refresh` fences a member before source I/O. The same operation replaces its
chunks after a verified full read. `GatewaySource` uses an authenticated gateway peer, declares the extension on each
read and verifies the returned URI, observation and digest. It never connects to the
host named by a resource URI. Unavailable and unqualified not-found responses leave
the member stale. A conditional response reuses chunks only when every observation
field except `observedAt` matches the cached observation and the enumeration title is
unchanged. Store normalizes `notModified` before updating the cached observation.
Changed titles require a full read and new embeddings.

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
When HNSW returns fewer candidates than its window, Store completes that ranking with
an exact SQL cosine-distance query under the same permissions and a 10-second timeout.

The final access check uses the existing shared `decide` implementation through the
foundational `AccessGrant` trait. An access discrepancy aborts the response; it never
silently drops a decoded row and returns a shortened page. Source record/grant expiry
is evaluated at query time and again before returning results.

## Hosted Requests

The `knowledge-mcp` binary mounts stateless Streamable HTTP and authenticated document
HTTP routes under the installation's Knowledge mount. The shared transport enforces
final MCP request metadata and the serialized response limit. Host admission and an
internal JWT signature check run before protocol dispatch.

`authority.rs` reads the active control-plane revision for each domain request. It
checks the signed token's lifetime, current OAuth registration, Work Context membership,
session-family revocation and individual JWT revocation. Store checks the enabled
enterprise, tenant and both source and delegated actor identities in SQL. The gateway
synchronizes those identities before forwarding. Managed requests also bind to the
current instance, generation, dispatch epoch and published tool allowlist; the gateway
owns runtime-template admission.

Tool and resource discovery evaluate each descriptor with the shared policy evaluator.
A narrow tool rule does not require a separate rule granting the whole server. Search
and embedding recheck authority before delivery. A changed control revision or search
collection registration aborts delivery. Tools allow 60 seconds for their domain work;
each admission or delivery check has a separate 10-second deadline. Resource reads
have a 60-second deadline and catalog SQL statements stop after 10 seconds.

The binary reads Store credentials and internal public trust from installation
configuration. It connects to the shared embedding endpoint using its API key and a
JSON `EmbeddingSpace` file. Database migrations belong to installation bootstrap.
`healthz` checks that Store has a current control revision; installed readiness and
coordinator readiness require further qualification.

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
The gateway applies ordinary caller authorization plus the registered collection and
installation label ceiling. `GatewaySource` attaches the typed collection and
enumeration/member intent to each read. The indexer checks observation admission before
chunking or embedding; member construction checks it again before storage.

Indexing reads currently commit one audit record per read. The hosted integration must
add one record per collection and five-minute window, as
[the audit design](../../docs/AUDIT.md#event-selection) specifies. That aggregation and
the installation's indexing client are pending with coordinator delivery.

## Catalog

| Resource | Content |
|---|---|
| `knowledge://sources{?cursor}` | Paged `DataService` entries: server slug, contract revision, and declared collections |
| `knowledge://source/{server}` | One source with its collections and approval state |
| `knowledge://collection/{collection}` | One `Dataset`: descriptor, approval and index generation; indexing statistics pending |
| `knowledge://docs`, `knowledge://docs/{doc_id}`, `knowledge://contract` | Well-known surface |

Catalog queries match the tenant, current approval and required scopes before decoding.
Source pages group and order in SQL, returning 100 sources plus one lookahead. Exact
source and collection requests bind their selected collection IDs to the query.
The source declaration revision comes from each stored registration; inconsistent
revisions within one source require rediscovery.

The current collection response includes its descriptor, approval and matching active
generation. Member counts, observation/change statistics, collection subscriptions and
completion are pending with coordinator integration.

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
them only after the owning server definitively confirms deletion. A member absent from
a fresh enumeration keeps its cached rows under the older source epoch, which SQL
excludes. Generation reclamation removes those rows with the rest of the cache.

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
`lastModified`, `observedAt`, and `stale`. SQL excludes expired observations; the
delivery check rejects a response if a result expires after selection. Returned results
therefore have `stale: false`. Its content carries one `resource_link` per result.

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
| `src/coordinator.rs` | tenant lease renewal, listener ownership, change reconciliation, freshness timers and generation recovery |
| `src/source.rs`, `src/source/gateway.rs` | checked source pages and bodies; authenticated gateway peer adapter |
| `src/contract/` | owner scopes, typed routes, search models and shared domain re-exports |
| `src/embed.rs` | shared embedding client, bulk documents and interactive query batches |
| `src/authority.rs` | current policy, directory, session and registration checks |
| `src/mcp/` | typed setup, per-descriptor discovery, search, embed and catalog reads |
| `src/host.rs`, `src/bin/server.rs` | authenticated HTTP mount and installation configuration |
| `src/search.rs` | hybrid query, rank fusion, effective-access filtering, and result links |
| `platform/store/src/knowledge.rs` | typed catalog, chunk, and index-generation records |
| `platform/gateway/src/mcp/resource_read.rs`, `knowledge_indexing.rs` | collection approval, observation and label admission, and per-read audit; indexing windows pending |
| `agents/kernel/src/resource.rs` | observation retention and provenance lines in model context |

## Verification

`tests/http.rs` runs the maintained MCP client against real HTTP listeners and owned
SurrealDB fixtures. It checks search links, cross-replica reads, malformed hidden
catalog rows, query batches, scope reduction, directory disablement, browser-session
revocation and JWT revocation during an embedding call. Its vectors are synthetic;
the shared runtime owns GPU inference qualification. `platform/store/tests/knowledge.rs`
checks SQL source paging across 105 owners and exact selection before decoding.
`tests/coordination.rs` checks listener ordering, changes during a paused source read,
lease renewal during that read, scheduled revalidation, source-loss recovery, generation reuse, conditional
embedding reuse and rejection of unchanged revisions with changed access. Store's
coordinator tests qualify takeover, late member/coverage rejection and SQL freshness
exclusion before decoding malformed cached rows. These fixtures use synthetic embeddings.

Installed acceptance also requires the following cases:

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
