# Audit Record Contract

## Standards And Protocols

| Boundary | Profile |
|---|---|
| RFC 9562 | Canonical UUIDv7 record and HTTP request identities |
| W3C Trace Context | Nonzero lowercase 32-hex trace IDs and 16-hex span IDs |
| RFC 9162, RFC 8785 and RFC 8032 | Typed block heads, decimal-string versionstamps, Merkle roots and Ed25519 signatures |
| RFC 3339 and JSON | Typed timestamps, tagged closed enums and checked deserialization |
| `veoveo.ai/audit-record/v1` | One reviewed record shape across platform producers and readers |
| `ai.veoveo/knowledge-source` | Reviewed observation fields from the protocol-independent extension contract |

This library owns the protocol-independent audit vocabulary. It depends on foundational
names and lightweight Artifact identities. Computers supplies its Audit target codec
through the installation registry. Store and MCP both depend
on this crate. The writer depends on Store; separating this contract avoids a Cargo
cycle and prevents persistence from importing the MCP runtime.

The draft builder derives the partition from the authenticated actor. Accepted outcomes
require the accepted reason. Serialization and input admission share these checks.
Details expose only reviewed enums, identifiers, digests, counters and timestamps.
`tests/detail_schema.rs` walks the generated detail schema and rejects free text,
open maps and unreviewed string wrappers. It also recognizes the reviewed, inlined
SHA-256 pattern. These identifier allowlists belong to this contract and must be
reviewed when a detail gains a new identifier type.
Numeric counters must fit the exact I-JSON integer range before a draft can be stored.
Tool, discovery and Task details must match the corresponding typed target. A platform
Task uses its UUID record link; an external Task uses its opaque gateway route and
server identity. These identities have separate variants.
Knowledge read details bind the member and collection owner to their resource target.
Successful reads require an observation whose conditional status matches the outcome.
Denied or failed reads cannot claim a returned revision. `KnowledgeReadObservation`
copies source identity, revision, digest, times, attribution and access fields. Its
external identity excludes navigation URLs because they can carry signed query
credentials. The closed audit schema rejects those URLs on input as well.
The access descriptor includes the source's closed read policy and its optional
`GatewayProfileId` restriction. That identifier is an installation profile route token
validated by the foundational type; it carries no credential or free-form description.
`IndexingRead` admits only collection-matching resource reads by a tenant service
client. Denials cannot enter an aggregate. `IndexingWindow` requires a matching server
target, five-minute UTC bounds, positive reads and consistent outcome counters. Its
occurrence time equals the window end. Constructor and decode checks share these rules.
Store owns conversion to compound record IDs and native record links. A partition key
distinguishes the installation partition from a tenant literally named `installation`.

The reader's policy owner supplies an `AuditReadScope`. Store checks it before query
execution and includes its admitted partition in SQL before decoding or limiting rows.
Cursors carry their partition and ordering and cannot move a reader to another tenant. Daily counts
use UTC day bounds and a keyset of day, class and outcome; readers can traverse every
page without silently truncating at a fixed total.

The audit domain owns `AuditScope::Read`. Protocol cores carry its `ScopeName` without
defining the domain vocabulary. Gateway readers check that scope on the current actor.
The actor's tenant selects its tenant partition; the `administrator` and `auditor`
roles also admit installation records.

`AuditRecordSummary` keeps typed targets and activities through browser delivery.
`reader_schema(registry)` composes this projection, page queries, daily counts and view receipts
for the existing client generator. A view receipt refers to a committed access record;
it grants no authority. Each read verifies the current actor, profile, selected
partition and receipt lifetime. JSON Lines exports contain a header, records and a
completion footer. Readers require that footer before accepting a complete download.

`AuditContext` carries verified actor, request correlation and invocation authority into
a domain operation. The MCP adapter derives it from `GatewayRequestContext` only after
validating the source-principal, actor and invocation relationship. Domain services keep
that attribution for background expiry and terminal records. Dictation summaries expose
an end reason, accepted chunk count and audio duration; they cannot carry transcript text.

Implementation and qualification are in progress under the
[audit acceptance](../../docs/CONTRACT_CONSISTENCY_PLAN.md#unified-audit-log).

Destination IDs wrap configuration hashes separately from content digests. Export
intents bind both content and signed-block hashes. Closed rejection codes carry no
provider response text and survive replica changes.

## Activity Vocabularies

Activity enums use the foundation's `Vocabulary` derive. Their snake_case spellings,
variant order and Serde unit-enum profile form part of the frozen audit format.
Owner-local tests compare every activity enum's JSON and schema with its previous
declaration and check nonhuman unit-variant ordinals. The derive introduces no audit
variants or authorization decisions.

## Identity Declaration Mechanics

Audit identities use `Id` with Audit-owned canonical RFC UUIDv7 and lowercase nonzero hexadecimal admission. Their owner schema functions preserve each UUID pattern, trace/span length and nonzero constraint. Serde uses checked String conversion for these IDs; the independent Activity enum wire profile does not change.

## Integrity Counter Admission

`AuditBlockSequence` admits 1 through i64::MAX; `AuditVersionstamp` also admits zero.
Their owner validator and decimal parser require the canonical unsigned spelling.
Serde converts through String in both human-readable and binary formats. JSON and
JCS therefore preserve versionstamps that exceed exact IEEE-754 integer precision.
The nominal counter types keep copied getters and their existing schema profiles.

## Owner Target Admission

`AuditTargetRegistry` binds each owner discriminator to its typed decoder, closed
JSON Schema and checked lookup reference. `AuditTargetRegistration<T>` constructs
and retrieves the registered owner type. The immutable registry shares its identity
through clones; a target from another registry cannot enter an append or query.
Target equality compares admitted JSON. Serialization emits the owner object directly,
and excludes registry identity, cached payloads and lookup references.

`AuditDecoder` admits target-bearing drafts, records, queries, summaries, pages and
export lines with an explicit registry. Its JSON parser rejects duplicate object
fields before creating a JSON value. Private input models carry unadmitted values;
the decoder then applies target admission and the draft's existing relationship
checks. Attribution-only `AuditContext` has no registry state.

Owner registration rejects unknown, repeated and core-colliding discriminators.
Schema composition rejects duplicate definition names and joins owner branches into
the target's closed union. An installation keeps supported read codecs registered
when it disables an owner's workload. Missing codecs produce a configuration error
when reading existing records. Registering a codec installs no runtime or database
schema.

Store extracts unadmitted driver values privately and decodes them with its configured
registry. Every returned row, including pagination's extra row, validates its record
identity, partition and lookup reference. Queries validate target registry identity
before executing SQL and match the entire `draft.target` object. Transaction writes
receive the same registry and append alongside the caller's domain write.

## Audit Target Codec

`ComputerAuditTarget` owns the frozen `{"kind":"computer","computer":"..."}`
Audit target object. Its `ComputerId` remains typed until the owner projects the
`computer` table and UUID lookup key. `register_audit_target` binds decoder, closed
schema and projection together in the installation's Audit registry. The codec
requires no Computers runtime or schema lane and supports reading historical Computer
records when the workload is disabled. Audit and Store import no Computer identities.

## Target Registry Composition

Store supplies the Audit writer and all readers with one immutable target registry.
Owner codecs are independent of enabled workloads. The installation composition keeps
the Computer read codec available when it disables the Computers workload. Audit's
normal dependency graph includes no Computers contract or runtime. Store performs
contextual admission before sealing, LIVE delivery, export or indexing recovery and
checks each stored target reference against its owner projection. Domain transaction
append keeps the same registry and commits audit rows atomically with domain changes.

## Indexing Read Windows

The writer commits each admitted indexing read to an accumulator and retry receipt
before acknowledging delivery. Store groups reads by the admitted actor, authority
and collection into database-clock five-minute windows. A receipt binds the draft ID
to the collection and draft fingerprint; replay with a different fingerprint fails.
New reads must have occurred within the preceding hour and at most one minute ahead
of database time. Receipts survive for one day, so pruning cannot admit an expired retry.

Each observed resource URI and revision contributes a SHA-256 member digest to the
window's ordered digest chain under `veoveo.ai/audit-indexing-members/v1`. Counters
record total, not-modified and failed reads. Open accumulators survive process loss.
The writer checks elapsed windows every five seconds, and any replica may finalize
up to 32 at a time. Finalization compares the persisted read count, appends the
immutable aggregate and deletes its accumulator in one transaction. A competing
update requires another pass. The worker stops Audit admission if finalization or
receipt cleanup fails.

## Persistence And Native Queries

Store owns Audit persistence statements. Audit native fixture statements live in
`tests/queries/` with their existing target-registry and transaction bindings.
The fresh Audit lane creates `audit_sealer:active` with generation and replay cursor
zero, a fresh owner UUID and initial lease/replay timestamps. Recorded lane replay
preserves this state; the selected-lane native matrix qualifies that behavior.
