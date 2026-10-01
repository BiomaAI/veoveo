# Knowledge Service

Knowledge catalogs approved source collections and retrieves relevant source members
with their revision, freshness and resource links. The service contract below defines
its hosted surface. The HTTP adapter serves search, embedding and catalog reads through
signed gateway identities. The library coordinates source listeners and reconciliation.
The binary runs tenant indexing workers with machine authentication and connection
rotation. Its Helm workload uses the shared embedding runtime and installation-owned
collection approvals and signing credentials.

## Standards And Protocols

| Standard or interface | Profile |
|---|---|
| MCP `2026-07-28` | Required hosted profile under [contract revision 3](../../mcp/contract/DESIGN.md); transport qualification pending |
| `ai.veoveo/knowledge-source` | [Collection declarations and observed reads](../../mcp/knowledge-extension/DESIGN.md), with 100-member enumeration pages |
| W3C DCAT 3 | Required JSON catalog shape; no RDF serialization or full DCAT conformance claim |
| SurrealDB 3.3.0 | BM25, filtered HNSW cosine search and native `search::rrf` with k=60 |
| JSON Schema 2020-12 | Schemars-generated contract models and checked request deserialization |
| OAuth 2.0 / RFC 6749, RFC 7523, RFC 8707 | Client credentials with a signed JWT assertion, explicit resource and Veoveo Work Context; HTTPS endpoints, with loopback HTTP for native fixtures |
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
builds another generation. `indexing.rs` discovers registrations, replaces the catalog
against current control authority and reconnects the authenticated gateway client.

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
`healthz` requires every configured tenant worker to have an active index or a complete
catalog-only selection, and checks Store's control pointer within two seconds.
`livez` reports HTTP process liveness independently of indexing. The reference
installation reaches readiness after indexing its sixteen approved collections.

## Packaging And Deployment

The image contains the shared-build Rust binary and CA certificates, using the existing
qualified Debian Trixie runtime family. It runs as UID 10001. The `knowledge` Helm
selection requires Gateway, Store and the shared GPU embedding runtime. Knowledge
requests CPU and memory; its index lives in Store and its embeddings run in the shared
GPU service. It requires no local volume or free-disk reservation.

The chart deploys one replica with a Recreate strategy. An update drains the worker
before starting its replacement. Store leases still fence an unexpectedly overlapping
process. Multiple active indexing replicas and standby workers are outside this hosting
profile. Public read APIs can use the library's separately qualified replica support.

`knowledge.existingConfigMap` supplies the tenant configuration files named by
`knowledge.indexingConfigKeys`. The chart mounts them under
`/etc/veoveo/knowledge/config`. `knowledge.configurationRevision` identifies those
public bytes and controls Pod rollout. Private keys and optional additional CA
certificates come from `knowledge.existingSigningSecret`, mounted read-only with mode
0440 under `/etc/veoveo/knowledge/signing`. Workers reread key files on connection
rotation. The embedding key comes from `embedding.apiKeySecret`; the chart generates
the embedding-space document from the qualified model, revision, dimension and runtime
image. Helm rejects inline credentials and missing deployment dependencies.

Startup and liveness probes use `livez`; readiness uses `healthz`. The 45-second
termination allowance covers worker cleanup and HTTP draining. Configuration changes
use a new public bundle revision; key rotation uses Kubernetes Secret projection and
the worker's next authenticated connection. The installation registers the service at
`http://knowledge-mcp:8800/knowledge/mcp` with health URL
`http://knowledge-mcp:8800/knowledge/healthz`.

## Indexing Configuration And Lifecycle

The binary requires one `--indexing-config` JSON file per tenant, up to 128 distinct
tenants. Each file selects the registered machine client and signing key file. The
control plane supplies its token endpoint, protected resource, scopes and default Work
Context. Exactly one indexing client may serve a tenant's single active generation.
The service rejects missing, ambiguous or invalid registrations before authentication.

```json
{
  "tenant": "example",
  "client_id": "knowledge-indexer",
  "key_id": "indexer-v1",
  "signing_algorithm": "ed_dsa",
  "private_key_file": "/etc/veoveo/knowledge/signing/private.pem",
  "trusted_ca_file": null,
  "chunk_settings": {
    "version": "structure-v1",
    "maxCharacters": 1500,
    "overlapCharacters": 150
  },
  "query_task": "Given a web search query, retrieve relevant passages that answer the query"
}
```

The connection adapter uses the existing JWT library for assertions carrying the
configured `kid`, a unique nonce and a two-minute lifetime. The pinned MCP SDK's JWT
flow cannot supply `kid`; its native Streamable HTTP client still owns MCP transport
and subscription lifecycle. Supported signing algorithms are RS256, ES256 and EdDSA.
HTTP redirects are disabled. Token responses have a 64 KiB limit, require a Bearer
token valid for 2–86,400 seconds, and must preserve requested scopes when supplied.
Authentication and MCP discovery share a 30-second deadline. Errors omit credentials
and provider response bodies.

Workers rotate at 80 percent of token lifetime and reread the private key and optional
CA file on each connection. They stop the current coordinator, release its lease and
close the old connection before authenticating again. A native control-plane LIVE
observer is established before configuration is read. Control or catalog changes
restart discovery; the Store rejects publication under an old control revision.
Unchanged registration fingerprints reuse the active generation after reconciliation.

Connection recovery allows eight failed epochs with delays of 1, 2, 4, 8, 16, 30 and
30 seconds. It does not poll source state. Exhaustion terminates the worker and shuts
down the process. Shutdown signals cancel all tenant workers; the binary allows
20 seconds for lease cleanup and ten seconds for HTTP shutdown. A failure in one
tenant worker makes aggregate readiness fail.

## Sources And Approval

The service discovers candidate sources from the gateway catalog: every server in
its profile whose `server/discover` result declares `ai.veoveo/knowledge-source`.
Discovery alone does not admit a source.

`source/discovery.rs` consumes an authenticated gateway peer and a typed set of
installation approvals. It starts a catalog listener, waits for its initial
invalidation, and traverses resource-template pages without the SDK catalog cache.
Pending discovery waits for native completion events. Any catalog change restarts
traversal. Unavailable sources, repeated cursors, duplicate approved descriptors and
missing approved collections fail the operation. Discovery allows 60 seconds, 1,000
cursors and 10,000 templates. It checks each source's scheme and hosted contract
revision through the approved source-contract read path before returning registrations.

`GatewaySource` opens each collection's root listener and waits for its first resource
invalidation before returning it to the coordinator. Later invalidations trigger
reconciliation; stream loss fails the source connection. The indexing worker publishes
the complete discovered catalog in one Store transaction that checks both the active
control revision and the previous catalog. A stale discovery cannot restore a revoked
collection. Failed discovery leaves the stored catalog unchanged.

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

Indexing reads commit window counters and retry receipts before delivery. The gateway
writer finalizes one record per collection, service actor and authorization context in
each five-minute interval. Denials keep individual records. The
[audit writer](../../platform/audit/DESIGN.md#indexing-read-windows) owns restart recovery,
window finalization and digest construction. The reference installation provisions its indexing client and signing Secret
separately from public worker configuration.

## Catalog

| Resource | Content |
|---|---|
| `knowledge://sources{?cursor}` | Paged `DataService` entries: server slug, contract revision, and declared collections |
| `knowledge://source/{server}` | One source with its collections and approval state |
| `knowledge://collection/{collection}` | One `Dataset`: descriptor, approval, matching active generation and caller-visible indexing statistics |
| `knowledge://docs`, `knowledge://docs/{doc_id}`, `knowledge://contract` | Well-known surface |

Catalog queries match the tenant, current approval and required scopes before decoding.
Source pages group and order in SQL, returning 100 sources plus one lookahead. Exact
source and collection requests bind their selected collection IDs to the query.
The source declaration revision comes from each stored registration; inconsistent
revisions within one source require rediscovery.

Collection statistics count readable indexed members and chunks. They report the
latest source observation and modification timestamps among those members. Store
applies the search admission predicate before aggregation, including profile URI
selection, current grants, source epochs, freshness and retention. Statistics are
absent without a matching active generation; an indexed collection with no readable
members has zero counts and no timestamps. Responses are private with a zero cache TTL.

Completion accepts the declared source, collection and document template arguments,
including page cursors. Source and collection candidates use current approvals,
required scopes and Knowledge resource policy. SQL applies the case-sensitive prefix,
deduplication, ordering and 101-row lookahead. The MCP response returns at most 100
values. Document completion uses the embedded document catalog. Both paths recheck
current request authority before delivery.

`subscriptions/listen` accepts up to 32 catalog resource addresses and optional
resource-list changes. Embedded documents are immutable and do not accept content
subscriptions. Catalog-list observation requires a server-target subscription grant;
individual addresses require read and subscription grants for that resource.
One host observer consumes Store LIVE invalidations and changefeed
recovery for catalog, member, source-sync, coordinator and authorization changes.
Each listener registers before its baseline read, waits for the database observation
to start, and then sends initial invalidations. Later writes trigger authorized SQL
snapshots; changed snapshots notify their requested identities. Hidden member activity
does not invalidate readable statistics. Domain content changes do not invalidate the
static discovery list.

Reconnection requests a fresh snapshot and invalidates accepted identities even when
the resulting bytes match. A watch channel coalesces writes without keeping a queue of
record payloads. The next grant, record, freshness or coordinator deadline schedules
one re-read; idle listeners do not poll. Token expiry and lost authority end the stream.
Snapshot work has a 60-second deadline, initial observation has ten seconds, and sink
delivery has ten seconds with request cancellation. The reference gateway exposes completion and subscriptions under the Knowledge read
scope. Installation rollout pairs that registration with its matching service image.

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
| `src/indexing.rs`, `src/indexing/` | tenant worker lifecycle, current control selection, machine JWT authentication, token rotation and atomic catalog publication |
| `src/source.rs`, `src/source/gateway.rs`, `src/source/discovery.rs` | checked source pages and bodies, approved catalog discovery, and native listeners over an authenticated gateway peer |
| `src/contract/` | owner scopes, typed routes, search models and shared domain re-exports |
| `src/embed.rs` | shared embedding client, bulk documents and interactive query batches |
| `src/authority.rs` | current policy, directory, session and registration checks |
| `src/mcp/` | typed setup, per-descriptor discovery, search, embed and catalog reads |
| `src/host.rs`, `src/bin/server.rs` | authenticated HTTP mount and installation configuration |
| `Dockerfile`, `deploy/helm/veoveo/templates/knowledge.yaml` | CPU service image, public configuration and private key mounts, shared embedding identity and deployment probes |
| `src/search.rs` | hybrid query, rank fusion, effective-access filtering, and result links |
| `platform/store/src/knowledge.rs` | typed catalog, chunk, and index-generation records |
| `platform/gateway/src/mcp/resource_read.rs`, `knowledge_indexing.rs` | collection approval, observation and label admission, and durable indexing audit windows |
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
`tests/source_gateway.rs` uses native MCP duplex streams to check discovery completion,
invalid catalogs and observation readiness after the SDK acknowledgement. Gateway tests
qualify mixed catalog/root filters and sources that end before observation starts.
`tests/indexing_host.rs` signs a real machine assertion and drives the worker through
HTTP authentication, token rotation, listener loss, reconnection and catalog-only
approval. It checks readiness and lease release, rejects invalid scopes, lifetimes,
empty tokens and redirects, and verifies generation and vector reuse after an unrelated
control edit. Its source and vectors are isolated fixtures. Store's catalog test races
two discoveries, rejects an obsolete control revision and removes a tenant's revoked
registrations without changing another tenant's catalog.
`testing/deployment-smoke/tests/knowledge_helm.rs` renders the actual chart to check
dependencies, secret references, model identity, liveness/readiness separation,
configuration-driven rollout and the absence of local storage or GPU requests.
`tests/support/catalog.rs` qualifies completion and caller-visible statistics through
signed HTTP requests. It corrupts excluded catalog and chunk metadata, checks two
replicas' source-sync invalidations, suppresses hidden writes and static-list wakes,
observes lease expiry without another mutation, and revokes an active listener.

The ignored `http::installed` test reads an installation's public control plane and
connects through its HTTPS gateway with an ordinary caller token. The caller must
have read access to every approved collection. It compares the complete paged catalog
with those approvals, checks that indexed collections share an active generation,
and searches each approved documentation collection by a source document's title.
Every returned link is read through the source server and checked against its content
digest and indexed revision. The test also exercises the public embedding tool.
It allows five minutes, performs no source mutations, closes its MCP connection, and
writes a private, create-only JSON report without tokens or document text.
Relative input and report paths resolve from the repository root. A running
hardware embedding workload is a prerequisite; the shared runtime's GPU acceptance
establishes hardware execution. These retrieval checks do not measure recall on the
domain evaluation set.

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
