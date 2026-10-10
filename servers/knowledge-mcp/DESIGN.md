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
| MCP `2026-07-28` | Required hosted profile under [contract revision 4](../../mcp/contract/DESIGN.md); transport qualification pending |
| `ai.veoveo/knowledge-source` | [Collection declarations and observed reads](../../mcp/knowledge-extension/DESIGN.md), with 100-member enumeration pages |
| W3C DCAT 3 | Required JSON catalog shape; no RDF serialization or full DCAT conformance claim |
| SurrealDB 3.3.0 | BM25, filtered HNSW cosine search and native `search::rrf` with k=60 |
| JSON Schema 2020-12 | Schemars-generated contract models and checked request deserialization |
| `veoveo.ai/knowledge-installed-outcome/v1` | Private installed consumer outcome with request intents, protocol codes, selected discovery identities, native observer and SDK cleanup states; contains the successful baseline report and optional cold-start observations |
| `veoveo.ai/knowledge-cold-start-input/v1` | Closed private unattended-startup fixture with installation, workload and qualified embedding prerequisites; outside the public MCP API |
| Kubernetes streaming initial events | Kubernetes 1.32+ API-server support for `sendInitialEvents`, `NotOlderThan` and the initial-events-end bookmark is required by the observation-only cold-start case; kubectl uses the installation context |
| `veoveo.ai/knowledge-retrieval-evaluation/v1` | Private JSON benchmark reports with generation identity, judgments, ranks, recall and throughput; outside the public MCP surface |
| OAuth 2.0 / RFC 6749, RFC 7523, RFC 8707 | Client credentials with a signed JWT assertion, explicit resource and Veoveo Work Context; HTTPS endpoints, with loopback HTTP for native fixtures |
| [Embedding runtime](../../platform/runtimes/embedding/DESIGN.md) | Internal HTTP API through the shared client; query priority is interactive and indexing priority is bulk |
| [Veoveo resource URI profile](../../platform/types/DESIGN.md) | Owner routes through `ResourceAddress` and component builders |

Machine indexing configuration uses camelCase JSON members and rejects retired or unknown members before credential files are read. Source page roots admit `items`, optional `limit` and `nextCursor`. Each item projects a typed URI and optional title from its source-owned body; source-specific item fields remain with their source. A retired root cursor cannot silently end traversal. DCAT class identifiers keep the DCAT 3 standard spelling.

## Library And Coordination

`contract` exposes Knowledge scopes, resource addresses, search models and the shared
Knowledge domain types. It excludes Store, MCP integration, HTTP and asynchronous
runtime dependencies. `runtime` adds source ingestion, retrieval and the HTTP server.

The MCP router binds search and embedding directly to typed request handlers.
RMCP derives each input schema from the handler's parameter type and decodes that
type before invocation. Each handler admits its declared scope and tool target,
limits domain work to sixty seconds, and rechecks current authority before returning
results. Invalid tool arguments return RMCP's completed `isError: true` validation
result before domain work. Search still applies caller policy in SQL before ranking
and decoding.

`Indexer::prepare` creates an inactive generation from the complete approved collection
set; `build` also populates it. The coordinator uses `prepare` and reconciles that
set. It traverses at most 10,000 pages and 100,000 unique members per collection under
a one-hour build deadline. Each enumeration call has 30 seconds; each source read,
embedding and replacement has 120 seconds. Duplicate members, repeated cursors and
failed reads prevent coverage. The coordinator must establish source listeners before
building, settle queued invalidations and activate through Store's compare-and-set.
A failed build leaves the active generation pointer untouched and can be explicitly reclaimed.

Store binds generation requirements and indexed chunks through nominal native records.
Each persisted requirement contains only its collection record link and approval
revision; activation reads those declared children. Chunk rows preserve native member
and collection links, authorization facts, vectors and the checked Observation codec.
Requirement links do not cascade collection deletion into generations.

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
Conditional validation failures name the collection and differing field in service
logs; source contents, access values and external URLs are excluded.

The reader, source document constructor, chunker and indexed-member admission share
the Knowledge contract's `MAX_SOURCE_MEMBER_BYTES` limit of 256 KiB for complete
UTF-8 members. Reads still require one URI-matching text item and its exact digest.
Oversized members fail whole. Each member admits at most 256 chunks; embedding
text and batch limits apply independently. This source bound requires no stored
wire migration and preserves the chunker output of already admitted input.

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
current instance, generation, dispatch epoch and published tool allowlist. Knowledge
consumes the gateway-signed `AuditManagedExecution` from the v2 internal assertion;
checked positive-counter conversion compares it with current Store counters without
truncation. Attribution supplies execution identity and grants no permission. The
gateway owns runtime-template admission.

Tool and resource discovery evaluate each descriptor with the shared policy evaluator.
A narrow tool rule does not require a separate rule granting the whole server. Search
and embedding recheck authority before delivery. A changed control revision or search
collection registration aborts delivery. Tools allow 60 seconds for their domain work;
each admission or delivery check has a separate 10-second deadline. Resource reads
have a 60-second deadline and catalog SQL statements stop after 10 seconds.

The binary reads Store credentials and internal public trust from installation
configuration. It connects to the shared embedding endpoint using its API key and a
JSON `QualifiedEmbeddingRuntime` bundle through `VEOVEO_EMBEDDING_RUNTIME_FILE`. Database migrations belong to installation bootstrap.
`readyz` requires every configured tenant worker to have an active index or a complete
catalog-only selection, and checks Store's control pointer within two seconds.
After initial synchronization, the worker keeps serving while reconciling collection
changes. SQL source epochs and freshness still exclude invalidated members until their
source observations are current. Initial builds, connection recovery, failed workers
and shutdown withdraw readiness; a content or grant update does not remove the HTTP
endpoint that clients use to observe its completion.
`healthz` reports HTTP process liveness independently of indexing. The reference
installation reaches readiness after indexing its approved collections.

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
rotation. The embedding key comes from `embedding.apiKeySecret`. The public ConfigMap
also supplies the installation-admitted immutable runtime bundle named by
`knowledge.embeddingRuntimeConfigKey`. Its bytes belong to `configurationRevision`;
Helm rejects a missing bundle key, inline credentials and missing deployment dependencies.

Startup and liveness probes use `healthz`; readiness uses `readyz`. The 45-second
termination allowance covers worker cleanup and HTTP draining. Configuration changes
use a new public bundle revision; key rotation uses Kubernetes Secret projection and
the worker's next authenticated connection. The installation registers the service at
`http://knowledge-mcp:8800/knowledge/mcp` with health URL
`http://knowledge-mcp:8800/knowledge/readyz`.

## Indexing Configuration And Lifecycle

The binary requires one `--indexing-config` JSON file per tenant, up to 128 distinct
tenants. Each file selects the registered machine client and signing key file. The
control plane supplies its token endpoint, protected resource, scopes and default Work
Context. Exactly one indexing client may serve a tenant's single active generation.
The service rejects missing, ambiguous or invalid registrations before authentication.

```json
{
  "tenant": "example",
  "clientId": "knowledge-indexer",
  "keyId": "indexer-v1",
  "signingAlgorithm": "ed_dsa",
  "privateKeyFile": "/etc/veoveo/knowledge/signing/private.pem",
  "trustedCaFile": null,
  "chunkSettings": {
    "version": "structure-v1",
    "maxCharacters": 1500,
    "overlapCharacters": 150
  },
  "queryTask": "Given a web search query, retrieve relevant passages that answer the query"
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

An index generation records model/checkpoint, dimension, pooling, normalization,
effective precision and token ceiling, together with the query task, chunker and
approved collection fingerprints. Changing those semantics builds a generation
beside the active one. An execution-image or same-space serving change requires
qualified compatibility with every retained producer before reuse; it does not
change generation identity. The active pointer moves after complete approved
collection coverage. Different vector spaces never share an index. The Knowledge
evaluation set supplies the runtime selection workload.

The [knowledge domain contract](../../platform/knowledge/contract/DESIGN.md) owns generation
identities and specifications below both Store and the service. Store creates a
separate chunk table and vector index for each generation, checks current approval
fingerprints before writes and activation, and reclaims retired generations explicitly.

## Retrieval Evaluation

The library evaluator runs every judged query through `SearchService` with a limit of
ten. It requires a complete manifest of the caller-visible members in the selected
collections. SQL-admitted candidate pages check that manifest before and after the run.
Every result must match the expected source revision. A changed generation, omitted
member or unexpected indexed revision rejects the run. The evaluator allows one hour;
each search keeps its existing sixty-second deadline.

The report keeps generation and specification identity, an audience-policy fingerprint,
source revisions and content digests, query judgments, ranks and latency. Recall at ten
is the macro average across queries. Persisting a report uses the Store's append-only
generation measurement API; evaluation does not alter activation policy.

The private measurement format is `veoveo.ai/knowledge-retrieval-evaluation/v1`.
It is a JSON benchmark artifact, separate from the public MCP protocol.

`tests/gpu_retrieval.rs` compares embedding configurations using an isolated RocksDB
fixture and the shared CUDA runtime client. It first builds and evaluates a serving
generation, then builds a second generation while issuing searches against the first.
The closed-loop search load allows one in-flight request and schedules arrivals every
100 milliseconds. Per-collection timings include reconciliation, embedding and index
writes. Full rebuild time also includes schema preparation, completion of the last
search and activation. Reports include successful embedded chunk counts, throughput,
search latency percentiles, the generation specification and all retrieval measurements.
The harness rejects an empty concurrent workload and verifies report readback through
a second Store connection.

The input contains typed source registrations, captured member text and observations,
an explicit caller selection and relevance judgments. The fixture re-observes those
fixed bytes at read time and preserves their source revision, provenance and access
policy. It does not qualify live source authorization, source-change delivery or
restart recovery. At least eleven members and ten judged queries are required for
a model comparison. The complete operation has a thirty-minute deadline and owns its
database cleanup. Its private JSON output is create-only and contains no credentials.
See [the evaluation runbook](evaluation/README.md) for configuration and commands.

The repository corpus serializes twelve fictional inspection scenarios through Map,
Artifact, Time and Reason contract libraries. It includes 153 members across nineteen
collections and 78 judged queries, including Spanish paraphrases. Five members contain
the source servers' actual design text. All queries search the complete corpus;
relevance judgments are fixed before running a model. This workload compares retrieval
on controlled domain examples and does not establish production-wide search quality.

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
| `src/evaluation.rs`, `tests/evaluation.rs`, `tests/gpu_retrieval.rs` | judged retrieval through production search, generation measurement persistence and CUDA model comparison workload |
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

`tests/support/conformance.rs` runs the shared hosted checker over the same HTTP
fixture. It checks discovery, schemas, authentication rejection, host admission, health,
the contract declaration and document delivery. The immutable `knowledge.docs`
collection exercises K01 through K06. K07 and K08 do not apply because that collection
declares neither change subscriptions nor its own search tool. K09 and K10 require
review: the shared document provider derives provenance from embedded documents and
uses their content digests as revisions. Synthetic embeddings in this fixture do not
qualify GPU execution or retrieval quality.

The ignored `http::installed` test reads an installation's public control plane and
connects through its HTTPS gateway with an ordinary caller token. The caller must
have read access to every approved collection. It compares the complete paged catalog
with those approvals, checks that indexed collections share an active generation,
and searches each approved documentation collection by a source document's title.
Every returned link is read through the source server and checked against its content
digest and indexed revision. The test also exercises the public embedding tool.
It allows five minutes and performs no source mutations. Before network access, it
creates a mode-0600 outcome file with schema
`veoveo.ai/knowledge-installed-outcome/v1`. The successful baseline keeps its
`veoveo.ai/knowledge-installed-acceptance/v2` report inside that outcome. Typed
request intents, completion or MCP codes, failure digests and SDK close states
record partial failures without response bodies or credentials. An owner cleanup
action retains the actual client and catalog listener outside cancellable work,
then awaits their original consuming close futures within one cleanup deadline.
Relative input and report paths resolve from the repository root. A running
hardware embedding workload is a prerequisite; the shared runtime's GPU acceptance
establishes hardware execution. These retrieval checks do not measure recall on the
domain evaluation set.

The separate ignored `http::installed::policy` case admits a closed fixture from
`VEOVEO_KNOWLEDGE_POLICY_INPUT`: public control-plane path, selected profile,
private token-file path, new absolute output path, independently expected
Knowledge collections, gateway tool names, concrete resources and templates, plus
one to eight denied document IDs. Endpoint selection comes from the supplied
profile. Visible collections must be a proper subset of the indexing client's
approved selection; legitimately empty visible sets are accepted. Declaration and
profile exposure checks reject ineligible expectations before network access.
Mixed profiles contribute only typed Knowledge-owned discovery members to these
assertions. Catalog and completion must agree with the expected collection set;
selected existing documents must return MCP policy denial `-32600`. Discovery
allows at most 32 pages per surface, source catalogs 100 pages and the journal
512 requests. This read-only policy case uses the same private journal and owned
SDK cleanup. Native input, matching and cancellation controls qualify harness
behavior; cold startup, installed recovery and GPU execution require their own
installed qualification.

The separately selected ignored cold-start case reads
`VEOVEO_KNOWLEDGE_COLD_START_INPUT`. Its closed private JSON contains `schema`,
`source` (the shared installed-source installation-target path, public endpoint,
private caller-token file, deployment and new absolute output path), `profile`,
`context`, `namespace`, `deploymentUid`, `namespaceUid`, digest-pinned `image`,
`embeddingDeployment`, `embeddingRuntime`, `launchGeneration`, `startupSeconds`
and `stabilitySeconds`. The runtime is the installation's independently qualified
`QualifiedEmbeddingRuntime`; native synthetic fixtures cannot supply installation
qualification. Admission binds its image digest and required NVIDIA request and
limit to the selected embedding workload. The successful public verification must
report that runtime's embedding space.

The operator prepares the selected Knowledge Deployment at zero replicas and
supplies the next deployment generation for one externally authorized launch.
The test performs only reads and local watch/port-forward operations. It rejects
initial selected Pods, arms a streaming initial-events watch, and synchronizes
`watchArmedResourceVersion` to the private mode-0600 outcome before the operator
launches. The operator makes no further changes during observation. Kubernetes
must supply the initial-events-end bookmark; a gap, API error or missing handshake
fails qualification without a transport fallback. A consumed watch event stays
owned by the watch while its ReplicaSet ownership read is pending. Competing
verification or probe work can interrupt that read without losing the event;
state, journal and event settlement commit together after admission.

Startup allows 30–1800 seconds and records up to 16 Pod identities and 64 restarts
per container before first Ready. A selected container must return readyz 503 then
200 through its Pod port-forward with the owner-declared Host authority. Native
identity reads fence both sides of each accepted HTTP sample. This qualifies a
cold process start against the admitted retained installation state; first-ever
index construction requires separate proof of empty state. HTTP 200 freezes the
successful Pod UID, container ID, image ID and restart count. A post-200 Pod GET
admits that instance and supplies an opaque resourceVersion fence for the same
prearmed watch. Earlier queued startup observations may drain before this fence.
Kubelet's initial Ready condition may lag HTTP success; the original startup
deadline covers that wait. After the fence and first selected Pod Ready condition,
readiness regressions fail qualification. Stale observations from previously
admitted startup Pods cannot replace the successful instance, and newly appearing
competing Pods are refused. The test then runs the existing OAuth catalog,
search, document and embedding assertions against the frozen instance.
A 5–120 second stability interval and final watch/current-Pod fence check readiness
and one common Knowledge generation. The total case has an additional 300 seconds
for admission and verification. The shared owner's default overall deadline is
300 seconds; longer cases require an explicit `VEOVEO_SMOKE_DEADLINE_UNIX_MS`.
The fixture cannot extend that global deadline. Original native child cleanup and SDK close futures
stay owned outside cancellation, share the owner's cleanup deadline and keep
failed or expired cleanup unqualified. Native controls establish fixture,
observation and cleanup behavior; installed unattended startup and hardware
execution require a separate successful run.

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

## Value Admission

Search requests and source pages use `Checked` over unchanged Wire fields. Their owner checks retain request bounds, concrete unique members and the existing continuation policy; collection relationships remain in the knowledge owner.

## Native Query Fixture Placement

Store-backed native fixture statements live in `tests/queries/`, grouped by the
calling harness. Colocated fixtures include those files with their existing bindings
and result slots. Complete static statements cover finite SQL grammar choices.

## Installation And Client Authority

The reusable `runtime` receives an explicit Policy internal-client resolver and
has no normal Agent persistence dependency. Each read resolves current authority
and repeats it before delivery. Catalog observation uses the resolver's owner table
declarations; the static resolver watches no Agent tables. Kernel-only HTTP and
subscription fixtures assert those tables are absent.

Server composition reads the revisioned module plan through `VEOVEO_MODULE_PLAN`
and the existing expected composition, generation, credential revision and runtime
username inputs. Modules readiness verifies Knowledge/Gateway/Identity dependencies
and committed preparation in one read-only native snapshot. A selected Agents lane
adds the managed adapter and its prerequisites. `managed-clients` enables that
adapter's compiled availability; lane selection chooses its use. An unavailable
selected adapter or missing/drifting preparation fails configuration before source
workers or embeddings start. Startup never infers an authority mode from table
existence and never substitutes static authority for selected Agents.


## Embedding Execution Provenance

The installation supplies a measured runtime bundle through its existing public
configuration ConfigMap. `knowledge.embeddingRuntimeConfigKey` names that file and
`configurationRevision` covers it along with indexing configuration. The chart does
not construct qualification from an image pin. The endpoint and bundle identify the
installation-selected deployment; model discovery establishes only an advertised name.

Generation identity covers vector-space semantics, query task, chunking and approved
collections. Execution provenance has separate immutable profile and qualification
identities. The Store publishes each indexed batch receipt atomically with its chunks
and records its producer in the generation's retained set. Conditional revalidation
preserves that original producer and receipt. Replacing data under a qualified new
same-space runtime adds its producer without relabeling old batches.

Search captures one tenant/generation admission before requesting the query vector.
Every ranking depth checks that admission's retained set and publication epoch in the
ranking transaction, together with whole immutable registry facts. The epoch advances
when a new producer enters the set. Same-producer writes preserve it. Changed producer
sets fail with `knowledge_embedding_publication_changed`; the caller can re-admit in a
new request, whose existing 60-second deadline includes embedding and ranking. Source
policy, approvals, leases and context checks still precede ranking limits.

Activation and indexing reservations check qualification for every retained producer
inside their existing coordinator-fenced transactions. Unknown, incompatible, changed
or failed qualification blocks reuse. Registry publication is idempotent under the same
content identity and rejects any different contents under that ID. Read admission may
publish trusted installation facts but never changes a generation's producer set.

## Provenance Installation Transition

The current persistent format requires producer receipts for every indexed batch and
explicit measured execution settings. An older generation cannot acquire attribution
by inference from its vector shape, model name or image pin. Installations must drain
Knowledge consumers and create fresh Knowledge state after the replacement NVIDIA
profile passes reference, retrieval, scheduling and capacity acceptance. Historical
measurements do not constitute receipts for a newly constructed profile. The existing
qualified installation stays in service until those report artifacts exist; deploying
this format without them is blocked. Synthetic test bundles qualify codecs, fencing
and rollback only.

The ranking transaction reports a changed captured admission when a producer set or
active generation changes during query embedding. Store recognizes only SurrealDB
3.3's typed `Thrown` kind with the exact owned
`An error occurred: knowledge_embedding_publication_changed` message. Knowledge asks the caller to repeat
that read or search against current admission. The service does not retry a mutation
or classify other database failures as changed admission.
