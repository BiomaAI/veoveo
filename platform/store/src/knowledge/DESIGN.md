# Knowledge Storage

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| SurrealDB 3.3.0 / SurrealQL | schemafull catalog records, transactions, native record-reference cleanup, FULLTEXT BM25 and HNSW cosine indexes |
| [Knowledge domain contract](../../../knowledge/contract/DESIGN.md) | typed approvals, generation specifications, retrieval measurements and source-bound chunks |
| [Knowledge-source extension](../../../../mcp/knowledge-extension/DESIGN.md) | typed source observations; the Store imports its contract feature without MCP runtime |
| RFC 9562 / SHA-256 | generation identity and checked specification/approval fingerprints |

## Tables And Ownership

The version-zero Knowledge owner lane defines `knowledge_collection`, `knowledge_generation`,
`knowledge_active`, `knowledge_member`, `knowledge_coverage`, `knowledge_coordinator`
and `knowledge_sync`. `knowledge_evaluation` stores retrieval measurements attached to
their generation. Catalog, source-sync and member
state use the native seven-day change feed. The indexing service must reconcile from
source enumeration when its saved cursor falls outside retention.

Each generation creates one schemafull `knowledge_chunk_<uuid>` table with its own
BM25 and HNSW indexes. A table contains vectors from one embedding space. Its dimension
comes from the checked generation specification. The schema uses `PERMISSIONS NONE`;
only authenticated system users with the existing database-editor role can operate it.
This generation-owned DDL is a domain operation, separate from installation migrations.
Table names derive from a canonical UUID and use bound table expressions. INSERT
receives a native SDK Table parameter. The HNSW dimension uses a checked integer
literal because the native grammar requires it. Resource URIs, collection names and
caller values use bound parameters. Complete statements live in `../queries/knowledge/`.

The [SurrealDB index reference](https://surrealdb.com/docs/reference/query-language/statements/define/indexes)
defines each HNSW index over a field with a fixed dimension. Separate generation
tables keep both vectors and index structures apart during rebuilds.

## Transactions And Recovery

`claim_knowledge_coordinator` claims a tenant's 30-second lease and advances its
process epoch after expiry. A live different owner prevents acquisition. Every
generation, collection and member mutation checks the owner, process epoch and lease
expiry inside its transaction. The transaction writes the lease row to conflict with
takeover. Cloned lease handles serialize their local mutations and renewal to avoid
conflicts within one worker. Store still arbitrates independent workers. Release checks
owner and epoch; an old process cannot expire its successor's lease.

`knowledge_sync` records a source epoch for each generation and collection. Invalidation
advances it and marks the collection unready. A Store-issued `CollectionSyncTicket`
captures the epoch before enumeration. Completion checks that ticket, current approval
and coordinator, preventing an old traversal from marking a newer one ready.

Catalog registration compares the previous fingerprint. Concurrent discovery cannot
overwrite a newer approval or revocation. An identical registration is idempotent.
Writes and exact reads validate the full typed approval against its descriptor.
`begin_knowledge_catalog` captures a tenant's prior registrations and the active control
revision before discovery. `replace_knowledge_catalog` locks that control pointer,
checks both snapshots and replaces the complete tenant catalog in one transaction.
The service submits at most 1,024 registrations; an empty selection revokes all of that
tenant's registrations. Source and approval fingerprints exclude the control revision,
allowing unrelated installation changes to reuse index generations after reconciliation.
Publication still checks the control pointer and its digest independently.
`knowledge_member_observed` selects a member in SQL against the tenant's active
generation and current approved registration fingerprint. It admits only an observed,
non-stale, non-deleted member in a ready source epoch with a valid freshness deadline.
Mutable sources also require a live coordinator lease. Gateway indexing subscriptions use this check; an initial
build subscribes to the declared collection enumeration resource instead.
Generation creation checks every selected approval and creates its schema and immutable
specification in one transaction. Its state begins at `building`.

`begin_knowledge_member_read` increments the member's epoch, marks it stale and removes
its collection-coverage receipt before a source request begins. Search excludes stale
members immediately. A replacement checks the epoch, tenant, current approval and
generation specification, then replaces the chunks and observation atomically. A
late source response cannot restore content after another member, source or coordinator
invalidation. Failed source
reads leave the member stale. Only definitive owner confirmation permits deletion;
timeouts, unavailable reads and subscription loss do not establish that fact.

Chunk replacement uses the Store's 64 MiB message profile. The WebSocket write ceiling
allows one such message beside its 128 KiB flush buffer; it does not preallocate that
memory on connection. Replacement returns no inserted vectors, avoiding a second copy
of the bulk payload in the response. One SurrealQL transaction still owns the fence,
deletion, insert and observation update. Native qualification exercises the contract's
256-chunk and 8,192-dimension limits through the default remote connection.

Conditional replacement reuses the existing text and vectors only when the observation
preserves revision, digest, access and provenance. It updates observation timestamps
and the member's freshness deadline in the same transaction. The ticket's member,
source and process epochs must still match.

The service records collection coverage after complete source enumeration. Store rejects
coverage while any admitted member from that source epoch is unresolved. Members absent
from this traversal keep their earlier epoch and are excluded from search; absence does
not establish deletion.
Activation checks the full current approval set and every coverage receipt, compares
the previous active generation, retires it and switches the tenant's active pointer
in one transaction. New reads and approval changes conflict with activation through
the rows that both operations access. The service can recover the active generation
after process restart without reconstructing it from local memory.

Reclamation accepts only a building or retired generation that is absent from the
active pointer. It deletes generation metadata while the chunk schema exists, allowing
native references to cascade member, coverage and chunk records. It then drops the
empty chunk table and indexes in the same transaction. Outstanding read tickets
fail because their generation no longer exists. Operators choose when to reclaim a
retired generation; retaining it does not authorize rolling the active pointer backward.

## Evaluation Measurements

`record_knowledge_evaluation` appends a checked report only when its generation belongs
to the tenant and its specification fingerprint agrees. Its corpus collections must
belong to that specification. The report digest identifies the row, making an identical
repeat idempotent. The transaction rejects a conflicting row and a missing or changed
generation. It changes neither the generation specification nor the active pointer.

Measurements retain the source revisions, judged queries, ordered results and timing
needed to reproduce recall at ten. They describe the evaluator's observation interval;
source changes do not rewrite a prior score. Exact report reads select tenant,
generation and digest before decoding. Reclaiming a generation cascades its reports
through a native reference. These APIs use the private database-editor connection and
do not expose corpus contents through the public Knowledge catalog.

## Catalog Reads

`CatalogSelection` binds all approved collections, one source or one collection.
`read_catalog.surql` checks tenant, full current approval equality and required scopes
before decoding. `read_sources.surql` applies the same admission and cursor before
source grouping, ordering and its 101-row limit. The service returns 100 rows and uses
the lookahead to advertise a next cursor. Approval input is capped at 1,024 collections.

Registration stores the checked enumeration root alongside the document.
`read_root.surql` selects subscription admission by tenant and exact root, indexing
approval, current approval equality and required scopes before decoding. Its two-row
limit detects ambiguous approved roots, which the Store rejects. Catalog-only
collections never authorize enumeration subscriptions.

`CatalogCompletion` selects source or collection identities. Its query applies the
same catalog admission, a bound prefix, deduplication and ordering before a 101-value
limit. Store parses results into `ServerSlug` or `CollectionId`; the MCP adapter
converts them to completion strings at delivery.

`knowledge_collection_statistics` requires one caller-admitted collection and uses
the candidate admission fragment before aggregation. It counts first chunks for member
totals and all chunks for chunk totals, then selects the latest observation and optional
modification timestamps. Its ten-second SQL deadline bounds the scan. The result also
carries the earliest future grant, record, freshness or mutable-coordinator deadline
among admitted rows, allowing catalog listeners to schedule a re-read when visibility
can expire. Missing timestamps use SQL sentinels within aggregation and return `NONE`
at the result boundary; sentinels never enter the public contract.

## Candidate Admission

`CandidateScope` carries current caller policy, including the source collections exposed
by its profile. Each collection carries the owning URI scheme intersected with its
current profile selectors. `resource_selection.rs` binds schemes, prefixes and checked
template literal segments as values. The SQL closure embedded in the complete candidate and ranking statements
uses first-delimiter consumption to match the foundational template semantics. It
executes within candidate admission in both ranking queries. SQL selects the tenant and active generation, current collection
approval fingerprint, collection-required scopes, ready source epoch, live mutable-source
coordinator, observation freshness, non-stale member, source read policy and every required
clearance label before ordering and LIMIT. `admission.rs` derives the typed SQL fields
from the observation. The `work-context` policy permits read membership in the stored
context. The `selected-work-context` policy additionally requires that the caller
selects it; owner and live grants remain independent read paths.
`selected-work-context-members` requires selected-context membership for owners and
grant holders too. The `subjects` policy requires an owner/grant match even when
the caller shares that context. `subjects-in-context` additionally matches the current
selected context and the source profile when recorded; membership in another context
cannot satisfy that condition. `tenant` permits readers admitted to the collection
within its tenant. Every policy stops at the source's record deadline. SQL tests
principal and group grant deadlines at query time; it never turns an expiring grant
into a permanent subject in the index. Profile collections have no record access descriptor and use the
collection's current profile admission.

Cursor pages contain at most 100
chunks; cursors bind the tenant, generation and collection. The decoder checks
selected observation/projection agreement; it does not
discard unauthorized rows after pagination.

The knowledge service still applies the shared canonical access decision before
returning results. `search_knowledge` applies the same admission predicate in its complete BM25 and HNSW statements.
The database executes native `search::rrf`, groups chunks by member and selects the
best result per member in one transaction. Its HNSW K/EF values come from a closed
expansion-window enum. Query vectors must match the complete generation embedding
space. Source-current reconciliation belongs to the service coordinator.

An HNSW response shorter than the candidate window triggers exact cosine-distance
ranking in SurrealQL with the same admission predicate, ordering and candidate limit.
That query has a 10-second timeout within the service's 60-second search deadline.
An approximate graph's short page cannot establish that all readable candidates were
considered. The explicit exact query recovers those candidates before rank fusion;
it never loads denied observations into the service. This follows SurrealDB's
[filtered-search guidance](https://surrealdb.com/blog/why-does-my-vector-search-return-nothing-when-i-add-a-filter-2).

## Qualification

`platform/store/tests/knowledge.rs` runs against an isolated pinned SurrealDB container
with two database-editor connections and a 120-second timeout. Synthetic normalized
vectors qualify storage, not inference or GPU execution. The fixture owns cleanup.
It checks incomplete activation, SQL denial before decoding and limits, owner-only and
group-grant reads, selected-context and profile restrictions, record and grant expiry
without reindexing, tenant sharing, stale-reader
fencing, approval revocation, space separation, active-pointer compare-and-set,
definitive deletion, and generation reclamation with native referential cleanup.
The catalog suite checks concurrent replacement, changed control authority, removal of
revoked collections and isolation between tenants.

`servers/knowledge-mcp/tests/pipeline.rs` exercises the complete source-to-search
library path with synthetic vectors, including 140 malformed denied rows before three
readable members, metadata-only indexing, source-scope and selected-context denial,
failed-refresh invalidation, changed labels, entity-kind selection, 200 chunks for one
member and a failed rebuild that preserves the active generation.

Resource-selection fixtures compare SQL pages with the foundational matcher across
prefixes, multiple variables, repeated suffixes, percent-encoded delimiters and literal
quote characters. A hybrid-search fixture places 145 otherwise-readable but
profile-denied members ahead of three permitted results, corrupts denied observations,
and verifies full keyword and semantic-only result pages. Revoking exposure takes
effect without changing the index.
