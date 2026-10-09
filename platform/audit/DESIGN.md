# Audit Writer And Integrity Services

## Standards And Protocols

| Boundary | Profile |
|---|---|
| RFC 9562 | Canonical UUIDv7 record and HTTP request identities |
| W3C Trace Context | Nonzero lowercase 32-hex trace IDs and 16-hex span IDs |
| RFC 9162, RFC 8785 and RFC 8032 | Typed block heads, decimal-string versionstamps, Merkle roots and Ed25519 signatures |
| RFC 3339 and JSON | Typed timestamps, tagged closed enums and checked deserialization |
| `veoveo.ai/audit-record/v1` | One reviewed record shape across platform producers and readers |
| `veoveo.ai/audit-block/v1` | Persisted signed block heads and signature domain separation |
| `veoveo.ai/audit-key-transition-qualification/v1` | Native fixture counts and final sequence; no signing material |
| `ai.veoveo/knowledge-source` | Reviewed observation fields from the protocol-independent extension contract |

This crate owns group-commit acknowledgements, persisted indexing windows, sealing,
export and verification. The lightweight [Audit contract](contract/DESIGN.md) owns
records, checked identities, targets and read projections. Store owns persistence,
lease fencing and query admission. Domain writes append their records inside the
existing transaction; a writer acknowledgement requires the commit to finish.

## Signing-Key Transition

Drain request writers and completion queues before shutting down the old sealer.
A successful sealer shutdown seals its committed backlog and releases its lease.
Capture each partition's final checkpoint in separately protected storage, including
its sequence, head hash and last versionstamp. Retain the old public verification key
with archived blocks. Keep the signing seed out of receipts and archive metadata.
Drain and remove every process holding the retired seed, including standby gateway
sealers, before activating replacement-only suffix verification. A standby holding
the old seed could acquire a later lease and sign again. Start the replacement sealer
with its new key against the same Store; it continues the existing sequence and
previous-head link. Lease generations fence a stale owner but do not retire its key.

For complete archive verification, supply the historical and replacement public keys
and require the independently protected final checkpoint. For post-cutover trust,
supply only the replacement public key, start at the protected cutover checkpoint and
require the protected replacement tail. A historical key in the archive key ring also
accepts new signatures by that key; use separate verifier instances for these two
trust profiles. A compromised historical seed cannot establish the cutover anchor or
historical authenticity. Protecting checkpoints after compromise cannot recover trust
that was already lost.

`tests/service/key_transition.rs` qualifies this procedure with persisted RocksDB
records and one selected writer/sealer shutdown and lease handoff; it does not prove
installation-wide removal of old-key holders. It verifies retained records
and the new suffix, rejects incomplete chains, tampering and unknown signing keys,
and refuses an old-key-signed post-cutover candidate under the replacement-only ring.
This native qualification does not replace rebuilding the selected image, installing
the replacement credentials or verifying the installed cutover and archive.

## Native Writer Records

Store binds append and indexing-read rows through nominal native records. Append
rows retain native record links, UUIDs and dates while the existing draft codec
preserves the frozen JSON format. Optional actor/profile lookups and indexing member
digests encode absence as native NONE. Indexing digests retain their unprefixed
64-character hexadecimal storage profile. Transactional append and window settlement
reuse the same append encoder.

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
identity, partition, lookup reference and the whole target/detail receipts. Store
projects the admitted draft’s profile, target and detail into read-only columns in
the append transaction. Target lookup keeps registered owner extensions as whole
JSON objects; its decoder uses the same target registry as the frozen draft. Queries
validate registry identity before SQL and compare the whole target lookup object.
Page, LIVE, replay, seal/export and view readers reject disagreement with the draft. Transaction writes
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

## Seal Driver Records

Store's nominal block row, block write and seal binding preserve native IDs, record
links, UUID fencing, datetime and integer sequence/cursor fields. Frozen block and
checkpoint documents use the existing JSON encoder. Hash text keeps its declared
spelling, and an absent previous hash stays native NONE. Sealing SQL and its
owner/generation/cursor comparison run unchanged in one transaction.
